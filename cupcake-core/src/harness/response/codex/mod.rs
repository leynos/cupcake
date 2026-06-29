//! Codex response translation for Cupcake policy decisions.
//!
//! This module maps Cupcake's synthesized decisions into the event-specific
//! JSON shapes that Codex command hooks accept on stdout.

use crate::engine::decision::FinalDecision;
use crate::harness::events::codex::CodexEvent;
use serde_json::{json, Map, Value};

/// Builds Codex-compatible lifecycle hook responses.
pub struct CodexResponseBuilder;

impl CodexResponseBuilder {
    pub fn build_response(event: &CodexEvent, decision: &FinalDecision) -> Value {
        let event_name = event.event_name();
        match response_kind(event) {
            CodexResponseKind::PreToolUse => build_pre_tool_use(event, decision),
            CodexResponseKind::PermissionRequest => build_permission_request(event_name, decision),
            CodexResponseKind::BlockOrContext => build_block_or_context(event_name, decision),
            CodexResponseKind::ContextOnly => build_context_only(event_name, decision),
            CodexResponseKind::Stop => build_stop(decision),
            CodexResponseKind::Compact => build_universal_stop_only(decision),
        }
    }
}

enum CodexResponseKind {
    PreToolUse,
    PermissionRequest,
    BlockOrContext,
    ContextOnly,
    Stop,
    Compact,
}

fn response_kind(event: &CodexEvent) -> CodexResponseKind {
    match event {
        CodexEvent::PreToolUse(_) => CodexResponseKind::PreToolUse,
        CodexEvent::PermissionRequest(_) => CodexResponseKind::PermissionRequest,
        CodexEvent::PostToolUse(_) | CodexEvent::UserPromptSubmit(_) => {
            CodexResponseKind::BlockOrContext
        }
        CodexEvent::SessionStart(_) | CodexEvent::SubagentStart(_) => {
            CodexResponseKind::ContextOnly
        }
        CodexEvent::Stop(_) | CodexEvent::SubagentStop(_) => CodexResponseKind::Stop,
        CodexEvent::PreCompact(_) | CodexEvent::PostCompact(_) => CodexResponseKind::Compact,
    }
}

fn build_pre_tool_use(event: &CodexEvent, decision: &FinalDecision) -> Value {
    let event_name = event.event_name();
    let payload = match pre_tool_use_outcome(decision) {
        PreToolUseOutcome::Deny { reason } => json!({
            "hookEventName": event_name,
            "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }),
        PreToolUseOutcome::Ask { reason } => json!({
            "hookEventName": event_name,
            "permissionDecision": "ask",
            "permissionDecisionReason": reason,
        }),
        PreToolUseOutcome::Modify {
            reason,
            updated_input,
        } => json!({
            "hookEventName": event_name,
            "permissionDecision": "allow",
            "permissionDecisionReason": reason,
            "updatedInput": normalize_updated_input(event.tool_input(), updated_input),
        }),
        PreToolUseOutcome::AllowWithContext { context } => json!({
            "hookEventName": event_name,
            "permissionDecision": "allow",
            "additionalContext": context,
        }),
        PreToolUseOutcome::Allow => json!({
            "hookEventName": event_name,
            "permissionDecision": "allow",
        }),
    };

    hook_output(payload)
}

fn build_permission_request(event_name: &str, decision: &FinalDecision) -> Value {
    if let Some(reason) = terminal_reason(decision) {
        return hook_output(json!({
            "hookEventName": event_name,
            "decision": {
                "behavior": "deny",
                "message": reason,
            },
        }));
    }

    if matches!(decision, FinalDecision::Ask { .. }) {
        return hook_output(json!({
            "hookEventName": event_name,
        }));
    }

    hook_output(json!({
        "hookEventName": event_name,
        "decision": {
            "behavior": "allow",
        },
    }))
}

fn normalize_updated_input(original_input: Option<&Value>, updated_input: &Value) -> Value {
    let (Some(Value::Object(original)), Value::Object(updated)) = (original_input, updated_input)
    else {
        return updated_input.clone();
    };

    let mut normalized = original.clone();
    overlay_json_object(&mut normalized, updated);

    if let Some(original_command) = original.get("command").filter(|value| value.is_string()) {
        let command_is_string = normalized
            .get("command")
            .is_some_and(|value| value.is_string());
        if !command_is_string {
            normalized.insert("command".to_string(), original_command.clone());
        }
    }

    Value::Object(normalized)
}

fn overlay_json_object(base: &mut Map<String, Value>, update: &Map<String, Value>) {
    for (key, update_value) in update {
        match (base.get_mut(key), update_value) {
            (Some(Value::Object(base_object)), Value::Object(update_object)) => {
                overlay_json_object(base_object, update_object);
            }
            _ => {
                base.insert(key.clone(), update_value.clone());
            }
        }
    }
}

fn build_block_or_context(event_name: &str, decision: &FinalDecision) -> Value {
    if let Some(reason) = blocking_reason(decision) {
        return top_level_block_response(reason);
    }

    if let Some(context) = context_text(decision) {
        return context_output(event_name, context);
    }

    json!({})
}

fn build_context_only(event_name: &str, decision: &FinalDecision) -> Value {
    if let Some(context) = context_text(decision) {
        return context_output(event_name, context);
    }

    json!({})
}

fn build_stop(decision: &FinalDecision) -> Value {
    if let Some(reason) = blocking_reason(decision) {
        return top_level_block_response(reason);
    }

    json!({})
}

fn build_universal_stop_only(decision: &FinalDecision) -> Value {
    if let Some(reason) = blocking_reason(decision) {
        return json!({
            "continue": false,
            "stopReason": reason,
        });
    }

    // Codex defaults to continuing processing when universal output fields are omitted.
    // Compact outputs have no hook-specific context fields, so `{}` is the
    // valid non-blocking response.
    json!({})
}

fn context_output(event_name: &str, context: String) -> Value {
    hook_output(json!({
        "hookEventName": event_name,
        "additionalContext": context,
    }))
}

fn hook_output(hook_specific_output: Value) -> Value {
    json!({ "hookSpecificOutput": hook_specific_output })
}

fn top_level_block_response(reason: &str) -> Value {
    json!({
        "decision": "block",
        "reason": reason,
    })
}

enum PreToolUseOutcome<'a> {
    Deny {
        reason: &'a str,
    },
    Ask {
        reason: &'a str,
    },
    Modify {
        reason: &'a str,
        updated_input: &'a Value,
    },
    AllowWithContext {
        context: String,
    },
    Allow,
}

fn pre_tool_use_outcome(decision: &FinalDecision) -> PreToolUseOutcome<'_> {
    if let Some(reason) = terminal_reason(decision) {
        return PreToolUseOutcome::Deny { reason };
    }

    if let Some(reason) = ask_reason(decision) {
        return PreToolUseOutcome::Ask { reason };
    }

    if let Some((reason, updated_input)) = modification(decision) {
        return PreToolUseOutcome::Modify {
            reason,
            updated_input,
        };
    }

    if let Some(context) = allow_context(decision) {
        return PreToolUseOutcome::AllowWithContext {
            context: join_context(context),
        };
    }

    PreToolUseOutcome::Allow
}

fn terminal_reason(decision: &FinalDecision) -> Option<&str> {
    match decision {
        FinalDecision::Halt { reason, .. }
        | FinalDecision::Deny { reason, .. }
        | FinalDecision::Block { reason, .. } => Some(reason.as_str()),
        _ => None,
    }
}

fn ask_reason(decision: &FinalDecision) -> Option<&str> {
    match decision {
        FinalDecision::Ask { reason, .. } => Some(reason.as_str()),
        _ => None,
    }
}

fn blocking_reason(decision: &FinalDecision) -> Option<&str> {
    terminal_reason(decision).or_else(|| ask_reason(decision))
}

fn modification(decision: &FinalDecision) -> Option<(&str, &Value)> {
    match decision {
        FinalDecision::Modify {
            reason,
            updated_input,
            ..
        } => Some((reason.as_str(), updated_input)),
        _ => None,
    }
}

fn allow_context(decision: &FinalDecision) -> Option<&[String]> {
    match decision {
        FinalDecision::Allow { context } if !context.is_empty() => Some(context),
        _ => None,
    }
}

fn context_text(decision: &FinalDecision) -> Option<String> {
    if let Some((reason, _)) = modification(decision) {
        return Some(reason.to_string());
    }

    allow_context(decision).map(join_context)
}

fn join_context(context: &[String]) -> String {
    context.join("\n")
}
