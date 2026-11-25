//! Signal and JSON generation for enabled builtins.

use super::*;

impl BuiltinsConfig {
    /// Generate signals required by enabled builtins
    pub fn generate_signals(&self) -> HashMap<String, SignalConfig> {
        let mut signals = HashMap::new();

        self.insert_prompt_context_signals(&mut signals);
        self.insert_git_check_signals(&mut signals);
        self.insert_post_edit_signals(&mut signals);

        // rulebook_security_guardrails: No signals needed - static config injected directly

        // protected_paths: No signals needed - static config injected directly

        // git_block_no_verify: No signals needed for this simple builtin - it just blocks
        // patterns. We could add a signal for the message if desired.

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

    /// Signals for claude_code_always_inject_on_prompt (only for dynamic sources)
    fn insert_prompt_context_signals(&self, signals: &mut HashMap<String, SignalConfig>) {
        let Some(config) = self
            .claude_code_always_inject_on_prompt
            .as_ref()
            .filter(|c| c.enabled)
        else {
            return;
        };
        for (idx, source) in config.context.iter().enumerate() {
            // Only generate signals for dynamic sources (commands and files)
            // Static strings are now injected directly via builtin_config
            if !matches!(source, ContextSource::Dynamic { .. }) {
                continue;
            }
            if let Some(signal) = context_source_to_signal(source) {
                signals.insert(format!("__builtin_prompt_context_{idx}"), signal);
            }
        }
    }

    /// Signals for git_pre_check
    fn insert_git_check_signals(&self, signals: &mut HashMap<String, SignalConfig>) {
        let Some(config) = self.git_pre_check.as_ref().filter(|c| c.enabled) else {
            return;
        };
        for (idx, check) in config.checks.iter().enumerate() {
            signals.insert(
                format!("__builtin_git_check_{idx}"),
                SignalConfig {
                    command: check.command.clone(),
                    timeout_seconds: 30, // Reasonable timeout for tests/linting
                },
            );
        }
    }

    /// Signals for post_edit_check
    fn insert_post_edit_signals(&self, signals: &mut HashMap<String, SignalConfig>) {
        let Some(config) = self.post_edit_check.as_ref().filter(|c| c.enabled) else {
            return;
        };
        for (ext, check) in &config.by_extension {
            signals.insert(
                format!("__builtin_post_edit_{ext}"),
                SignalConfig {
                    command: check.command.clone(),
                    timeout_seconds: 10, // Quick feedback for edit checks
                },
            );
        }
    }

    /// Convert enabled builtin configurations to JSON format for injection into policy input
    /// This method encapsulates the logic for transforming Rust config structs into the JSON
    /// format expected by Rego policies at input.builtin_config.<builtin_name>
    pub fn to_json_configs(&self) -> serde_json::Map<String, serde_json::Value> {
        let entries = [
            ("protected_paths", self.protected_paths_json()),
            (
                "rulebook_security_guardrails",
                self.rulebook_security_guardrails_json(),
            ),
            (
                "claude_code_enforce_full_file_read",
                self.enforce_full_file_read_json(),
            ),
            ("git_block_no_verify", self.git_block_no_verify_json()),
            ("system_protection", self.system_protection_json()),
            (
                "sensitive_data_protection",
                self.sensitive_data_protection_json(),
            ),
            (
                "cupcake_exec_protection",
                self.cupcake_exec_protection_json(),
            ),
            (
                "claude_code_always_inject_on_prompt",
                self.always_inject_on_prompt_json(),
            ),
        ];

        let mut configs = serde_json::Map::new();
        for (name, config) in entries {
            if let Some(config) = config {
                configs.insert(name.to_string(), config);
            }
        }
        configs
    }

    /// `protected_paths` as injected config, when enabled.
    fn protected_paths_json(&self) -> Option<serde_json::Value> {
        let config = self.protected_paths.as_ref().filter(|c| c.enabled)?;
        Some(serde_json::json!({
            "message": config.message,
            "paths": config.paths,
        }))
    }

    /// `rulebook_security_guardrails` as injected config, when enabled.
    fn rulebook_security_guardrails_json(&self) -> Option<serde_json::Value> {
        let config = self
            .rulebook_security_guardrails
            .as_ref()
            .filter(|c| c.enabled)?;
        Some(serde_json::json!({
            "message": config.message,
            "protected_paths": config.protected_paths,
        }))
    }

    /// `claude_code_enforce_full_file_read` as injected config, when enabled.
    fn enforce_full_file_read_json(&self) -> Option<serde_json::Value> {
        let config = self
            .claude_code_enforce_full_file_read
            .as_ref()
            .filter(|c| c.enabled)?;
        Some(serde_json::json!({
            "message": config.message,
            "max_lines": config.max_lines,
        }))
    }

    /// `git_block_no_verify` as injected config, when enabled.
    fn git_block_no_verify_json(&self) -> Option<serde_json::Value> {
        let config = self.git_block_no_verify.as_ref().filter(|c| c.enabled)?;
        Some(serde_json::json!({
            "message": config.message,
            "exceptions": config.exceptions,
        }))
    }

    /// `system_protection` as injected config, when enabled.
    fn system_protection_json(&self) -> Option<serde_json::Value> {
        let config = self.system_protection.as_ref().filter(|c| c.enabled)?;
        Some(serde_json::json!({
            "message": config.message,
            "additional_paths": config.additional_paths,
        }))
    }

    /// `sensitive_data_protection` as injected config, when enabled.
    fn sensitive_data_protection_json(&self) -> Option<serde_json::Value> {
        let config = self
            .sensitive_data_protection
            .as_ref()
            .filter(|c| c.enabled)?;
        Some(serde_json::json!({
            "message": config.message,
            "additional_patterns": config.additional_patterns,
        }))
    }

    /// `cupcake_exec_protection` as injected config, when enabled.
    fn cupcake_exec_protection_json(&self) -> Option<serde_json::Value> {
        let config = self
            .cupcake_exec_protection
            .as_ref()
            .filter(|c| c.enabled)?;
        Some(serde_json::json!({
            "message": config.message,
            "allowed_commands": config.allowed_commands,
        }))
    }

    /// `claude_code_always_inject_on_prompt` static strings as injected
    /// config, when enabled and at least one static string is configured.
    fn always_inject_on_prompt_json(&self) -> Option<serde_json::Value> {
        let config = self
            .claude_code_always_inject_on_prompt
            .as_ref()
            .filter(|c| c.enabled)?;
        let static_contexts: Vec<String> = config
            .context
            .iter()
            .filter_map(|source| match source {
                ContextSource::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        if static_contexts.is_empty() {
            return None;
        }
        Some(serde_json::json!({
            "static_contexts": static_contexts,
        }))
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
