//! Response types shared by every harness: the engine decision and the Claude Code response structure.

use serde::{Deserialize, Serialize};

/// The main response structure for Claude Code hooks
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CupcakeResponse {
    /// Event-specific output, serialized as `hookSpecificOutput`. Omitted when `None`.
    #[serde(skip_serializing_if = "Option::is_none", rename = "hookSpecificOutput")]
    pub hook_specific_output: Option<HookSpecificOutput>,

    /// Top-level `decision`. Set to `"block"` for blocked feedback-loop events, otherwise omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,

    /// Explanation paired with `decision`, carrying the block feedback.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// Serialized as `continue`. `Some(false)` stops Claude from continuing; set for
    /// generic-event blocks.
    #[serde(skip_serializing_if = "Option::is_none", rename = "continue")]
    pub continue_execution: Option<bool>,

    /// Message shown when `continue` is `false`, serialized as `stopReason`.
    #[serde(skip_serializing_if = "Option::is_none", rename = "stopReason")]
    pub stop_reason: Option<String>,

    /// Serialized as `suppressOutput`. `Some(true)` asks the harness to hide the hook's
    /// output from the transcript. Set only when the caller requests it.
    #[serde(skip_serializing_if = "Option::is_none", rename = "suppressOutput")]
    pub suppress_output: Option<bool>,

    /// Message shown to the user, serialized as `systemMessage`.
    #[serde(skip_serializing_if = "Option::is_none", rename = "systemMessage")]
    pub system_message: Option<String>,
}

impl CupcakeResponse {
    /// Response with every field unset, which lets the event proceed unchanged.
    pub fn empty() -> Self {
        Self::default()
    }
}

/// Hook-specific output structures
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "hookEventName")]
pub enum HookSpecificOutput {
    /// Permission decision for a tool call (`PreToolUse` hook)
    PreToolUse {
        /// Allow, deny or ask, serialized as `permissionDecision`
        #[serde(rename = "permissionDecision")]
        permission_decision: PermissionDecision,
        /// Explanation for the decision, serialized as `permissionDecisionReason`.
        /// Omitted when `None`.
        #[serde(
            rename = "permissionDecisionReason",
            skip_serializing_if = "Option::is_none"
        )]
        permission_decision_reason: Option<String>,
        /// Updated input for modifying tool parameters (Factory AI specific)
        #[serde(rename = "updatedInput", skip_serializing_if = "Option::is_none")]
        updated_input: Option<serde_json::Value>,
    },
    /// PermissionRequest response (newer API with nested decision object)
    PermissionRequest {
        /// Nested decision object containing behavior, updatedInput, and reason
        decision: PermissionRequestDecision,
    },
    /// Context injected into the prompt (`UserPromptSubmit` hook)
    UserPromptSubmit {
        /// Text added to the conversation, serialized as `additionalContext`
        #[serde(rename = "additionalContext", skip_serializing_if = "Option::is_none")]
        additional_context: Option<String>,
    },
    /// Context injected at session start (`SessionStart` hook)
    SessionStart {
        /// Text added to the session, serialized as `additionalContext`
        #[serde(rename = "additionalContext", skip_serializing_if = "Option::is_none")]
        additional_context: Option<String>,
    },
    /// Context returned after a tool has run (`PostToolUse` hook)
    PostToolUse {
        /// Text added to the conversation, serialized as `additionalContext`
        #[serde(rename = "additionalContext", skip_serializing_if = "Option::is_none")]
        additional_context: Option<String>,
    },
    /// Output for the `PreCompact` hook
    PreCompact {
        /// Extra guidance for the compaction summary, serialized as `customInstructions`
        #[serde(rename = "customInstructions", skip_serializing_if = "Option::is_none")]
        custom_instructions: Option<String>,
    },
}

/// Permission decision for PreToolUse
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionDecision {
    /// Let the tool call run without prompting the user
    Allow,
    /// Block the tool call
    Deny,
    /// Require the user to confirm the tool call
    Ask,
}

/// Decision behavior for PermissionRequest (newer API)
///
/// Only `allow` and `deny` are supported - there is no `ask` because
/// PermissionRequest IS the ask dialog hook (opportunity to bypass user prompt).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionRequestBehavior {
    /// Grant the requested permission, optionally with updated input
    Allow,
    /// Refuse the requested permission; `message` is shown to the model
    Deny,
}

/// Decision object for PermissionRequest hook (nested structure)
///
/// For `allow`: optionally pass `updatedInput` to modify tool parameters
/// For `deny`: optionally pass `message` (shown to model) and `interrupt` (stops Claude)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PermissionRequestDecision {
    /// The permission behavior: allow or deny
    pub behavior: PermissionRequestBehavior,

    /// Optional updated input for modifying tool parameters (used with allow)
    #[serde(rename = "updatedInput", skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<serde_json::Value>,

    /// Message explaining why permission was denied (used with deny)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    /// Whether to interrupt/stop Claude entirely (used with deny)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interrupt: Option<bool>,
}

/// Engine decision - maps our PolicyDecision to response actions
#[derive(Debug, Clone)]
pub enum EngineDecision {
    /// The event may proceed. `reason` is an optional explanation.
    Allow {
        /// Optional explanation attached to the allow result
        reason: Option<String>,
    },
    /// The event is blocked. `feedback` explains why.
    Block {
        /// Explanation for the block, passed through to the harness response
        feedback: String,
    },
    /// The user must confirm before the event proceeds
    Ask {
        /// Explanation shown with the confirmation prompt
        reason: String,
    },
    /// The event proceeds with the tool input replaced by `updated_input`
    Modify {
        /// Explanation for the modification
        reason: String,
        /// Replacement tool input, applied in place of the original parameters
        updated_input: serde_json::Value,
    },
}
