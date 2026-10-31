//! Input preprocessing to defend against adversarial patterns
//!
//! This module provides automatic normalization of input data before policy
//! evaluation, implementing defense-in-depth against bypass techniques identified
//! in TOB-EQTY-LAB-CUPCAKE-3.
//!
//! ## Architecture
//!
//! The preprocessing pipeline operates in phases:
//! - Phase 1: Whitespace normalization (implemented)
//! - Phase 2: Pattern detection (future)
//! - Phase 3: AST analysis (future)
//!
//! ## Security Model
//!
//! Rather than requiring every policy to implement secure parsing, we normalize
//! adversarial patterns at the engine level, providing automatic protection for
//! all policies (user and builtin).

use crate::harness::types::HarnessType;
use serde_json::Value;
use tracing::{debug, trace};

pub mod command_path_extractor;
pub mod config;
pub mod normalizers;
pub mod script_inspector;
pub mod symlink_resolver;

use command_path_extractor::extract_target_paths;
pub use config::{PreprocessConfig, PreprocessResult};
use normalizers::WhitespaceNormalizer;
use script_inspector::ScriptInspector;
use symlink_resolver::SymlinkResolver;

/// Preprocess input JSON to normalize adversarial patterns
///
/// This is the main entry point for input preprocessing. It:
/// 1. Identifies tool-specific fields that need normalization
/// 2. Applies appropriate normalizers based on configuration
/// 3. Logs all transformations for auditability
/// 4. Returns a record of which operations were actually applied
///
/// # Arguments
/// * `input` - Mutable reference to the input JSON
/// * `config` - Configuration controlling what normalizations to apply
/// * `harness` - The harness type (Claude Code or Cursor)
///
/// # Returns
/// A `PreprocessResult` containing the list of operations that were actually applied.
///
/// # Example
/// ```
/// # use serde_json::json;
/// # use cupcake_core::preprocessing::{preprocess_input, PreprocessConfig};
/// # use cupcake_core::harness::types::HarnessType;
/// let mut input = json!({
///     "tool_name": "Bash",
///     "tool_input": {
///         "command": "rm  -rf  .cupcake"  // Double spaces
///     }
/// });
///
/// let config = PreprocessConfig::default();
/// let result = preprocess_input(&mut input, &config, HarnessType::ClaudeCode);
///
/// // Command is now normalized: "rm -rf .cupcake"
/// // result.operations() contains ["whitespace_normalization"]
/// ```
pub fn preprocess_input(
    input: &mut Value,
    config: &PreprocessConfig,
    harness: HarnessType,
) -> PreprocessResult {
    let mut result = PreprocessResult::new();
    // Extract tool/event information based on harness type
    // We copy these strings out of the JSON so we can modify the JSON later.
    // Can't modify data while something points into it.
    let (tool_name, event_name): (String, String) = match harness {
        HarnessType::ClaudeCode | HarnessType::Factory => {
            // Claude Code and Factory AI use tool_name (same structure)
            (
                string_field_or_unknown(input, "tool_name"),
                string_field_or_unknown(input, "hook_event_name"),
            )
        }
        HarnessType::OpenCode => {
            if map_opencode_fields(input) {
                result.record("opencode_field_mapping");
            }

            // Re-read the values we just inserted to get proper string slices
            (
                string_field_or_unknown(input, "tool_name"),
                string_field_or_unknown(input, "hook_event_name"),
            )
        }
        HarnessType::Cursor => {
            // Cursor uses hook_event_name to determine the action type
            let event = string_field_or_unknown(input, "hook_event_name");
            // For Cursor, we treat certain events as equivalent to tools
            let tool = cursor_event_tool(&event).to_string();
            (tool, event)
        }
    };

    trace!(
        "Preprocessing input for harness: {}, tool: {}, event: {}",
        harness,
        tool_name,
        event_name
    );

    // Apply tool-specific preprocessing based on the tool type
    if tool_name == "Bash" && config.normalize_whitespace {
        let applied = match harness {
            HarnessType::ClaudeCode => preprocess_claude_bash_command(input, config),
            HarnessType::Factory => preprocess_claude_bash_command(input, config),
            HarnessType::Cursor => preprocess_cursor_shell_command(input, config),
            HarnessType::OpenCode => preprocess_claude_bash_command(input, config), // Same format as Claude/Factory
        };
        if applied {
            result.record("whitespace_normalization");
        }
    } else {
        // Future: Add other tool-specific preprocessing
        // "Task" => preprocess_task_prompt(input, config),
        // "WebFetch" => preprocess_url(input, config),
        trace!("No preprocessing rules for tool: {}", tool_name);
    }

    // ==========================================================================
    // CONTENT FIELD NORMALIZATION FOR WRITE/EDIT UNIFICATION
    // ==========================================================================
    //
    // Problem: Write and Edit tools use different field names for content:
    //   - Write: tool_input.content (full file content)
    //   - Edit:  tool_input.new_string (replacement text)
    //
    // This forces policy authors to write duplicate rules or helper functions
    // just to handle the field name difference.
    //
    // Solution: Copy Write's `content` to `new_string` during preprocessing,
    // allowing policies to use a single field name for both tools:
    //
    //   ```rego
    //   deny contains decision if {
    //       input.tool_name in {"Write", "Edit"}
    //       content := input.tool_input.new_string  # Works for both!
    //       contains(content, "bad pattern")
    //   }
    //   ```
    //
    // This is similar to how we normalize file paths with `resolved_file_path`.
    // The original `content` field is preserved for backwards compatibility.
    //
    // Note: We copy Write→new_string (not Edit→content) because:
    //   - Edit's new_string is always the "new" content being written
    //   - Write's content is also the "new" content being written
    //   - This makes `new_string` semantically consistent across both tools
    //
    // ==========================================================================
    if harness == HarnessType::ClaudeCode && normalize_write_edit_content_fields(input, &tool_name)
    {
        result.record("content_unification");
    }

    // Apply symlink resolution for file operations (TOB-4 defense)
    if config.enable_symlink_resolution && resolve_and_attach_symlinks(input, harness) {
        result.record("symlink_resolution");
    }

    // Future: Apply cross-tool normalizations
    // if config.detect_substitution {
    //     detect_command_substitution(input);
    // }

    result
}

/// Read a string field from `input`, or `"unknown"` when absent or not a string.
fn string_field_or_unknown(input: &Value, key: &str) -> String {
    input
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Map OpenCode's lowercase tool names to Cupcake format (bash -> Bash, edit -> Edit, etc.).
fn map_opencode_tool(tool_lowercase: String) -> String {
    match tool_lowercase.as_str() {
        "bash" => "Bash".to_string(),
        "edit" => "Edit".to_string(),
        "write" => "Write".to_string(),
        "read" => "Read".to_string(),
        "grep" => "Grep".to_string(),
        "glob" => "Glob".to_string(),
        "list" => "List".to_string(),
        "patch" => "Patch".to_string(),
        "todowrite" => "TodoWrite".to_string(),
        "todoread" => "TodoRead".to_string(),
        "webfetch" => "WebFetch".to_string(),
        "task" => "Task".to_string(),
        _ => tool_lowercase, // Unknown tools pass through
    }
}

/// Add the engine-compatible `tool_name`, `tool_input` and `tool_response` fields to an
/// OpenCode event. Returns true if the event was an object and the fields were mapped.
fn map_opencode_fields(input: &mut Value) -> bool {
    // Clone fields before mutating input
    let args = input.get("args").cloned();
    let opencode_result = input.get("result").cloned();
    let tool_mapped = map_opencode_tool(string_field_or_unknown(input, "tool"));

    let Some(obj) = input.as_object_mut() else {
        return false;
    };

    // Add tool_name field for engine compatibility
    obj.insert(
        "tool_name".to_string(),
        serde_json::Value::String(tool_mapped),
    );

    // Add tool_input field by renaming args to tool_input for engine compatibility
    if let Some(args_value) = args {
        obj.insert("tool_input".to_string(), args_value);
    }

    // Add tool_response field by renaming result to tool_response for PostToolUse events
    if let Some(opencode_result_value) = opencode_result {
        obj.insert("tool_response".to_string(), opencode_result_value);
    }

    true
}

/// The tool a Cursor hook event is treated as equivalent to.
fn cursor_event_tool(event: &str) -> &'static str {
    match event {
        "beforeShellExecution" => "Bash",
        "beforeFileEdit" | "afterFileEdit" => "Edit",
        "beforeFileWrite" | "afterFileWrite" => "Write",
        _ => "unknown",
    }
}

/// Normalize content fields between Write and Edit tools for unified policy access
///
/// This function copies Write's `content` field to `new_string`, allowing policies
/// to use a single field name when checking content for both Write and Edit operations.
///
/// # Why this matters
///
/// Without normalization, policies need duplicate rules:
/// ```rego
/// # Rule for Write
/// deny contains decision if {
///     input.tool_name == "Write"
///     content := input.tool_input.content
///     # ... check content
/// }
///
/// # Duplicate rule for Edit
/// deny contains decision if {
///     input.tool_name == "Edit"
///     content := input.tool_input.new_string
///     # ... same check
/// }
/// ```
///
/// With normalization, a single rule works:
/// ```rego
/// deny contains decision if {
///     input.tool_name in {"Write", "Edit"}
///     content := input.tool_input.new_string  # Works for both!
///     # ... check content
/// }
/// ```
///
/// # Field semantics
///
/// - `new_string`: The content being written (unified field)
/// - `content`: Original Write field (preserved for backwards compatibility)
/// - `old_string`: Edit-only field for the text being replaced
fn normalize_write_edit_content_fields(input: &mut Value, tool_name: &str) -> bool {
    // Only normalize Write tool - Edit already has new_string
    if tool_name != "Write" {
        return false;
    }

    if let Some(tool_input) = input.get_mut("tool_input") {
        if let Some(tool_input_obj) = tool_input.as_object_mut() {
            // Copy content to new_string if content exists
            if let Some(content) = tool_input_obj.get("content").cloned() {
                tool_input_obj.insert("new_string".to_string(), content);
                trace!("Normalized Write content → new_string for unified policy access");
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod command_tests;
mod commands;
#[cfg(test)]
mod directory_tests;
#[cfg(test)]
mod symlink_tests;
mod symlinks;

use commands::{preprocess_claude_bash_command, preprocess_cursor_shell_command};
use symlinks::resolve_and_attach_symlinks;
