//! Payload for the Cursor `afterFileEdit` hook event.

use serde::{Deserialize, Serialize};

use super::common::CommonCursorData;

/// Individual edit operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEdit {
    /// Text the edit replaces in the file
    pub old_string: String,
    /// Text that takes the place of `old_string`
    pub new_string: String,
}

/// Cursor's afterFileEdit hook event
///
/// Fired after a file has been edited by the agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AfterFileEditPayload {
    /// Conversation, generation, workspace and user fields shared by all Cursor events,
    /// flattened into this JSON object
    #[serde(flatten)]
    pub common: CommonCursorData,

    /// Absolute path to the file that was edited
    pub file_path: String,

    /// Array of edit operations performed
    pub edits: Vec<FileEdit>,
}
