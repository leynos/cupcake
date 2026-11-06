//! Payload for the Cursor `beforeReadFile` hook event.

use serde::{Deserialize, Serialize};

use super::common::CommonCursorData;

/// Attachment information for read file events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    /// Attachment kind, either `file` or `rule`. Serialized as `type`.
    #[serde(rename = "type")]
    pub attachment_type: String,

    /// Path of the attached file or rule file
    pub file_path: String,
}

/// Cursor's beforeReadFile hook event
///
/// Fired before the agent reads a file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeforeReadFilePayload {
    /// Conversation, generation, workspace and user fields shared by all Cursor events,
    /// flattened into this JSON object
    #[serde(flatten)]
    pub common: CommonCursorData,

    /// Absolute path to the file being read
    pub file_path: String,

    /// The content of the file
    pub content: String,

    /// Any attachments (rules) being included
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}
