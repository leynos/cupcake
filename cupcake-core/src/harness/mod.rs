// Place this code in cupcake-rewrite/src/harness/mod.rs

pub mod events;
pub mod response;
pub mod types;

use crate::engine::decision::FinalDecision;
use anyhow::Result;
use events::claude_code::ClaudeCodeEvent;
use events::codex::CodexEvent;
use events::cursor::CursorEvent;
use events::factory::FactoryEvent;
use events::opencode::OpenCodeEvent;
use response::{
    ClaudeCodeResponseBuilder, CodexResponseBuilder, CursorResponseBuilder, EngineDecision,
    FactoryResponseBuilder, OpenCodeResponse,
};
use serde_json::Value;

/// The ClaudeHarness - a pure translator
pub struct ClaudeHarness;

/// The CursorHarness - a pure translator for Cursor events
pub struct CursorHarness;

/// The FactoryHarness - a pure translator for Factory AI events
pub struct FactoryHarness;

/// The OpenCodeHarness - a pure translator for OpenCode events
pub struct OpenCodeHarness;

/// The CodexHarness - a pure translator for Codex hook events
pub struct CodexHarness;

impl ClaudeHarness {
    /// Parse the raw hook event from stdin
    pub fn parse_event(input: &str) -> Result<ClaudeCodeEvent> {
        Ok(serde_json::from_str(input)?)
    }

    /// Format the response for this specific harness
    pub fn format_response(event: &ClaudeCodeEvent, decision: &FinalDecision) -> Result<Value> {
        // 1. Convert our new FinalDecision into the old EngineDecision format
        //    that the response builders expect.
        let engine_decision = Self::adapt_decision(decision);

        // 2. Extract context from FinalDecision for the response builder
        let context = Self::extract_context(decision);

        // 3. Use the spec-compliant response builder with extracted context
        let cupcake_response = ClaudeCodeResponseBuilder::build_response(
            &engine_decision,
            event,
            context,
            false, // suppress_output can be made configurable later
        );

        // 3. Convert the final response to a JSON Value.
        Ok(serde_json::to_value(cupcake_response)?)
    }

    /// This is the ADAPTER function. It's the bridge between the new engine
    /// and the old, correct response builders.
    fn adapt_decision(decision: &FinalDecision) -> EngineDecision {
        match decision {
            FinalDecision::Halt { reason, .. } => EngineDecision::Block {
                feedback: reason.clone(),
            },
            FinalDecision::Deny { reason, .. } => EngineDecision::Block {
                feedback: reason.clone(),
            },
            FinalDecision::Block { reason, .. } => EngineDecision::Block {
                feedback: reason.clone(),
            },
            FinalDecision::Ask { reason, .. } => EngineDecision::Ask {
                reason: reason.clone(),
            },
            FinalDecision::Modify {
                reason,
                updated_input,
                ..
            } => EngineDecision::Modify {
                reason: reason.clone(),
                updated_input: updated_input.clone(),
            },
            FinalDecision::Allow { context } => EngineDecision::Allow {
                reason: if !context.is_empty() {
                    Some(context.join("\n"))
                } else {
                    None
                },
            },
        }
    }

    /// Extract context information from FinalDecision for response building
    fn extract_context(decision: &FinalDecision) -> Option<Vec<String>> {
        match decision {
            FinalDecision::Allow { context } => {
                if context.is_empty() {
                    None
                } else {
                    Some(context.clone())
                }
            }
            // All other decision types don't carry additional context
            // The reason is already captured in the EngineDecision
            _ => None,
        }
    }
}

impl CursorHarness {
    /// Parse the raw hook event from stdin (Cursor format)
    pub fn parse_event(input: &str) -> Result<CursorEvent> {
        Ok(serde_json::from_str(input)?)
    }

    /// Format the response for Cursor harness
    ///
    /// IMPORTANT: Cursor has more limited response capabilities:
    /// - beforeSubmitPrompt: Only {continue: true/false} - NO context injection
    /// - beforeReadFile: Only {permission: "allow"|"deny"} - minimal schema
    /// - Other events: Full permission model with messages
    pub fn format_response(event: &CursorEvent, decision: &FinalDecision) -> Result<Value> {
        // 1. Convert FinalDecision to EngineDecision format
        let engine_decision = Self::adapt_decision(decision);

        // 2. Extract agent messages for separate user/agent messaging
        let agent_messages = Self::extract_agent_messages(decision);

        // 3. Use Cursor's response builder with agent messages
        let response =
            CursorResponseBuilder::build_response(&engine_decision, event, agent_messages);

        // 4. Return as JSON Value
        Ok(response)
    }

    /// Adapt FinalDecision to EngineDecision (same logic as ClaudeHarness)
    /// NOTE: Cursor does not support Modify/updatedInput, so Modify is treated as Allow
    fn adapt_decision(decision: &FinalDecision) -> EngineDecision {
        match decision {
            FinalDecision::Halt { reason, .. } => EngineDecision::Block {
                feedback: reason.clone(),
            },
            FinalDecision::Deny { reason, .. } => EngineDecision::Block {
                feedback: reason.clone(),
            },
            FinalDecision::Block { reason, .. } => EngineDecision::Block {
                feedback: reason.clone(),
            },
            FinalDecision::Ask { reason, .. } => EngineDecision::Ask {
                reason: reason.clone(),
            },
            // Cursor doesn't support updatedInput - treat Modify as Allow
            FinalDecision::Modify { reason, .. } => EngineDecision::Allow {
                reason: Some(reason.clone()),
            },
            FinalDecision::Allow { context } => EngineDecision::Allow {
                reason: if !context.is_empty() {
                    Some(context.join("\n"))
                } else {
                    None
                },
            },
        }
    }

    /// Extract agent messages from FinalDecision
    /// These are used by Cursor to populate the agentMessage field separately from userMessage
    fn extract_agent_messages(decision: &FinalDecision) -> Option<Vec<String>> {
        match decision {
            FinalDecision::Halt { agent_messages, .. }
            | FinalDecision::Deny { agent_messages, .. }
            | FinalDecision::Block { agent_messages, .. }
            | FinalDecision::Ask { agent_messages, .. }
            | FinalDecision::Modify { agent_messages, .. } => {
                if agent_messages.is_empty() {
                    None
                } else {
                    Some(agent_messages.clone())
                }
            }
            FinalDecision::Allow { .. } => None,
        }
    }
}

impl FactoryHarness {
    /// Parse the raw hook event from stdin (Factory AI format)
    pub fn parse_event(input: &str) -> Result<FactoryEvent> {
        Ok(serde_json::from_str(input)?)
    }

    /// Format the response for Factory AI harness
    ///
    /// Factory AI supports the same capabilities as Claude Code with additional features:
    /// - updatedInput for PreToolUse (allows modifying tool parameters)
    /// - permission_mode field in all events
    pub fn format_response(event: &FactoryEvent, decision: &FinalDecision) -> Result<Value> {
        // 1. Convert FinalDecision to EngineDecision format
        let engine_decision = Self::adapt_decision(decision);

        // 2. Extract context for separate context injection
        let context = Self::extract_context(decision);

        // 3. Use Factory's response builder with extracted context
        let cupcake_response = FactoryResponseBuilder::build_response(
            &engine_decision,
            event,
            context,
            false, // suppress_output can be made configurable later
        );

        // 4. Return as JSON Value
        Ok(serde_json::to_value(cupcake_response)?)
    }

    /// Adapt FinalDecision to EngineDecision (same logic as Claude and Cursor)
    fn adapt_decision(decision: &FinalDecision) -> EngineDecision {
        match decision {
            FinalDecision::Halt { reason, .. } => EngineDecision::Block {
                feedback: reason.clone(),
            },
            FinalDecision::Deny { reason, .. } => EngineDecision::Block {
                feedback: reason.clone(),
            },
            FinalDecision::Block { reason, .. } => EngineDecision::Block {
                feedback: reason.clone(),
            },
            FinalDecision::Ask { reason, .. } => EngineDecision::Ask {
                reason: reason.clone(),
            },
            FinalDecision::Modify {
                reason,
                updated_input,
                ..
            } => EngineDecision::Modify {
                reason: reason.clone(),
                updated_input: updated_input.clone(),
            },
            FinalDecision::Allow { context } => EngineDecision::Allow {
                reason: if !context.is_empty() {
                    Some(context.join("\n"))
                } else {
                    None
                },
            },
        }
    }

    /// Extract context information from FinalDecision for response building
    fn extract_context(decision: &FinalDecision) -> Option<Vec<String>> {
        match decision {
            FinalDecision::Allow { context } => {
                if context.is_empty() {
                    None
                } else {
                    Some(context.clone())
                }
            }
            // All other decision types don't carry additional context
            _ => None,
        }
    }
}

impl OpenCodeHarness {
    /// Parse the raw hook event from stdin (OpenCode format)
    pub fn parse_event(input: &str) -> Result<OpenCodeEvent> {
        Ok(serde_json::from_str(input)?)
    }

    /// Format the response for OpenCode harness
    ///
    /// OpenCode uses a simple JSON response format:
    /// {
    ///   "decision": "allow"|"deny"|"block"|"ask",
    ///   "reason": "...",
    ///   "context": ["..."]
    /// }
    ///
    /// The TypeScript plugin will interpret this and either:
    /// - Throw an error (deny/block/ask)
    /// - Return normally (allow)
    pub fn format_response(_event: &OpenCodeEvent, decision: &FinalDecision) -> Result<Value> {
        let response = match decision {
            FinalDecision::Halt { reason, .. } => OpenCodeResponse::block(reason.clone()),
            FinalDecision::Deny { reason, .. } => OpenCodeResponse::deny(reason.clone()),
            FinalDecision::Block { reason, .. } => OpenCodeResponse::block(reason.clone()),
            FinalDecision::Ask { reason, .. } => {
                // OpenCode plugin will convert "ask" to deny with approval message
                OpenCodeResponse::ask(reason.clone())
            }
            // OpenCode doesn't support updatedInput - treat Modify as Allow with reason
            FinalDecision::Modify { reason, .. } => {
                OpenCodeResponse::allow_with_context(vec![reason.clone()])
            }
            FinalDecision::Allow { context } => {
                if context.is_empty() {
                    OpenCodeResponse::allow()
                } else {
                    OpenCodeResponse::allow_with_context(context.clone())
                }
            }
        };

        Ok(response.to_json_value())
    }
}

impl CodexHarness {
    /// Parse the raw hook event from stdin (Codex format)
    pub fn parse_event(input: &str) -> Result<CodexEvent> {
        Ok(serde_json::from_str(input)?)
    }

    /// Format the response for the Codex harness.
    pub fn format_response(event: &CodexEvent, decision: &FinalDecision) -> Result<Value> {
        Ok(CodexResponseBuilder::build_response(event, decision))
    }
}

#[cfg(test)]
mod codex_tests {
    use super::*;
    use crate::engine::decision::FinalDecision;
    use crate::harness::events::codex::CodexEvent;
    use serde_json::json;

    #[test]
    fn parses_codex_pre_tool_use_event() {
        let input = r#"{
            "session_id": "thread-1",
            "turn_id": "turn-1",
            "transcript_path": null,
            "cwd": "/tmp/project",
            "hook_event_name": "PreToolUse",
            "model": "gpt-5-codex",
            "permission_mode": "default",
            "tool_name": "Bash",
            "tool_input": {"command": "git status"},
            "tool_use_id": "call-1"
        }"#;

        let event = CodexHarness::parse_event(input).expect("parse Codex event");

        assert!(matches!(event, CodexEvent::PreToolUse(_)));
        assert_eq!(event.event_name(), "PreToolUse");
        assert_eq!(event.common().session_id, "thread-1");
        assert_eq!(event.tool_name(), Some("Bash"));
        assert_eq!(event.tool_input(), Some(&json!({"command": "git status"})));
    }

    #[test]
    fn parses_codex_permission_request_event() {
        let input = r#"{
            "session_id": "thread-1",
            "turn_id": "turn-1",
            "transcript_path": null,
            "cwd": "/tmp/project",
            "hook_event_name": "PermissionRequest",
            "model": "gpt-5-codex",
            "permission_mode": "on-request",
            "tool_name": "Bash",
            "tool_input": {"command": "git push"}
        }"#;

        let event = CodexHarness::parse_event(input).expect("parse Codex event");

        assert!(matches!(event, CodexEvent::PermissionRequest(_)));
        assert_eq!(event.event_name(), "PermissionRequest");
        assert_eq!(event.tool_name(), Some("Bash"));
    }

    #[test]
    fn rejects_codex_subagent_start_without_identity() {
        let input = r#"{
            "session_id": "thread-1",
            "turn_id": "turn-1",
            "transcript_path": null,
            "cwd": "/tmp/project",
            "hook_event_name": "SubagentStart",
            "model": "gpt-5-codex",
            "permission_mode": "default"
        }"#;

        let error = CodexHarness::parse_event(input).expect_err("reject missing subagent identity");

        assert!(
            error
                .to_string()
                .contains("Subagent hooks require agent_id"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_codex_subagent_stop_without_identity() {
        let input = r#"{
            "session_id": "thread-1",
            "turn_id": "turn-1",
            "transcript_path": null,
            "agent_transcript_path": null,
            "cwd": "/tmp/project",
            "hook_event_name": "SubagentStop",
            "model": "gpt-5-codex",
            "permission_mode": "default",
            "stop_hook_active": false,
            "last_assistant_message": null
        }"#;

        let error = CodexHarness::parse_event(input).expect_err("reject missing subagent identity");

        assert!(
            error
                .to_string()
                .contains("Subagent hooks require agent_id"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_codex_subagent_stop_with_blank_identity() {
        let input = r#"{
            "session_id": "thread-1",
            "turn_id": "turn-1",
            "agent_id": "   ",
            "agent_type": "worker",
            "transcript_path": null,
            "agent_transcript_path": null,
            "cwd": "/tmp/project",
            "hook_event_name": "SubagentStop",
            "model": "gpt-5-codex",
            "permission_mode": "default",
            "stop_hook_active": false,
            "last_assistant_message": null
        }"#;

        let error = CodexHarness::parse_event(input).expect_err("reject blank subagent identity");

        assert!(
            error
                .to_string()
                .contains("Subagent hooks require agent_id"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn formats_codex_pre_tool_use_deny_response() {
        let event = CodexHarness::parse_event(
            r#"{
                "session_id": "thread-1",
                "turn_id": "turn-1",
                "transcript_path": null,
                "cwd": "/tmp/project",
                "hook_event_name": "PreToolUse",
                "model": "gpt-5-codex",
                "permission_mode": "default",
                "tool_name": "Bash",
                "tool_input": {"command": "rm -rf target"},
                "tool_use_id": "call-1"
            }"#,
        )
        .expect("parse Codex event");
        let decision = FinalDecision::Deny {
            reason: "blocked by policy".to_string(),
            agent_messages: vec![],
        };

        let response =
            CodexHarness::format_response(&event, &decision).expect("format Codex response");

        assert_eq!(
            response,
            json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": "blocked by policy"
                }
            })
        );
    }

    #[test]
    fn formats_codex_pre_tool_use_ask_response() {
        let event = CodexHarness::parse_event(
            r#"{
                "session_id": "thread-1",
                "turn_id": "turn-1",
                "transcript_path": null,
                "cwd": "/tmp/project",
                "hook_event_name": "PreToolUse",
                "model": "gpt-5-codex",
                "permission_mode": "default",
                "tool_name": "Bash",
                "tool_input": {"command": "git status"},
                "tool_use_id": "call-1"
            }"#,
        )
        .expect("parse Codex event");
        let decision = FinalDecision::Ask {
            reason: "confirm this command".to_string(),
            agent_messages: vec![],
        };

        let response =
            CodexHarness::format_response(&event, &decision).expect("format Codex response");

        assert_eq!(
            response,
            json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "ask",
                    "permissionDecisionReason": "confirm this command"
                }
            })
        );
    }

    #[test]
    fn formats_codex_pre_tool_use_modify_response_with_original_command() {
        let event = CodexHarness::parse_event(
            r#"{
                "session_id": "thread-1",
                "turn_id": "turn-1",
                "transcript_path": null,
                "cwd": "/tmp/project",
                "hook_event_name": "PreToolUse",
                "model": "gpt-5-codex",
                "permission_mode": "default",
                "tool_name": "Bash",
                "tool_input": {"command": "git status", "timeout": 10},
                "tool_use_id": "call-1"
            }"#,
        )
        .expect("parse Codex event");
        let decision = FinalDecision::Modify {
            reason: "adjust timeout".to_string(),
            updated_input: json!({"timeout": 30}),
            agent_messages: vec![],
        };

        let response =
            CodexHarness::format_response(&event, &decision).expect("format Codex response");

        assert_eq!(
            response,
            json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "allow",
                    "permissionDecisionReason": "adjust timeout",
                    "updatedInput": {
                        "command": "git status",
                        "timeout": 30
                    }
                }
            })
        );
    }

    #[test]
    fn formats_codex_permission_request_deny_response() {
        let event = CodexHarness::parse_event(
            r#"{
                "session_id": "thread-1",
                "turn_id": "turn-1",
                "transcript_path": null,
                "cwd": "/tmp/project",
                "hook_event_name": "PermissionRequest",
                "model": "gpt-5-codex",
                "permission_mode": "on-request",
                "tool_name": "Bash",
                "tool_input": {"command": "git push"}
            }"#,
        )
        .expect("parse Codex event");
        let decision = FinalDecision::Block {
            reason: "approval denied by policy".to_string(),
            agent_messages: vec![],
        };

        let response =
            CodexHarness::format_response(&event, &decision).expect("format Codex response");

        assert_eq!(
            response,
            json!({
                "hookSpecificOutput": {
                    "hookEventName": "PermissionRequest",
                    "decision": {
                        "behavior": "deny",
                        "message": "approval denied by policy"
                    }
                }
            })
        );
    }

    #[test]
    fn formats_codex_permission_request_allow_response() {
        let event = CodexHarness::parse_event(
            r#"{
                "session_id": "thread-1",
                "turn_id": "turn-1",
                "transcript_path": null,
                "cwd": "/tmp/project",
                "hook_event_name": "PermissionRequest",
                "model": "gpt-5-codex",
                "permission_mode": "on-request",
                "tool_name": "Bash",
                "tool_input": {"command": "git status"}
            }"#,
        )
        .expect("parse Codex event");
        let decision = FinalDecision::Allow { context: vec![] };

        let response =
            CodexHarness::format_response(&event, &decision).expect("format Codex response");

        assert_eq!(
            response,
            json!({
                "hookSpecificOutput": {
                    "hookEventName": "PermissionRequest",
                    "decision": {
                        "behavior": "allow"
                    }
                }
            })
        );
    }

    #[test]
    fn formats_codex_permission_request_ask_as_no_verdict_response() {
        let event = CodexHarness::parse_event(
            r#"{
                "session_id": "thread-1",
                "turn_id": "turn-1",
                "transcript_path": null,
                "cwd": "/tmp/project",
                "hook_event_name": "PermissionRequest",
                "model": "gpt-5-codex",
                "permission_mode": "on-request",
                "tool_name": "Bash",
                "tool_input": {"command": "git status"}
            }"#,
        )
        .expect("parse Codex event");
        let decision = FinalDecision::Ask {
            reason: "confirm before continuing".to_string(),
            agent_messages: vec![],
        };

        let response =
            CodexHarness::format_response(&event, &decision).expect("format Codex response");

        assert_eq!(
            response,
            json!({
                "hookSpecificOutput": {
                    "hookEventName": "PermissionRequest"
                }
            })
        );
    }

    #[test]
    fn formats_codex_session_start_context_response() {
        let event = CodexHarness::parse_event(
            r#"{
                "session_id": "thread-1",
                "transcript_path": null,
                "cwd": "/tmp/project",
                "hook_event_name": "SessionStart",
                "model": "gpt-5-codex",
                "permission_mode": "default",
                "source": "startup"
            }"#,
        )
        .expect("parse Codex event");
        let decision = FinalDecision::Allow {
            context: vec!["remember policy context".to_string()],
        };

        let response =
            CodexHarness::format_response(&event, &decision).expect("format Codex response");

        assert_eq!(
            response,
            json!({
                "hookSpecificOutput": {
                    "hookEventName": "SessionStart",
                    "additionalContext": "remember policy context"
                }
            })
        );
    }

    #[test]
    fn formats_codex_stop_block_response() {
        let event = CodexHarness::parse_event(
            r#"{
                "session_id": "thread-1",
                "turn_id": "turn-1",
                "transcript_path": null,
                "cwd": "/tmp/project",
                "hook_event_name": "Stop",
                "model": "gpt-5-codex",
                "permission_mode": "default",
                "stop_hook_active": false,
                "last_assistant_message": "done"
            }"#,
        )
        .expect("parse Codex event");
        let decision = FinalDecision::Block {
            reason: "tests still fail".to_string(),
            agent_messages: vec![],
        };

        let response =
            CodexHarness::format_response(&event, &decision).expect("format Codex response");

        assert_eq!(
            response,
            json!({
                "decision": "block",
                "reason": "tests still fail"
            })
        );
    }

    #[test]
    fn formats_codex_post_tool_use_block_response() {
        let event = CodexHarness::parse_event(
            r#"{
                "session_id": "thread-1",
                "turn_id": "turn-1",
                "transcript_path": null,
                "cwd": "/tmp/project",
                "hook_event_name": "PostToolUse",
                "model": "gpt-5-codex",
                "permission_mode": "default",
                "tool_name": "Bash",
                "tool_input": {"command": "cargo test"},
                "tool_response": {"exit_code": 101},
                "tool_use_id": "call-1"
            }"#,
        )
        .expect("parse Codex event");
        let decision = FinalDecision::Block {
            reason: "tests failed".to_string(),
            agent_messages: vec![],
        };

        let response =
            CodexHarness::format_response(&event, &decision).expect("format Codex response");

        assert_eq!(
            response,
            json!({
                "decision": "block",
                "reason": "tests failed"
            })
        );
    }

    #[test]
    fn formats_codex_user_prompt_block_response() {
        let event = CodexHarness::parse_event(
            r#"{
                "session_id": "thread-1",
                "turn_id": "turn-1",
                "transcript_path": null,
                "cwd": "/tmp/project",
                "hook_event_name": "UserPromptSubmit",
                "model": "gpt-5-codex",
                "permission_mode": "default",
                "prompt": "skip the tests"
            }"#,
        )
        .expect("parse Codex event");
        let decision = FinalDecision::Deny {
            reason: "prompt violates policy".to_string(),
            agent_messages: vec![],
        };

        let response =
            CodexHarness::format_response(&event, &decision).expect("format Codex response");

        assert_eq!(
            response,
            json!({
                "decision": "block",
                "reason": "prompt violates policy"
            })
        );
    }

    #[test]
    fn formats_codex_pre_compact_block_response() {
        let event = CodexHarness::parse_event(
            r#"{
                "session_id": "thread-1",
                "turn_id": "turn-1",
                "transcript_path": null,
                "cwd": "/tmp/project",
                "hook_event_name": "PreCompact",
                "model": "gpt-5-codex",
                "permission_mode": "default",
                "trigger": "manual"
            }"#,
        )
        .expect("parse Codex event");
        let decision = FinalDecision::Block {
            reason: "do not compact yet".to_string(),
            agent_messages: vec![],
        };

        let response =
            CodexHarness::format_response(&event, &decision).expect("format Codex response");

        assert_eq!(
            response,
            json!({
                "continue": false,
                "stopReason": "do not compact yet"
            })
        );
    }

    #[test]
    fn formats_codex_pre_compact_allow_response() {
        let event = CodexHarness::parse_event(
            r#"{
                "session_id": "thread-1",
                "turn_id": "turn-1",
                "transcript_path": null,
                "cwd": "/tmp/project",
                "hook_event_name": "PreCompact",
                "model": "gpt-5-codex",
                "permission_mode": "default",
                "trigger": "manual"
            }"#,
        )
        .expect("parse Codex event");
        let decision = FinalDecision::Allow { context: vec![] };

        let response =
            CodexHarness::format_response(&event, &decision).expect("format Codex response");

        assert_eq!(response, json!({}));
    }
}
