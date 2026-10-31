//! Integration test for the signals that global builtins generate.

use anyhow::Result;
use cupcake_core::engine::Engine;
use cupcake_core::fs_access;
use serde_json::json;
use serial_test::serial;
use tempfile::TempDir;

#[tokio::test]
#[serial]
async fn test_global_builtin_signals() -> Result<()> {
    crate::init_test_logging();

    let global_temp = TempDir::new()?;
    let project_temp = TempDir::new()?;

    // Setup global config
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

    // Create builtin that uses signals - signals come through input.signals, not data!
    fs_access::write(
        builtins_dir.join("system_protection.rego"),
        r#"# METADATA
# scope: package
# custom:
#   routing:
#     required_events: ["PreToolUse"]
#     required_tools: ["Edit"]
package cupcake.global.policies.builtins.system_protection
import rego.v1

halt contains decision if {
    input.hook_event_name == "PreToolUse"
    input.tool_name == "Edit"
    
    # Signals are passed in input.signals, not data!
    additional_paths := input.signals.__builtin_system_protection_paths
    message := input.signals.__builtin_system_protection_message
    
    # Check if the file path is in the additional protected paths
    some path in additional_paths
    input.tool_input.file_path == path
    
    decision := {
        "rule_id": "TEST-SYSTEM-PROTECTION-SIGNAL",
        "reason": message,
        "severity": "CRITICAL"
    }
}"#,
    )?;

    // Create rulebook with additional paths AND the generated signals
    // The signals must be present for the builtin to work!
    fs_access::write(
        global_temp.path().join("rulebook.yml"),
        r#"signals:
  __builtin_system_protection_paths:
    command: "echo '[\"/custom/protected/path\"]'"
    timeout_seconds: 1
  __builtin_system_protection_message:
    command: "echo 'Custom block message from signal'"
    timeout_seconds: 1
builtins:
  system_protection:
    enabled: true
    additional_paths: 
      - "/custom/protected/path"
    message: "Custom block message from signal"
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

    // Test that builtin uses signal for additional path
    let event = json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Edit",
        "tool_input": {
            "file_path": "/custom/protected/path",
            "old_string": "test",
            "new_string": "malicious"
        }
    });

    let decision = engine.evaluate(&event, None).await?;

    // Debug what we got
    eprintln!("Decision for custom path: {decision:?}");
    eprintln!("Is halt? {}", decision.is_halt());
    eprintln!("Reason: {:?}", decision.reason());

    // Should be HALT with custom message
    assert!(
        decision.is_halt(),
        "Expected HALT for custom protected path, got: {decision:?}"
    );
    assert_eq!(decision.reason(), Some("Custom block message from signal"));

    Ok(())
}
