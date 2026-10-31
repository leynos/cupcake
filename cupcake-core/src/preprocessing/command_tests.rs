//! Tests for command preprocessing, content unification and harness handling.

use super::*;
use serde_json::json;

#[test]
fn test_preprocess_claude_bash_command() {
    let mut input = json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": {
            "command": "rm  -rf  .cupcake",
            "timeout": 5000
        }
    });

    let config = PreprocessConfig::default();
    preprocess_input(&mut input, &config, HarnessType::ClaudeCode);

    assert_eq!(
        input["tool_input"]["command"].as_str().unwrap(),
        "rm -rf .cupcake"
    );
    // Other fields unchanged
    assert_eq!(input["tool_input"]["timeout"].as_i64().unwrap(), 5000);
}

#[test]
fn test_write_content_normalized_to_new_string() {
    // Test that Write's content field is copied to new_string for unified access
    let mut input = json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Write",
        "tool_input": {
            "file_path": "/tmp/test.tsx",
            "content": "<input type=\"date\" />"
        }
    });

    let config = PreprocessConfig::default();
    preprocess_input(&mut input, &config, HarnessType::ClaudeCode);

    // new_string should be added with same value as content
    assert_eq!(
        input["tool_input"]["new_string"].as_str().unwrap(),
        "<input type=\"date\" />",
        "Write's content should be copied to new_string"
    );

    // Original content field should be preserved
    assert_eq!(
        input["tool_input"]["content"].as_str().unwrap(),
        "<input type=\"date\" />",
        "Original content field should be preserved for backwards compatibility"
    );
}

#[test]
fn test_edit_new_string_unchanged() {
    // Test that Edit's existing new_string is not modified
    let mut input = json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Edit",
        "tool_input": {
            "file_path": "/tmp/test.tsx",
            "old_string": "old content",
            "new_string": "new content"
        }
    });

    let config = PreprocessConfig::default();
    preprocess_input(&mut input, &config, HarnessType::ClaudeCode);

    // new_string should remain unchanged
    assert_eq!(
        input["tool_input"]["new_string"].as_str().unwrap(),
        "new content",
        "Edit's new_string should not be modified"
    );

    // old_string should also be unchanged
    assert_eq!(
        input["tool_input"]["old_string"].as_str().unwrap(),
        "old content",
        "Edit's old_string should not be modified"
    );
}

#[test]
fn test_write_edit_unified_field_access() {
    // Test that both Write and Edit can be accessed via new_string
    let config = PreprocessConfig::default();

    // Write input
    let mut write_input = json!({
        "tool_name": "Write",
        "tool_input": {
            "file_path": "/tmp/test.txt",
            "content": "unified content"
        }
    });
    preprocess_input(&mut write_input, &config, HarnessType::ClaudeCode);

    // Edit input
    let mut edit_input = json!({
        "tool_name": "Edit",
        "tool_input": {
            "file_path": "/tmp/test.txt",
            "old_string": "old",
            "new_string": "unified content"
        }
    });
    preprocess_input(&mut edit_input, &config, HarnessType::ClaudeCode);

    // Both should have new_string with the same semantic meaning
    assert_eq!(
        write_input["tool_input"]["new_string"].as_str().unwrap(),
        edit_input["tool_input"]["new_string"].as_str().unwrap(),
        "Both Write and Edit should have new_string with same value for unified access"
    );
}

#[test]
fn test_preprocess_cursor_shell_command() {
    let mut input = json!({
        "hook_event_name": "beforeShellExecution",
        "command": "rm  -rf  .cupcake",
        "cwd": "/tmp"
    });

    let config = PreprocessConfig::default();
    preprocess_input(&mut input, &config, HarnessType::Cursor);

    assert_eq!(input["command"].as_str().unwrap(), "rm -rf .cupcake");
    // Other fields unchanged
    assert_eq!(input["cwd"].as_str().unwrap(), "/tmp");
}

#[test]
fn test_preprocess_preserves_non_bash() {
    let mut input = json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Read",
        "tool_input": {
            "file_path": "test  file.txt"  // Spaces in filename preserved
        }
    });

    let config = PreprocessConfig::default();
    preprocess_input(&mut input, &config, HarnessType::ClaudeCode);

    // Bash-specific preprocessing (whitespace normalization) is not applied
    // But symlink resolution IS applied to all file operations (including Read)
    assert_eq!(
        input["tool_input"]["file_path"].as_str().unwrap(),
        "test  file.txt",
        "Original file_path should be preserved"
    );

    // Symlink resolution metadata should be attached
    assert!(
        input.get("resolved_file_path").is_some(),
        "Should have resolved_file_path from symlink resolution"
    );
    assert!(
        input.get("original_file_path").is_some(),
        "Should have original_file_path from symlink resolution"
    );
    assert!(
        input.get("is_symlink").is_some(),
        "Should have is_symlink flag from symlink resolution"
    );
}

#[test]
fn test_preprocess_disabled() {
    let mut input = json!({
        "tool_name": "Bash",
        "tool_input": {
            "command": "rm  -rf  test"
        }
    });

    let original = input.clone();
    let config = PreprocessConfig {
        normalize_whitespace: false,
        ..Default::default()
    };

    preprocess_input(&mut input, &config, HarnessType::ClaudeCode);

    // No changes when disabled
    assert_eq!(input, original);
}

#[test]
fn test_preprocess_handles_missing_fields() {
    let mut input = json!({
        "tool_name": "Bash",
        // Missing tool_input
    });

    let config = PreprocessConfig::default();
    // Should not panic
    preprocess_input(&mut input, &config, HarnessType::ClaudeCode);
}

#[test]
fn test_preprocess_handles_non_string_command() {
    let mut input = json!({
        "tool_name": "Bash",
        "tool_input": {
            "command": 123  // Not a string
        }
    });

    let config = PreprocessConfig::default();
    // Should not panic
    preprocess_input(&mut input, &config, HarnessType::ClaudeCode);
}
