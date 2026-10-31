//! Tests for merging hook settings and global path discovery.

use super::*;

#[test]
fn test_merge_empty_settings() {
    let mut existing = json!({});
    let new = json!({
        "hooks": {
            "PreToolUse": [{
                "matcher": "*",
                "hooks": [{
                    "type": "command",
                    "command": "cupcake eval --harness claude"
                }]
            }]
        }
    });

    merge_hooks(&mut existing, new.clone()).unwrap();
    assert_eq!(existing["hooks"]["PreToolUse"][0]["matcher"], "*");
}

#[test]
fn test_merge_preserves_existing() {
    let mut existing = json!({
        "env": {"FOO": "bar"},
        "hooks": {
            "PostToolUse": [{
                "matcher": "Write",
                "hooks": [{
                    "type": "command",
                    "command": "echo done"
                }]
            }]
        }
    });

    let new = json!({
        "hooks": {
            "PreToolUse": [{
                "matcher": "*",
                "hooks": [{
                    "type": "command",
                    "command": "cupcake eval --harness claude"
                }]
            }]
        }
    });

    merge_hooks(&mut existing, new).unwrap();

    // Existing settings preserved
    assert_eq!(existing["env"]["FOO"], "bar");
    assert_eq!(existing["hooks"]["PostToolUse"][0]["matcher"], "Write");

    // New hooks added
    assert_eq!(existing["hooks"]["PreToolUse"][0]["matcher"], "*");
}

#[test]
fn test_duplicate_detection() {
    let mut existing = json!({
        "hooks": {
            "PreToolUse": [{
                "matcher": "*",
                "hooks": [{
                    "type": "command",
                    "command": "cupcake eval --policy-dir /path"
                }]
            }]
        }
    });

    let new = json!({
        "hooks": {
            "PreToolUse": [{
                "matcher": "*",
                "hooks": [{
                    "type": "command",
                    "command": "cupcake eval --policy-dir /path"
                }]
            }]
        }
    });

    merge_hooks(&mut existing, new).unwrap();

    // Should not duplicate
    assert_eq!(existing["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
}
