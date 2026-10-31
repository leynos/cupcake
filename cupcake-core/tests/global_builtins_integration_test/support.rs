//! Fixtures for the global builtin policy tests.

use anyhow::Result;
use cupcake_core::fs_access;
use std::path::Path;

/// Helper to create a test global configuration with test policies
pub fn setup_test_global_config(global_dir: &Path) -> Result<()> {
    let policies_dir = global_dir.join("policies");
    // Use Claude harness-specific directory
    let claude_dir = policies_dir.join("claude");
    let system_dir = claude_dir.join("system");

    fs_access::create_dir_all(&system_dir)?;

    // Use the same fixture that works in other global tests
    fs_access::write(
        system_dir.join("evaluate.rego"),
        include_str!("../fixtures/global_system_evaluate.rego"),
    )?;

    // Create test policies (not in builtins directory to avoid filtering)
    fs_access::write(
        claude_dir.join("test_system_protection.rego"),
        r#"# METADATA
# scope: package
# custom:
#   routing:
#     required_events: ["PreToolUse"]
#     required_tools: ["Edit"]
package cupcake.global.policies.test_system_protection
import rego.v1

halt contains decision if {
    input.hook_event_name == "PreToolUse"
    input.tool_name == "Edit"
    input.tool_input.file_path == "/etc/hosts"
    decision := {
        "rule_id": "TEST-SYSTEM-PROTECTION",
        "reason": "Test: System file blocked",
        "severity": "CRITICAL"
    }
}"#,
    )?;

    fs_access::write(
        claude_dir.join("test_sensitive_data.rego"),
        r#"# METADATA
# scope: package
# custom:
#   routing:
#     required_events: ["PreToolUse"]
#     required_tools: ["Read"]
package cupcake.global.policies.test_sensitive_data
import rego.v1

deny contains decision if {
    input.hook_event_name == "PreToolUse"
    input.tool_name == "Read"
    input.tool_input.file_path == "/home/user/.env"
    decision := {
        "rule_id": "TEST-SENSITIVE-DATA",
        "reason": "Test: Sensitive file blocked",
        "severity": "HIGH"
    }
}"#,
    )?;

    fs_access::write(
        claude_dir.join("test_cupcake_exec.rego"),
        r#"# METADATA
# scope: package
# custom:
#   routing:
#     required_events: ["PreToolUse"]
#     required_tools: ["Bash"]
package cupcake.global.policies.test_cupcake_exec
import rego.v1

halt contains decision if {
    input.hook_event_name == "PreToolUse"
    input.tool_name == "Bash"
    contains(input.tool_input.command, "cupcake init")
    decision := {
        "rule_id": "TEST-CUPCAKE-EXEC",
        "reason": "Test: Cupcake execution blocked",
        "severity": "CRITICAL"
    }
}"#,
    )?;

    // Create rulebook with builtins enabled
    // Note: The global builtins aren't in "builtins" subdirectory in tests,
    // so we need to ensure they're not filtered out
    fs_access::write(
        global_dir.join("rulebook.yml"),
        r#"signals: {}
builtins: {}
"#,
    )?;

    Ok(())
}
