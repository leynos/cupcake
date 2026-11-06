//! Payload for the Factory AI `Notification` hook event.

use super::CommonFactoryData;
use serde::{Deserialize, Serialize};

/// Payload for Notification hook events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPayload {
    /// Session fields, flattened so they sit at the top level of the hook JSON
    #[serde(flatten)]
    pub common: CommonFactoryData,

    /// The notification message
    pub message: String,
}
