//! Tests for affected parent directory extraction during preprocessing.

use super::*;
use serde_json::json;

#[test]
fn test_affected_parent_directories_extracted_claude() {
    // Test that rm -rf commands extract affected directories for parent path protection
    let mut input = json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": {
            "command": "rm -rf /home/user/*"
        },
        "cwd": "/tmp"
    });

    let config = PreprocessConfig::default();
    preprocess_input(&mut input, &config, HarnessType::ClaudeCode);

    // Should have affected_parent_directories attached
    let affected = input.get("affected_parent_directories");
    assert!(
        affected.is_some(),
        "Should have affected_parent_directories for rm -rf command"
    );

    let affected_array = affected.unwrap().as_array().unwrap();
    assert!(
        !affected_array.is_empty(),
        "affected_parent_directories should not be empty"
    );

    // The path should be /home/user/ (parent of glob)
    let first_path = affected_array[0].as_str().unwrap();
    assert!(
        first_path.contains("home/user"),
        "Affected path should contain 'home/user', got: {first_path}"
    );
}

#[test]
fn test_affected_parent_directories_extracted_cursor() {
    // Test that Cursor shell commands also extract affected directories
    let mut input = json!({
        "hook_event_name": "beforeShellExecution",
        "command": "rm -rf /var/data/*",
        "cwd": "/tmp"
    });

    let config = PreprocessConfig::default();
    preprocess_input(&mut input, &config, HarnessType::Cursor);

    // Should have affected_parent_directories attached
    let affected = input.get("affected_parent_directories");
    assert!(
        affected.is_some(),
        "Cursor shell commands should have affected_parent_directories"
    );

    let affected_array = affected.unwrap().as_array().unwrap();
    let first_path = affected_array[0].as_str().unwrap();
    assert!(
        first_path.contains("var/data"),
        "Affected path should contain 'var/data', got: {first_path}"
    );
}

#[test]
fn test_paths_extracted_for_all_commands() {
    // All commands with paths get affected_parent_directories
    let mut input = json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": {
            "command": "ls -la /home/user/*"
        },
        "cwd": "/tmp"
    });

    let config = PreprocessConfig::default();
    preprocess_input(&mut input, &config, HarnessType::ClaudeCode);

    // Should have affected_parent_directories for any command with paths
    let affected = input.get("affected_parent_directories");
    assert!(
        affected.is_some(),
        "ls command should have affected_parent_directories"
    );
}

#[test]
fn test_no_paths_for_echo() {
    let mut input = json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": {
            "command": "echo \"hello world\""
        },
        "cwd": "/tmp"
    });

    let config = PreprocessConfig::default();
    preprocess_input(&mut input, &config, HarnessType::ClaudeCode);

    // echo with no paths should not have affected_parent_directories
    assert!(
        input.get("affected_parent_directories").is_none(),
        "echo with no paths should not have affected_parent_directories"
    );
}
