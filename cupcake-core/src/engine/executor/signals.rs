//! Signal execution and global signal gathering for the executor.

use super::*;

impl<'a> Executor<'a> {
    /// Execute signals.
    pub(super) async fn execute_signals(
        &self,
        signal_names: &[String],
        rulebook: &Rulebook,
        event_data: &Value,
        mut signal_telemetry: Option<&mut SignalTelemetry>,
    ) -> Result<HashMap<String, Value>> {
        if signal_names.is_empty() {
            return Ok(HashMap::new());
        }

        debug!("Executing {} signals", signal_names.len());

        let futures: Vec<_> = signal_names
            .iter()
            .map(|name| {
                let name = name.clone();
                let signal_config = rulebook.get_signal(&name).cloned();
                let event_data = event_data.clone();

                async move {
                    let signal = match signal_config {
                        Some(s) => s,
                        None => {
                            return (
                                name.clone(),
                                Err(anyhow::anyhow!("Signal '{}' not found", name)),
                                None,
                            );
                        }
                    };

                    // Execute the signal and measure time
                    let signal_start = Instant::now();
                    let result = rulebook.execute_signal_with_input(&name, &event_data).await;
                    let signal_duration = signal_start.elapsed();

                    let signal_execution = SignalExecution {
                        name: name.clone(),
                        command: signal.command.clone(),
                        result: result.as_ref().unwrap_or(&Value::Null).clone(),
                        duration_ms: Some(signal_duration.as_millis() as u64),
                        exit_code: None,
                    };

                    (name, result, Some(signal_execution))
                }
            })
            .collect();

        let results = join_all(futures).await;

        let mut signal_data = HashMap::new();
        for (name, result, signal_execution) in results {
            match result {
                Ok(value) => {
                    debug!("Signal '{}' executed successfully", name);
                    signal_data.insert(name, value);

                    if let (Some(ref mut telemetry), Some(execution)) =
                        (&mut signal_telemetry, signal_execution)
                    {
                        telemetry.signals.push(execution);
                    }
                }
                Err(e) => {
                    error!("Signal '{}' failed: {}", name, e);

                    if let (Some(ref mut telemetry), Some(execution)) =
                        (&mut signal_telemetry, signal_execution)
                    {
                        telemetry.signals.push(execution);
                    }
                }
            }
        }

        Ok(signal_data)
    }

    /// Gather signals for global policy evaluation.
    ///
    /// This uses the global rulebook for signal definitions and builtin configs.
    pub async fn gather_global_signals(
        &self,
        input: &Value,
        matched_policies: &[PolicyUnit],
        signal_telemetry: Option<&mut SignalTelemetry>,
    ) -> Result<Value> {
        let Some(rulebook) = self.global_rulebook else {
            debug!("No global rulebook - returning input unchanged");
            return Ok(input.clone());
        };

        // Collect required signals from matched policies
        let mut required_signals = std::collections::HashSet::new();
        for policy in matched_policies {
            for signal_name in &policy.routing.required_signals {
                required_signals.insert(signal_name.clone());
            }
        }

        // Auto-add signals for global builtin policies
        for policy in matched_policies {
            let is_global_builtin = policy
                .package_name
                .starts_with("cupcake.global.policies.builtins.");
            let is_project_builtin = policy
                .package_name
                .starts_with("cupcake.policies.builtins.");

            if is_global_builtin || is_project_builtin {
                let prefix = if is_global_builtin {
                    "cupcake.global.policies.builtins."
                } else {
                    "cupcake.policies.builtins."
                };

                let builtin_name = policy.package_name.strip_prefix(prefix).unwrap_or("");

                // Add all signals that match this builtin's pattern
                let signal_prefix = format!("__builtin_{builtin_name}_");
                for signal_name in rulebook.signals.keys() {
                    if signal_name.starts_with(&signal_prefix) {
                        debug!(
                            "Auto-adding signal '{}' for global builtin '{}'",
                            signal_name, builtin_name
                        );
                        required_signals.insert(signal_name.clone());
                    }
                }

                // Also check for signals without the trailing underscore
                let signal_prefix_no_underscore = format!("__builtin_{builtin_name}");
                for signal_name in rulebook.signals.keys() {
                    if signal_name.starts_with(&signal_prefix_no_underscore)
                        && !signal_name.starts_with(&signal_prefix)
                    {
                        debug!(
                            "Auto-adding signal '{}' for global builtin '{}'",
                            signal_name, builtin_name
                        );
                        required_signals.insert(signal_name.clone());
                    }
                }
            }
        }

        // Always inject builtin config from global rulebook
        let mut enriched_input = input.clone();
        if let Some(input_obj) = enriched_input.as_object_mut() {
            debug!("Injecting builtin configs from global rulebook");
            let builtin_config = rulebook.builtins.to_json_configs();

            if !builtin_config.is_empty() {
                debug!("Injected {} builtin configurations", builtin_config.len());
                input_obj.insert(
                    "builtin_config".to_string(),
                    serde_json::Value::Object(builtin_config),
                );
            }
        }

        if required_signals.is_empty() {
            debug!("No global signals required - returning with builtin config");
            return Ok(enriched_input);
        }

        let signal_names: Vec<String> = required_signals.into_iter().collect();
        info!("Gathering {} global signals", signal_names.len());

        // Execute signals using global rulebook
        let signal_data = match self
            .execute_signals(&signal_names, rulebook, input, signal_telemetry)
            .await
        {
            Ok(signal_data) => signal_data,
            Err(e) => {
                warn!("Global signal execution failed: {}", e);
                HashMap::new()
            }
        };

        // Merge signal data into enriched input
        if let Some(obj) = enriched_input.as_object_mut() {
            obj.insert("signals".to_string(), serde_json::json!(signal_data));
        }

        Ok(enriched_input)
    }
}
