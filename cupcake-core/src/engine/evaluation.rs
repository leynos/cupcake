//! Event routing and evaluation for the engine.

use super::*;

impl Engine {
    /// Find policies that match the given event criteria
    #[instrument(
        name = "route_event",
        skip(self),
        fields(
            routing_key = %routing::create_event_key(event_name, tool_name),
            matched_count = tracing::field::Empty,
            policy_names = tracing::field::Empty
        )
    )]
    pub fn route_event(&self, event_name: &str, tool_name: Option<&str>) -> Vec<&PolicyUnit> {
        let start = Instant::now();
        let key = routing::create_event_key(event_name, tool_name);

        // First try the specific key
        let mut result: Vec<&PolicyUnit> = self
            .routing_map
            .get(&key)
            .map(|policies| policies.iter().collect())
            .unwrap_or_default();

        // ALSO check for event-only policies when there's a tool
        // This handles the case where ONLY wildcard policies exist (nothing to duplicate into)
        if tool_name.is_some() {
            let wildcard_key = event_name.to_string();
            if let Some(wildcard_policies) = self.routing_map.get(&wildcard_key) {
                // Only add if not already present (avoid duplicates from build-time merging)
                for policy in wildcard_policies {
                    if !result.iter().any(|p| p.package_name == policy.package_name) {
                        result.push(policy);
                    }
                }
            }
        }

        // Record matched policies
        let current_span = tracing::Span::current();
        current_span.record("matched_count", result.len());
        if !result.is_empty() {
            let policy_names: Vec<&str> = result.iter().map(|p| p.package_name.as_str()).collect();
            current_span.record("policy_names", format!("{policy_names:?}").as_str());
        }

        trace!(
            duration_us = start.elapsed().as_micros(),
            matched = result.len(),
            "Policy routing complete"
        );

        result
    }

    /// Evaluate policies for a hook event.
    ///
    /// This is the main public API for policy evaluation.
    #[instrument(
        name = "evaluate",
        skip(self, input, telemetry),
        fields(
            trace_id = %trace::generate_trace_id(),
            event_name = tracing::field::Empty,
            tool_name = tracing::field::Empty,
            session_id = tracing::field::Empty,
            matched_policy_count = tracing::field::Empty,
            final_decision = tracing::field::Empty,
            duration_ms = tracing::field::Empty
        )
    )]
    pub async fn evaluate(
        &self,
        input: &Value,
        mut telemetry: Option<&mut TelemetryContext>,
    ) -> Result<decision::FinalDecision> {
        let eval_start = Instant::now();

        // STEP 0: ALWAYS PREPROCESS - Self-Defending Engine Architecture
        // The Engine never accepts raw, unpreprocessed input. This provides
        // defense-in-depth by ensuring ALL paths (CLI, FFI, tests) are protected
        // from TOB-3 (spacing bypass) and TOB-4 (symlink bypass) attacks.
        //
        // This preprocessing is:
        // - Automatic: No caller action required
        // - Universal: Protects all policies (builtin and custom)
        // - Idempotent: Safe to call multiple times (e.g., if CLI also preprocesses)
        // - Fast: ~30-100μs overhead (<0.1% of total evaluation time)
        //
        // See TOB4_IMPLEMENTATION_LOG.md Phase 4 for architectural rationale.
        //
        // IMPORTANT: This clone is intentional and required for security.
        // DO NOT OPTIMIZE: The preprocessing defends against adversarial input attacks
        // (TOB findings) and must never modify the original input. The overhead is
        // <0.1% of evaluation time. With default config, preprocessing modifies:
        // - Bash commands: if whitespace normalization needed
        // - File operations: always (adds resolved_file_path, is_symlink fields)
        // - Other events: no-op but clone still required for uniform security model
        //
        // NOTE: Copy-on-write (CoW) optimization considered and rejected:
        // - Clone cost: ~10-50μs even for large files, <0.1% of 10-100ms eval time
        // - CoW complexity: requires custom types, mutation tracking, architectural changes
        // - Security value: immutable input pattern prevents accidental modifications
        // - Tradeoff: Security and simplicity take priority over micro-optimization
        let mut safe_input = input.clone();
        let preprocess_config = crate::preprocessing::PreprocessConfig::default();
        crate::preprocessing::preprocess_input(
            &mut safe_input,
            &preprocess_config,
            self.config.harness,
        );
        trace!("Input preprocessing completed (self-defending engine)");

        // STEP 1: Extract event info from SAFE input for routing
        // Try both camelCase and snake_case for compatibility
        let event_name = safe_input
            .get("hookEventName")
            .or_else(|| safe_input.get("hook_event_name"))
            .and_then(|v| v.as_str())
            .context("Missing hookEventName/hook_event_name in input")?;

        let tool_name = safe_input.get("tool_name").and_then(|v| v.as_str());

        // Set span fields
        let current_span = tracing::Span::current();
        current_span.record("event_name", event_name);
        current_span.record("tool_name", tool_name);
        if let Some(session_id) = trace::extract_session_id(&safe_input) {
            current_span.record("session_id", session_id.as_str());
        }

        info!("Evaluating event: {} tool: {:?}", event_name, tool_name);

        // Create Executor early - used for both global and project evaluation
        // The Executor handles all OS/IO interactions (signals)
        let exec = executor::Executor {
            rulebook: self.rulebook.as_ref(),
            global_rulebook: self.global_rulebook.as_ref(),
            watchdog: self.watchdog.as_ref(),
            working_dir: &self.paths.root,
        };

        // PHASE 1: Evaluate global policies first (if they exist)
        if self.global_wasm_runtime.is_some() {
            debug!("Phase 1: Evaluating global policies");
            let capture_telemetry = telemetry.is_some();
            let (global_decision, global_decision_set, global_signal_executions) = self
                .evaluate_global(&safe_input, event_name, tool_name, &exec, capture_telemetry)
                .await?;

            // Record global evaluation in telemetry
            if let Some(ref mut ctx) = telemetry {
                let phase = ctx.start_phase("global");
                phase.evaluation_mut().record_routing(
                    true,
                    &self.global_routing_map.keys().cloned().collect::<Vec<_>>(),
                );
                // Record signal executions from global evaluation
                for signal in global_signal_executions {
                    phase.record_signal(signal);
                }
                phase
                    .evaluation_mut()
                    .record_wasm_result(&global_decision_set);
                phase
                    .evaluation_mut()
                    .record_final_decision(&global_decision);
            }

            // Early termination on global blocking decisions
            match &global_decision {
                decision::FinalDecision::Halt { reason, .. } => {
                    info!("Global policy HALT - immediate termination: {}", reason);
                    // Record exit reason in telemetry
                    if let Some(ref mut ctx) = telemetry {
                        if let Some(phase) = ctx.current_phase_mut() {
                            phase
                                .evaluation_mut()
                                .record_exit(format!("Global halt: {reason}"));
                            phase.finalize();
                        }
                    }
                    current_span.record("final_decision", "GlobalHalt");
                    current_span.record("duration_ms", eval_start.elapsed().as_millis());
                    return Ok(global_decision);
                }
                decision::FinalDecision::Deny { reason, .. } => {
                    info!("Global policy DENY - immediate termination: {}", reason);
                    // Record exit reason in telemetry
                    if let Some(ref mut ctx) = telemetry {
                        if let Some(phase) = ctx.current_phase_mut() {
                            phase
                                .evaluation_mut()
                                .record_exit(format!("Global deny: {reason}"));
                            phase.finalize();
                        }
                    }
                    current_span.record("final_decision", "GlobalDeny");
                    current_span.record("duration_ms", eval_start.elapsed().as_millis());
                    return Ok(global_decision);
                }
                decision::FinalDecision::Block { reason, .. } => {
                    info!("Global policy BLOCK - immediate termination: {}", reason);
                    // Record exit reason in telemetry
                    if let Some(ref mut ctx) = telemetry {
                        if let Some(phase) = ctx.current_phase_mut() {
                            phase
                                .evaluation_mut()
                                .record_exit(format!("Global block: {reason}"));
                            phase.finalize();
                        }
                    }
                    current_span.record("final_decision", "GlobalBlock");
                    current_span.record("duration_ms", eval_start.elapsed().as_millis());
                    return Ok(global_decision);
                }
                _ => {
                    debug!("Global policies did not halt/deny/block - proceeding to project evaluation");
                    // Finalize global phase before continuing
                    if let Some(ref mut ctx) = telemetry {
                        if let Some(phase) = ctx.current_phase_mut() {
                            phase.finalize();
                        }
                    }
                    // TODO: In the future, preserve Ask/Allow/Context for merging with project decisions
                }
            }
        }

        // PHASE 2: Evaluate project policies
        debug!("Phase 2: Evaluating project policies");

        // Start project evaluation phase
        if let Some(ref mut ctx) = telemetry {
            ctx.start_phase("project");
        }

        // Step 1: Route - find relevant policies (collect owned PolicyUnits)
        let matched_policies: Vec<PolicyUnit> = self
            .route_event(event_name, tool_name)
            .into_iter()
            .cloned()
            .collect();

        let policy_names: Vec<String> = matched_policies
            .iter()
            .map(|p| p.package_name.clone())
            .collect();

        // Record routing in telemetry
        if let Some(ref mut ctx) = telemetry {
            if let Some(phase) = ctx.current_phase_mut() {
                phase
                    .evaluation_mut()
                    .record_routing(!matched_policies.is_empty(), &policy_names);
            }
        }

        if matched_policies.is_empty() {
            info!("No policies matched for this event - allowing");
            // Record early exit in telemetry
            if let Some(ref mut ctx) = telemetry {
                if let Some(phase) = ctx.current_phase_mut() {
                    phase
                        .evaluation_mut()
                        .record_exit("No policies matched - implicit allow");
                    phase
                        .evaluation_mut()
                        .record_final_decision(&decision::FinalDecision::Allow { context: vec![] });
                    phase.finalize();
                }
            }
            current_span.record("matched_policy_count", 0);
            current_span.record("final_decision", "Allow");
            current_span.record("duration_ms", eval_start.elapsed().as_millis());
            return Ok(decision::FinalDecision::Allow { context: vec![] });
        }

        current_span.record("matched_policy_count", matched_policies.len());
        info!("Found {} matching policies", matched_policies.len());

        // Step 2: Gather signals using the Executor (created earlier for global evaluation)
        // Gather signals - collect telemetry if enabled
        let (enriched_input, signal_executions) = if telemetry.is_some() {
            let mut signal_telemetry = SignalTelemetry::new();
            let result = exec
                .gather_signals(&safe_input, &matched_policies, Some(&mut signal_telemetry))
                .await?;
            (result, signal_telemetry.signals)
        } else {
            let result = exec
                .gather_signals(&safe_input, &matched_policies, None)
                .await?;
            (result, Vec::new())
        };

        // Record signal executions in telemetry
        if let Some(ref mut ctx) = telemetry {
            if let Some(phase) = ctx.current_phase_mut() {
                for signal in signal_executions {
                    phase.record_signal(signal);
                }
            }
        }

        // Step 3: Evaluate using single aggregation entrypoint with enriched input
        debug!("About to evaluate decision set with enriched input");
        let decision_set = self.evaluate_decision_set(&enriched_input).await?;

        // Record WASM results in telemetry
        if let Some(ref mut ctx) = telemetry {
            if let Some(phase) = ctx.current_phase_mut() {
                phase.evaluation_mut().record_wasm_result(&decision_set);
            }
        }

        // Step 4: Apply Intelligence Layer synthesis
        let final_decision = synthesis::SynthesisEngine::synthesize(&decision_set)?;

        info!("Synthesized final decision: {:?}", final_decision);

        // Record final decision in telemetry
        if let Some(ref mut ctx) = telemetry {
            if let Some(phase) = ctx.current_phase_mut() {
                phase
                    .evaluation_mut()
                    .record_final_decision(&final_decision);
                phase.finalize();
            }
        }

        // Record final decision type and duration
        let duration = eval_start.elapsed();
        let current_span = tracing::Span::current();
        current_span.record("final_decision", format!("{final_decision:?}").as_str());
        current_span.record("duration_ms", duration.as_millis());

        trace!(
            decision = ?final_decision,
            duration_ms = duration.as_millis(),
            "Evaluation complete"
        );

        Ok(final_decision)
    }
}
