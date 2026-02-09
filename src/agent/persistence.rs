//! Conversation persistence for debugging and auditing
//!
//! Stores subagent conversation history in an embedded database
//! with support for replay and historical analysis.

use crate::error::{Result, XzatomaError};
use crate::providers::Message;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sled::Db;
use std::path::Path;
use ulid::Ulid;

/// Persisted conversation record
///
/// Represents a single subagent conversation with all its metadata,
/// messages, and execution information.
///
/// # Examples
///
/// ```
/// use xzatoma::agent::ConversationRecord;
///
/// let record = ConversationRecord {
///     id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string(),
///     parent_id: None,
///     label: "analyze_code".to_string(),
///     depth: 1,
///     messages: vec![],
///     started_at: "2025-11-07T18:12:07.982682Z".to_string(),
///     completed_at: None,
///     metadata: Default::default(),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationRecord {
    /// Unique conversation identifier (ULID)
    pub id: String,

    /// Parent conversation ID (if this is a subagent conversation)
    pub parent_id: Option<String>,

    /// Subagent label (from input)
    pub label: String,

    /// Recursion depth (0=root, 1=first subagent, etc.)
    pub depth: usize,

    /// Conversation messages
    pub messages: Vec<Message>,

    /// Start timestamp (RFC-3339)
    pub started_at: String,

    /// End timestamp (RFC-3339), None if still running
    pub completed_at: Option<String>,

    /// Execution metadata
    pub metadata: ConversationMetadata,
}

/// Metadata about a conversation execution
///
/// Captures execution statistics and configuration used for the conversation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConversationMetadata {
    /// Number of conversation turns used
    pub turns_used: usize,

    /// Number of tokens consumed
    pub tokens_consumed: usize,

    /// Final completion status ("complete", "incomplete", "error", etc.)
    pub completion_status: String,

    /// Whether max turns limit was reached
    pub max_turns_reached: bool,

    /// The task prompt given to the subagent
    pub task_prompt: String,

    /// Optional summary prompt for final output
    pub summary_prompt: Option<String>,

    /// List of tools allowed for this subagent
    pub allowed_tools: Vec<String>,
}

/// Conversation persistence manager
///
/// Manages persistent storage and retrieval of subagent conversations
/// using an embedded `sled` key-value database.
pub struct ConversationStore {
    db: Db,
}

impl ConversationStore {
    /// Open or create a conversation store
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the database directory
    ///
    /// # Returns
    ///
    /// Returns a new `ConversationStore` instance
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Storage` if database cannot be opened
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::ConversationStore;
    ///
    /// # fn main() -> xzatoma::error::Result<()> {
    /// let store = ConversationStore::new("/tmp/conversations.db")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let db = sled::open(path)
            .map_err(|e| XzatomaError::Storage(format!("Failed to open database: {}", e)))?;
        Ok(Self { db })
    }

    /// Save a conversation record to the store
    ///
    /// # Arguments
    ///
    /// * `record` - The conversation record to persist
    ///
    /// # Returns
    ///
    /// Returns Ok(()) on success
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Storage` if serialization or insertion fails
    pub fn save(&self, record: &ConversationRecord) -> Result<()> {
        let key = record.id.as_bytes();
        let value = serde_json::to_vec(record)
            .map_err(|e| XzatomaError::Storage(format!("Serialization failed: {}", e)))?;

        self.db
            .insert(key, value)
            .map_err(|e| XzatomaError::Storage(format!("Insert failed: {}", e)))?;

        self.db
            .flush()
            .map_err(|e| XzatomaError::Storage(format!("Flush failed: {}", e)))?;

        Ok(())
    }

    /// Retrieve a conversation record by ID
    ///
    /// # Arguments
    ///
    /// * `id` - The conversation ID to retrieve
    ///
    /// # Returns
    ///
    /// Returns Some(ConversationRecord) if found, None if not found
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Storage` if retrieval or deserialization fails
    pub fn get(&self, id: &str) -> Result<Option<ConversationRecord>> {
        let key = id.as_bytes();
        match self
            .db
            .get(key)
            .map_err(|e| XzatomaError::Storage(format!("Get failed: {}", e)))?
        {
            Some(bytes) => {
                let record = serde_json::from_slice(&bytes)
                    .map_err(|e| XzatomaError::Storage(format!("Deserialization failed: {}", e)))?;
                Ok(Some(record))
            }
            None => Ok(None),
        }
    }

    /// List all conversations with pagination support
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of records to return
    /// * `offset` - Number of records to skip from the beginning
    ///
    /// # Returns
    ///
    /// Returns a vector of conversation records
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Storage` if iteration or deserialization fails
    pub fn list(&self, limit: usize, offset: usize) -> Result<Vec<ConversationRecord>> {
        let mut records = Vec::new();
        for (i, result) in self.db.iter().enumerate() {
            if i < offset {
                continue;
            }
            if records.len() >= limit {
                break;
            }

            let (_, value) =
                result.map_err(|e| XzatomaError::Storage(format!("Iteration failed: {}", e)))?;

            let record: ConversationRecord = serde_json::from_slice(&value)
                .map_err(|e| XzatomaError::Storage(format!("Deserialization failed: {}", e)))?;

            records.push(record);
        }

        Ok(records)
    }

    /// Find all conversations with a specific parent ID
    ///
    /// Used to reconstruct the conversation tree and find all subagent
    /// conversations spawned from a specific parent conversation.
    ///
    /// # Arguments
    ///
    /// * `parent_id` - The parent conversation ID to search for
    ///
    /// # Returns
    ///
    /// Returns a vector of conversation records with matching parent_id
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Storage` if iteration or deserialization fails
    pub fn find_by_parent(&self, parent_id: &str) -> Result<Vec<ConversationRecord>> {
        let mut records = Vec::new();
        for result in self.db.iter() {
            let (_, value) =
                result.map_err(|e| XzatomaError::Storage(format!("Iteration failed: {}", e)))?;

            let record: ConversationRecord = serde_json::from_slice(&value)
                .map_err(|e| XzatomaError::Storage(format!("Deserialization failed: {}", e)))?;

            if record.parent_id.as_deref() == Some(parent_id) {
                records.push(record);
            }
        }

        Ok(records)
    }
}

/// Generate a new ULID for a conversation
///
/// ULIDs (Universally Unique Lexicographically Sortable Identifiers)
/// are preferred over UUIDs as they are sortable by timestamp and more
/// human-readable.
///
/// # Returns
///
/// A new ULID string
///
/// # Examples
///
/// ```
/// use xzatoma::agent::new_conversation_id;
///
/// let id = new_conversation_id();
/// assert!(id.len() > 0);
/// ```
pub fn new_conversation_id() -> String {
    Ulid::new().to_string()
}

/// Get current timestamp in RFC-3339 format
///
/// Used consistently for all conversation timestamps to ensure
/// compatibility with standard time parsing and comparison.
///
/// # Returns
///
/// Current UTC time as RFC-3339 formatted string
///
/// # Examples
///
/// ```
/// use xzatoma::agent::now_rfc3339;
///
/// let timestamp = now_rfc3339();
/// // RFC3339 format: should contain the 'T' separator and be parseable
/// assert!(timestamp.contains("T"));
/// assert!(chrono::DateTime::parse_from_rfc3339(&timestamp).is_ok());
/// ```
pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_conversation_id_generates_valid_ulid() {
        let id = new_conversation_id();
        assert!(!id.is_empty());
        assert_eq!(id.len(), 26); // ULID string length
    }

    #[test]
    fn test_new_conversation_id_is_unique() {
        let id1 = new_conversation_id();
        let id2 = new_conversation_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_now_rfc3339_format() {
        let timestamp = now_rfc3339();
        // RFC3339 format: 2025-11-07T18:12:07.982682+00:00 or ends with Z
        assert!(
            timestamp.contains("T"),
            "Timestamp should contain 'T' separator"
        );
        // Should be parseable as RFC3339
        assert!(chrono::DateTime::parse_from_rfc3339(&timestamp).is_ok());
    }

    #[test]
    fn test_conversation_metadata_default() {
        let metadata = ConversationMetadata::default();
        assert_eq!(metadata.turns_used, 0);
        assert_eq!(metadata.tokens_consumed, 0);
        assert!(metadata.allowed_tools.is_empty());
        assert!(!metadata.max_turns_reached);
    }

    #[test]
    fn test_conversation_record_creation() {
        let record = ConversationRecord {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string(),
            parent_id: None,
            label: "test_task".to_string(),
            depth: 1,
            messages: vec![],
            started_at: "2025-11-07T18:12:07.982682Z".to_string(),
            completed_at: None,
            metadata: ConversationMetadata::default(),
        };

        assert_eq!(record.label, "test_task");
        assert_eq!(record.depth, 1);
        assert!(record.parent_id.is_none());
        assert!(record.completed_at.is_none());
    }

    #[test]
    fn test_conversation_store_new() {
        let temp_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("test.db");

        let result = ConversationStore::new(&db_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_conversation_store_save_and_get() {
        let temp_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("test.db");
        let store = ConversationStore::new(&db_path).expect("Failed to create store");

        let record = ConversationRecord {
            id: new_conversation_id(),
            parent_id: None,
            label: "test".to_string(),
            depth: 0,
            messages: vec![],
            started_at: now_rfc3339(),
            completed_at: None,
            metadata: ConversationMetadata {
                turns_used: 5,
                tokens_consumed: 1000,
                completion_status: "complete".to_string(),
                max_turns_reached: false,
                task_prompt: "analyze this".to_string(),
                summary_prompt: None,
                allowed_tools: vec!["terminal".to_string()],
            },
        };

        let id = record.id.clone();
        store.save(&record).expect("Failed to save record");

        let retrieved = store.get(&id).expect("Failed to get record");
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, id);
        assert_eq!(retrieved.label, "test");
        assert_eq!(retrieved.metadata.turns_used, 5);
    }

    #[test]
    fn test_conversation_store_list() {
        let temp_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("test.db");
        let store = ConversationStore::new(&db_path).expect("Failed to create store");

        // Save multiple records
        for i in 0..5 {
            let record = ConversationRecord {
                id: new_conversation_id(),
                parent_id: None,
                label: format!("task_{}", i),
                depth: 0,
                messages: vec![],
                started_at: now_rfc3339(),
                completed_at: None,
                metadata: ConversationMetadata::default(),
            };
            store.save(&record).expect("Failed to save record");
        }

        let records = store.list(10, 0).expect("Failed to list records");
        assert_eq!(records.len(), 5);
    }

    #[test]
    fn test_conversation_store_list_pagination() {
        let temp_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("test.db");
        let store = ConversationStore::new(&db_path).expect("Failed to create store");

        // Save 5 records
        for i in 0..5 {
            let record = ConversationRecord {
                id: new_conversation_id(),
                parent_id: None,
                label: format!("task_{}", i),
                depth: 0,
                messages: vec![],
                started_at: now_rfc3339(),
                completed_at: None,
                metadata: ConversationMetadata::default(),
            };
            store.save(&record).expect("Failed to save record");
        }

        let page1 = store.list(2, 0).expect("Failed to list page 1");
        assert_eq!(page1.len(), 2);

        let page2 = store.list(2, 2).expect("Failed to list page 2");
        assert_eq!(page2.len(), 2);

        let page3 = store.list(10, 4).expect("Failed to list page 3");
        assert_eq!(page3.len(), 1);
    }

    #[test]
    fn test_conversation_store_find_by_parent() {
        let temp_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("test.db");
        let store = ConversationStore::new(&db_path).expect("Failed to create store");

        let parent_id = new_conversation_id();

        // Save parent record
        let parent = ConversationRecord {
            id: parent_id.clone(),
            parent_id: None,
            label: "parent".to_string(),
            depth: 0,
            messages: vec![],
            started_at: now_rfc3339(),
            completed_at: None,
            metadata: ConversationMetadata::default(),
        };
        store.save(&parent).expect("Failed to save parent");

        // Save child records
        for i in 0..3 {
            let child = ConversationRecord {
                id: new_conversation_id(),
                parent_id: Some(parent_id.clone()),
                label: format!("child_{}", i),
                depth: 1,
                messages: vec![],
                started_at: now_rfc3339(),
                completed_at: None,
                metadata: ConversationMetadata::default(),
            };
            store.save(&child).expect("Failed to save child");
        }

        let children = store
            .find_by_parent(&parent_id)
            .expect("Failed to find children");
        assert_eq!(children.len(), 3);

        for child in children {
            assert_eq!(child.parent_id, Some(parent_id.clone()));
        }
    }

    #[test]
    fn test_conversation_store_find_by_parent_nonexistent() {
        let temp_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("test.db");
        let store = ConversationStore::new(&db_path).expect("Failed to create store");

        let children = store
            .find_by_parent("nonexistent_id")
            .expect("Failed to find children");
        assert_eq!(children.len(), 0);
    }

    #[test]
    fn test_conversation_record_serialization() {
        let record = ConversationRecord {
            id: "test_id".to_string(),
            parent_id: Some("parent_id".to_string()),
            label: "test".to_string(),
            depth: 1,
            messages: vec![],
            started_at: "2025-11-07T18:12:07Z".to_string(),
            completed_at: Some("2025-11-07T18:13:07Z".to_string()),
            metadata: ConversationMetadata::default(),
        };

        let json = serde_json::to_string(&record).expect("Failed to serialize");
        let deserialized: ConversationRecord =
            serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(deserialized.id, record.id);
        assert_eq!(deserialized.parent_id, record.parent_id);
        assert_eq!(deserialized.label, record.label);
    }
}
