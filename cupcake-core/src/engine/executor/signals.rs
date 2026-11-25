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

        let required_signals = collect_required_global_signals(matched_policies, rulebook);
        let mut enriched_input = inject_builtin_config(input, rulebook);

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

const GLOBAL_BUILTIN_PREFIX: &str = "cupcake.global.policies.builtins.";
const PROJECT_BUILTIN_PREFIX: &str = "cupcake.policies.builtins.";

/// Collect the signals the matched policies declare, plus those their builtins imply.
fn collect_required_global_signals(
    matched_policies: &[PolicyUnit],
    rulebook: &Rulebook,
) -> std::collections::HashSet<String> {
    let mut required_signals = std::collections::HashSet::new();
    for policy in matched_policies {
        required_signals.extend(policy.routing.required_signals.iter().cloned());
    }
    for policy in matched_policies {
        if let Some(builtin_name) = builtin_name_of(&policy.package_name) {
            add_builtin_signals(&mut required_signals, rulebook, builtin_name);
        }
    }
    required_signals
}

/// Return the builtin name when the package belongs to a global or project builtin.
fn builtin_name_of(package_name: &str) -> Option<&str> {
    package_name
        .strip_prefix(GLOBAL_BUILTIN_PREFIX)
        .or_else(|| package_name.strip_prefix(PROJECT_BUILTIN_PREFIX))
}

/// Add every rulebook signal whose name starts with the builtin's `__builtin_<name>` prefix.
fn add_builtin_signals(
    required_signals: &mut std::collections::HashSet<String>,
    rulebook: &Rulebook,
    builtin_name: &str,
) {
    let signal_prefix = format!("__builtin_{builtin_name}");
    for signal_name in rulebook.signals.keys() {
        if signal_name.starts_with(&signal_prefix) {
            debug!(
                "Auto-adding signal '{}' for global builtin '{}'",
                signal_name, builtin_name
            );
            required_signals.insert(signal_name.clone());
        }
    }
}

/// Clone the input and attach the global rulebook's builtin configs when it is an object.
fn inject_builtin_config(input: &Value, rulebook: &Rulebook) -> Value {
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
    enriched_input
}

#[cfg(test)]
mod tests {
    //! Tests for the global-signal collection helpers.

    use super::*;

    fn signal(command: &str) -> crate::engine::rulebook::SignalConfig {
        crate::engine::rulebook::SignalConfig {
            command: command.to_string(),
            timeout_seconds: 5,
        }
    }

    #[test]
    fn builtin_name_of_strips_either_builtin_prefix() {
        assert_eq!(
            builtin_name_of("cupcake.global.policies.builtins.git_guard"),
            Some("git_guard")
        );
        assert_eq!(
            builtin_name_of("cupcake.policies.builtins.git_guard"),
            Some("git_guard")
        );
        assert_eq!(builtin_name_of("cupcake.policies.custom"), None);
    }

    #[test]
    fn add_builtin_signals_matches_with_and_without_trailing_underscore() {
        let mut rulebook = Rulebook::default();
        for name in [
            "__builtin_git_guard_status",
            "__builtin_git_guard",
            "__builtin_other_status",
        ] {
            rulebook.signals.insert(name.to_string(), signal("true"));
        }

        let mut required = std::collections::HashSet::new();
        add_builtin_signals(&mut required, &rulebook, "git_guard");

        let mut found: Vec<_> = required.into_iter().collect();
        found.sort();
        assert_eq!(found, ["__builtin_git_guard", "__builtin_git_guard_status"]);
    }

    fn policy(package_name: &str, required_signals: &[&str]) -> PolicyUnit {
        PolicyUnit {
            path: std::path::PathBuf::from("policy.rego"),
            package_name: package_name.to_string(),
            routing: crate::engine::metadata::RoutingDirective {
                required_events: Vec::new(),
                required_tools: Vec::new(),
                required_signals: required_signals.iter().map(|s| s.to_string()).collect(),
            },
            metadata: None,
        }
    }

    #[test]
    fn collect_required_global_signals_returns_the_deduplicated_union() {
        let mut rulebook = Rulebook::default();
        for name in [
            "declared",
            "__builtin_git_guard_status",
            "__builtin_project_rule",
            "__builtin_unused_status",
        ] {
            rulebook.signals.insert(name.to_string(), signal("true"));
        }
        let policies = [
            // Declared signals, one of which the builtin scan also finds.
            policy(
                "cupcake.policies.custom",
                &["declared", "__builtin_git_guard_status"],
            ),
            policy("cupcake.global.policies.builtins.git_guard", &[]),
            policy("cupcake.policies.builtins.project_rule", &[]),
        ];

        let required = collect_required_global_signals(&policies, &rulebook);

        let mut found: Vec<_> = required.into_iter().collect();
        found.sort();
        assert_eq!(
            found,
            [
                "__builtin_git_guard_status",
                "__builtin_project_rule",
                "declared"
            ]
        );
    }

    #[test]
    fn inject_builtin_config_leaves_non_objects_and_empty_configs_unchanged() {
        let rulebook = Rulebook::default();
        let object = serde_json::json!({"tool": "Bash"});
        assert_eq!(inject_builtin_config(&object, &rulebook), object);

        let scalar = serde_json::json!("text");
        assert_eq!(inject_builtin_config(&scalar, &rulebook), scalar);
    }
}
