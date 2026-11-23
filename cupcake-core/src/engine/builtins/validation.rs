//! Validation and enablement queries for builtin configuration.

use super::*;

impl BuiltinsConfig {
    /// Validate configuration and return errors if invalid
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        self.validate_prompt_context(&mut errors);
        self.validate_git_pre_check(&mut errors);
        self.validate_post_edit_check(&mut errors);
        self.validate_rulebook_security_guardrails(&mut errors);
        self.validate_protected_paths(&mut errors);

        // Validate git_block_no_verify (no specific validation needed - it's simple)

        self.validate_enforce_full_file_read(&mut errors);

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Validate claude_code_always_inject_on_prompt
    fn validate_prompt_context(&self, errors: &mut Vec<String>) {
        let Some(config) = &self.claude_code_always_inject_on_prompt else {
            return;
        };
        if config.enabled && config.context.is_empty() {
            errors.push(
                "claude_code_always_inject_on_prompt: enabled but no context configured"
                    .to_string(),
            );
        }

        for (idx, source) in config.context.iter().enumerate() {
            if is_dynamic_source_without_target(source) {
                errors.push(format!(
                    "claude_code_always_inject_on_prompt.context[{idx}]: dynamic source must have either 'file' or 'command'"
                ));
            }
        }
    }

    /// Validate git_pre_check
    fn validate_git_pre_check(&self, errors: &mut Vec<String>) {
        let Some(config) = &self.git_pre_check else {
            return;
        };
        if config.enabled && config.checks.is_empty() {
            errors.push("git_pre_check: enabled but no checks configured".to_string());
        }

        for (idx, check) in config.checks.iter().enumerate() {
            if check.command.trim().is_empty() {
                errors.push(format!(
                    "git_pre_check.checks[{idx}]: command cannot be empty"
                ));
            }
        }
    }

    /// Validate post_edit_check
    fn validate_post_edit_check(&self, errors: &mut Vec<String>) {
        let Some(config) = &self.post_edit_check else {
            return;
        };
        if config.enabled && config.by_extension.is_empty() {
            errors.push("post_edit_check: enabled but no extensions configured".to_string());
        }

        for (ext, check) in &config.by_extension {
            if check.command.trim().is_empty() {
                errors.push(format!(
                    "post_edit_check.by_extension.{ext}: command cannot be empty"
                ));
            }

            // Warn about common extension mistakes
            if ext.contains('.') {
                errors.push(format!(
                    "post_edit_check.by_extension.{ext}: extension should not include dot (use 'rs' not '.rs')"
                ));
            }
        }
    }

    /// Validate rulebook_security_guardrails
    fn validate_rulebook_security_guardrails(&self, errors: &mut Vec<String>) {
        let Some(config) = &self.rulebook_security_guardrails else {
            return;
        };
        if config.enabled && config.protected_paths.is_empty() {
            errors.push(
                "rulebook_security_guardrails: enabled but no protected paths configured"
                    .to_string(),
            );
        }

        for (idx, path) in config.protected_paths.iter().enumerate() {
            if path.trim().is_empty() {
                errors.push(format!(
                    "rulebook_security_guardrails.protected_paths[{idx}]: path cannot be empty"
                ));
            }
        }
    }

    /// Validate protected_paths
    fn validate_protected_paths(&self, errors: &mut Vec<String>) {
        let Some(config) = &self.protected_paths else {
            return;
        };
        if config.enabled && config.paths.is_empty() {
            errors.push("protected_paths: enabled but no paths configured".to_string());
        }

        for (idx, path) in config.paths.iter().enumerate() {
            if path.trim().is_empty() {
                errors.push(format!(
                    "protected_paths.paths[{idx}]: path cannot be empty"
                ));
            }
        }
    }

    /// Validate claude_code_enforce_full_file_read
    fn validate_enforce_full_file_read(&self, errors: &mut Vec<String>) {
        let Some(config) = &self.claude_code_enforce_full_file_read else {
            return;
        };
        if config.enabled && config.max_lines == 0 {
            errors.push("claude_code_enforce_full_file_read: max_lines cannot be 0".to_string());
        }
    }

    /// Check if any builtin is enabled
    pub fn any_enabled(&self) -> bool {
        self.claude_code_always_inject_on_prompt
            .as_ref()
            .is_some_and(|c| c.enabled)
            || self.git_pre_check.as_ref().is_some_and(|c| c.enabled)
            || self.post_edit_check.as_ref().is_some_and(|c| c.enabled)
            || self
                .rulebook_security_guardrails
                .as_ref()
                .is_some_and(|c| c.enabled)
            || self.protected_paths.as_ref().is_some_and(|c| c.enabled)
            || self.git_block_no_verify.as_ref().is_some_and(|c| c.enabled)
            || self.system_protection.as_ref().is_some_and(|c| c.enabled)
            || self
                .sensitive_data_protection
                .as_ref()
                .is_some_and(|c| c.enabled)
            || self
                .cupcake_exec_protection
                .as_ref()
                .is_some_and(|c| c.enabled)
            || self
                .claude_code_enforce_full_file_read
                .as_ref()
                .is_some_and(|c| c.enabled)
    }

    /// Get list of enabled builtin names
    pub fn enabled_builtins(&self) -> Vec<String> {
        let mut enabled = Vec::new();

        if self
            .claude_code_always_inject_on_prompt
            .as_ref()
            .is_some_and(|c| c.enabled)
        {
            enabled.push("claude_code_always_inject_on_prompt".to_string());
        }
        if self.git_pre_check.as_ref().is_some_and(|c| c.enabled) {
            enabled.push("git_pre_check".to_string());
        }
        if self.post_edit_check.as_ref().is_some_and(|c| c.enabled) {
            enabled.push("post_edit_check".to_string());
        }
        if self
            .rulebook_security_guardrails
            .as_ref()
            .is_some_and(|c| c.enabled)
        {
            enabled.push("rulebook_security_guardrails".to_string());
        }
        if self.protected_paths.as_ref().is_some_and(|c| c.enabled) {
            enabled.push("protected_paths".to_string());
        }
        if self.git_block_no_verify.as_ref().is_some_and(|c| c.enabled) {
            enabled.push("git_block_no_verify".to_string());
        }
        if self.system_protection.as_ref().is_some_and(|c| c.enabled) {
            enabled.push("system_protection".to_string());
        }
        if self
            .sensitive_data_protection
            .as_ref()
            .is_some_and(|c| c.enabled)
        {
            enabled.push("sensitive_data_protection".to_string());
        }
        if self
            .cupcake_exec_protection
            .as_ref()
            .is_some_and(|c| c.enabled)
        {
            enabled.push("cupcake_exec_protection".to_string());
        }
        if self
            .claude_code_enforce_full_file_read
            .as_ref()
            .is_some_and(|c| c.enabled)
        {
            enabled.push("claude_code_enforce_full_file_read".to_string());
        }

        enabled
    }
}

/// A dynamic context source must name a file or a command to run.
fn is_dynamic_source_without_target(source: &ContextSource) -> bool {
    matches!(
        source,
        ContextSource::Dynamic {
            file: None,
            command: None,
        }
    )
}
