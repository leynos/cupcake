//! Global policy evaluation for the engine.

use super::*;

impl Engine {
    /// Evaluate global policies
    ///
    /// Returns (decision, decision_set, signal_executions) where signal_executions
    /// is populated only when capture_telemetry is true.
    pub(super) async fn evaluate_global(
        &self,
        input: &Value,
        event_name: &str,
        tool_name: Option<&str>,
        exec: &executor::Executor<'_>,
        capture_telemetry: bool,
    ) -> Result<(
        decision::FinalDecision,
        decision::DecisionSet,
        Vec<crate::telemetry::span::SignalExecution>,
    )> {
        // Route through global policies
        let global_matched: Vec<PolicyUnit> = self
            .route_global_event(event_name, tool_name)
            .into_iter()
            .cloned()
            .collect();

        if global_matched.is_empty() {
            debug!("No global policies matched for this event");
            return Ok((
                decision::FinalDecision::Allow { context: vec![] },
                decision::DecisionSet::default(),
                Vec::new(),
            ));
        }

        info!("Found {} matching global policies", global_matched.len());

        // Gather signals for global policies using Executor - collect telemetry if enabled
        let (enriched_input, signal_executions) = if capture_telemetry {
            let mut signal_telemetry = SignalTelemetry::new();
            let result = exec
                .gather_global_signals(input, &global_matched, Some(&mut signal_telemetry))
                .await?;
            (result, signal_telemetry.signals)
        } else {
            let result = exec
                .gather_global_signals(input, &global_matched, None)
                .await?;
            (result, Vec::new())
        };

        // Evaluate using global WASM runtime
        let global_runtime = self
            .global_wasm_runtime
            .as_ref()
            .context("Global WASM runtime not initialized")?;

        let global_decision_set = global_runtime.query_decision_set(&enriched_input)?;
        debug!(
            "Global DecisionSet: {} total decisions",
            global_decision_set.decision_count()
        );

        // Synthesize global decision
        let global_decision = synthesis::SynthesisEngine::synthesize(&global_decision_set)?;
        info!("Global policy decision: {:?}", global_decision);

        Ok((global_decision, global_decision_set, signal_executions))
    }

    /// Route event through global policies
    pub(super) fn route_global_event(
        &self,
        event_name: &str,
        tool_name: Option<&str>,
    ) -> Vec<&PolicyUnit> {
        let key = routing::create_event_key(event_name, tool_name);

        // First try the specific key
        let mut result: Vec<&PolicyUnit> = self
            .global_routing_map
            .get(&key)
            .map(|policies| policies.iter().collect())
            .unwrap_or_default();

        // If we have a tool name, also check for wildcards
        if tool_name.is_some() {
            let wildcard_key = event_name.to_string();
            if let Some(wildcard_policies) = self.global_routing_map.get(&wildcard_key) {
                result.extend(wildcard_policies.iter());
            }
        }

        debug!("Routed to {} global policies", result.len());
        result
    }

    /// Evaluate using the Hybrid Model single aggregation entrypoint
    pub(super) async fn evaluate_decision_set(
        &self,
        input: &Value,
    ) -> Result<decision::DecisionSet> {
        let runtime = self
            .wasm_runtime
            .as_ref()
            .context("WASM runtime not initialized")?;

        debug!("Evaluating using single cupcake.system.evaluate entrypoint");

        // Query the single aggregation entrypoint
        let decision_set = runtime.query_decision_set(input)?;

        debug!(
            "Raw DecisionSet from WASM: {} total decisions",
            decision_set.decision_count()
        );
        debug!(
            "DecisionSet summary: {}",
            synthesis::SynthesisEngine::summarize_decision_set(&decision_set)
        );

        Ok(decision_set)
    }
}
