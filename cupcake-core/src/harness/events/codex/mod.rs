//! Typed Codex hook events consumed by the Cupcake harness layer.
//!
//! The structs in this module mirror Codex command-hook JSON so Cupcake can
//! parse stdin into stable Rust types before evaluating policies or formatting
//! Codex-compatible responses.

use serde::{de, Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// Common fields present on Codex hook command input.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommonCodexData {
    pub session_id: String,
    #[serde(default)]
    pub turn_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub agent_type: Option<String>,
    pub transcript_path: Option<String>,
    pub cwd: String,
    pub model: String,
    pub permission_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreToolUsePayload {
    #[serde(flatten)]
    pub common: CommonCodexData,
    pub tool_name: String,
    pub tool_input: Value,
    pub tool_use_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PermissionRequestPayload {
    #[serde(flatten)]
    pub common: CommonCodexData,
    pub tool_name: String,
    pub tool_input: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PostToolUsePayload {
    #[serde(flatten)]
    pub common: CommonCodexData,
    pub tool_name: String,
    pub tool_input: Value,
    pub tool_response: Value,
    pub tool_use_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptPayload {
    #[serde(flatten)]
    pub common: CommonCodexData,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionStartPayload {
    #[serde(flatten)]
    pub common: CommonCodexData,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SubagentStartPayload {
    #[serde(flatten)]
    pub common: CommonCodexData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactPayload {
    #[serde(flatten)]
    pub common: CommonCodexData,
    pub trigger: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StopPayload {
    #[serde(flatten)]
    pub common: CommonCodexData,
    pub stop_hook_active: bool,
    pub last_assistant_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SubagentStopPayload {
    #[serde(flatten)]
    pub common: CommonCodexData,
    pub agent_transcript_path: Option<String>,
    pub stop_hook_active: bool,
    pub last_assistant_message: Option<String>,
}

#[derive(Deserialize)]
struct SubagentStartPayloadWire {
    #[serde(flatten)]
    common: CommonCodexData,
}

#[derive(Deserialize)]
struct SubagentStopPayloadWire {
    #[serde(flatten)]
    common: CommonCodexData,
    agent_transcript_path: Option<String>,
    stop_hook_active: bool,
    last_assistant_message: Option<String>,
}

impl<'de> Deserialize<'de> for SubagentStartPayload {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = SubagentStartPayloadWire::deserialize(deserializer)?;
        validate_subagent_common(&wire.common).map_err(de::Error::custom)?;
        Ok(Self {
            common: wire.common,
        })
    }
}

impl<'de> Deserialize<'de> for SubagentStopPayload {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = SubagentStopPayloadWire::deserialize(deserializer)?;
        validate_subagent_common(&wire.common).map_err(de::Error::custom)?;
        Ok(Self {
            common: wire.common,
            agent_transcript_path: wire.agent_transcript_path,
            stop_hook_active: wire.stop_hook_active,
            last_assistant_message: wire.last_assistant_message,
        })
    }
}

fn validate_subagent_common(common: &CommonCodexData) -> Result<(), &'static str> {
    if !has_subagent_identity_value(common.agent_id.as_deref()) {
        return Err("Subagent hooks require agent_id");
    }

    if !has_subagent_identity_value(common.agent_type.as_deref()) {
        return Err("Subagent hooks require agent_type");
    }

    Ok(())
}

fn has_subagent_identity_value(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.trim().is_empty())
}

/// All supported Codex lifecycle hook events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "hook_event_name")]
pub enum CodexEvent {
    PreToolUse(PreToolUsePayload),
    PermissionRequest(PermissionRequestPayload),
    PostToolUse(PostToolUsePayload),
    PreCompact(CompactPayload),
    PostCompact(CompactPayload),
    SessionStart(SessionStartPayload),
    UserPromptSubmit(PromptPayload),
    SubagentStart(SubagentStartPayload),
    SubagentStop(SubagentStopPayload),
    Stop(StopPayload),
}

impl CodexEvent {
    pub fn common(&self) -> &CommonCodexData {
        self.projection().common
    }

    pub fn event_name(&self) -> &'static str {
        self.projection().event_name
    }

    pub fn tool_name(&self) -> Option<&str> {
        self.projection()
            .tool
            .map(|CodexToolProjection { name, .. }| name)
    }

    pub fn tool_input(&self) -> Option<&Value> {
        self.projection()
            .tool
            .map(|CodexToolProjection { input, .. }| input)
    }

    fn projection(&self) -> CodexEventProjection<'_> {
        match self {
            CodexEvent::PreToolUse(payload) => CodexEventProjection::tool(
                "PreToolUse",
                &payload.common,
                &payload.tool_name,
                &payload.tool_input,
            ),
            CodexEvent::PermissionRequest(payload) => CodexEventProjection::tool(
                "PermissionRequest",
                &payload.common,
                &payload.tool_name,
                &payload.tool_input,
            ),
            CodexEvent::PostToolUse(payload) => CodexEventProjection::tool(
                "PostToolUse",
                &payload.common,
                &payload.tool_name,
                &payload.tool_input,
            ),
            CodexEvent::PreCompact(payload) => {
                CodexEventProjection::lifecycle("PreCompact", &payload.common)
            }
            CodexEvent::PostCompact(payload) => {
                CodexEventProjection::lifecycle("PostCompact", &payload.common)
            }
            CodexEvent::SessionStart(payload) => {
                CodexEventProjection::lifecycle("SessionStart", &payload.common)
            }
            CodexEvent::UserPromptSubmit(payload) => {
                CodexEventProjection::lifecycle("UserPromptSubmit", &payload.common)
            }
            CodexEvent::SubagentStart(payload) => {
                CodexEventProjection::lifecycle("SubagentStart", &payload.common)
            }
            CodexEvent::SubagentStop(payload) => {
                CodexEventProjection::lifecycle("SubagentStop", &payload.common)
            }
            CodexEvent::Stop(payload) => CodexEventProjection::lifecycle("Stop", &payload.common),
        }
    }
}

struct CodexEventProjection<'a> {
    common: &'a CommonCodexData,
    event_name: &'static str,
    tool: Option<CodexToolProjection<'a>>,
}

impl<'a> CodexEventProjection<'a> {
    fn lifecycle(event_name: &'static str, common: &'a CommonCodexData) -> Self {
        Self {
            common,
            event_name,
            tool: None,
        }
    }

    fn tool(
        event_name: &'static str,
        common: &'a CommonCodexData,
        name: &'a str,
        input: &'a Value,
    ) -> Self {
        Self {
            common,
            event_name,
            tool: Some(CodexToolProjection { name, input }),
        }
    }
}

struct CodexToolProjection<'a> {
    name: &'a str,
    input: &'a Value,
}
