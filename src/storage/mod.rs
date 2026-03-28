use crate::acp::runtime::{AcpRuntimeEvent, AcpRuntimeExecuteMode};
use crate::acp::{
    AcpAwaitPayload, AcpEvent, AcpEventKind, AcpRun, AcpRunCreateRequest, AcpRunId, AcpRunOutput,
    AcpRunSession, AcpRunState, AcpRunStatus, AcpSessionId,
};
use crate::error::{Result, XzatomaError};
use crate::providers::Message;
use crate::storage::types::{
    StoredAcpAwaitState, StoredAcpCancellation, StoredAcpRun, StoredAcpRunEvent, StoredAcpSession,
    StoredSession,
};
use anyhow::Context;
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub mod types;
pub use types::{
    StoredAcpAwaitState as PublicStoredAcpAwaitState,
    StoredAcpCancellation as PublicStoredAcpCancellation, StoredAcpRun as PublicStoredAcpRun,
    StoredAcpRunEvent as PublicStoredAcpRunEvent, StoredAcpSession as PublicStoredAcpSession,
    StoredSession as PublicStoredSession,
};

/// Alias for a deserialized conversation record: (title, model, messages).
type LoadedConversation = (String, Option<String>, Vec<Message>);

/// Storage backend for conversation history and ACP persistence.
///
/// This type provides the existing conversation storage surface together with
/// durable ACP session, run, event, await-state, and cancellation persistence
/// backed by the same SQLite database.
///
/// # Examples
///
/// ```
/// use xzatoma::storage::SqliteStorage;
///
/// let storage = SqliteStorage::new_with_path("/tmp/xzatoma_storage_example.db")?;
/// assert!(storage.database_path().ends_with("xzatoma_storage_example.db"));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub struct SqliteStorage {
    db_path: PathBuf,
}

impl SqliteStorage {
    /// Create a new storage instance.
    ///
    /// Initializes the database file in the user's data directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the data directory cannot be determined, created, or
    /// initialized.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzatoma::storage::SqliteStorage;
    ///
    /// let storage = SqliteStorage::new()?;
    /// let _ = storage;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn new() -> Result<Self> {
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
    /// directory is not desirable.
    ///
    /// # Arguments
    ///
    /// * `db_path` - Path to the SQLite database file
    ///
    /// # Errors
    ///
    /// Returns an error if the parent directory cannot be created or the schema
    /// initialization fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::storage::SqliteStorage;
    ///
    /// let storage = SqliteStorage::new_with_path("/tmp/test_history.db")?;
    /// assert!(storage.database_path().ends_with("test_history.db"));
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn new_with_path<P: Into<PathBuf>>(db_path: P) -> Result<Self> {
        let db_path = db_path.into();

        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create parent directory for database")
                .map_err(|e| XzatomaError::Storage(e.to_string()))?;
        }

        let storage = Self { db_path };
        storage.init()?;
        Ok(storage)
    }

    /// Returns the database path used by this storage instance.
    ///
    /// # Returns
    ///
    /// Returns the SQLite database path.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::storage::SqliteStorage;
    ///
    /// let storage = SqliteStorage::new_with_path("/tmp/test_storage_path.db")?;
    /// assert!(storage.database_path().ends_with("test_storage_path.db"));
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn database_path(&self) -> &PathBuf {
        &self.db_path
    }

    /// Initialize the database schema.
    ///
    /// # Errors
    ///
    /// Returns an error if any table or index creation fails.
    fn init(&self) -> Result<()> {
        let conn = Connection::open(&self.db_path)
            .context("Failed to open database")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        conn.execute_batch(
            "
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                model TEXT,
                messages JSON NOT NULL
            );

            CREATE TABLE IF NOT EXISTS acp_sessions (
                session_id TEXT PRIMARY KEY,
                conversation_id TEXT,
                title TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                metadata_json TEXT NOT NULL DEFAULT '{}',
                last_run_id TEXT
            );

            CREATE TABLE IF NOT EXISTS acp_runs (
                run_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                conversation_id TEXT,
                mode TEXT NOT NULL,
                state TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                completed_at TEXT,
                failure_reason TEXT,
                cancellation_reason TEXT,
                await_kind TEXT,
                await_detail TEXT,
                input_json TEXT NOT NULL,
                output_json TEXT NOT NULL,
                metadata_json TEXT NOT NULL DEFAULT '{}',
                FOREIGN KEY(session_id) REFERENCES acp_sessions(session_id)
            );

            CREATE TABLE IF NOT EXISTS acp_run_events (
                run_id TEXT NOT NULL,
                sequence INTEGER NOT NULL,
                kind TEXT NOT NULL,
                created_at TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                terminal INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (run_id, sequence),
                FOREIGN KEY(run_id) REFERENCES acp_runs(run_id)
            );

            CREATE TABLE IF NOT EXISTS acp_await_states (
                run_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                detail TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                resumed_at TEXT,
                resume_payload_json TEXT,
                FOREIGN KEY(run_id) REFERENCES acp_runs(run_id),
                FOREIGN KEY(session_id) REFERENCES acp_sessions(session_id)
            );

            CREATE TABLE IF NOT EXISTS acp_cancellations (
                run_id TEXT PRIMARY KEY,
                requested_at TEXT NOT NULL,
                acknowledged_at TEXT,
                completed_at TEXT,
                reason TEXT,
                acknowledged INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY(run_id) REFERENCES acp_runs(run_id)
            );

            CREATE INDEX IF NOT EXISTS idx_conversations_updated_at
                ON conversations(updated_at DESC);

            CREATE INDEX IF NOT EXISTS idx_acp_sessions_updated_at
                ON acp_sessions(updated_at DESC);

            CREATE INDEX IF NOT EXISTS idx_acp_runs_session_id
                ON acp_runs(session_id);

            CREATE INDEX IF NOT EXISTS idx_acp_runs_updated_at
                ON acp_runs(updated_at DESC);

            CREATE INDEX IF NOT EXISTS idx_acp_run_events_run_id_sequence
                ON acp_run_events(run_id, sequence);

            CREATE INDEX IF NOT EXISTS idx_acp_await_states_session_id
                ON acp_await_states(session_id);
            ",
        )
        .context("Failed to create tables")
        .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Save or update a conversation.
    ///
    /// # Arguments
    ///
    /// * `id` - Conversation identifier
    /// * `title` - Conversation title
    /// * `model` - Optional model name
    /// * `messages` - Serialized conversation messages
    ///
    /// # Errors
    ///
    /// Returns an error if the conversation cannot be persisted.
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

    /// Load a conversation by ID.
    ///
    /// Supports full UUID or prefix matching.
    ///
    /// # Arguments
    ///
    /// * `id` - Full conversation ID or prefix
    ///
    /// # Returns
    ///
    /// Returns the conversation title, optional model, and messages when found.
    ///
    /// # Errors
    ///
    /// Returns an error if the conversation lookup or deserialization fails.
    pub fn load_conversation(&self, id: &str) -> Result<Option<LoadedConversation>> {
        let conn = Connection::open(&self.db_path)
            .context("Failed to open database")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        let query = if id.len() == 36 {
            "SELECT title, model, messages FROM conversations WHERE id = ?"
        } else {
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

    /// List all stored sessions.
    ///
    /// # Returns
    ///
    /// Returns conversation session summaries ordered by last update time.
    ///
    /// # Errors
    ///
    /// Returns an error if session listing fails.
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

                let created_at =
                    parse_rfc3339_to_utc(&created_at_str).unwrap_or_else(|_| Utc::now());
                let updated_at =
                    parse_rfc3339_to_utc(&updated_at_str).unwrap_or_else(|_| Utc::now());

                let message_count =
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&messages_json) {
                        value.as_array().map(|items| items.len()).unwrap_or(0)
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
        for session in sessions_iter.flatten() {
            sessions.push(session);
        }

        Ok(sessions)
    }

    /// Delete a conversation.
    ///
    /// Supports full UUID or prefix matching.
    ///
    /// # Arguments
    ///
    /// * `id` - Full conversation ID or prefix
    ///
    /// # Errors
    ///
    /// Returns an error if deletion fails.
    pub fn delete_conversation(&self, id: &str) -> Result<()> {
        let conn = Connection::open(&self.db_path)
            .context("Failed to open database")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        let (query, param) = if id.len() == 36 {
            ("DELETE FROM conversations WHERE id = ?", id.to_string())
        } else {
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

    /// Save or update an ACP session summary.
    ///
    /// # Arguments
    ///
    /// * `session` - Persisted ACP session metadata
    ///
    /// # Errors
    ///
    /// Returns an error if the ACP session cannot be saved.
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::Utc;
    /// use xzatoma::storage::{types::StoredAcpSession, SqliteStorage};
    ///
    /// let storage = SqliteStorage::new_with_path("/tmp/acp_session_save_example.db")?;
    /// let now = Utc::now();
    /// let session = StoredAcpSession {
    ///     session_id: "session_123".to_string(),
    ///     conversation_id: None,
    ///     title: Some("Example".to_string()),
    ///     created_at: now,
    ///     updated_at: now,
    ///     run_count: 0,
    ///     last_run_id: None,
    ///     metadata: Default::default(),
    /// };
    ///
    /// storage.save_acp_session(&session)?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn save_acp_session(&self, session: &StoredAcpSession) -> Result<()> {
        let mut conn = self.open_connection()?;
        let metadata_json = serialize_metadata(&session.metadata)?;
        let tx = conn
            .transaction()
            .context("Failed to start ACP session transaction")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        tx.execute(
            "
            INSERT INTO acp_sessions (
                session_id,
                conversation_id,
                title,
                created_at,
                updated_at,
                metadata_json,
                last_run_id
            )
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(session_id) DO UPDATE SET
                conversation_id = excluded.conversation_id,
                title = excluded.title,
                updated_at = excluded.updated_at,
                metadata_json = excluded.metadata_json,
                last_run_id = excluded.last_run_id
            ",
            params![
                session.session_id,
                session.conversation_id,
                session.title,
                session.created_at.to_rfc3339(),
                session.updated_at.to_rfc3339(),
                metadata_json,
                session.last_run_id
            ],
        )
        .context("Failed to save ACP session")
        .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        tx.commit()
            .context("Failed to commit ACP session transaction")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Load an ACP session by identifier.
    ///
    /// # Arguments
    ///
    /// * `session_id` - ACP session identifier
    ///
    /// # Returns
    ///
    /// Returns the stored ACP session when found.
    ///
    /// # Errors
    ///
    /// Returns an error if the session cannot be loaded or deserialized.
    pub fn load_acp_session(&self, session_id: &str) -> Result<Option<StoredAcpSession>> {
        let conn = self.open_connection()?;

        let result = conn
            .query_row(
                "
                SELECT
                    session_id,
                    conversation_id,
                    title,
                    created_at,
                    updated_at,
                    metadata_json,
                    last_run_id
                FROM acp_sessions
                WHERE session_id = ?
                ",
                params![session_id],
                |row| {
                    let session_id: String = row.get(0)?;
                    let conversation_id: Option<String> = row.get(1)?;
                    let title: Option<String> = row.get(2)?;
                    let created_at: String = row.get(3)?;
                    let updated_at: String = row.get(4)?;
                    let metadata_json: String = row.get(5)?;
                    let last_run_id: Option<String> = row.get(6)?;
                    Ok((
                        session_id,
                        conversation_id,
                        title,
                        created_at,
                        updated_at,
                        metadata_json,
                        last_run_id,
                    ))
                },
            )
            .optional()
            .context("Failed to load ACP session")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        match result {
            Some((
                session_id,
                conversation_id,
                title,
                created_at,
                updated_at,
                metadata_json,
                last_run_id,
            )) => {
                let created_at = parse_rfc3339_to_utc(&created_at)?;
                let updated_at = parse_rfc3339_to_utc(&updated_at)?;
                let metadata = deserialize_metadata(&metadata_json)?;
                let run_count = self.count_acp_runs_for_session(&session_id)?;

                Ok(Some(StoredAcpSession {
                    session_id,
                    conversation_id,
                    title,
                    created_at,
                    updated_at,
                    run_count,
                    last_run_id,
                    metadata,
                }))
            }
            None => Ok(None),
        }
    }

    /// Save or update an ACP run record.
    ///
    /// # Arguments
    ///
    /// * `run` - Persisted ACP run summary
    ///
    /// # Errors
    ///
    /// Returns an error if the run cannot be persisted.
    pub fn save_acp_run(&self, run: &StoredAcpRun) -> Result<()> {
        let mut conn = self.open_connection()?;
        let metadata_json = serialize_metadata(&run.metadata)?;
        let tx = conn
            .transaction()
            .context("Failed to start ACP run transaction")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        tx.execute(
            "
            INSERT INTO acp_runs (
                run_id,
                session_id,
                conversation_id,
                mode,
                state,
                created_at,
                updated_at,
                completed_at,
                failure_reason,
                cancellation_reason,
                await_kind,
                await_detail,
                input_json,
                output_json,
                metadata_json
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(run_id) DO UPDATE SET
                session_id = excluded.session_id,
                conversation_id = excluded.conversation_id,
                mode = excluded.mode,
                state = excluded.state,
                updated_at = excluded.updated_at,
                completed_at = excluded.completed_at,
                failure_reason = excluded.failure_reason,
                cancellation_reason = excluded.cancellation_reason,
                await_kind = excluded.await_kind,
                await_detail = excluded.await_detail,
                input_json = excluded.input_json,
                output_json = excluded.output_json,
                metadata_json = excluded.metadata_json
            ",
            params![
                run.run_id,
                run.session_id,
                run.conversation_id,
                run.mode,
                run.state,
                run.created_at.to_rfc3339(),
                run.updated_at.to_rfc3339(),
                run.completed_at.map(|value| value.to_rfc3339()),
                run.failure_reason,
                run.cancellation_reason,
                run.await_kind,
                run.await_detail,
                run.input_json,
                run.output_json,
                metadata_json,
            ],
        )
        .context("Failed to save ACP run")
        .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        tx.execute(
            "
            UPDATE acp_sessions
            SET updated_at = ?, last_run_id = ?
            WHERE session_id = ?
            ",
            params![run.updated_at.to_rfc3339(), run.run_id, run.session_id],
        )
        .context("Failed to update ACP session last_run_id")
        .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        tx.commit()
            .context("Failed to commit ACP run transaction")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Load an ACP run by identifier.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    ///
    /// # Returns
    ///
    /// Returns the stored ACP run when found.
    ///
    /// # Errors
    ///
    /// Returns an error if the run cannot be loaded or deserialized.
    pub fn load_acp_run(&self, run_id: &str) -> Result<Option<StoredAcpRun>> {
        let conn = self.open_connection()?;

        let result = conn
            .query_row(
                "
                SELECT
                    run_id,
                    session_id,
                    conversation_id,
                    mode,
                    state,
                    created_at,
                    updated_at,
                    completed_at,
                    failure_reason,
                    cancellation_reason,
                    await_kind,
                    await_detail,
                    input_json,
                    output_json,
                    metadata_json
                FROM acp_runs
                WHERE run_id = ?
                ",
                params![run_id],
                |row| {
                    Ok(StoredAcpRun {
                        run_id: row.get(0)?,
                        session_id: row.get(1)?,
                        conversation_id: row.get(2)?,
                        mode: row.get(3)?,
                        state: row.get(4)?,
                        created_at: parse_rfc3339_to_utc(&row.get::<_, String>(5)?)
                            .map_err(to_rusqlite_error)?,
                        updated_at: parse_rfc3339_to_utc(&row.get::<_, String>(6)?)
                            .map_err(to_rusqlite_error)?,
                        completed_at: match row.get::<_, Option<String>>(7)? {
                            Some(value) => {
                                Some(parse_rfc3339_to_utc(&value).map_err(to_rusqlite_error)?)
                            }
                            None => None,
                        },
                        failure_reason: row.get(8)?,
                        cancellation_reason: row.get(9)?,
                        await_kind: row.get(10)?,
                        await_detail: row.get(11)?,
                        input_json: row.get(12)?,
                        output_json: row.get(13)?,
                        metadata: deserialize_metadata(&row.get::<_, String>(14)?)
                            .map_err(to_rusqlite_error)?,
                    })
                },
            )
            .optional()
            .context("Failed to load ACP run")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        Ok(result)
    }

    /// List persisted ACP runs for a session.
    ///
    /// # Arguments
    ///
    /// * `session_id` - ACP session identifier
    ///
    /// # Returns
    ///
    /// Returns runs ordered by creation time.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub fn list_acp_runs_for_session(&self, session_id: &str) -> Result<Vec<StoredAcpRun>> {
        let conn = self.open_connection()?;
        let mut stmt = conn
            .prepare(
                "
                SELECT
                    run_id,
                    session_id,
                    conversation_id,
                    mode,
                    state,
                    created_at,
                    updated_at,
                    completed_at,
                    failure_reason,
                    cancellation_reason,
                    await_kind,
                    await_detail,
                    input_json,
                    output_json,
                    metadata_json
                FROM acp_runs
                WHERE session_id = ?
                ORDER BY created_at ASC
                ",
            )
            .context("Failed to prepare ACP runs statement")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        let iter = stmt
            .query_map(params![session_id], |row| {
                Ok(StoredAcpRun {
                    run_id: row.get(0)?,
                    session_id: row.get(1)?,
                    conversation_id: row.get(2)?,
                    mode: row.get(3)?,
                    state: row.get(4)?,
                    created_at: parse_rfc3339_to_utc(&row.get::<_, String>(5)?)
                        .map_err(to_rusqlite_error)?,
                    updated_at: parse_rfc3339_to_utc(&row.get::<_, String>(6)?)
                        .map_err(to_rusqlite_error)?,
                    completed_at: match row.get::<_, Option<String>>(7)? {
                        Some(value) => {
                            Some(parse_rfc3339_to_utc(&value).map_err(to_rusqlite_error)?)
                        }
                        None => None,
                    },
                    failure_reason: row.get(8)?,
                    cancellation_reason: row.get(9)?,
                    await_kind: row.get(10)?,
                    await_detail: row.get(11)?,
                    input_json: row.get(12)?,
                    output_json: row.get(13)?,
                    metadata: deserialize_metadata(&row.get::<_, String>(14)?)
                        .map_err(to_rusqlite_error)?,
                })
            })
            .context("Failed to query ACP runs")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        let mut runs = Vec::new();
        for run in iter.flatten() {
            runs.push(run);
        }
        Ok(runs)
    }

    /// Save or replace ACP run events for a run.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    /// * `events` - Ordered ACP runtime events
    ///
    /// # Errors
    ///
    /// Returns an error if event persistence fails.
    pub fn save_acp_run_events(&self, run_id: &str, events: &[StoredAcpRunEvent]) -> Result<()> {
        let mut conn = self.open_connection()?;
        let tx = conn
            .transaction()
            .context("Failed to start ACP event transaction")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        tx.execute(
            "DELETE FROM acp_run_events WHERE run_id = ?",
            params![run_id],
        )
        .context("Failed to clear existing ACP events")
        .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        for event in events {
            tx.execute(
                "
                INSERT INTO acp_run_events (
                    run_id,
                    sequence,
                    kind,
                    created_at,
                    payload_json,
                    terminal
                )
                VALUES (?, ?, ?, ?, ?, ?)
                ",
                params![
                    event.run_id,
                    i64::try_from(event.sequence).map_err(|error| {
                        XzatomaError::Storage(format!(
                            "ACP event sequence overflow for run '{}': {}",
                            event.run_id, error
                        ))
                    })?,
                    event.kind,
                    event.created_at.to_rfc3339(),
                    event.payload_json,
                    bool_to_sqlite(event.terminal),
                ],
            )
            .context("Failed to insert ACP event")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;
        }

        tx.commit()
            .context("Failed to commit ACP event transaction")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Load ACP run events for a run.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    ///
    /// # Returns
    ///
    /// Returns persisted run events ordered by sequence.
    ///
    /// # Errors
    ///
    /// Returns an error if event loading fails.
    pub fn load_acp_run_events(&self, run_id: &str) -> Result<Vec<StoredAcpRunEvent>> {
        let conn = self.open_connection()?;
        let mut stmt = conn
            .prepare(
                "
                SELECT
                    run_id,
                    sequence,
                    kind,
                    created_at,
                    payload_json,
                    terminal
                FROM acp_run_events
                WHERE run_id = ?
                ORDER BY sequence ASC
                ",
            )
            .context("Failed to prepare ACP event query")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        let iter = stmt
            .query_map(params![run_id], |row| {
                Ok(StoredAcpRunEvent {
                    run_id: row.get(0)?,
                    sequence: {
                        let value: i64 = row.get(1)?;
                        u64::try_from(value).map_err(|error| {
                            to_rusqlite_error(anyhow::Error::from(XzatomaError::Storage(format!(
                                "Invalid persisted ACP event sequence for run '{}': {}",
                                run_id, error
                            ))))
                        })?
                    },
                    kind: row.get(2)?,
                    created_at: parse_rfc3339_to_utc(&row.get::<_, String>(3)?)
                        .map_err(to_rusqlite_error)?,
                    payload_json: row.get(4)?,
                    terminal: sqlite_to_bool(row.get::<_, i64>(5)?),
                })
            })
            .context("Failed to load ACP run events")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        let mut events = Vec::new();
        for event in iter.flatten() {
            events.push(event);
        }
        Ok(events)
    }

    /// Save or update ACP await state.
    ///
    /// # Arguments
    ///
    /// * `await_state` - Persisted await state
    ///
    /// # Errors
    ///
    /// Returns an error if the await state cannot be persisted.
    pub fn save_acp_await_state(&self, await_state: &StoredAcpAwaitState) -> Result<()> {
        let conn = self.open_connection()?;
        conn.execute(
            "
            INSERT INTO acp_await_states (
                run_id,
                session_id,
                kind,
                detail,
                created_at,
                updated_at,
                resumed_at,
                resume_payload_json
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(run_id) DO UPDATE SET
                session_id = excluded.session_id,
                kind = excluded.kind,
                detail = excluded.detail,
                updated_at = excluded.updated_at,
                resumed_at = excluded.resumed_at,
                resume_payload_json = excluded.resume_payload_json
            ",
            params![
                await_state.run_id,
                await_state.session_id,
                await_state.kind,
                await_state.detail,
                await_state.created_at.to_rfc3339(),
                await_state.updated_at.to_rfc3339(),
                await_state.resumed_at.map(|value| value.to_rfc3339()),
                await_state.resume_payload_json,
            ],
        )
        .context("Failed to save ACP await state")
        .map_err(|e| XzatomaError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Load ACP await state for a run.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    ///
    /// # Returns
    ///
    /// Returns the stored await state when present.
    ///
    /// # Errors
    ///
    /// Returns an error if loading fails.
    pub fn load_acp_await_state(&self, run_id: &str) -> Result<Option<StoredAcpAwaitState>> {
        let conn = self.open_connection()?;

        let result = conn
            .query_row(
                "
                SELECT
                    run_id,
                    session_id,
                    kind,
                    detail,
                    created_at,
                    updated_at,
                    resumed_at,
                    resume_payload_json
                FROM acp_await_states
                WHERE run_id = ?
                ",
                params![run_id],
                |row| {
                    Ok(StoredAcpAwaitState {
                        run_id: row.get(0)?,
                        session_id: row.get(1)?,
                        kind: row.get(2)?,
                        detail: row.get(3)?,
                        created_at: parse_rfc3339_to_utc(&row.get::<_, String>(4)?)
                            .map_err(to_rusqlite_error)?,
                        updated_at: parse_rfc3339_to_utc(&row.get::<_, String>(5)?)
                            .map_err(to_rusqlite_error)?,
                        resumed_at: match row.get::<_, Option<String>>(6)? {
                            Some(value) => {
                                Some(parse_rfc3339_to_utc(&value).map_err(to_rusqlite_error)?)
                            }
                            None => None,
                        },
                        resume_payload_json: row.get(7)?,
                    })
                },
            )
            .optional()
            .context("Failed to load ACP await state")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        Ok(result)
    }

    /// Save or update ACP cancellation state.
    ///
    /// # Arguments
    ///
    /// * `cancellation` - Persisted cancellation state
    ///
    /// # Errors
    ///
    /// Returns an error if the cancellation state cannot be saved.
    pub fn save_acp_cancellation(&self, cancellation: &StoredAcpCancellation) -> Result<()> {
        let conn = self.open_connection()?;
        conn.execute(
            "
            INSERT INTO acp_cancellations (
                run_id,
                requested_at,
                acknowledged_at,
                completed_at,
                reason,
                acknowledged
            )
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(run_id) DO UPDATE SET
                requested_at = excluded.requested_at,
                acknowledged_at = excluded.acknowledged_at,
                completed_at = excluded.completed_at,
                reason = excluded.reason,
                acknowledged = excluded.acknowledged
            ",
            params![
                cancellation.run_id,
                cancellation.requested_at.to_rfc3339(),
                cancellation.acknowledged_at.map(|value| value.to_rfc3339()),
                cancellation.completed_at.map(|value| value.to_rfc3339()),
                cancellation.reason,
                bool_to_sqlite(cancellation.acknowledged),
            ],
        )
        .context("Failed to save ACP cancellation")
        .map_err(|e| XzatomaError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Load ACP cancellation state for a run.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    ///
    /// # Returns
    ///
    /// Returns the stored cancellation state when present.
    ///
    /// # Errors
    ///
    /// Returns an error if loading fails.
    pub fn load_acp_cancellation(&self, run_id: &str) -> Result<Option<StoredAcpCancellation>> {
        let conn = self.open_connection()?;

        let result = conn
            .query_row(
                "
                SELECT
                    run_id,
                    requested_at,
                    acknowledged_at,
                    completed_at,
                    reason,
                    acknowledged
                FROM acp_cancellations
                WHERE run_id = ?
                ",
                params![run_id],
                |row| {
                    Ok(StoredAcpCancellation {
                        run_id: row.get(0)?,
                        requested_at: parse_rfc3339_to_utc(&row.get::<_, String>(1)?)
                            .map_err(to_rusqlite_error)?,
                        acknowledged_at: match row.get::<_, Option<String>>(2)? {
                            Some(value) => {
                                Some(parse_rfc3339_to_utc(&value).map_err(to_rusqlite_error)?)
                            }
                            None => None,
                        },
                        completed_at: match row.get::<_, Option<String>>(3)? {
                            Some(value) => {
                                Some(parse_rfc3339_to_utc(&value).map_err(to_rusqlite_error)?)
                            }
                            None => None,
                        },
                        reason: row.get(4)?,
                        acknowledged: sqlite_to_bool(row.get::<_, i64>(5)?),
                    })
                },
            )
            .optional()
            .context("Failed to load ACP cancellation")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        Ok(result)
    }

    /// Persist a canonical ACP run and related durable session linkage.
    ///
    /// This helper converts the canonical ACP runtime run shape into the durable
    /// storage records used by the SQLite schema.
    ///
    /// # Arguments
    ///
    /// * `run` - Canonical ACP run
    /// * `mode` - Effective runtime mode
    /// * `conversation_id` - Optional mapped conversation identifier
    ///
    /// # Errors
    ///
    /// Returns an error if serialization or persistence fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::{AcpMessage, AcpMessagePart, AcpRole, AcpRun, AcpRunCreateRequest, AcpRunId, AcpRunSession, AcpSessionId, AcpTextPart};
    /// use xzatoma::acp::runtime::AcpRuntimeExecuteMode;
    /// use xzatoma::storage::SqliteStorage;
    ///
    /// let storage = SqliteStorage::new_with_path("/tmp/persist_acp_run_example.db")?;
    /// let session_id = AcpSessionId::new("session_123".to_string())?;
    /// let session = AcpRunSession::new(session_id.clone())?;
    /// let request = AcpRunCreateRequest::new(
    ///     session_id,
    ///     vec![AcpMessage::new(
    ///         AcpRole::User,
    ///         vec![AcpMessagePart::Text(AcpTextPart::new("hello".to_string()))],
    ///     )?],
    /// )?;
    /// let run = AcpRun::new(AcpRunId::new("run_123".to_string())?, request, session)?;
    ///
    /// storage.persist_acp_run(&run, AcpRuntimeExecuteMode::Sync, None)?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn persist_acp_run(
        &self,
        run: &AcpRun,
        mode: AcpRuntimeExecuteMode,
        conversation_id: Option<String>,
    ) -> Result<()> {
        let session = StoredAcpSession {
            session_id: run.session.id.as_str().to_string(),
            conversation_id: conversation_id.clone(),
            title: None,
            created_at: parse_rfc3339_to_utc(&run.session.created_at)?,
            updated_at: parse_rfc3339_to_utc(&run.status.updated_at)?,
            run_count: self
                .count_acp_runs_for_session(run.session.id.as_str())
                .unwrap_or(0)
                + 1,
            last_run_id: Some(run.id.as_str().to_string()),
            metadata: BTreeMap::new(),
        };
        self.save_acp_session(&session)?;

        let input_json = serde_json::to_string(&run.request.input)
            .context("Failed to serialize ACP run input")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;
        let output_json = serde_json::to_string(&run.output)
            .context("Failed to serialize ACP run output")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        let stored_run = StoredAcpRun {
            run_id: run.id.as_str().to_string(),
            session_id: run.session.id.as_str().to_string(),
            conversation_id,
            mode: mode.as_str().to_string(),
            state: run.status.state.to_string(),
            created_at: parse_rfc3339_to_utc(&run.status.created_at)?,
            updated_at: parse_rfc3339_to_utc(&run.status.updated_at)?,
            completed_at: match &run.status.completed_at {
                Some(value) => Some(parse_rfc3339_to_utc(value)?),
                None => None,
            },
            failure_reason: run.status.failure_reason.clone(),
            cancellation_reason: run.status.cancellation_reason.clone(),
            await_kind: run
                .await_payload
                .as_ref()
                .map(|payload| payload.kind.clone()),
            await_detail: run
                .await_payload
                .as_ref()
                .map(|payload| payload.detail.clone()),
            input_json,
            output_json,
            metadata: BTreeMap::new(),
        };
        self.save_acp_run(&stored_run)?;

        if let Some(await_payload) = &run.await_payload {
            let await_state = StoredAcpAwaitState {
                run_id: run.id.as_str().to_string(),
                session_id: run.session.id.as_str().to_string(),
                kind: await_payload.kind.clone(),
                detail: await_payload.detail.clone(),
                created_at: parse_rfc3339_to_utc(&run.status.updated_at)?,
                updated_at: parse_rfc3339_to_utc(&run.status.updated_at)?,
                resumed_at: None,
                resume_payload_json: None,
            };
            self.save_acp_await_state(&await_state)?;
        }

        Ok(())
    }

    /// Restore a canonical ACP run from persistent storage.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    ///
    /// # Returns
    ///
    /// Returns the restored canonical ACP run when found.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization or reconstruction fails.
    pub fn restore_acp_run(&self, run_id: &str) -> Result<Option<AcpRun>> {
        let stored = match self.load_acp_run(run_id)? {
            Some(value) => value,
            None => return Ok(None),
        };

        let session_id = AcpSessionId::new(stored.session_id.clone())?;
        let run_session = AcpRunSession {
            id: session_id.clone(),
            created_at: stored.created_at.to_rfc3339(),
        };

        let input: Vec<crate::acp::AcpMessage> = serde_json::from_str(&stored.input_json)
            .context("Failed to deserialize stored ACP input")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;
        let output: AcpRunOutput = serde_json::from_str(&stored.output_json)
            .context("Failed to deserialize stored ACP output")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;

        let request = AcpRunCreateRequest {
            session_id,
            input,
            metadata: BTreeMap::new(),
        };

        let run_id = AcpRunId::new(stored.run_id.clone())?;
        let state = parse_run_state(&stored.state)?;
        let status = AcpRunStatus {
            state,
            created_at: stored.created_at.to_rfc3339(),
            updated_at: stored.updated_at.to_rfc3339(),
            completed_at: stored.completed_at.map(|value| value.to_rfc3339()),
            failure_reason: stored.failure_reason.clone(),
            cancellation_reason: stored.cancellation_reason.clone(),
        };

        let await_payload = match (&stored.await_kind, &stored.await_detail) {
            (Some(kind), Some(detail)) => Some(AcpAwaitPayload::new(kind.clone(), detail.clone())),
            _ => None,
        };

        Ok(Some(AcpRun {
            id: run_id,
            session: run_session,
            request,
            status,
            output,
            await_payload,
        }))
    }

    /// Restore persisted ACP runtime events for one run.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    ///
    /// # Returns
    ///
    /// Returns restored runtime events ordered by sequence.
    ///
    /// # Errors
    ///
    /// Returns an error if event deserialization fails.
    pub fn restore_acp_runtime_events(&self, run_id: &str) -> Result<Vec<AcpRuntimeEvent>> {
        let stored_events = self.load_acp_run_events(run_id)?;
        let mut restored = Vec::with_capacity(stored_events.len());

        for stored in stored_events {
            let payload: Value = serde_json::from_str(&stored.payload_json)
                .context("Failed to deserialize stored ACP event payload")
                .map_err(|e| XzatomaError::Storage(e.to_string()))?;
            let event = AcpEvent {
                kind: parse_event_kind(&stored.kind)?,
                run_id: Some(stored.run_id.clone()),
                created_at: stored.created_at.to_rfc3339(),
                payload,
            };
            restored.push(AcpRuntimeEvent {
                sequence: stored.sequence,
                event,
                terminal: stored.terminal,
            });
        }

        Ok(restored)
    }

    /// Count persisted ACP runs for a session.
    ///
    /// # Arguments
    ///
    /// * `session_id` - ACP session identifier
    ///
    /// # Returns
    ///
    /// Returns the number of stored runs associated with the session.
    ///
    /// # Errors
    ///
    /// Returns an error if the count query fails.
    pub fn count_acp_runs_for_session(&self, session_id: &str) -> Result<usize> {
        let conn = self.open_connection()?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM acp_runs WHERE session_id = ?",
                params![session_id],
                |row| row.get(0),
            )
            .context("Failed to count ACP runs for session")
            .map_err(|e| XzatomaError::Storage(e.to_string()))?;
        usize::try_from(count)
            .map_err(|e| XzatomaError::Storage(format!("Invalid ACP run count: {}", e)).into())
    }

    fn open_connection(&self) -> Result<Connection> {
        Connection::open(&self.db_path)
            .context("Failed to open database")
            .map_err(|e| XzatomaError::Storage(e.to_string()).into())
    }
}

fn serialize_metadata(metadata: &BTreeMap<String, String>) -> Result<String> {
    serde_json::to_string(metadata)
        .context("Failed to serialize metadata")
        .map_err(|e| XzatomaError::Storage(e.to_string()).into())
}

fn deserialize_metadata(json_str: &str) -> Result<BTreeMap<String, String>> {
    serde_json::from_str(json_str)
        .context("Failed to deserialize metadata")
        .map_err(|e| XzatomaError::Storage(e.to_string()).into())
}

fn parse_rfc3339_to_utc(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .context("Failed to parse RFC 3339 timestamp")
        .map_err(|e| XzatomaError::Storage(e.to_string()).into())
}

fn bool_to_sqlite(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn sqlite_to_bool(value: i64) -> bool {
    value != 0
}

fn to_rusqlite_error(error: anyhow::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            error.to_string(),
        )),
    )
}

fn parse_run_state(value: &str) -> Result<AcpRunState> {
    match value {
        "created" => Ok(AcpRunState::Created),
        "queued" => Ok(AcpRunState::Queued),
        "running" => Ok(AcpRunState::Running),
        "awaiting" => Ok(AcpRunState::Awaiting),
        "completed" => Ok(AcpRunState::Completed),
        "failed" => Ok(AcpRunState::Failed),
        "cancelled" => Ok(AcpRunState::Cancelled),
        other => {
            Err(XzatomaError::Storage(format!("Unknown stored ACP run state: {}", other)).into())
        }
    }
}

fn parse_event_kind(value: &str) -> Result<AcpEventKind> {
    match value {
        "run_created" => Ok(AcpEventKind::RunCreated),
        "run_status_changed" => Ok(AcpEventKind::RunStatusChanged),
        "run_output_appended" => Ok(AcpEventKind::RunOutputAppended),
        "run_completed" => Ok(AcpEventKind::RunCompleted),
        "run_failed" => Ok(AcpEventKind::RunFailed),
        "run_cancelled" => Ok(AcpEventKind::RunCancelled),
        "run_awaiting_input" => Ok(AcpEventKind::RunAwaitingInput),
        "session_created" => Ok(AcpEventKind::SessionCreated),
        other => {
            Err(XzatomaError::Storage(format!("Unknown stored ACP event kind: {}", other)).into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::{AcpMessage, AcpMessagePart, AcpRole, AcpTextPart};
    use chrono::Timelike;
    use serial_test::serial;
    use std::thread::sleep;
    use std::time::Duration;
    use tempfile::tempdir;

    fn create_test_storage() -> (SqliteStorage, tempfile::TempDir) {
        let dir = tempdir().expect("failed to create tempdir");
        let db_path = dir.path().join("history.db");
        let storage = SqliteStorage::new_with_path(db_path).expect("failed to create storage");
        (storage, dir)
    }

    fn sample_run() -> AcpRun {
        let session_id = AcpSessionId::new("session_123".to_string()).expect("valid session id");
        let session = AcpRunSession::new(session_id.clone()).expect("valid session");
        let request = AcpRunCreateRequest::new(
            session_id,
            vec![AcpMessage::new(
                AcpRole::User,
                vec![AcpMessagePart::Text(AcpTextPart::new(
                    "hello from acp".to_string(),
                ))],
            )
            .expect("valid message")],
        )
        .expect("valid create request");

        AcpRun::new(
            AcpRunId::new("run_123".to_string()).expect("valid run id"),
            request,
            session,
        )
        .expect("valid run")
    }

    #[test]
    fn test_sqlite_storage_init_creates_conversation_and_acp_tables() {
        let (storage, _dir) = create_test_storage();
        let conn = Connection::open(storage.database_path()).expect("open connection");

        let tables = [
            "conversations",
            "acp_sessions",
            "acp_runs",
            "acp_run_events",
            "acp_await_states",
            "acp_cancellations",
        ];

        for table in tables {
            let count: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?",
                    params![table],
                    |row| row.get(0),
                )
                .expect("query row");
            assert_eq!(count, 1, "missing table {}", table);
        }
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

        let (loaded_title, loaded_model, loaded_messages) =
            loaded.expect("conversation should exist");
        assert_eq!(loaded_title, title.to_string());
        assert_eq!(loaded_model, model.map(|value| value.to_string()));
        assert_eq!(loaded_messages.len(), 1);
        assert_eq!(loaded_messages[0].content.as_deref(), Some("Hello"));
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
            .find(|value| value.id == id)
            .expect("session found");
        let created_at = session.created_at;
        let updated_at = session.updated_at;

        sleep(Duration::from_millis(10));

        let new_title = "Updated Title";
        let new_messages = vec![crate::providers::Message::user("Second")];
        storage
            .save_conversation(id, new_title, Some("gpt-4"), &new_messages)
            .expect("update failed");

        let sessions = storage.list_sessions().expect("list failed");
        let session_updated = sessions
            .into_iter()
            .find(|value| value.id == id)
            .expect("updated session found");

        assert_eq!(session_updated.created_at, created_at);
        assert!(session_updated.updated_at > updated_at);
    }

    #[test]
    fn test_load_conversation_returns_none_for_missing_id() {
        let (storage, _dir) = create_test_storage();
        let result = storage
            .load_conversation("non-existent-id")
            .expect("load failed");
        assert!(result.is_none());
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
        assert!(sessions.len() >= 2);
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
            .find(|value| value.id == id)
            .expect("session not found");
        assert_eq!(session.message_count, 3);
    }

    #[test]
    fn test_save_acp_session_and_load_round_trip() {
        let (storage, _dir) = create_test_storage();
        let now = Utc::now();
        let session = StoredAcpSession {
            session_id: "session_123".to_string(),
            conversation_id: Some("conv_123".to_string()),
            title: Some("ACP Session".to_string()),
            created_at: now,
            updated_at: now,
            run_count: 0,
            last_run_id: None,
            metadata: BTreeMap::from([("source".to_string(), "acp".to_string())]),
        };

        storage
            .save_acp_session(&session)
            .expect("save session failed");
        let loaded = storage
            .load_acp_session("session_123")
            .expect("load session failed")
            .expect("session should exist");

        assert_eq!(loaded.session_id, session.session_id);
        assert_eq!(loaded.conversation_id, session.conversation_id);
        assert_eq!(loaded.title, session.title);
        assert_eq!(loaded.last_run_id, session.last_run_id);
        assert_eq!(
            loaded.metadata.get("source").map(String::as_str),
            Some("acp")
        );
    }

    #[test]
    fn test_save_acp_run_and_restore_round_trip() {
        let (storage, _dir) = create_test_storage();
        let run = sample_run();

        storage
            .persist_acp_run(
                &run,
                AcpRuntimeExecuteMode::Sync,
                Some("conv_123".to_string()),
            )
            .expect("persist run failed");

        let restored = storage
            .restore_acp_run(run.id.as_str())
            .expect("restore failed")
            .expect("run should exist");

        assert_eq!(restored.id.as_str(), run.id.as_str());
        assert_eq!(restored.session.id.as_str(), run.session.id.as_str());
        assert_eq!(restored.status.state, run.status.state);
        assert_eq!(restored.request.input.len(), 1);
    }

    #[test]
    fn test_save_acp_run_events_and_restore_runtime_events() {
        let (storage, _dir) = create_test_storage();
        let run = sample_run();
        storage
            .persist_acp_run(&run, AcpRuntimeExecuteMode::Sync, None)
            .expect("persist run failed");

        let created_at = Utc::now()
            .with_nanosecond(0)
            .expect("timestamp truncation should succeed");
        let event = StoredAcpRunEvent {
            run_id: run.id.as_str().to_string(),
            sequence: 1,
            kind: "run_created".to_string(),
            created_at,
            payload_json: r#"{"event":"run.created","state":"created"}"#.to_string(),
            terminal: false,
        };

        storage
            .save_acp_run_events(run.id.as_str(), std::slice::from_ref(&event))
            .expect("save events failed");

        let loaded = storage
            .load_acp_run_events(run.id.as_str())
            .expect("load events failed");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], event);

        let restored = storage
            .restore_acp_runtime_events(run.id.as_str())
            .expect("restore runtime events failed");
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].sequence, 1);
        assert_eq!(restored[0].event.kind, AcpEventKind::RunCreated);
    }

    #[test]
    fn test_save_acp_await_state_and_load_round_trip() {
        let (storage, _dir) = create_test_storage();
        let mut run = sample_run();
        run.transition_to(AcpRunState::Queued)
            .expect("transition to queued");
        run.transition_to(AcpRunState::Running)
            .expect("transition to running");
        run.set_await_payload(
            "approval_required".to_string(),
            "Need confirmation".to_string(),
        )
        .expect("set await payload");
        storage
            .persist_acp_run(&run, AcpRuntimeExecuteMode::Async, None)
            .expect("persist run failed");

        let now = Utc::now()
            .with_nanosecond(0)
            .expect("timestamp truncation should succeed");
        let await_state = StoredAcpAwaitState {
            run_id: run.id.as_str().to_string(),
            session_id: run.session.id.as_str().to_string(),
            kind: "approval_required".to_string(),
            detail: "Need confirmation".to_string(),
            created_at: now,
            updated_at: now,
            resumed_at: None,
            resume_payload_json: None,
        };

        storage
            .save_acp_await_state(&await_state)
            .expect("save await state failed");

        let loaded = storage
            .load_acp_await_state(run.id.as_str())
            .expect("load await state failed")
            .expect("await state should exist");

        assert_eq!(loaded, await_state);
    }

    #[test]
    fn test_save_acp_cancellation_and_load_round_trip() {
        let (storage, _dir) = create_test_storage();
        let run = sample_run();
        storage
            .persist_acp_run(&run, AcpRuntimeExecuteMode::Async, None)
            .expect("persist run failed");

        let requested_at = Utc::now()
            .with_nanosecond(0)
            .expect("timestamp truncation should succeed");
        let cancellation = StoredAcpCancellation {
            run_id: run.id.as_str().to_string(),
            requested_at,
            acknowledged_at: None,
            completed_at: None,
            reason: Some("user requested stop".to_string()),
            acknowledged: false,
        };

        storage
            .save_acp_cancellation(&cancellation)
            .expect("save cancellation failed");

        let loaded = storage
            .load_acp_cancellation(run.id.as_str())
            .expect("load cancellation failed")
            .expect("cancellation should exist");

        assert_eq!(loaded, cancellation);
    }

    #[test]
    fn test_count_acp_runs_for_session_counts_persisted_runs() {
        let (storage, _dir) = create_test_storage();
        let mut run_one = sample_run();
        let mut run_two = sample_run();

        run_two.id = AcpRunId::new("run_456".to_string()).expect("valid run id");
        run_two.session.id =
            AcpSessionId::new("session_123".to_string()).expect("valid session id");
        run_two.request.session_id =
            AcpSessionId::new("session_123".to_string()).expect("valid session id");

        storage
            .persist_acp_run(&run_one, AcpRuntimeExecuteMode::Async, None)
            .expect("persist run one failed");
        storage
            .persist_acp_run(&run_two, AcpRuntimeExecuteMode::Async, None)
            .expect("persist run two failed");

        let count = storage
            .count_acp_runs_for_session("session_123")
            .expect("count runs failed");
        assert_eq!(count, 2);

        // suppress mut lint in test context by performing benign update
        run_one.status.updated_at = run_one.status.updated_at.clone();
    }

    #[test]
    fn test_list_acp_runs_for_session_returns_created_order() {
        let (storage, _dir) = create_test_storage();
        let run_one = sample_run();
        storage
            .persist_acp_run(&run_one, AcpRuntimeExecuteMode::Sync, None)
            .expect("persist first run failed");

        sleep(Duration::from_millis(5));

        let session_id = AcpSessionId::new("session_123".to_string()).expect("valid session id");
        let session = AcpRunSession::new(session_id.clone()).expect("valid session");
        let request = AcpRunCreateRequest::new(
            session_id,
            vec![AcpMessage::new(
                AcpRole::User,
                vec![AcpMessagePart::Text(AcpTextPart::new("second".to_string()))],
            )
            .expect("valid message")],
        )
        .expect("valid request");
        let run_two = AcpRun::new(
            AcpRunId::new("run_789".to_string()).expect("valid run id"),
            request,
            session,
        )
        .expect("valid run");
        storage
            .persist_acp_run(&run_two, AcpRuntimeExecuteMode::Sync, None)
            .expect("persist second run failed");

        let runs = storage
            .list_acp_runs_for_session("session_123")
            .expect("list runs failed");
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].run_id, "run_123");
        assert_eq!(runs[1].run_id, "run_789");
    }

    #[test]
    fn test_persist_acp_run_persists_await_payload_when_present() {
        let (storage, _dir) = create_test_storage();
        let mut run = sample_run();
        run.transition_to(AcpRunState::Queued)
            .expect("transition to queued");
        run.transition_to(AcpRunState::Running)
            .expect("transition to running");
        run.set_await_payload(
            "approval_required".to_string(),
            "Need confirmation".to_string(),
        )
        .expect("set await payload");

        storage
            .persist_acp_run(&run, AcpRuntimeExecuteMode::Async, None)
            .expect("persist run failed");

        let await_state = storage
            .load_acp_await_state(run.id.as_str())
            .expect("load await state failed")
            .expect("await state should exist");

        assert_eq!(await_state.kind, "approval_required");
        assert_eq!(await_state.detail, "Need confirmation");

        let restored = storage
            .restore_acp_run(run.id.as_str())
            .expect("restore failed")
            .expect("run should exist");
        assert_eq!(restored.status.state, AcpRunState::Awaiting);
        assert!(restored.await_payload.is_some());
    }

    #[test]
    fn test_restore_acp_run_returns_none_for_missing_run() {
        let (storage, _dir) = create_test_storage();
        let restored = storage
            .restore_acp_run("run_missing")
            .expect("restore failed");
        assert!(restored.is_none());
    }

    #[test]
    fn test_load_acp_session_returns_none_for_missing_session() {
        let (storage, _dir) = create_test_storage();
        let loaded = storage
            .load_acp_session("session_missing")
            .expect("load failed");
        assert!(loaded.is_none());
    }

    #[test]
    #[serial]
    fn test_new_respects_history_db_environment_override() {
        let dir = tempdir().expect("failed to create tempdir");
        let db_path = dir.path().join("override_history.db");
        std::env::set_var("XZATOMA_HISTORY_DB", &db_path);

        let storage = SqliteStorage::new().expect("storage should initialize");
        assert_eq!(storage.database_path(), &db_path);

        std::env::remove_var("XZATOMA_HISTORY_DB");
    }
}
