//! Global configuration fixtures shared by the integration tests that include it.

use anyhow::Result;
use cupcake_core::fs_access;
use std::path::Path;

/// Create global configuration structure for testing
pub fn create_test_global_config(global_path: &Path) -> Result<()> {
    let policies_dir = global_path.join("policies");
    // Use Claude harness-specific directory structure
    let claude_dir = policies_dir.join("claude");
    let system_dir = claude_dir.join("system");

    fs_access::create_dir_all(&system_dir)?;
    fs_access::create_dir_all(global_path.join("signals"))?;

    // Create minimal rulebook
    fs_access::write(
        global_path.join("rulebook.yml"),
        "signals: {}\nbuiltins: {}",
    )?;

    // Use fixture for global system policy
    let global_system_policy = include_str!("../fixtures/global_system_evaluate.rego");
    fs_access::write(system_dir.join("evaluate.rego"), global_system_policy)?;

    Ok(())
}
