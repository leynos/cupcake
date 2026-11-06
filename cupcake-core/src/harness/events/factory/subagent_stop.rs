//! Payload for the Factory AI `SubagentStop` hook event.

use super::CommonFactoryData;
use serde::{Deserialize, Serialize};

/// Payload for SubagentStop hook events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentStopPayload {
    /// Session, transcript, working directory and permission mode, flattened into the JSON
    /// (camelCase keys)
    #[serde(flatten)]
    pub common: CommonFactoryData,

    /// Whether a stop hook is already active (prevents infinite loops)
    pub stop_hook_active: bool,
}
