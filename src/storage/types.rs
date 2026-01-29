use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Metadata for a stored conversation session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSession {
    /// Unique identifier for the session
    pub id: String,
    /// User-friendly title (or summary)
    pub title: String,
    /// When the session was created
    pub created_at: DateTime<Utc>,
    /// When the session was last updated
    pub updated_at: DateTime<Utc>,
    /// The model used in the session
    pub model: Option<String>,
    /// Number of messages in the session
    pub message_count: usize,
}
