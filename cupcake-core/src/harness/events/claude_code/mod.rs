use serde::{Deserialize, Serialize};

mod notification;
mod permission_request;
mod post_tool_use;
mod pre_compact;
mod pre_tool_use;
mod session_end;
mod session_start;
mod stop;
mod subagent_stop;
mod user_prompt_submit;

pub use notification::{NotificationPayload, NotificationType};
pub use permission_request::PermissionRequestPayload;
pub use post_tool_use::PostToolUsePayload;
pub use pre_compact::PreCompactPayload;
pub use pre_tool_use::PreToolUsePayload;
pub use session_end::{SessionEndPayload, SessionEndReason};
pub use session_start::SessionStartPayload;
pub use stop::StopPayload;
pub use subagent_stop::SubagentStopPayload;
pub use user_prompt_submit::UserPromptSubmitPayload;

/// Trait for all event payloads - ensures access to common data
pub trait EventPayload {
    fn common(&self) -> &CommonEventData;
}

/// Marker trait for events that can inject context via stdout
pub trait InjectsContext {}

/// Permission mode for Claude Code sessions
///
/// Indicates the current permission level for the session, which affects
/// how Claude Code handles tool permissions.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    /// Default permission mode - user is prompted for dangerous operations
    #[default]
    Default,
    /// Plan mode - Claude creates plans without executing
    Plan,
    /// Accept edits mode - file edits are auto-approved
    AcceptEdits,
    /// Bypass permissions - all tool calls auto-approved (dangerous)
    BypassPermissions,
}

impl std::fmt::Display for PermissionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PermissionMode::Default => write!(f, "default"),
            PermissionMode::Plan => write!(f, "plan"),
            PermissionMode::AcceptEdits => write!(f, "acceptEdits"),
            PermissionMode::BypassPermissions => write!(f, "bypassPermissions"),
        }
    }
}

/// Common fields present in all Claude Code hook events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonEventData {
    /// Unique identifier for the Claude Code session
    pub session_id: String,

    /// Path to the session transcript file
    pub transcript_path: String,

    /// Current working directory when the hook is invoked
    pub cwd: String,

    /// Current permission mode for the session
    #[serde(default)]
    pub permission_mode: PermissionMode,
}

/// All possible Claude Code hook events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "hook_event_name", rename_all = "PascalCase")]
pub enum ClaudeCodeEvent {
    /// Before tool execution
    PreToolUse(PreToolUsePayload),

    /// After tool execution (success only)
    PostToolUse(PostToolUsePayload),

    /// Claude Code notifications
    Notification(NotificationPayload),

    /// Main agent stopping
    Stop(StopPayload),

    /// Subagent (Task tool) stopping
    SubagentStop(SubagentStopPayload),

    /// Before memory compaction
    PreCompact(PreCompactPayload),

    /// User prompt submission
    UserPromptSubmit(UserPromptSubmitPayload),

    /// Session start event
    SessionStart(SessionStartPayload),

    /// Session end event
    SessionEnd(SessionEndPayload),

    /// Permission request for tool execution (newer API)
    ///
    /// PermissionRequest is the newer, cleaner hook for tool permission decisions.
    /// It provides the same functionality as PreToolUse but with:
    /// - A `tool_use_id` field for tracking specific tool invocations
    /// - A cleaner response format with nested `decision` object
    PermissionRequest(PermissionRequestPayload),
}

/// Type of compaction trigger
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompactTrigger {
    /// User-initiated via /compact command
    Manual,
    /// Automatic due to full context
    Auto,
}

impl std::fmt::Display for CompactTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompactTrigger::Manual => write!(f, "manual"),
            CompactTrigger::Auto => write!(f, "auto"),
        }
    }
}

/// Source of session start event
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionSource {
    /// Normal startup
    Startup,
    /// Resumed via --resume, --continue, or /resume
    Resume,
    /// After /clear command
    Clear,
    /// After compact (auto or manual)
    Compact,
}

impl std::fmt::Display for SessionSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionSource::Startup => write!(f, "startup"),
            SessionSource::Resume => write!(f, "resume"),
            SessionSource::Clear => write!(f, "clear"),
            SessionSource::Compact => write!(f, "compact"),
        }
    }
}

/// Specific tool input structures for common tools
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashToolInput {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadToolInput {
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteToolInput {
    pub file_path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditToolInput {
    pub file_path: String,
    pub old_string: String,
    pub new_string: String,
    #[serde(default)]
    pub replace_all: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskToolInput {
    pub description: String,
    pub prompt: String,
}

impl ClaudeCodeEvent {
    /// Get the common event data
    pub fn common(&self) -> &CommonEventData {
        match self {
            ClaudeCodeEvent::PreToolUse(payload) => &payload.common,
            ClaudeCodeEvent::PostToolUse(payload) => &payload.common,
            ClaudeCodeEvent::Notification(payload) => &payload.common,
            ClaudeCodeEvent::Stop(payload) => &payload.common,
            ClaudeCodeEvent::SubagentStop(payload) => &payload.common,
            ClaudeCodeEvent::PreCompact(payload) => &payload.common,
            ClaudeCodeEvent::UserPromptSubmit(payload) => &payload.common,
            ClaudeCodeEvent::SessionStart(payload) => &payload.common,
            ClaudeCodeEvent::SessionEnd(payload) => &payload.common,
            ClaudeCodeEvent::PermissionRequest(payload) => &payload.common,
        }
    }

    /// Get the tool name for tool-related events
    pub fn tool_name(&self) -> Option<&str> {
        match self {
            ClaudeCodeEvent::PreToolUse(payload) => Some(&payload.tool_name),
            ClaudeCodeEvent::PostToolUse(payload) => Some(&payload.tool_name),
            ClaudeCodeEvent::PermissionRequest(payload) => Some(&payload.tool_name),
            _ => None,
        }
    }

    /// Get the tool input for tool-related events
    pub fn tool_input(&self) -> Option<&serde_json::Value> {
        match self {
            ClaudeCodeEvent::PreToolUse(payload) => Some(&payload.tool_input),
            ClaudeCodeEvent::PostToolUse(payload) => Some(&payload.tool_input),
            ClaudeCodeEvent::PermissionRequest(payload) => Some(&payload.tool_input),
            _ => None,
        }
    }

    /// Get the event name as a string
    pub fn event_name(&self) -> &'static str {
        match self {
            ClaudeCodeEvent::PreToolUse { .. } => "PreToolUse",
            ClaudeCodeEvent::PostToolUse(_) => "PostToolUse",
            ClaudeCodeEvent::Notification { .. } => "Notification",
            ClaudeCodeEvent::Stop { .. } => "Stop",
            ClaudeCodeEvent::SubagentStop { .. } => "SubagentStop",
            ClaudeCodeEvent::PreCompact { .. } => "PreCompact",
            ClaudeCodeEvent::UserPromptSubmit { .. } => "UserPromptSubmit",
            ClaudeCodeEvent::SessionStart { .. } => "SessionStart",
            ClaudeCodeEvent::SessionEnd { .. } => "SessionEnd",
            ClaudeCodeEvent::PermissionRequest { .. } => "PermissionRequest",
        }
    }

    /// Check if this is a tool-related event
    pub fn is_tool_event(&self) -> bool {
        matches!(
            self,
            ClaudeCodeEvent::PreToolUse(_)
                | ClaudeCodeEvent::PostToolUse(_)
                | ClaudeCodeEvent::PermissionRequest(_)
        )
    }

    /// Check if this is a stop event
    pub fn is_stop_event(&self) -> bool {
        matches!(
            self,
            ClaudeCodeEvent::Stop { .. } | ClaudeCodeEvent::SubagentStop { .. }
        )
    }

    /// Check if this is a permission event (pre-execution decision)
    pub fn is_permission_event(&self) -> bool {
        matches!(
            self,
            ClaudeCodeEvent::PreToolUse(_) | ClaudeCodeEvent::PermissionRequest(_)
        )
    }

    /// Get the permission mode for the session
    pub fn permission_mode(&self) -> &PermissionMode {
        &self.common().permission_mode
    }

    /// Get the tool use ID for tool-related events
    ///
    /// Returns the unique identifier for this tool invocation.
    /// Available for PreToolUse, PostToolUse, and PermissionRequest events.
    pub fn tool_use_id(&self) -> Option<&str> {
        match self {
            ClaudeCodeEvent::PreToolUse(payload) => payload.tool_use_id.as_deref(),
            ClaudeCodeEvent::PostToolUse(payload) => payload.tool_use_id.as_deref(),
            ClaudeCodeEvent::PermissionRequest(payload) => Some(&payload.tool_use_id),
            _ => None,
        }
    }

    /// Parse tool input as specific tool type
    pub fn parse_tool_input<T>(&self) -> Result<T, serde_json::Error>
    where
        T: for<'de> Deserialize<'de>,
    {
        match self.tool_input() {
            Some(input) => serde_json::from_value(input.clone()),
            None => Err(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "No tool input available",
            ))),
        }
    }
}

#[cfg(test)]
mod tests;
