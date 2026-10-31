//! Integration tests for global builtin policies.

// Import only the specific helpers we need
#[path = "common/logging.rs"]
mod logging;
#[path = "common/project.rs"]
mod project;
use logging::init_test_logging;
use project::create_test_project_for_harness;

#[cfg(test)]
#[path = "global_builtins_integration_test/signal_tests.rs"]
mod signal_tests;
#[cfg(test)]
#[path = "global_builtins_integration_test/support.rs"]
mod support;

#[cfg(test)]
mod tests {
    //! Integration tests for global builtin policies.

    use super::support::setup_test_global_config;
    use anyhow::Result;
    use cupcake_core::engine::Engine;
    use cupcake_core::fs_access;
    use serde_json::json;
    use serial_test::serial;
    use tempfile::TempDir;

    #[tokio::test]
    #[serial]
    async fn test_global_system_protection_builtin() -> Result<()> {
        crate::init_test_logging();

        let global_temp = TempDir::new()?;
        let project_temp = TempDir::new()?;

        // Setup global config
        setup_test_global_config(global_temp.path())?;

        // Setup project using test helper
        crate::create_test_project_for_harness(
            project_temp.path(),
            cupcake_core::harness::types::HarnessType::ClaudeCode,
        )?;

        // Set global config env var
        // Create engine with global config (pass project root, not policies directory)
        let config = cupcake_core::engine::EngineConfig {
            global_config: Some(global_temp.path().to_path_buf()),
            harness: cupcake_core::harness::types::HarnessType::ClaudeCode,
            wasm_max_memory: None,
            opa_path: None,
            debug_routing: false,
        };
        let engine = Engine::new_with_config(project_temp.path(), config).await?;

        // Test system protection blocks /etc/hosts edit
        let event = json!({
            "hook_event_name": "PreToolUse",
            "tool_name": "Edit",
            "tool_input": {
                "file_path": "/etc/hosts",
                "old_string": "localhost",
                "new_string": "malicious"
            }
        });

        let decision = engine.evaluate(&event, None).await?;

        // Debug what decision we got
        eprintln!("Decision for /etc/hosts edit: {decision:?}");
        eprintln!("Decision reason: {:?}", decision.reason());

        // Should be HALT decision from global builtin
        assert!(
            decision.is_halt(),
            "Expected HALT decision for /etc/hosts edit, got: {decision:?}"
        );
        assert_eq!(decision.reason(), Some("Test: System file blocked"));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_global_sensitive_data_builtin() -> Result<()> {
        crate::init_test_logging();

        let global_temp = TempDir::new()?;
        let project_temp = TempDir::new()?;

        // Setup global config
        setup_test_global_config(global_temp.path())?;

        // Setup project using test helper
        crate::create_test_project_for_harness(
            project_temp.path(),
            cupcake_core::harness::types::HarnessType::ClaudeCode,
        )?;

        // Set global config env var
        // Create engine with global config (pass project root, not policies directory)
        let config = cupcake_core::engine::EngineConfig {
            global_config: Some(global_temp.path().to_path_buf()),
            harness: cupcake_core::harness::types::HarnessType::ClaudeCode,
            wasm_max_memory: None,
            opa_path: None,
            debug_routing: false,
        };
        let engine = Engine::new_with_config(project_temp.path(), config).await?;

        // Test sensitive data blocks .env read
        let event = json!({
            "hook_event_name": "PreToolUse",
            "tool_name": "Read",
            "tool_input": {
                "file_path": "/home/user/.env"
            }
        });

        let decision = engine.evaluate(&event, None).await?;

        // Should be DENY decision from global builtin
        assert!(
            decision.is_blocking(),
            "Expected blocking decision for .env read"
        );
        assert_eq!(decision.reason(), Some("Test: Sensitive file blocked"));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_global_cupcake_exec_builtin() -> Result<()> {
        crate::init_test_logging();

        let global_temp = TempDir::new()?;
        let project_temp = TempDir::new()?;

        // Setup global config
        setup_test_global_config(global_temp.path())?;

        // Setup project using test helper
        crate::create_test_project_for_harness(
            project_temp.path(),
            cupcake_core::harness::types::HarnessType::ClaudeCode,
        )?;

        // Set global config env var
        // Create engine with global config (pass project root, not policies directory)
        let config = cupcake_core::engine::EngineConfig {
            global_config: Some(global_temp.path().to_path_buf()),
            harness: cupcake_core::harness::types::HarnessType::ClaudeCode,
            wasm_max_memory: None,
            opa_path: None,
            debug_routing: false,
        };
        let engine = Engine::new_with_config(project_temp.path(), config).await?;

        // Test cupcake execution blocks
        let event = json!({
            "hook_event_name": "PreToolUse",
            "tool_name": "Bash",
            "tool_input": {
                "command": "cupcake init --global"
            }
        });

        let decision = engine.evaluate(&event, None).await?;

        // Should be HALT decision from global builtin
        assert!(
            decision.is_halt(),
            "Expected HALT decision for cupcake execution"
        );
        assert_eq!(decision.reason(), Some("Test: Cupcake execution blocked"));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_global_builtins_disabled() -> Result<()> {
        crate::init_test_logging();

        let global_temp = TempDir::new()?;
        let project_temp = TempDir::new()?;

        // Setup global config with builtins disabled
        let policies_dir = global_temp.path().join("policies");
        // Use Claude harness-specific directory
        let claude_dir = policies_dir.join("claude");
        let builtins_dir = claude_dir.join("builtins");
        let system_dir = claude_dir.join("system");

        fs_access::create_dir_all(&builtins_dir)?;
        fs_access::create_dir_all(&system_dir)?;

        // Create global system evaluate policy
        fs_access::write(
            system_dir.join("evaluate.rego"),
            r#"package cupcake.global.system
import rego.v1

halts := collect_verbs("halt")
denials := collect_verbs("deny")
blocks := collect_verbs("block")
asks := collect_verbs("ask")
modifications := collect_verbs("modify")
add_context := collect_verbs("add_context")

evaluate := {
    "halts": halts,
    "denials": denials,
    "blocks": blocks,
    "asks": asks,
    "modifications": modifications,
    "add_context": add_context
}

default collect_verbs(_) := []

collect_verbs(verb_name) := result if {
    verb_sets := [value |
        walk(data.cupcake.global.policies, [path, value])
        path[count(path) - 1] == verb_name
    ]
    all_decisions := [decision |
        some verb_set in verb_sets
        some decision in verb_set
    ]
    result := all_decisions
}"#,
        )?;

        // Create builtin policies (they exist but are disabled)
        fs_access::write(
            builtins_dir.join("system_protection.rego"),
            r#"package cupcake.global.policies.builtins.system_protection
import rego.v1

halt contains decision if {
    input.hook_event_name == "PreToolUse"
    input.tool_name == "Edit"
    input.tool_input.file_path == "/etc/hosts"
    decision := {
        "rule_id": "TEST-SYSTEM-PROTECTION-DISABLED",
        "reason": "Should not fire when disabled",
        "severity": "CRITICAL"
    }
}"#,
        )?;

        // Create rulebook with builtins DISABLED
        fs_access::write(
            global_temp.path().join("rulebook.yml"),
            r#"signals: {}
builtins:
  system_protection:
    enabled: false
  sensitive_data_protection:
    enabled: false
  cupcake_exec_protection:
    enabled: false
"#,
        )?;

        // Setup project using test helper
        crate::create_test_project_for_harness(
            project_temp.path(),
            cupcake_core::harness::types::HarnessType::ClaudeCode,
        )?;

        // Set global config env var
        // Create engine with global config (pass project root, not policies directory)
        let config = cupcake_core::engine::EngineConfig {
            global_config: Some(global_temp.path().to_path_buf()),
            harness: cupcake_core::harness::types::HarnessType::ClaudeCode,
            wasm_max_memory: None,
            opa_path: None,
            debug_routing: false,
        };
        let engine = Engine::new_with_config(project_temp.path(), config).await?;

        // Test that disabled builtin does NOT fire
        let event = json!({
            "hook_event_name": "PreToolUse",
            "tool_name": "Edit",
            "tool_input": {
                "file_path": "/etc/hosts",
                "old_string": "localhost",
                "new_string": "malicious"
            }
        });

        let decision = engine.evaluate(&event, None).await?;

        // Should be ALLOW since builtin is disabled
        assert!(
            !decision.is_halt(),
            "Expected no HALT when builtin is disabled"
        );
        assert_ne!(decision.reason(), Some("Should not fire when disabled"));

        Ok(())
    }
}
