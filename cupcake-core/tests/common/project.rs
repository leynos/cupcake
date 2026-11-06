//! Cupcake project fixtures shared by the integration tests that include it.

use anyhow::Result;
use cupcake_core::fs_access;
use cupcake_core::harness::types::HarnessType;
use std::path::Path;

/// Create a cupcake project structure for a specific harness
///
/// This only creates the directory for the specified harness, avoiding
/// duplicate package errors during compilation.
pub fn create_test_project_for_harness(project_path: &Path, harness: HarnessType) -> Result<()> {
    let cupcake_dir = project_path.join(".cupcake");
    let policies_dir = cupcake_dir.join("policies");

    // Create harness-specific directory structure
    let harness_name = match harness {
        HarnessType::ClaudeCode => "claude",
        HarnessType::Cursor => "cursor",
        HarnessType::Factory => "factory",
        HarnessType::OpenCode => "opencode",
    };
    let harness_dir = policies_dir.join(harness_name);
    let system_dir = harness_dir.join("system");

    fs_access::create_dir_all(&system_dir)?;
    fs_access::create_dir_all(cupcake_dir.join("signals"))?;

    // Create minimal rulebook
    fs_access::write(
        cupcake_dir.join("rulebook.yml"),
        "signals: {}\nbuiltins: {}",
    )?;

    // Use fixture for system policy
    let system_policy = include_str!("../fixtures/system_evaluate.rego");
    fs_access::write(system_dir.join("evaluate.rego"), system_policy)?;

    // Add minimal policy to ensure compilation works
    let minimal_policy = include_str!("../fixtures/minimal_policy.rego");
    fs_access::write(harness_dir.join("minimal.rego"), minimal_policy)?;

    Ok(())
}
