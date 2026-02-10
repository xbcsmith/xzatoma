use crate::error::{Result, XzatomaError};
use crate::providers::Message;
use anyhow::Context;
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::PathBuf;

pub mod types;
pub use types::StoredSession;

/// Alias for a deserialized conversation record: (title, model, messages).
type LoadedConversation = (String, Option<String>, Vec<Message>);

/// Storage backend for conversation history
pub struct SqliteStorage {
    db_path: PathBuf,
}

impl SqliteStorage {
    /// Create a new storage instance
    ///
    /// Initializes the database file in the user's data directory.
    pub fn new() -> Result<Self> {
        // Allow override of the history DB path via environment variable.
        // This makes it easy to point the binary at a test DB or alternate file
        // without changing the user's application data dir.
        if let Ok(override_path) = std::env::var("XZATOMA_HISTORY_DB") {
            return Self::new_with_path(override_path);
        }

        let proj_dirs = ProjectDirs::from("com", "xbcsmith", "xzatoma")
            .ok_or_else(|| XzatomaError::Storage("Could not determine data directory".into()))?;

        let data_dir = proj_dirs.data_dir();
        std::fs::create_dir_all(data_dir)
            .context("Failed to create data directory")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        let db_path = data_dir.join("history.db");
        let storage = Self { db_path };

        storage.init()?;

        Ok(storage)
    }

    /// Create a new storage instance that uses the specified database path.
    ///
    /// This is primarily useful for tests where the default application data
    /// directory is not desirable (for example, using a temporary directory).
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::storage::SqliteStorage;
    ///
    /// let storage = SqliteStorage::new_with_path("/tmp/test_history.db").unwrap();
    /// ```
    pub fn new_with_path<P: Into<PathBuf>>(db_path: P) -> Result<Self> {
        let db_path = db_path.into();

        // Ensure parent directory exists so opening the DB file succeeds.
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create parent directory for database")
                .map_err(|e| XzatomaError::Storage(e.to_string()))?;
        }

        let storage = Self { db_path };
        storage.init()?;
        Ok(storage)
    }

    /// Initialize the database schema
    fn init(&self) -> Result<()> {
        let conn = Connection::open(&self.db_path)
            .context("Failed to open database")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                model TEXT,
                messages JSON NOT NULL
            )",
            [],
        )
        .context("Failed to create tables")
        .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Save or update a conversation
    pub fn save_conversation(
        &self,
        id: &str,
        title: &str,
        model: Option<&str>,
        messages: &[Message],
    ) -> Result<()> {
        let mut conn = Connection::open(&self.db_path)
            .context("Failed to open database")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        let messages_json = serde_json::to_string(messages)
            .context("Failed to serialize messages")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        let now = Utc::now().to_rfc3339();

        let tx = conn
            .transaction()
            .context("Failed to start transaction")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        // Check if exists to preserve created_at
        let exists: bool = tx
            .query_row(
                "SELECT 1 FROM conversations WHERE id = ?",
                params![id],
                |_| Ok(true),
            )
            .optional()
            .unwrap_or(Some(false))
            .unwrap_or(false);

        if exists {
            tx.execute(
                "UPDATE conversations SET
                    title = ?,
                    updated_at = ?,
                    model = ?,
                    messages = ?
                WHERE id = ?",
                params![title, now, model, messages_json, id],
            )
            .context("Failed to update conversation")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;
        } else {
            tx.execute(
                "INSERT INTO conversations (id, title, created_at, updated_at, model, messages)
                VALUES (?, ?, ?, ?, ?, ?)",
                params![id, title, now, now, model, messages_json],
            )
            .context("Failed to insert conversation")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;
        }

        tx.commit()
            .context("Failed to commit transaction")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Load a conversation by ID (supports full UUID or 8-char prefix)
    pub fn load_conversation(&self, id: &str) -> Result<Option<LoadedConversation>> {
        let conn = Connection::open(&self.db_path)
            .context("Failed to open database")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        // Support both full UUID and 8-char prefix matching
        let query = if id.len() == 36 {
            // Full UUID provided
            "SELECT title, model, messages FROM conversations WHERE id = ?"
        } else {
            // Prefix matching (e.g., first 8 chars)
            "SELECT title, model, messages FROM conversations WHERE id LIKE ?"
        };

        let search_param = if id.len() == 36 {
            id.to_string()
        } else {
            format!("{}%", id)
        };

        let result = conn
            .query_row(query, params![search_param], |row| {
                let title: String = row.get(0)?;
                let model: Option<String> = row.get(1)?;
                let messages_json: String = row.get(2)?;
                Ok((title, model, messages_json))
            })
            .optional()
            .context("Failed to query conversation")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        match result {
            Some((title, model, messages_json)) => {
                let messages: Vec<Message> = serde_json::from_str(&messages_json)
                    .context("Failed to deserialize messages")
                    .map_err(|e| XzatomaError::Storage(e.to_string()))?;
                Ok(Some((title, model, messages)))
            }
            None => Ok(None),
        }
    }

    /// List all stored sessions
    pub fn list_sessions(&self) -> Result<Vec<StoredSession>> {
        let conn = Connection::open(&self.db_path)
            .context("Failed to open database")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, title, created_at, updated_at, model, messages
                FROM conversations
                ORDER BY updated_at DESC",
            )
            .context("Failed to prepare statement")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        let sessions_iter = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let title: String = row.get(1)?;
                let created_at_str: String = row.get(2)?;
                let updated_at_str: String = row.get(3)?;
                let model: Option<String> = row.get(4)?;
                let messages_json: String = row.get(5)?;

                // Parse dates
                let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()); // Fallback if parsing fails

                let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                // Count messages (optimization: could happen in SQL with json_array_length if supported/enabled)
                // For now, simple parse. If performance is an issue, we can add a 'message_count' column.
                // Or just do a rough count of braces/objects? No, correct way is parse or SQL function.
                // Let's assume JSON parse is efficient enough for list view for now, or just don't parse fully?
                // Actually, `serde_json::Value` would be faster than full struct.
                let message_count =
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&messages_json) {
                        val.as_array().map(|a| a.len()).unwrap_or(0)
                    } else {
                        0
                    };

                Ok(StoredSession {
                    id,
                    title,
                    created_at,
                    updated_at,
                    model,
                    message_count,
                })
            })
            .context("Failed to query sessions")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        let mut sessions = Vec::new();
        for s in sessions_iter.flatten() {
            sessions.push(s);
        }

        Ok(sessions)
    }

    /// Delete a conversation (supports full UUID or 8-char prefix)
    pub fn delete_conversation(&self, id: &str) -> Result<()> {
        let conn = Connection::open(&self.db_path)
            .context("Failed to open database")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        // Support both full UUID and 8-char prefix matching
        let (query, param) = if id.len() == 36 {
            // Full UUID provided
            ("DELETE FROM conversations WHERE id = ?", id.to_string())
        } else {
            // Prefix matching (e.g., first 8 chars)
            (
                "DELETE FROM conversations WHERE id LIKE ?",
                format!("{}%", id),
            )
        };

        conn.execute(query, params![param])
            .context("Failed to delete conversation")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use serial_test::serial;
    use std::env;
    use std::thread::sleep;
    use std::time::Duration;
    use tempfile::tempdir;

    /// Helper: create a temporary storage instance backed by a temp directory.
    ///
    /// Returns both the `SqliteStorage` and the `TempDir` so the caller keeps
    /// ownership of the directory (preventing it from being removed).
    fn create_test_storage() -> (SqliteStorage, tempfile::TempDir) {
        let dir = tempdir().expect("failed to create tempdir");
        let db_path = dir.path().join("history.db");
        let storage = SqliteStorage::new_with_path(db_path).expect("failed to create storage");
        (storage, dir)
    }

    #[test]
    fn test_sqlite_storage_init_creates_table() {
        let (storage, _dir) = create_test_storage();
        let conn = Connection::open(&storage.db_path).expect("open connection");
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='conversations'",
                [],
                |r| r.get(0),
            )
            .expect("query row");
        assert_eq!(count, 1);
    }

    #[test]
    fn test_save_conversation_creates_new_record() {
        let (storage, _dir) = create_test_storage();
        let id = "test-save-1";
        let title = "Test Save 1";
        let model = Some("gpt-4");
        let messages = vec![crate::providers::Message::user("Hello")];

        storage
            .save_conversation(id, title, model, &messages)
            .expect("save failed");

        let loaded = storage.load_conversation(id).expect("load failed");
        assert!(loaded.is_some());

        let (ltitle, lmodel, lmessages) = loaded.unwrap();
        assert_eq!(ltitle, title.to_string());
        assert_eq!(lmodel, model.map(|s| s.to_string()));
        assert_eq!(lmessages.len(), 1);
        assert_eq!(lmessages[0].content.as_deref(), Some("Hello"));
    }

    #[test]
    fn test_save_conversation_updates_existing_record() {
        let (storage, _dir) = create_test_storage();
        let id = "test-update-1";
        let title = "Original Title";
        let messages = vec![crate::providers::Message::user("First")];

        storage
            .save_conversation(id, title, Some("gpt-4"), &messages)
            .expect("initial save failed");

        let sessions = storage.list_sessions().expect("list failed");
        let session = sessions
            .into_iter()
            .find(|s| s.id == id)
            .expect("session found");
        let created_at = session.created_at;
        let updated_at = session.updated_at;

        // Small delay to ensure timestamps differ
        sleep(Duration::from_millis(10));

        let new_title = "Updated Title";
        let new_messages = vec![crate::providers::Message::user("Second")];
        storage
            .save_conversation(id, new_title, Some("gpt-4"), &new_messages)
            .expect("update failed");

        let sessions = storage.list_sessions().expect("list failed 2");
        let session_updated = sessions
            .into_iter()
            .find(|s| s.id == id)
            .expect("session updated found");

        // Created at preserved, updated_at should be later than before
        assert_eq!(session_updated.created_at, created_at);
        assert!(session_updated.updated_at > updated_at);
    }

    #[test]
    fn test_load_conversation_returns_none_for_missing_id() {
        let (storage, _dir) = create_test_storage();
        let res = storage
            .load_conversation("non-existent-id")
            .expect("load failed");
        assert!(res.is_none());
    }

    #[test]
    fn test_list_sessions_returns_ordered_by_updated_at() {
        let (storage, _dir) = create_test_storage();

        let id1 = "session-1";
        storage
            .save_conversation(
                id1,
                "A",
                Some("gpt-4"),
                &[crate::providers::Message::user("a")],
            )
            .expect("save1 failed");

        // Ensure a later updated_at for the second session
        sleep(Duration::from_millis(10));

        let id2 = "session-2";
        storage
            .save_conversation(
                id2,
                "B",
                Some("gpt-4"),
                &[crate::providers::Message::user("b")],
            )
            .expect("save2 failed");

        let sessions = storage.list_sessions().expect("list failed");
        // At least our two sessions should exist in the returned list.
        assert!(sessions.len() >= 2);
        // Sessions are ordered by updated_at DESC: most recent first.
        assert_eq!(sessions[0].id, id2);
        assert_eq!(sessions[1].id, id1);
    }

    #[test]
    fn test_list_sessions_returns_empty_for_new_db() {
        let (storage, _dir) = create_test_storage();
        let sessions = storage.list_sessions().expect("list failed");
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_delete_conversation_removes_record() {
        let (storage, _dir) = create_test_storage();
        let id = "to-delete";
        storage
            .save_conversation(id, "Title", None, &[crate::providers::Message::user("x")])
            .expect("save failed");

        storage.delete_conversation(id).expect("delete failed");
        assert!(storage
            .load_conversation(id)
            .expect("load failed")
            .is_none());
    }

    #[test]
    fn test_delete_conversation_is_idempotent() {
        let (storage, _dir) = create_test_storage();
        let id = "to-delete-2";
        storage
            .save_conversation(id, "Title", None, &[crate::providers::Message::user("x")])
            .expect("save failed");

        storage
            .delete_conversation(id)
            .expect("first delete failed");
        // Second delete should not error (idempotent)
        storage
            .delete_conversation(id)
            .expect("second delete failed");
    }

    #[test]
    fn test_stored_session_calculates_message_count() {
        let (storage, _dir) = create_test_storage();
        let id = "count-1";
        let messages = vec![
            crate::providers::Message::user("a"),
            crate::providers::Message::assistant("b"),
            crate::providers::Message::system("c"),
        ];
        storage
            .save_conversation(id, "Count test", None, &messages)
            .expect("save failed");

        let sessions = storage.list_sessions().expect("list failed");
        let session = sessions
            .into_iter()
            .find(|s| s.id == id)
            .expect("session not found");
        assert_eq!(session.message_count, 3);
    }

    #[test]
    fn test_save_conversation_preserves_created_at_on_update() {
        let (storage, _dir) = create_test_storage();
        let id = "preserve-1";
        storage
            .save_conversation(
                id,
                "Original",
                None,
                &[crate::providers::Message::user("1")],
            )
            .expect("save failed");

        let first = storage
            .list_sessions()
            .expect("list failed")
            .into_iter()
            .find(|s| s.id == id)
            .unwrap();
        let created = first.created_at;

        sleep(Duration::from_millis(10));
        storage
            .save_conversation(id, "Updated", None, &[crate::providers::Message::user("2")])
            .expect("update failed");

        let second = storage
            .list_sessions()
            .expect("list failed 2")
            .into_iter()
            .find(|s| s.id == id)
            .unwrap();
        assert_eq!(second.created_at, created);
    }

    #[test]
    fn test_messages_serialize_deserialize_roundtrip() {
        let messages = vec![
            crate::providers::Message::user("a"),
            crate::providers::Message::assistant("b"),
        ];
        let json = serde_json::to_string(&messages).expect("serialize failed");
        let deserialized: Vec<crate::providers::Message> =
            serde_json::from_str(&json).expect("deserialize failed");
        assert_eq!(deserialized.len(), 2);
        assert_eq!(deserialized[0].role, "user");
        assert_eq!(deserialized[1].role, "assistant");
    }

    #[test]
    fn test_load_conversation_by_full_uuid() {
        let (storage, _dir) = create_test_storage();
        let full_id = "21173421-201f-4e56-87a0-8e13fc02f7e5";
        let title = "Test UUID Load";
        let messages = vec![crate::providers::Message::user("Full UUID test")];

        storage
            .save_conversation(full_id, title, Some("gpt-4"), &messages)
            .expect("save failed");

        let loaded = storage.load_conversation(full_id).expect("load failed");
        assert!(loaded.is_some());

        let (ltitle, _, lmessages) = loaded.unwrap();
        assert_eq!(ltitle, title);
        assert_eq!(lmessages.len(), 1);
    }

    #[test]
    fn test_load_conversation_by_8char_prefix() {
        let (storage, _dir) = create_test_storage();
        let full_id = "abcdef12-3456-7890-abcd-ef1234567890";
        let prefix = "abcdef12";
        let title = "Test Prefix Load";
        let messages = vec![crate::providers::Message::user("Prefix test")];

        storage
            .save_conversation(full_id, title, Some("gpt-4"), &messages)
            .expect("save failed");

        let loaded = storage
            .load_conversation(prefix)
            .expect("load failed by prefix");
        assert!(loaded.is_some());

        let (ltitle, _, lmessages) = loaded.unwrap();
        assert_eq!(ltitle, title);
        assert_eq!(lmessages.len(), 1);
    }

    #[test]
    fn test_delete_conversation_by_8char_prefix() {
        let (storage, _dir) = create_test_storage();
        let full_id = "ffffffff-1234-5678-abcd-ef1234567890";
        let prefix = "ffffffff";

        storage
            .save_conversation(
                full_id,
                "To Delete",
                None,
                &[crate::providers::Message::user("x")],
            )
            .expect("save failed");

        // Delete using prefix
        storage
            .delete_conversation(prefix)
            .expect("delete by prefix failed");

        // Verify it's gone (by full ID)
        assert!(storage
            .load_conversation(full_id)
            .expect("load failed")
            .is_none());

        // And by prefix
        assert!(storage
            .load_conversation(prefix)
            .expect("load failed")
            .is_none());
    }

    #[test]
    #[serial]
    fn test_new_respects_env_override() {
        // Use nested path to ensure parent directory creation is exercised.
        let dir = tempfile::tempdir().expect("failed to create tempdir");
        let db_path = dir.path().join("nested").join("history.db");
        env::set_var("XZATOMA_HISTORY_DB", db_path.to_string_lossy().to_string());

        let storage = SqliteStorage::new().expect("new failed with env override");
        assert_eq!(storage.db_path, db_path);

        // Parent directory should have been created by new_with_path
        assert!(db_path.parent().unwrap().exists());

        env::remove_var("XZATOMA_HISTORY_DB");
    }
}
