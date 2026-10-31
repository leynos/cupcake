//! Tests for symlink resolution during preprocessing.

use super::*;
use serde_json::json;

#[test]
fn test_symlink_resolution_claude_code_write() {
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    #[cfg(windows)]
    use std::os::windows::fs::symlink_file as symlink;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let link_path = temp_dir.path().join("link.txt");

    // Create target file
    crate::fs_access::write(&target_path, "test").unwrap();

    // Create symlink
    symlink(&target_path, &link_path).unwrap();

    let mut input = json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Write",
        "tool_input": {
            "file_path": link_path.to_str().unwrap(),
            "content": "data"
        },
        "cwd": temp_dir.path().to_str().unwrap()
    });

    let config = PreprocessConfig::default();
    preprocess_input(&mut input, &config, HarnessType::ClaudeCode);

    // Should have symlink metadata attached
    assert_eq!(input["is_symlink"], json!(true));
    assert!(input.get("resolved_file_path").is_some());
    assert!(input.get("original_file_path").is_some());
}

#[test]
fn test_symlink_resolution_cursor_file_edit() {
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    #[cfg(windows)]
    use std::os::windows::fs::symlink_file as symlink;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let link_path = temp_dir.path().join("link.txt");

    // Create target file
    crate::fs_access::write(&target_path, "test").unwrap();

    // Create symlink
    symlink(&target_path, &link_path).unwrap();

    let mut input = json!({
        "hook_event_name": "beforeFileEdit",
        "file_path": link_path.to_str().unwrap(),
        "cwd": temp_dir.path().to_str().unwrap()
    });

    let config = PreprocessConfig::default();
    preprocess_input(&mut input, &config, HarnessType::Cursor);

    // Should have symlink metadata attached
    assert_eq!(input["is_symlink"], json!(true));
    assert!(input.get("resolved_file_path").is_some());
}

#[test]
fn test_symlink_resolution_disabled() {
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    #[cfg(windows)]
    use std::os::windows::fs::symlink_file as symlink;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let target_path = temp_dir.path().join("target.txt");
    let link_path = temp_dir.path().join("link.txt");

    // Create target and symlink
    crate::fs_access::write(&target_path, "test").unwrap();
    symlink(&target_path, &link_path).unwrap();

    let mut input = json!({
        "tool_name": "Write",
        "tool_input": {
            "file_path": link_path.to_str().unwrap()
        }
    });

    // Disable symlink resolution
    let config = PreprocessConfig {
        enable_symlink_resolution: false,
        ..Default::default()
    };
    preprocess_input(&mut input, &config, HarnessType::ClaudeCode);

    // Should NOT have symlink metadata
    assert!(input.get("is_symlink").is_none());
    assert!(input.get("resolved_file_path").is_none());
}

#[test]
fn test_symlink_resolution_non_symlink() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("regular.txt");

    // Create regular file (not a symlink)
    crate::fs_access::write(&file_path, "test").unwrap();

    let mut input = json!({
        "tool_name": "Read",
        "tool_input": {
            "file_path": file_path.to_str().unwrap()
        }
    });

    let config = PreprocessConfig::default();
    preprocess_input(&mut input, &config, HarnessType::ClaudeCode);

    // Always-on approach: Regular files SHOULD have canonical path metadata
    assert_eq!(
        input["is_symlink"],
        json!(false),
        "Regular file should be marked as NOT a symlink"
    );
    assert!(
        input.get("resolved_file_path").is_some(),
        "Should ALWAYS have canonical path"
    );
}
