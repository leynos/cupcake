//! Tests for the decision and event compatibility matrix.

use super::*;

#[test]
fn test_pre_tool_use_supports_all() {
    let matrix = DecisionEventMatrix::new();

    // PreToolUse should support everything
    for verb in DecisionVerb::all() {
        assert!(
            matrix.is_compatible("PreToolUse", verb),
            "PreToolUse should support {verb:?}"
        );
    }
}

#[test]
fn test_session_start_only_context() {
    let matrix = DecisionEventMatrix::new();

    assert!(matrix.is_compatible("SessionStart", DecisionVerb::AddContext));
    assert!(!matrix.is_compatible("SessionStart", DecisionVerb::Ask));
    assert!(!matrix.is_compatible("SessionStart", DecisionVerb::Block));
    assert!(!matrix.is_compatible("SessionStart", DecisionVerb::Deny));
    assert!(!matrix.is_compatible("SessionStart", DecisionVerb::Halt));
}

#[test]
fn test_post_tool_use_no_ask() {
    let matrix = DecisionEventMatrix::new();

    assert!(!matrix.is_compatible("PostToolUse", DecisionVerb::Ask));
    assert!(matrix.is_compatible("PostToolUse", DecisionVerb::Block));
    assert!(matrix.is_compatible("PostToolUse", DecisionVerb::AddContext));
}

#[test]
fn test_incompatibility_reasons() {
    let matrix = DecisionEventMatrix::new();

    let reason = matrix.incompatibility_reason("SessionStart", DecisionVerb::Ask);
    assert!(reason.contains("SessionStart"));
    assert!(reason.contains("context injection"));

    let reason = matrix.incompatibility_reason("PostToolUse", DecisionVerb::Ask);
    assert!(reason.contains("already executed"));
}

#[test]
fn test_verb_parsing() {
    assert_eq!(DecisionVerb::from_rego_name("ask"), Some(DecisionVerb::Ask));
    assert_eq!(
        DecisionVerb::from_rego_name("add_context"),
        Some(DecisionVerb::AddContext)
    );
    assert_eq!(
        DecisionVerb::from_rego_name("modify"),
        Some(DecisionVerb::Modify)
    );
    assert_eq!(DecisionVerb::from_rego_name("invalid"), None);
}

#[test]
fn test_modify_only_pre_tool_use() {
    let matrix = DecisionEventMatrix::new();

    // Modify should only be supported for PreToolUse
    assert!(matrix.is_compatible("PreToolUse", DecisionVerb::Modify));
    assert!(!matrix.is_compatible("PostToolUse", DecisionVerb::Modify));
    assert!(!matrix.is_compatible("UserPromptSubmit", DecisionVerb::Modify));
    assert!(!matrix.is_compatible("SessionStart", DecisionVerb::Modify));
    assert!(!matrix.is_compatible("Stop", DecisionVerb::Modify));
}

#[test]
fn test_all_events_have_entries() {
    let matrix = DecisionEventMatrix::new();
    let events = vec![
        "PreToolUse",
        "PermissionRequest",
        "PostToolUse",
        "UserPromptSubmit",
        "SessionStart",
        "SessionEnd",
        "Stop",
        "SubagentStop",
        "PreCompact",
        "Notification",
    ];

    for event in events {
        // Should not panic - all events have entries
        let _verbs = matrix.compatible_verbs(event);
    }
}

#[test]
fn test_deprecation_warning_block_pretooluse() {
    let matrix = DecisionEventMatrix::new();

    // block + PreToolUse is deprecated
    let warning = matrix.deprecation_warning("PreToolUse", DecisionVerb::Block);
    assert!(warning.is_some(), "block + PreToolUse should be deprecated");
    assert!(warning.unwrap().contains("deny"));

    // block + PostToolUse is NOT deprecated (correct usage)
    let warning = matrix.deprecation_warning("PostToolUse", DecisionVerb::Block);
    assert!(
        warning.is_none(),
        "block + PostToolUse should NOT be deprecated"
    );

    // deny + PreToolUse is NOT deprecated (correct usage)
    let warning = matrix.deprecation_warning("PreToolUse", DecisionVerb::Deny);
    assert!(
        warning.is_none(),
        "deny + PreToolUse should NOT be deprecated"
    );
}

#[test]
fn test_permission_request_compatibility() {
    let matrix = DecisionEventMatrix::new();

    // PermissionRequest supports: Halt, Deny, Block, Modify
    assert!(matrix.is_compatible("PermissionRequest", DecisionVerb::Halt));
    assert!(matrix.is_compatible("PermissionRequest", DecisionVerb::Deny));
    assert!(matrix.is_compatible("PermissionRequest", DecisionVerb::Block));
    assert!(matrix.is_compatible("PermissionRequest", DecisionVerb::Modify));

    // PermissionRequest does NOT support Ask (it IS the ask dialog)
    assert!(
        !matrix.is_compatible("PermissionRequest", DecisionVerb::Ask),
        "PermissionRequest should NOT support Ask - it IS the ask dialog"
    );

    // PermissionRequest does NOT support AddContext
    assert!(
        !matrix.is_compatible("PermissionRequest", DecisionVerb::AddContext),
        "PermissionRequest should NOT support AddContext"
    );
}

#[test]
fn test_deprecation_warning_block_permission_request() {
    let matrix = DecisionEventMatrix::new();

    // block + PermissionRequest is deprecated (same as PreToolUse)
    let warning = matrix.deprecation_warning("PermissionRequest", DecisionVerb::Block);
    assert!(
        warning.is_some(),
        "block + PermissionRequest should be deprecated"
    );
    assert!(warning.unwrap().contains("deny"));

    // deny + PermissionRequest is NOT deprecated (correct usage)
    let warning = matrix.deprecation_warning("PermissionRequest", DecisionVerb::Deny);
    assert!(
        warning.is_none(),
        "deny + PermissionRequest should NOT be deprecated"
    );
}
