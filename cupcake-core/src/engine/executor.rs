//! Executor - Handles OS/IO operations for the engine.
//!
//! This module separates "Doing" (shell commands, process execution, filesystem I/O)
//! from "Thinking" (routing, WASM evaluation, synthesis) in the engine.
//!
//! The Executor is an ephemeral struct created for each evaluation, holding references
//! to the resources needed for signal gathering.

use anyhow::Result;
use futures::future::join_all;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;
use tracing::{debug, error, info, trace, warn};

use super::metadata::PolicyUnit;
use super::rulebook::Rulebook;
use crate::debug::SignalTelemetry;
use crate::telemetry::span::SignalExecution;
use crate::watchdog::Watchdog;

/// Executor handles all OS/IO interactions for policy evaluation.
///
/// This is an ephemeral struct created for each evaluation. It holds references
/// to the resources needed for executing signals, avoiding repeated
/// parameter passing through function signatures.
pub struct Executor<'a> {
    /// Rulebook containing signal definitions
    pub rulebook: Option<&'a Rulebook>,
    /// Global rulebook for global signals
    pub global_rulebook: Option<&'a Rulebook>,
    /// Watchdog for LLM-as-Judge evaluation
    pub watchdog: Option<&'a Watchdog>,
    /// Working directory for command execution
    pub working_dir: &'a Path,
}

impl<'a> Executor<'a> {
    /// Gather signal values by executing shell commands.
    ///
    /// This function:
    /// 1. Collects required signals from matched policies
    /// 2. Auto-adds signals for builtin policies
    /// 3. Injects builtin configuration
    /// 4. Executes signals
    /// 5. Runs watchdog evaluation if enabled
    #[tracing::instrument(
        skip(self, input, matched_policies, signal_telemetry),
        fields(
            signal_count = tracing::field::Empty,
            signals_executed = tracing::field::Empty,
            duration_ms = tracing::field::Empty
        )
    )]
    pub async fn gather_signals(
        &self,
        input: &Value,
        matched_policies: &[PolicyUnit],
        mut signal_telemetry: Option<&mut SignalTelemetry>,
    ) -> Result<Value> {
        let start = Instant::now();

        let required_signals = self.collect_required_signals(input, matched_policies);

        // Always inject builtin config, even when no signals are required
        let mut enriched_input = input.clone();
        self.inject_builtin_config(&mut enriched_input);

        // Check if watchdog should run
        let is_pre_action_event = is_pre_action_event(input);
        let watchdog_should_run = self.watchdog.is_some() && is_pre_action_event;

        if required_signals.is_empty() && !watchdog_should_run {
            debug!("No signals required and watchdog not enabled - returning with builtin config");
            return Ok(enriched_input);
        }

        let signal_names: Vec<String> = required_signals.into_iter().collect();
        info!(
            "Gathering {} signals: {:?}",
            signal_names.len(),
            signal_names
        );

        // Execute signals if we have a rulebook
        let signal_data = if let Some(rulebook) = self.rulebook {
            match self
                .execute_signals(
                    &signal_names,
                    rulebook,
                    input,
                    signal_telemetry.as_deref_mut(),
                )
                .await
            {
                Ok(signal_data) => signal_data,
                Err(e) => {
                    warn!("Signal execution failed: {}", e);
                    HashMap::new()
                }
            }
        } else {
            debug!("No rulebook available - no signals collected");
            HashMap::new()
        };

        // Merge signal data into enriched input
        let mut signal_count = signal_data.len();
        if let Some(input_obj) = enriched_input.as_object_mut() {
            let mut signals_obj = serde_json::to_value(signal_data)?;

            // Execute Watchdog if enabled
            if let Some(watchdog) = self.watchdog.filter(|_| is_pre_action_event) {
                let watchdog_value =
                    run_watchdog(watchdog, input, signal_telemetry.as_deref_mut()).await?;
                if let Some(signals_map) = signals_obj.as_object_mut() {
                    signals_map.insert("watchdog".to_string(), watchdog_value);
                    signal_count += 1;
                }
            }

            input_obj.insert("signals".to_string(), signals_obj);
        }

        // Record span fields
        let duration = start.elapsed();
        let current_span = tracing::Span::current();
        current_span.record("signal_count", signal_count);
        current_span.record("signals_executed", signal_names.join(",").as_str());
        current_span.record("duration_ms", duration.as_millis());

        debug!("Input enriched with {} signal values", signal_count);
        trace!(
            signal_count = signal_count,
            duration_ms = duration.as_millis(),
            "Signal gathering complete"
        );

        Ok(enriched_input)
    }

    /// Collect the signals the matched policies require, including the
    /// signals that builtin policies add automatically.
    fn collect_required_signals(
        &self,
        input: &Value,
        matched_policies: &[PolicyUnit],
    ) -> std::collections::HashSet<String> {
        let mut required_signals = std::collections::HashSet::new();
        for policy in matched_policies {
            for signal_name in &policy.routing.required_signals {
                required_signals.insert(signal_name.clone());
            }
        }

        if let Some(rulebook) = self.rulebook {
            for policy in matched_policies {
                add_builtin_signals(rulebook, policy, input, &mut required_signals);
            }
        }
        required_signals
    }

    /// Inject the builtin configuration into `enriched_input`: project
    /// configs first as the baseline, then global configs, which override.
    fn inject_builtin_config(&self, enriched_input: &mut Value) {
        let Some(input_obj) = enriched_input.as_object_mut() else {
            return;
        };
        let mut builtin_config = serde_json::Map::new();

        if let Some(project_rulebook) = self.rulebook {
            debug!("Injecting builtin configs from project rulebook");
            builtin_config.extend(project_rulebook.builtins.to_json_configs());
        }

        if let Some(global_rulebook) = self.global_rulebook {
            debug!("Injecting builtin configs from global rulebook (overrides project)");
            builtin_config.extend(global_rulebook.builtins.to_json_configs());
        }

        if !builtin_config.is_empty() {
            debug!(
                "Injected {} builtin configurations total",
                builtin_config.len()
            );
            input_obj.insert(
                "builtin_config".to_string(),
                serde_json::Value::Object(builtin_config),
            );
        }
    }
}

/// Whether `input` is an event that runs before the action it describes.
fn is_pre_action_event(input: &Value) -> bool {
    input
        .get("hook_event_name")
        .and_then(|v| v.as_str())
        .map(|s| {
            matches!(
                s,
                "PreToolUse" | "beforeShellExecution" | "beforeMCPExecution"
            )
        })
        .unwrap_or(false)
}

/// Add the signals a builtin policy needs to `required_signals`.
///
/// Policies outside the `cupcake.policies.builtins.` package add nothing.
fn add_builtin_signals(
    rulebook: &Rulebook,
    policy: &PolicyUnit,
    input: &Value,
    required_signals: &mut std::collections::HashSet<String>,
) {
    let Some(builtin_name) = policy
        .package_name
        .strip_prefix("cupcake.policies.builtins.")
    else {
        return;
    };

    // Special handling for post_edit_check
    if builtin_name == "post_edit_check" {
        add_post_edit_signal(rulebook, input, required_signals);
    } else {
        add_prefixed_signals(rulebook, builtin_name, required_signals);
    }
}

/// Add the extension-specific signal of the `post_edit_check` builtin, when
/// the edited file has one defined.
fn add_post_edit_signal(
    rulebook: &Rulebook,
    input: &Value,
    required_signals: &mut std::collections::HashSet<String>,
) {
    let Some(signal_name) = rulebook.builtins.get_post_edit_signal(input) else {
        return;
    };
    if rulebook.signals.contains_key(&signal_name) {
        debug!(
            "Auto-adding signal '{}' for post_edit_check builtin",
            signal_name
        );
        required_signals.insert(signal_name);
    }
}

/// Add every signal named `__builtin_<builtin_name>_*` (all matching signals
/// for the other builtins).
fn add_prefixed_signals(
    rulebook: &Rulebook,
    builtin_name: &str,
    required_signals: &mut std::collections::HashSet<String>,
) {
    let signal_prefix = format!("__builtin_{builtin_name}_");
    for signal_name in rulebook.signals.keys() {
        if signal_name.starts_with(&signal_prefix) {
            debug!(
                "Auto-adding signal '{}' for builtin '{}'",
                signal_name, builtin_name
            );
            required_signals.insert(signal_name.clone());
        }
    }
}

/// Run the watchdog on a pre-action event, record it in the signal telemetry
/// and return its output as JSON for the `watchdog` signal.
async fn run_watchdog(
    watchdog: &Watchdog,
    input: &Value,
    signal_telemetry: Option<&mut SignalTelemetry>,
) -> Result<Value> {
    debug!(
        "Executing Watchdog evaluation for {:?} event",
        input.get("hook_event_name")
    );
    let watchdog_start = Instant::now();
    let watchdog_input = Watchdog::input_from_event(input);
    let watchdog_output = watchdog.evaluate(watchdog_input).await;
    let watchdog_duration = watchdog_start.elapsed();
    debug!(
        "Watchdog result: allow={}, confidence={}",
        watchdog_output.allow, watchdog_output.confidence
    );

    // Capture watchdog in telemetry
    if let Some(telemetry) = signal_telemetry {
        telemetry.signals.push(SignalExecution {
            name: "watchdog".to_string(),
            command: format!("LLM evaluation via {}", watchdog.backend_name()),
            result: serde_json::to_value(&watchdog_output).unwrap_or_default(),
            duration_ms: Some(watchdog_duration.as_millis() as u64),
            exit_code: None,
        });
    }

    Ok(serde_json::to_value(&watchdog_output)?)
}

mod signals;
