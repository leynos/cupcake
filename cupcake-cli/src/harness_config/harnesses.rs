//! Hook configuration for the Claude Code, Cursor and Factory AI harnesses.

use super::*;

impl HarnessConfig for ClaudeHarness {
    fn name(&self) -> &str {
        "Claude Code"
    }

    fn settings_path(&self, global: bool) -> PathBuf {
        if global {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(".claude")
                .join("settings.json")
        } else {
            Path::new(".claude").join("settings.json")
        }
    }

    fn generate_hooks(&self, policy_dir: &Path, global: bool) -> Result<Value> {
        // Determine the policy path to use in commands
        let policy_path = if global {
            // Global config - use absolute path
            let abs_path = cupcake_core::ambient_fs::canonicalize(policy_dir)
                .unwrap_or_else(|_| policy_dir.to_path_buf());
            abs_path.display().to_string()
        } else {
            // Project config - use environment variable for portability
            "$CLAUDE_PROJECT_DIR/.cupcake".to_string()
        };

        Ok(json!({
            "hooks": {
                "PreToolUse": [{
                    "matcher": "*",
                    "hooks": [{
                        "type": "command",
                        "command": format!("cupcake eval --harness claude --policy-dir {}", policy_path)
                    }]
                }],
                "PostToolUse": [{
                    "matcher": "Edit|MultiEdit|Write",
                    "hooks": [{
                        "type": "command",
                        "command": format!("cupcake eval --harness claude --policy-dir {}", policy_path)
                    }]
                }],
                "UserPromptSubmit": [{
                    "hooks": [{
                        "type": "command",
                        "command": format!("cupcake eval --harness claude --policy-dir {}", policy_path)
                    }]
                }],
                "SessionStart": [{
                    "hooks": [{
                        "type": "command",
                        "command": format!("cupcake eval --harness claude --policy-dir {}", policy_path)
                    }]
                }]
            }
        }))
    }

    fn merge_settings(&self, mut existing: Value, new_hooks: Value) -> Result<Value> {
        merge_hooks(&mut existing, new_hooks)?;
        Ok(existing)
    }
}

impl HarnessConfig for CursorHarness {
    fn name(&self) -> &str {
        "Cursor"
    }

    fn settings_path(&self, global: bool) -> PathBuf {
        // Cursor now supports both project-level and user-level hooks
        // Priority order: Enterprise → Project → User
        // Reference: https://docs.cursor.com/context/hooks
        if global {
            // User-level hooks: ~/.cursor/hooks.json
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(".cursor")
                .join("hooks.json")
        } else {
            // Project-level hooks: .cursor/hooks.json
            Path::new(".cursor").join("hooks.json")
        }
    }

    fn generate_hooks(&self, policy_dir: &Path, global: bool) -> Result<Value> {
        // Determine the policy path to use in commands
        let policy_path = if global {
            // Global config - use absolute path
            let abs_path = cupcake_core::ambient_fs::canonicalize(policy_dir)
                .unwrap_or_else(|_| policy_dir.to_path_buf());
            abs_path.display().to_string()
        } else {
            // Project config - use relative path from workspace root
            ".cupcake".to_string()
        };

        // Cursor's hook configuration format - official hooks.json structure
        // Reference: https://cursor.com/docs/agent/hooks.md
        Ok(json!({
            "version": 1,
            "hooks": {
                "beforeShellExecution": [{
                    "command": format!("cupcake eval --harness cursor --policy-dir {}", policy_path)
                }],
                "beforeMCPExecution": [{
                    "command": format!("cupcake eval --harness cursor --policy-dir {}", policy_path)
                }],
                "afterFileEdit": [{
                    "command": format!("cupcake eval --harness cursor --policy-dir {}", policy_path)
                }],
                "beforeReadFile": [{
                    "command": format!("cupcake eval --harness cursor --policy-dir {}", policy_path)
                }],
                "beforeSubmitPrompt": [{
                    "command": format!("cupcake eval --harness cursor --policy-dir {}", policy_path)
                }],
                "stop": [{
                    "command": format!("cupcake eval --harness cursor --policy-dir {}", policy_path)
                }]
            }
        }))
    }

    fn merge_settings(&self, mut existing: Value, new_hooks: Value) -> Result<Value> {
        // For Cursor hooks.json, merge using the same hooks structure as Claude
        merge_hooks(&mut existing, new_hooks)?;
        Ok(existing)
    }
}

impl HarnessConfig for FactoryHarness {
    fn name(&self) -> &str {
        "Factory AI"
    }

    fn settings_path(&self, global: bool) -> PathBuf {
        if global {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(".factory")
                .join("settings.json")
        } else {
            Path::new(".factory").join("settings.json")
        }
    }

    fn generate_hooks(&self, policy_dir: &Path, global: bool) -> Result<Value> {
        // Determine the policy path to use in commands
        let policy_path = if global {
            // Global config - use absolute path
            let abs_path = cupcake_core::ambient_fs::canonicalize(policy_dir)
                .unwrap_or_else(|_| policy_dir.to_path_buf());
            abs_path.display().to_string()
        } else {
            // Project config - use environment variable for portability
            "\"$FACTORY_PROJECT_DIR\"/.cupcake".to_string()
        };

        Ok(json!({
            "hooks": {
                "PreToolUse": [{
                    "matcher": "*",
                    "hooks": [{
                        "type": "command",
                        "command": format!("cupcake eval --harness factory --policy-dir {}", policy_path)
                    }]
                }],
                "PostToolUse": [{
                    "matcher": "*",
                    "hooks": [{
                        "type": "command",
                        "command": format!("cupcake eval --harness factory --policy-dir {}", policy_path)
                    }]
                }],
                "UserPromptSubmit": [{
                    "hooks": [{
                        "type": "command",
                        "command": format!("cupcake eval --harness factory --policy-dir {}", policy_path)
                    }]
                }],
                "SessionStart": [{
                    "hooks": [{
                        "type": "command",
                        "command": format!("cupcake eval --harness factory --policy-dir {}", policy_path)
                    }]
                }],
                "Stop": [{
                    "hooks": [{
                        "type": "command",
                        "command": format!("cupcake eval --harness factory --policy-dir {}", policy_path)
                    }]
                }],
                "SubagentStop": [{
                    "hooks": [{
                        "type": "command",
                        "command": format!("cupcake eval --harness factory --policy-dir {}", policy_path)
                    }]
                }]
            }
        }))
    }

    fn merge_settings(&self, mut existing: Value, new_hooks: Value) -> Result<Value> {
        merge_hooks(&mut existing, new_hooks)?;
        Ok(existing)
    }
}
