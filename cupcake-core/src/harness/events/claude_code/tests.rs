//! Tests for Claude Code event deserialization and routing names.

use super::*;
use pretty_assertions::assert_eq;

#[test]
fn test_hook_event_common_data() {
    let event = ClaudeCodeEvent::PreToolUse(PreToolUsePayload {
        common: CommonEventData {
            session_id: "test-session".to_string(),
            transcript_path: "/path/to/transcript".to_string(),
            cwd: "/home/user/project".to_string(),
            permission_mode: Default::default(),
        },
        tool_name: "Bash".to_string(),
        tool_input: serde_json::json!({"command": "ls -la"}),
        tool_use_id: None,
    });

    assert_eq!(event.common().session_id, "test-session");
    assert_eq!(event.common().cwd, "/home/user/project");
    assert_eq!(event.tool_name(), Some("Bash"));
    assert!(event.is_tool_event());
    assert!(!event.is_stop_event());
}

#[test]
fn test_hook_event_deserialization() {
    let json = r#"
        {
            "hook_event_name": "PreToolUse",
            "session_id": "test-session",
            "transcript_path": "/path/to/transcript",
            "cwd": "/home/user/project",
            "tool_name": "Bash",
            "tool_input": {"command": "echo hello"}
        }
        "#;

    let event: ClaudeCodeEvent = serde_json::from_str(json).unwrap();

    match event {
        ClaudeCodeEvent::PreToolUse(payload) => {
            assert_eq!(payload.common.session_id, "test-session");
            assert_eq!(payload.common.cwd, "/home/user/project");
            assert_eq!(payload.tool_name, "Bash");
            assert_eq!(payload.tool_input["command"], "echo hello");
        }
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn test_bash_tool_input_parsing() {
    let event = ClaudeCodeEvent::PreToolUse(PreToolUsePayload {
        common: CommonEventData {
            session_id: "test".to_string(),
            transcript_path: "/path".to_string(),
            cwd: "/home/user/project".to_string(),
            permission_mode: Default::default(),
        },
        tool_name: "Bash".to_string(),
        tool_input: serde_json::json!({
            "command": "cargo build",
            "description": "Build the project",
            "timeout": 60
        }),
        tool_use_id: None,
    });

    let bash_input: BashToolInput = event.parse_tool_input().unwrap();
    assert_eq!(bash_input.command, "cargo build");
    assert_eq!(
        bash_input.description,
        Some("Build the project".to_string())
    );
    assert_eq!(bash_input.timeout, Some(60));
}

#[test]
fn test_compact_trigger_serialization() {
    let trigger = CompactTrigger::Manual;
    let json = serde_json::to_string(&trigger).unwrap();
    assert_eq!(json, r#""manual""#);

    let trigger = CompactTrigger::Auto;
    let json = serde_json::to_string(&trigger).unwrap();
    assert_eq!(json, r#""auto""#);
}

#[test]
fn test_notification_event() {
    let event = ClaudeCodeEvent::Notification(NotificationPayload {
        common: CommonEventData {
            session_id: "test".to_string(),
            transcript_path: "/path".to_string(),
            cwd: "/home/user/project".to_string(),
            permission_mode: Default::default(),
        },
        message: "Test notification".to_string(),
        notification_type: None,
    });

    assert!(!event.is_tool_event());
    assert_eq!(event.tool_name(), None);
}

#[test]
fn test_user_prompt_submit_event() {
    let json = r#"
        {
            "hook_event_name": "UserPromptSubmit",
            "session_id": "test-session",
            "transcript_path": "/path/to/transcript",
            "cwd": "/home/user/project",
            "prompt": "Write a function to calculate factorial"
        }
        "#;

    let event: ClaudeCodeEvent = serde_json::from_str(json).unwrap();

    match &event {
        ClaudeCodeEvent::UserPromptSubmit(payload) => {
            assert_eq!(payload.common.session_id, "test-session");
            assert_eq!(payload.common.cwd, "/home/user/project");
            assert_eq!(payload.prompt, "Write a function to calculate factorial");
            assert_eq!(event.event_name(), "UserPromptSubmit");
        }
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn test_session_start_event() {
    // Test all source types according to Claude Code hooks.md
    let test_cases = vec![
        ("startup", SessionSource::Startup),
        ("resume", SessionSource::Resume),
        ("clear", SessionSource::Clear),
        ("compact", SessionSource::Compact),
    ];

    for (source_str, expected_source) in test_cases {
        let json = format!(
            r#"
                {{
                    "hook_event_name": "SessionStart",
                    "session_id": "test-session",
                    "transcript_path": "~/.claude/projects/.../transcript.jsonl",
                    "cwd": "/home/user/project",
                    "source": "{source_str}"
                }}
                "#
        );

        let event: ClaudeCodeEvent = serde_json::from_str(&json).unwrap();

        match &event {
            ClaudeCodeEvent::SessionStart(payload) => {
                assert_eq!(payload.common.session_id, "test-session");
                assert_eq!(payload.common.cwd, "/home/user/project");
                assert_eq!(event.event_name(), "SessionStart");
                match (&payload.source, &expected_source) {
                    (SessionSource::Startup, SessionSource::Startup) => (),
                    (SessionSource::Resume, SessionSource::Resume) => (),
                    (SessionSource::Clear, SessionSource::Clear) => (),
                    (SessionSource::Compact, SessionSource::Compact) => (),
                    _ => panic!("Source mismatch"),
                }
            }
            _ => panic!("Wrong event type"),
        }
    }
}

#[test]
fn test_permission_request_event() {
    let json = r#"
        {
            "hook_event_name": "PermissionRequest",
            "session_id": "test-session",
            "transcript_path": "/path/to/transcript",
            "cwd": "/home/user/project",
            "tool_name": "Bash",
            "tool_input": {"command": "rm -rf /tmp"},
            "tool_use_id": "toolu_123"
        }
        "#;

    let event: ClaudeCodeEvent = serde_json::from_str(json).unwrap();

    match &event {
        ClaudeCodeEvent::PermissionRequest(payload) => {
            assert_eq!(payload.common.session_id, "test-session");
            assert_eq!(payload.common.cwd, "/home/user/project");
            assert_eq!(payload.tool_name, "Bash");
            assert_eq!(payload.tool_use_id, "toolu_123");
            assert_eq!(payload.tool_input["command"], "rm -rf /tmp");
            assert_eq!(event.event_name(), "PermissionRequest");
            assert!(event.is_tool_event());
            assert!(event.is_permission_event());
        }
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn test_permission_event_helper() {
    // PreToolUse is a permission event
    let pre_tool = ClaudeCodeEvent::PreToolUse(PreToolUsePayload {
        common: CommonEventData {
            session_id: "test".to_string(),
            transcript_path: "/path".to_string(),
            cwd: "/home".to_string(),
            permission_mode: Default::default(),
        },
        tool_name: "Bash".to_string(),
        tool_input: serde_json::json!({"command": "ls"}),
        tool_use_id: None,
    });
    assert!(pre_tool.is_permission_event());

    // PermissionRequest is a permission event
    let perm_req = ClaudeCodeEvent::PermissionRequest(PermissionRequestPayload {
        common: CommonEventData {
            session_id: "test".to_string(),
            transcript_path: "/path".to_string(),
            cwd: "/home".to_string(),
            permission_mode: Default::default(),
        },
        tool_name: "Bash".to_string(),
        tool_input: serde_json::json!({"command": "ls"}),
        tool_use_id: "toolu_xyz".to_string(),
    });
    assert!(perm_req.is_permission_event());

    // PostToolUse is NOT a permission event
    let post_tool = ClaudeCodeEvent::PostToolUse(PostToolUsePayload {
        common: CommonEventData {
            session_id: "test".to_string(),
            transcript_path: "/path".to_string(),
            cwd: "/home".to_string(),
            permission_mode: Default::default(),
        },
        tool_name: "Bash".to_string(),
        tool_input: serde_json::json!({"command": "ls"}),
        tool_response: serde_json::json!({"success": true}),
        tool_use_id: None,
    });
    assert!(!post_tool.is_permission_event());
}
