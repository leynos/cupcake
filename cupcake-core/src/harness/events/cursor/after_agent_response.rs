//! Payload for the Cursor `afterAgentResponse` hook event.

use serde::{Deserialize, Serialize};

use super::common::CommonCursorData;

/// Cursor's afterAgentResponse hook event
///
/// Called after the agent has completed an assistant message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AfterAgentResponsePayload {
    /// Conversation, generation, workspace and user fields shared by all Cursor events,
    /// flattened into this JSON object
    #[serde(flatten)]
    pub common: CommonCursorData,

    /// The assistant's final text response
    pub text: String,
}
