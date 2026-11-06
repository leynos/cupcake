//! Payload for the Factory AI `UserPromptSubmit` hook event.

use super::CommonFactoryData;
use serde::{Deserialize, Serialize};

/// Payload for UserPromptSubmit hook events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPromptSubmitPayload {
    /// Session fields, flattened so they sit at the top level of the hook JSON
    #[serde(flatten)]
    pub common: CommonFactoryData,

    /// The user's prompt text
    pub prompt: String,
}
