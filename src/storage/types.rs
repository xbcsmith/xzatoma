use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Metadata for a stored conversation session.
///
/// This structure is used by the existing conversation history views and remains
/// the summary form for non-ACP session listings.
///
/// # Examples
///
/// ```
/// use chrono::Utc;
/// use xzatoma::storage::types::StoredSession;
///
/// let now = Utc::now();
/// let session = StoredSession {
///     id: "session-1".to_string(),
///     title: "Example".to_string(),
///     created_at: now,
///     updated_at: now,
///     model: Some("gpt-5-mini".to_string()),
///     message_count: 3,
/// };
///
/// assert_eq!(session.id, "session-1");
/// assert_eq!(session.message_count, 3);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSession {
    /// Unique identifier for the session.
    pub id: String,
    /// User-friendly title or summary.
    pub title: String,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last updated.
    pub updated_at: DateTime<Utc>,
    /// The model used in the session.
    pub model: Option<String>,
    /// Number of messages in the session.
    pub message_count: usize,
}

/// Persisted ACP session summary.
///
/// This structure represents durable ACP session metadata that can be queried
/// independently of the in-memory runtime. It is intentionally small and
/// transport-oriented so ACP handlers can reconstruct stateful behavior after a
/// restart.
///
/// # Examples
///
/// ```
/// use chrono::Utc;
/// use xzatoma::storage::types::StoredAcpSession;
///
/// let now = Utc::now();
/// let session = StoredAcpSession {
///     session_id: "session_123".to_string(),
///     conversation_id: Some("conv-123".to_string()),
///     title: Some("ACP Session".to_string()),
///     created_at: now,
///     updated_at: now,
///     run_count: 2,
///     last_run_id: Some("run_456".to_string()),
///     metadata: Default::default(),
/// };
///
/// assert_eq!(session.session_id, "session_123");
/// assert_eq!(session.run_count, 2);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredAcpSession {
    /// ACP session identifier.
    pub session_id: String,
    /// Optional mapped conversation identifier in the existing conversation
    /// storage namespace.
    pub conversation_id: Option<String>,
    /// Optional human-readable title.
    pub title: Option<String>,
    /// When the ACP session was created.
    pub created_at: DateTime<Utc>,
    /// When the ACP session was last updated.
    pub updated_at: DateTime<Utc>,
    /// Number of persisted runs associated with the session.
    pub run_count: usize,
    /// Most recently associated run identifier.
    pub last_run_id: Option<String>,
    /// Arbitrary session metadata preserved for ACP state continuity.
    pub metadata: BTreeMap<String, String>,
}

/// Persisted ACP run summary.
///
/// This structure stores durable run-level metadata so completed or interrupted
/// ACP runs can be reloaded after process restart.
///
/// # Examples
///
/// ```
/// use chrono::Utc;
/// use xzatoma::storage::types::StoredAcpRun;
///
/// let now = Utc::now();
/// let run = StoredAcpRun {
///     run_id: "run_123".to_string(),
///     session_id: "session_123".to_string(),
///     conversation_id: Some("conv-123".to_string()),
///     mode: "async".to_string(),
///     state: "completed".to_string(),
///     created_at: now,
///     updated_at: now,
///     completed_at: Some(now),
///     failure_reason: None,
///     cancellation_reason: None,
///     await_kind: None,
///     await_detail: None,
///     input_json: "[]".to_string(),
///     output_json: "{\"messages\":[]}".to_string(),
///     metadata: Default::default(),
/// };
///
/// assert_eq!(run.run_id, "run_123");
/// assert_eq!(run.state, "completed");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredAcpRun {
    /// ACP run identifier.
    pub run_id: String,
    /// ACP session identifier for the run.
    pub session_id: String,
    /// Optional mapped conversation identifier in the existing conversation
    /// storage namespace.
    pub conversation_id: Option<String>,
    /// Requested or effective ACP execution mode.
    pub mode: String,
    /// Current persisted ACP run state.
    pub state: String,
    /// When the run was created.
    pub created_at: DateTime<Utc>,
    /// When the run was last updated.
    pub updated_at: DateTime<Utc>,
    /// Completion timestamp when terminal.
    pub completed_at: Option<DateTime<Utc>>,
    /// Failure reason when the run failed.
    pub failure_reason: Option<String>,
    /// Cancellation reason when the run was cancelled.
    pub cancellation_reason: Option<String>,
    /// Await payload kind when the run is currently awaiting resume input.
    pub await_kind: Option<String>,
    /// Await payload detail when the run is currently awaiting resume input.
    pub await_detail: Option<String>,
    /// Serialized ACP input payload.
    pub input_json: String,
    /// Serialized ACP output payload.
    pub output_json: String,
    /// Additional run metadata.
    pub metadata: BTreeMap<String, String>,
}

/// Persisted ACP event record.
///
/// This structure stores replayable event history for a run. Event ordering is
/// preserved through the monotonic sequence number.
///
/// # Examples
///
/// ```
/// use chrono::Utc;
/// use xzatoma::storage::types::StoredAcpRunEvent;
///
/// let event = StoredAcpRunEvent {
///     run_id: "run_123".to_string(),
///     sequence: 1,
///     kind: "run_created".to_string(),
///     created_at: Utc::now(),
///     payload_json: "{\"event\":\"run.created\"}".to_string(),
///     terminal: false,
/// };
///
/// assert_eq!(event.sequence, 1);
/// assert!(!event.terminal);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredAcpRunEvent {
    /// Associated ACP run identifier.
    pub run_id: String,
    /// Monotonic per-run event sequence.
    pub sequence: u64,
    /// Canonical event kind string.
    pub kind: String,
    /// Event creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Serialized event payload JSON.
    pub payload_json: String,
    /// Whether this event is terminal for the run.
    pub terminal: bool,
}

/// Persisted ACP await state.
///
/// This structure represents a durable pending resume contract for runs in the
/// `awaiting` state.
///
/// # Examples
///
/// ```
/// use chrono::Utc;
/// use xzatoma::storage::types::StoredAcpAwaitState;
///
/// let await_state = StoredAcpAwaitState {
///     run_id: "run_123".to_string(),
///     session_id: "session_123".to_string(),
///     kind: "approval_required".to_string(),
///     detail: "Need confirmation before continuing".to_string(),
///     created_at: Utc::now(),
///     updated_at: Utc::now(),
///     resumed_at: None,
///     resume_payload_json: None,
/// };
///
/// assert_eq!(await_state.kind, "approval_required");
/// assert!(await_state.resume_payload_json.is_none());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredAcpAwaitState {
    /// Associated ACP run identifier.
    pub run_id: String,
    /// Associated ACP session identifier.
    pub session_id: String,
    /// Await discriminator.
    pub kind: String,
    /// Human-readable await detail.
    pub detail: String,
    /// When the await state was created.
    pub created_at: DateTime<Utc>,
    /// When the await state was last updated.
    pub updated_at: DateTime<Utc>,
    /// When the await state was resumed, if it has been resumed.
    pub resumed_at: Option<DateTime<Utc>>,
    /// Serialized resume payload JSON, if present.
    pub resume_payload_json: Option<String>,
}

/// Persisted ACP cancellation record.
///
/// This structure stores cancellation requests and terminal-state audit data for
/// ACP runs.
///
/// # Examples
///
/// ```
/// use chrono::Utc;
/// use xzatoma::storage::types::StoredAcpCancellation;
///
/// let cancellation = StoredAcpCancellation {
///     run_id: "run_123".to_string(),
///     requested_at: Utc::now(),
///     acknowledged_at: None,
///     completed_at: None,
///     reason: Some("user requested stop".to_string()),
///     acknowledged: false,
/// };
///
/// assert_eq!(cancellation.run_id, "run_123");
/// assert!(!cancellation.acknowledged);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredAcpCancellation {
    /// Associated ACP run identifier.
    pub run_id: String,
    /// When cancellation was requested.
    pub requested_at: DateTime<Utc>,
    /// When the running task acknowledged cancellation.
    pub acknowledged_at: Option<DateTime<Utc>>,
    /// When cancellation reached terminal completion.
    pub completed_at: Option<DateTime<Utc>>,
    /// Optional human-readable cancellation reason.
    pub reason: Option<String>,
    /// Whether cancellation has been acknowledged by the executor.
    pub acknowledged: bool,
}
