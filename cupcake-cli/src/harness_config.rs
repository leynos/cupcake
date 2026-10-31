//! Agent harness configuration for automated integration setup
//!
//! Provides trait-based architecture for configuring various agent harnesses
//! (Claude Code, Cursor, etc.) with Cupcake policy evaluation.

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Trait for agent harness configuration
pub trait HarnessConfig {
    /// Get the harness name for display
    fn name(&self) -> &str;

    /// Get the settings file path relative to project root or user home
    fn settings_path(&self, global: bool) -> PathBuf;

    /// Generate the hook configuration JSON for this harness
    fn generate_hooks(&self, policy_dir: &Path, global: bool) -> Result<Value>;

    /// Merge hooks into existing settings without destroying other configuration
    fn merge_settings(&self, existing: Value, new_hooks: Value) -> Result<Value>;
}

/// Claude Code harness implementation
pub struct ClaudeHarness;

/// Cursor harness implementation
pub struct CursorHarness;

/// Factory AI harness implementation
pub struct FactoryHarness;

/// OpenCode harness implementation
pub struct OpenCodeHarness;

/// GitHub repository for downloading plugins
const GITHUB_REPO: &str = "eqtylab/cupcake";

/// Get the current Cupcake version (embedded at compile time)
fn get_cupcake_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Configure harness integration with error recovery
pub async fn configure_harness(
    harness_type: super::HarnessType,
    policy_dir: &Path,
    global: bool,
) -> Result<()> {
    use super::HarnessType;

    match harness_type {
        HarnessType::Claude => {
            let harness = ClaudeHarness;
            let settings_path = harness.settings_path(global);

            // Try to configure, fallback to manual instructions on error
            if let Err(e) =
                setup_harness_settings(&harness, &settings_path, policy_dir, global).await
            {
                eprintln!(
                    "⚠️  Could not automatically configure {}: {}",
                    harness.name(),
                    e
                );
                print_manual_instructions(&harness, policy_dir, global);
                // Don't fail the entire init - just warn
            } else {
                println!(
                    "✅ Configured {} integration in {}",
                    harness.name(),
                    settings_path.display()
                );
                println!("   - Added PreToolUse hook for all tools");
                println!("   - Added PostToolUse hook for file modifications");
                println!("   - Added UserPromptSubmit hook for prompt validation");
                println!("   - Added SessionStart hook for initial context");
                println!();
                println!("   {} will now evaluate all tool uses and prompts against your Cupcake policies.",
                    harness.name());
            }
        }
        HarnessType::Cursor => {
            let harness = CursorHarness;
            let settings_path = harness.settings_path(global);

            // Try to configure, fallback to manual instructions on error
            if let Err(e) =
                setup_harness_settings(&harness, &settings_path, policy_dir, global).await
            {
                eprintln!(
                    "⚠️  Could not automatically configure {}: {}",
                    harness.name(),
                    e
                );
                print_cursor_manual_instructions(policy_dir, global);
                // Don't fail the entire init - just warn
            } else {
                println!(
                    "✅ Configured {} integration in {}",
                    harness.name(),
                    settings_path.display()
                );
                println!("   - Added beforeShellExecution hook for shell commands");
                println!("   - Added beforeMCPExecution hook for MCP tools");
                println!("   - Added afterFileEdit hook for post-edit validation");
                println!("   - Added beforeReadFile hook for file access control");
                println!("   - Added beforeSubmitPrompt hook for prompt validation");
                println!("   - Added stop hook for cleanup");
                println!();
                println!(
                    "   {} will now evaluate all tool usage against your Cupcake policies.",
                    harness.name()
                );
            }
        }
        HarnessType::Factory => {
            let harness = FactoryHarness;
            let settings_path = harness.settings_path(global);

            // Try to configure, fallback to manual instructions on error
            if let Err(e) =
                setup_harness_settings(&harness, &settings_path, policy_dir, global).await
            {
                eprintln!(
                    "⚠️  Could not automatically configure {}: {}",
                    harness.name(),
                    e
                );
                print_manual_instructions(&harness, policy_dir, global);
                // Don't fail the entire init - just warn
            } else {
                println!(
                    "✅ Configured {} integration in {}",
                    harness.name(),
                    settings_path.display()
                );
                println!("   - Added PreToolUse hook for all tools");
                println!("   - Added PostToolUse hook for all tools");
                println!("   - Added UserPromptSubmit hook for prompt validation");
                println!("   - Added SessionStart hook for initial context");
                println!("   - Added Stop/SubagentStop hooks for cleanup");
                println!();
                println!("   {} will now evaluate all tool uses and prompts against your Cupcake policies.",
                    harness.name());
            }
        }
        HarnessType::OpenCode => {
            // OpenCode uses a plugin model - download the plugin from GitHub releases
            println!("   Configuring OpenCode integration...");

            // Determine the target directory for the plugin
            let target_dir = if global {
                // For global, use config directory
                dirs::config_dir().ok_or_else(|| anyhow!("Could not determine config directory"))?
            } else {
                // For project, use the policy_dir parent (project root)
                policy_dir.parent().unwrap_or(Path::new(".")).to_path_buf()
            };

            // Try to download the plugin from GitHub releases
            match OpenCodeHarness::download_plugin(&target_dir, global).await {
                Ok(()) => {
                    let plugin_location = if global {
                        "~/.config/opencode/plugin/cupcake.js".to_string()
                    } else {
                        ".opencode/plugin/cupcake.js".to_string()
                    };

                    println!("✅ Configured OpenCode integration");
                    println!("   - Plugin installed to: {plugin_location}");
                    println!();
                    println!(
                        "   OpenCode will automatically load the Cupcake plugin and enforce policies."
                    );
                    println!();
                    println!(
                        "   Optional: Create .cupcake/opencode.json to customize plugin behavior:"
                    );
                    println!("   {{");
                    println!("     \"enabled\": true,");
                    println!("     \"logLevel\": \"info\",");
                    println!("     \"failMode\": \"closed\"");
                    println!("   }}");
                }
                Err(e) => {
                    eprintln!("⚠️  Could not automatically download OpenCode plugin: {e}");
                    OpenCodeHarness::print_manual_instructions();
                    // Don't fail the entire init - just warn
                }
            }
        }
    }

    Ok(())
}

/// Setup harness settings file (create or merge)
async fn setup_harness_settings(
    harness: &dyn HarnessConfig,
    settings_path: &Path,
    policy_dir: &Path,
    global: bool,
) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = settings_path.parent() {
        cupcake_core::fs_access::create_dir_all(parent)?;
    }

    // Generate hook configuration
    let new_hooks = harness.generate_hooks(policy_dir, global)?;

    // Check if settings file exists
    let final_settings = if settings_path.exists() {
        // Read existing settings
        let content = cupcake_core::fs_access::read_to_string(settings_path)?;
        let existing: Value = serde_json::from_str(&content)
            .map_err(|e| anyhow!("Invalid JSON in existing settings: {}", e))?;

        println!("⚠️  Found existing {}", settings_path.display());
        println!("   Merging Cupcake hooks into existing configuration...");

        // Merge hooks
        harness.merge_settings(existing, new_hooks)?
    } else {
        // Create new settings with just hooks
        new_hooks
    };

    // Write settings with pretty formatting
    let json_str = serde_json::to_string_pretty(&final_settings)?;
    cupcake_core::fs_access::write(settings_path, json_str)?;

    Ok(())
}

/// Print manual configuration instructions as fallback
fn print_manual_instructions(harness: &dyn HarnessConfig, policy_dir: &Path, global: bool) {
    let policy_path = if global {
        policy_dir.display().to_string()
    } else {
        "$CLAUDE_PROJECT_DIR/.cupcake".to_string()
    };

    eprintln!();
    eprintln!(
        "   To manually configure, add this to your {}:",
        harness.settings_path(global).display()
    );
    eprintln!();
    eprintln!("   {{");
    eprintln!("     \"hooks\": {{");
    eprintln!("       \"PreToolUse\": [{{");
    eprintln!("         \"matcher\": \"*\",");
    eprintln!("         \"hooks\": [{{");
    eprintln!("           \"type\": \"command\",");
    eprintln!(
        "           \"command\": \"cupcake eval --harness claude --policy-dir {policy_path}\""
    );
    eprintln!("         }}]");
    eprintln!("       }}]");
    eprintln!("     }}");
    eprintln!("   }}");
    eprintln!();
}

/// Print manual configuration instructions for Cursor
fn print_cursor_manual_instructions(policy_dir: &Path, global: bool) {
    let policy_path = if global {
        policy_dir.display().to_string()
    } else {
        ".cupcake".to_string()
    };

    // Cursor hooks are always in ~/.cursor/hooks.json
    let settings_path = "~/.cursor/hooks.json";

    eprintln!();
    eprintln!("   To manually configure, add this to your {settings_path}:");
    eprintln!();
    eprintln!("   {{");
    eprintln!("     \"version\": 1,");
    eprintln!("     \"hooks\": {{");
    eprintln!("       \"beforeShellExecution\": [{{");
    eprintln!("         \"command\": \"cupcake eval --harness cursor --policy-dir {policy_path}\"");
    eprintln!("       }}],");
    eprintln!("       \"beforeMCPExecution\": [{{");
    eprintln!("         \"command\": \"cupcake eval --harness cursor --policy-dir {policy_path}\"");
    eprintln!("       }}],");
    eprintln!("       \"afterFileEdit\": [{{");
    eprintln!("         \"command\": \"cupcake eval --harness cursor --policy-dir {policy_path}\"");
    eprintln!("       }}],");
    eprintln!("       \"beforeReadFile\": [{{");
    eprintln!("         \"command\": \"cupcake eval --harness cursor --policy-dir {policy_path}\"");
    eprintln!("       }}],");
    eprintln!("       \"beforeSubmitPrompt\": [{{");
    eprintln!("         \"command\": \"cupcake eval --harness cursor --policy-dir {policy_path}\"");
    eprintln!("       }}],");
    eprintln!("       \"stop\": [{{");
    eprintln!("         \"command\": \"cupcake eval --harness cursor --policy-dir {policy_path}\"");
    eprintln!("       }}]");
    eprintln!("     }}");
    eprintln!("   }}");
    eprintln!();
}

mod harnesses;
mod merge;
mod opencode;
#[cfg(test)]
mod tests;

use merge::merge_hooks;
