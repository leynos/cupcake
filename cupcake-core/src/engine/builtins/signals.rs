//! Signal and JSON generation for enabled builtins.

use super::*;

impl BuiltinsConfig {
    /// Generate signals required by enabled builtins
    pub fn generate_signals(&self) -> HashMap<String, SignalConfig> {
        let mut signals = HashMap::new();

        // Generate signals for claude_code_always_inject_on_prompt (only for dynamic sources)
        if let Some(config) = &self.claude_code_always_inject_on_prompt {
            if config.enabled {
                for (idx, source) in config.context.iter().enumerate() {
                    // Only generate signals for dynamic sources (commands and files)
                    // Static strings are now injected directly via builtin_config
                    if matches!(source, ContextSource::Dynamic { .. }) {
                        let signal_name = format!("__builtin_prompt_context_{idx}");

                        if let Some(signal) = context_source_to_signal(source) {
                            signals.insert(signal_name, signal);
                        }
                    }
                }
            }
        }

        // Generate signals for git_pre_check
        if let Some(config) = &self.git_pre_check {
            if config.enabled {
                for (idx, check) in config.checks.iter().enumerate() {
                    let signal_name = format!("__builtin_git_check_{idx}");
                    signals.insert(
                        signal_name,
                        SignalConfig {
                            command: check.command.clone(),
                            timeout_seconds: 30, // Reasonable timeout for tests/linting
                        },
                    );
                }
            }
        }

        // Generate signals for post_edit_check
        if let Some(config) = &self.post_edit_check {
            if config.enabled {
                for (ext, check) in &config.by_extension {
                    let signal_name = format!("__builtin_post_edit_{ext}");
                    signals.insert(
                        signal_name,
                        SignalConfig {
                            command: check.command.clone(),
                            timeout_seconds: 10, // Quick feedback for edit checks
                        },
                    );
                }
            }
        }

        // rulebook_security_guardrails: No signals needed - static config injected directly

        // protected_paths: No signals needed - static config injected directly

        // Generate signals for git_block_no_verify (if needed)
        if let Some(config) = &self.git_block_no_verify {
            if config.enabled {
                // No signals needed for this simple builtin - it just blocks patterns
                // But we could add a signal for the message if desired
            }
        }

        // system_protection: No signals needed - static config injected directly

        // sensitive_data_protection: No signals needed - static config injected directly

        // cupcake_exec_protection: No signals needed - static config injected directly

        // enforce_full_file_read: No signals needed - static config injected directly

        if !signals.is_empty() {
            info!("Generated {} signals for enabled builtins", signals.len());
            for name in signals.keys() {
                debug!("  Generated signal: {}", name);
            }
        }

        signals
    }

    /// Convert enabled builtin configurations to JSON format for injection into policy input
    /// This method encapsulates the logic for transforming Rust config structs into the JSON
    /// format expected by Rego policies at input.builtin_config.<builtin_name>
    pub fn to_json_configs(&self) -> serde_json::Map<String, serde_json::Value> {
        use serde_json::json;
        let mut configs = serde_json::Map::new();

        // Add protected_paths config if enabled
        if let Some(config) = &self.protected_paths {
            if config.enabled {
                configs.insert(
                    "protected_paths".to_string(),
                    json!({
                        "message": config.message,
                        "paths": config.paths,
                    }),
                );
            }
        }

        // Add rulebook_security_guardrails config if enabled
        if let Some(config) = &self.rulebook_security_guardrails {
            if config.enabled {
                configs.insert(
                    "rulebook_security_guardrails".to_string(),
                    json!({
                        "message": config.message,
                        "protected_paths": config.protected_paths,
                    }),
                );
            }
        }

        // Add claude_code_enforce_full_file_read config if enabled
        if let Some(config) = &self.claude_code_enforce_full_file_read {
            if config.enabled {
                configs.insert(
                    "claude_code_enforce_full_file_read".to_string(),
                    json!({
                        "message": config.message,
                        "max_lines": config.max_lines,
                    }),
                );
            }
        }

        // Add git_block_no_verify config if enabled
        if let Some(config) = &self.git_block_no_verify {
            if config.enabled {
                configs.insert(
                    "git_block_no_verify".to_string(),
                    json!({
                        "message": config.message,
                        "exceptions": config.exceptions,
                    }),
                );
            }
        }

        // Add system_protection config if enabled
        if let Some(config) = &self.system_protection {
            if config.enabled {
                configs.insert(
                    "system_protection".to_string(),
                    json!({
                        "message": config.message,
                        "additional_paths": config.additional_paths,
                    }),
                );
            }
        }

        // Add sensitive_data_protection config if enabled
        if let Some(config) = &self.sensitive_data_protection {
            if config.enabled {
                configs.insert(
                    "sensitive_data_protection".to_string(),
                    json!({
                        "message": config.message,
                        "additional_patterns": config.additional_patterns,
                    }),
                );
            }
        }

        // Add cupcake_exec_protection config if enabled
        if let Some(config) = &self.cupcake_exec_protection {
            if config.enabled {
                configs.insert(
                    "cupcake_exec_protection".to_string(),
                    json!({
                        "message": config.message,
                        "allowed_commands": config.allowed_commands,
                    }),
                );
            }
        }

        // Add claude_code_always_inject_on_prompt static strings if enabled
        if let Some(config) = &self.claude_code_always_inject_on_prompt {
            if config.enabled {
                let mut static_contexts = Vec::new();
                for source in &config.context {
                    if let ContextSource::String(s) = source {
                        static_contexts.push(s.clone());
                    }
                }
                if !static_contexts.is_empty() {
                    configs.insert(
                        "claude_code_always_inject_on_prompt".to_string(),
                        json!({
                            "static_contexts": static_contexts,
                        }),
                    );
                }
            }
        }

        configs
    }

    /// Get the specific signal name for post_edit_check based on the file extension in the input
    /// Returns None if:
    /// - post_edit_check is not enabled
    /// - input doesn't contain file_path in params
    /// - file has no extension
    /// - extension is not valid UTF-8
    pub fn get_post_edit_signal(&self, input: &serde_json::Value) -> Option<String> {
        // Check if post_edit_check is enabled
        let config = self.post_edit_check.as_ref()?;
        if !config.enabled {
            return None;
        }

        // Extract file extension from input.params.file_path
        let params = input.get("params")?;
        let file_path = params.get("file_path")?;
        let path_str = file_path.as_str()?;

        // Get the file extension
        let extension = Path::new(path_str).extension().and_then(|e| e.to_str())?;

        // Return the signal name for this extension
        Some(format!("__builtin_post_edit_{extension}"))
    }
}

/// Convert a ContextSource to a SignalConfig (only for dynamic sources)
fn context_source_to_signal(source: &ContextSource) -> Option<SignalConfig> {
    match source {
        ContextSource::String(_) => {
            // Static strings are no longer converted to signals
            // They're injected directly via builtin_config
            None
        }
        ContextSource::Dynamic { file, command } => {
            if let Some(cmd) = command {
                Some(SignalConfig {
                    command: cmd.clone(),
                    timeout_seconds: 5,
                })
            } else {
                file.as_ref().map(|path| SignalConfig {
                    command: format!("cat '{}'", path.replace('\'', "\\'")),
                    timeout_seconds: 2,
                })
            }
        }
    }
}
