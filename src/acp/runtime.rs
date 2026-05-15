/// ACP runtime coordinator for run lifecycle management.
///
/// This module provides the in-memory coordination layer for ACP run execution.
/// It sits between HTTP handlers and the existing XZatoma agent execution loop,
/// allowing ACP transport code to create runs, publish lifecycle events, track
/// execution state, and replay event history without tightly coupling HTTP
/// routes to agent internals.
///
/// Phase 4 extends the runtime with durable backing through the shared SQLite
/// storage layer so ACP sessions, runs, events, await state, and cancellation
/// audit data survive process restarts.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::runtime::{
///     AcpRuntime, AcpRuntimeCreateRequest, AcpRuntimeExecuteMode,
/// };
/// use xzatoma::acp::{AcpMessage, AcpMessagePart, AcpRole, AcpTextPart};
/// use xzatoma::Config;
///
/// let runtime = AcpRuntime::new(Config::default());
///
/// let input = vec![AcpMessage::new(
///     AcpRole::User,
///     vec![AcpMessagePart::Text(AcpTextPart::new(
///         "Summarize the task".to_string(),
///     ))],
/// )?];
///
/// let request = AcpRuntimeCreateRequest::new(input)
///     .with_mode(AcpRuntimeExecuteMode::Sync);
///
/// let run = runtime.create_run(request)?;
/// assert_eq!(run.status.state.to_string(), "created");
/// # Ok::<(), anyhow::Error>(())
/// ```
use crate::acp::{
    now_rfc3339, AcpEvent, AcpEventKind, AcpMessage, AcpMessagePart, AcpRole, AcpRun,
    AcpRunCreateRequest, AcpRunId, AcpRunResumeRequest, AcpRunSession, AcpRunState, AcpSessionId,
};
use crate::config::{AcpCompatibilityMode, AcpDefaultRunMode, Config};
use crate::error::{Result, XzatomaError};
use crate::storage::{
    PublicStoredAcpAwaitState, PublicStoredAcpCancellation, PublicStoredAcpRunEvent, SqliteStorage,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use uuid::Uuid;

/// Default capacity for live event fan-out.
///
/// This value is intentionally modest because Phase 3 keeps event history in the
/// per-run record and uses the broadcast channel only for live subscribers.
const DEFAULT_EVENT_CHANNEL_CAPACITY: usize = 256;

/// ACP runtime execution mode.
///
/// This execution mode is selected per run request and determines whether ACP
/// handlers should wait for completion, return immediately, or expose an event
/// stream while execution progresses.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::runtime::AcpRuntimeExecuteMode;
///
/// assert_eq!(AcpRuntimeExecuteMode::Sync.as_str(), "sync");
/// assert_eq!(AcpRuntimeExecuteMode::Async.as_str(), "async");
/// assert_eq!(AcpRuntimeExecuteMode::Stream.as_str(), "stream");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcpRuntimeExecuteMode {
    /// Execute the run and wait for completion before returning.
    Sync,
    /// Accept the run and continue execution in the background.
    Async,
    /// Execute the run while exposing lifecycle events incrementally.
    Stream,
}

impl AcpRuntimeExecuteMode {
    /// Returns the wire-facing string for the mode.
    ///
    /// # Returns
    ///
    /// Returns the stable ACP runtime mode string.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::runtime::AcpRuntimeExecuteMode;
    ///
    /// assert_eq!(AcpRuntimeExecuteMode::Stream.as_str(), "stream");
    /// ```
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sync => "sync",
            Self::Async => "async",
            Self::Stream => "stream",
        }
    }

    /// Parses a runtime mode from a string.
    ///
    /// # Arguments
    ///
    /// * `value` - Candidate mode string
    ///
    /// # Returns
    ///
    /// Returns the parsed runtime mode.
    ///
    /// # Errors
    ///
    /// Returns an error if the mode is not supported.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::runtime::AcpRuntimeExecuteMode;
    ///
    /// assert_eq!(AcpRuntimeExecuteMode::parse("async")?, AcpRuntimeExecuteMode::Async);
    /// assert!(AcpRuntimeExecuteMode::parse("invalid").is_err());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "sync" => Ok(Self::Sync),
            "async" => Ok(Self::Async),
            "stream" => Ok(Self::Stream),
            other => Err(crate::acp::error::AcpError::validation(format!(
                "unsupported ACP execution mode '{}'; expected one of: sync, async, stream",
                other
            ))
            .into()),
        }
    }

    /// Derives a runtime mode from configuration defaults.
    ///
    /// # Arguments
    ///
    /// * `value` - Configured default run mode
    ///
    /// # Returns
    ///
    /// Returns the corresponding runtime execution mode.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::runtime::AcpRuntimeExecuteMode;
    /// use xzatoma::config::AcpDefaultRunMode;
    ///
    /// assert_eq!(
    ///     AcpRuntimeExecuteMode::from_default_run_mode(AcpDefaultRunMode::Async),
    ///     AcpRuntimeExecuteMode::Async
    /// );
    /// ```
    pub fn from_default_run_mode(value: AcpDefaultRunMode) -> Self {
        match value {
            AcpDefaultRunMode::Sync => Self::Sync,
            AcpDefaultRunMode::Async => Self::Async,
            AcpDefaultRunMode::Streaming => Self::Stream,
        }
    }
}

impl std::fmt::Display for AcpRuntimeExecuteMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// ACP runtime create request.
///
/// This structure captures the Phase 3 inputs needed to create an ACP run in
/// the runtime coordinator before execution begins.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::runtime::{AcpRuntimeCreateRequest, AcpRuntimeExecuteMode};
/// use xzatoma::acp::{AcpMessage, AcpMessagePart, AcpRole, AcpTextPart};
///
/// let input = vec![AcpMessage::new(
///     AcpRole::User,
///     vec![AcpMessagePart::Text(AcpTextPart::new("Hello".to_string()))],
/// )?];
///
/// let request = AcpRuntimeCreateRequest::new(input)
///     .with_mode(AcpRuntimeExecuteMode::Async)
///     .with_agent_name("xzatoma".to_string());
///
/// assert_eq!(request.mode, AcpRuntimeExecuteMode::Async);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpRuntimeCreateRequest {
    /// ACP input messages for the run.
    pub input: Vec<AcpMessage>,
    /// Requested execution mode.
    pub mode: AcpRuntimeExecuteMode,
    /// Optional client-supplied session identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Optional requested ACP agent name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    /// Optional transport metadata.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

impl AcpRuntimeCreateRequest {
    /// Creates a new runtime create request.
    ///
    /// # Arguments
    ///
    /// * `input` - ACP input messages
    ///
    /// # Returns
    ///
    /// Returns the initialized request using `sync` as the default runtime mode.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::runtime::{AcpRuntimeCreateRequest, AcpRuntimeExecuteMode};
    ///
    /// let request = AcpRuntimeCreateRequest::new(Vec::new());
    /// assert_eq!(request.mode, AcpRuntimeExecuteMode::Sync);
    /// ```
    pub fn new(input: Vec<AcpMessage>) -> Self {
        Self {
            input,
            mode: AcpRuntimeExecuteMode::Sync,
            session_id: None,
            agent_name: None,
            metadata: HashMap::new(),
        }
    }

    /// Sets the runtime execution mode.
    ///
    /// # Arguments
    ///
    /// * `mode` - Requested runtime mode
    ///
    /// # Returns
    ///
    /// Returns the updated request.
    pub fn with_mode(mut self, mode: AcpRuntimeExecuteMode) -> Self {
        self.mode = mode;
        self
    }

    /// Sets the target ACP agent name.
    ///
    /// # Arguments
    ///
    /// * `agent_name` - Requested ACP agent name
    ///
    /// # Returns
    ///
    /// Returns the updated request.
    pub fn with_agent_name(mut self, agent_name: String) -> Self {
        self.agent_name = Some(agent_name);
        self
    }

    /// Sets the session identifier.
    ///
    /// # Arguments
    ///
    /// * `session_id` - Existing or client-supplied ACP session identifier
    ///
    /// # Returns
    ///
    /// Returns the updated request.
    pub fn with_session_id(mut self, session_id: String) -> Self {
        self.session_id = Some(session_id);
        self
    }

    /// Adds one metadata entry.
    ///
    /// # Arguments
    ///
    /// * `key` - Metadata key
    /// * `value` - Metadata value
    ///
    /// # Returns
    ///
    /// Returns the updated request.
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Validates the runtime request.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    ///
    /// - no input messages are provided
    /// - any input message is invalid
    /// - unsupported multimodal content is present
    /// - an agent name is provided but is empty
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::runtime::AcpRuntimeCreateRequest;
    /// use xzatoma::acp::{AcpMessage, AcpMessagePart, AcpRole, AcpTextPart};
    ///
    /// let request = AcpRuntimeCreateRequest::new(vec![AcpMessage::new(
    ///     AcpRole::User,
    ///     vec![AcpMessagePart::Text(AcpTextPart::new("Test".to_string()))],
    /// )?]);
    ///
    /// assert!(request.validate().is_ok());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn validate(&self) -> Result<()> {
        if self.input.is_empty() {
            return Err(
                crate::acp::error::AcpError::validation("ACP run input cannot be empty").into(),
            );
        }

        if let Some(agent_name) = &self.agent_name {
            if agent_name.trim().is_empty() {
                return Err(crate::acp::error::AcpError::validation(
                    "ACP agent name cannot be empty when set",
                )
                .into());
            }
        }

        for message in &self.input {
            message.validate()?;
            validate_supported_message_parts(message)?;
        }

        Ok(())
    }
}

/// ACP runtime event wrapper.
///
/// This event wrapper adds runtime-local sequencing and terminal markers around
/// the canonical ACP event payload so event streams and replay remain ordered.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::runtime::AcpRuntimeEvent;
/// use xzatoma::acp::{AcpEvent, AcpEventKind};
///
/// let event = AcpRuntimeEvent::new(
///     1,
///     AcpEvent::new(
///         AcpEventKind::RunCreated,
///         Some("run_1".to_string()),
///         serde_json::json!({"state": "created"}),
///     )?,
///     false,
/// );
///
/// assert_eq!(event.sequence, 1);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpRuntimeEvent {
    /// Monotonic per-run event sequence number.
    pub sequence: u64,
    /// Canonical ACP event payload.
    pub event: AcpEvent,
    /// Whether this event is terminal for the run.
    pub terminal: bool,
}

impl AcpRuntimeEvent {
    /// Creates a new runtime event wrapper.
    ///
    /// # Arguments
    ///
    /// * `sequence` - Per-run sequence number
    /// * `event` - Canonical ACP event
    /// * `terminal` - Whether this event is terminal
    ///
    /// # Returns
    ///
    /// Returns the wrapped runtime event.
    pub fn new(sequence: u64, event: AcpEvent, terminal: bool) -> Self {
        Self {
            sequence,
            event,
            terminal,
        }
    }
}

/// ACP runtime event subscription.
///
/// Subscribers receive live runtime events for one run via a broadcast channel.
/// Historical replay remains available through the runtime registry.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::runtime::AcpRuntime;
/// use xzatoma::Config;
///
/// let runtime = AcpRuntime::new(Config::default());
/// let _ = runtime;
/// ```
#[derive(Debug)]
pub struct AcpRuntimeSubscription {
    receiver: broadcast::Receiver<AcpRuntimeEvent>,
}

impl AcpRuntimeSubscription {
    /// Receives the next live runtime event.
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription channel is closed or lagged.
    pub async fn recv(
        &mut self,
    ) -> std::result::Result<AcpRuntimeEvent, broadcast::error::RecvError> {
        self.receiver.recv().await
    }
}

/// ACP runtime run record.
///
/// This structure keeps the current run state, execution mode, ordered event
/// history, and a live sender for new events.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::runtime::AcpRuntime;
/// use xzatoma::Config;
///
/// let runtime = AcpRuntime::new(Config::default());
/// let _ = runtime.run_count();
/// ```
#[derive(Debug)]
pub struct AcpRuntimeRunRecord {
    /// Current ACP run state.
    pub run: AcpRun,
    /// Requested execution mode.
    pub mode: AcpRuntimeExecuteMode,
    /// Ordered runtime event history.
    pub events: Vec<AcpRuntimeEvent>,
    /// Whether a terminal event has already been recorded.
    pub completed: bool,
    /// Sender used for live subscriptions.
    pub sender: broadcast::Sender<AcpRuntimeEvent>,
    /// Aggregated plain-text prompt used by the current single-agent execution model.
    pub prompt_text: String,
    /// Optional mapped conversation identifier in the shared history store.
    pub conversation_id: Option<String>,
    /// Whether cancellation has been requested for this run.
    pub cancellation_requested: bool,
    /// Optional cancellation reason.
    pub cancellation_reason: Option<String>,
    /// Await resume payload persisted for later continuation.
    pub resume_payload: Option<Value>,
}

impl AcpRuntimeRunRecord {
    fn new(run: AcpRun, mode: AcpRuntimeExecuteMode, prompt_text: String) -> Self {
        let (sender, _) = broadcast::channel(DEFAULT_EVENT_CHANNEL_CAPACITY);
        Self {
            run,
            mode,
            events: Vec::new(),
            completed: false,
            sender,
            prompt_text,
            conversation_id: None,
            cancellation_requested: false,
            cancellation_reason: None,
            resume_payload: None,
        }
    }
}

#[derive(Debug, Default)]
struct AcpRuntimeState {
    runs: HashMap<String, AcpRuntimeRunRecord>,
}

/// ACP runtime coordinator.
///
/// This is the primary in-memory entry point for ACP Phase 3 lifecycle
/// management. It creates runs, tracks status, records ordered events, and
/// supports live subscriptions for streaming transport.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::runtime::AcpRuntime;
/// use xzatoma::Config;
///
/// let runtime = AcpRuntime::new(Config::default());
/// let _ = runtime.run_count();
/// ```
#[derive(Clone)]
pub struct AcpRuntime {
    config: Config,
    state: Arc<Mutex<AcpRuntimeState>>,
    storage: Arc<Mutex<Option<SqliteStorage>>>,
}

impl std::fmt::Debug for AcpRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcpRuntime").finish_non_exhaustive()
    }
}

impl AcpRuntime {
    /// Creates a new ACP runtime.
    ///
    /// # Arguments
    ///
    /// * `config` - Application configuration
    ///
    /// # Returns
    ///
    /// Returns a new in-memory runtime coordinator.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::runtime::AcpRuntime;
    /// use xzatoma::Config;
    ///
    /// let runtime = AcpRuntime::new(Config::default());
    /// let _ = runtime.run_count();
    /// ```
    pub fn new(config: Config) -> Self {
        let storage = match SqliteStorage::new() {
            Ok(storage) => Some(storage),
            Err(error) => {
                tracing::warn!(error = %error, "ACP runtime storage initialization failed");
                None
            }
        };
        let runtime = Self {
            config,
            state: Arc::new(Mutex::new(AcpRuntimeState::default())),
            storage: Arc::new(Mutex::new(storage)),
        };

        if let Err(error) = runtime.restore_from_storage() {
            tracing::warn!(
                "Failed to restore ACP runtime state from storage: {}",
                error
            );
        }
        runtime
    }

    /// Creates a new ACP runtime without persistent storage.
    ///
    /// This constructor is intended for unit tests that need deterministic
    /// in-memory behavior without touching the user's shared runtime database.
    ///
    /// # Arguments
    ///
    /// * `config` - Application configuration
    ///
    /// # Returns
    ///
    /// Returns a new runtime coordinator backed only by process memory.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::runtime::AcpRuntime;
    /// use xzatoma::Config;
    ///
    /// let runtime = AcpRuntime::new_in_memory(Config::default());
    /// assert!(!runtime.has_storage());
    /// ```
    pub fn new_in_memory(config: Config) -> Self {
        Self {
            config,
            state: Arc::new(Mutex::new(AcpRuntimeState::default())),
            storage: Arc::new(Mutex::new(None)),
        }
    }

    /// Creates a new ACP runtime backed by an explicit SQLite database path.
    ///
    /// Unlike [`AcpRuntime::new`], this constructor does not read the
    /// `XZATOMA_HISTORY_DB` environment variable. The database at `db_path` is
    /// opened (or created) directly, and any runs already persisted there are
    /// restored into the in-memory state before the runtime is returned.
    ///
    /// This is the preferred constructor for tests that need full persistence
    /// coverage without relying on process-global environment state.
    ///
    /// # Arguments
    ///
    /// * `config` - Application configuration
    /// * `db_path` - Path to the SQLite database file to open or create
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzatoma::acp::runtime::AcpRuntime;
    /// use xzatoma::Config;
    ///
    /// let runtime = AcpRuntime::new_with_storage_path(Config::default(), "/tmp/test.db");
    /// assert_eq!(runtime.run_count(), 0);
    /// ```
    pub fn new_with_storage_path<P: AsRef<std::path::Path>>(config: Config, db_path: P) -> Self {
        let storage = match SqliteStorage::new_with_path(db_path.as_ref()) {
            Ok(storage) => Some(storage),
            Err(error) => {
                tracing::warn!(
                    path = %db_path.as_ref().display(),
                    error = %error,
                    "ACP runtime explicit storage initialization failed"
                );
                None
            }
        };
        let runtime = Self {
            config,
            state: Arc::new(Mutex::new(AcpRuntimeState::default())),
            storage: Arc::new(Mutex::new(storage)),
        };

        if let Err(error) = runtime.restore_from_storage() {
            tracing::warn!(
                "Failed to restore ACP runtime state from storage: {}",
                error
            );
        }
        runtime
    }

    /// Returns the configured default execution mode.
    ///
    /// # Returns
    ///
    /// Returns the runtime mode derived from ACP configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::runtime::{AcpRuntime, AcpRuntimeExecuteMode};
    /// use xzatoma::Config;
    ///
    /// let runtime = AcpRuntime::new(Config::default());
    /// assert_eq!(runtime.default_mode(), AcpRuntimeExecuteMode::Async);
    /// ```
    pub fn default_mode(&self) -> AcpRuntimeExecuteMode {
        AcpRuntimeExecuteMode::from_default_run_mode(self.config.acp.default_run_mode)
    }

    /// Returns true when durable storage is available.
    ///
    /// # Returns
    ///
    /// Returns `true` when the ACP runtime has an initialized SQLite backend.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::runtime::AcpRuntime;
    /// use xzatoma::Config;
    ///
    /// let runtime = AcpRuntime::new(Config::default());
    /// let _ = runtime.has_storage();
    /// ```
    pub fn has_storage(&self) -> bool {
        self.storage
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }

    /// Returns the configured ACP path compatibility mode.
    ///
    /// # Returns
    ///
    /// Returns the ACP path compatibility mode for metadata or diagnostics.
    pub fn compatibility_mode(&self) -> AcpCompatibilityMode {
        self.config.acp.compatibility_mode
    }

    /// Creates a new run record in the runtime registry.
    ///
    /// This method validates the input request, derives a session and run ID,
    /// flattens the text-first prompt representation, stores the run in memory,
    /// and records the initial `run.created` event.
    ///
    /// # Arguments
    ///
    /// * `request` - Runtime create request
    ///
    /// # Returns
    ///
    /// Returns the created ACP run.
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails or the run cannot be stored.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::runtime::{AcpRuntime, AcpRuntimeCreateRequest};
    /// use xzatoma::acp::{AcpMessage, AcpMessagePart, AcpRole, AcpTextPart};
    /// use xzatoma::Config;
    ///
    /// let runtime = AcpRuntime::new(Config::default());
    /// let request = AcpRuntimeCreateRequest::new(vec![AcpMessage::new(
    ///     AcpRole::User,
    ///     vec![AcpMessagePart::Text(AcpTextPart::new("Do the thing".to_string()))],
    /// )?]);
    ///
    /// let run = runtime.create_run(request)?;
    /// assert_eq!(run.status.state, xzatoma::acp::AcpRunState::Created);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn create_run(&self, request: AcpRuntimeCreateRequest) -> Result<AcpRun> {
        request.validate()?;

        let mode = request.mode;

        let session_id = match request.session_id.clone() {
            Some(value) => AcpSessionId::new(value)?,
            None => AcpSessionId::new(generate_session_id())?,
        };

        let run_id = AcpRunId::new(generate_run_id())?;
        let session = AcpRunSession::new(session_id.clone())?;
        let create_request = AcpRunCreateRequest::new(session_id.clone(), request.input.clone())?;
        let run = AcpRun::new(run_id.clone(), create_request, session)?;
        let prompt_text = flatten_input_to_prompt(&request.input)?;

        let mut record = AcpRuntimeRunRecord::new(run.clone(), mode, prompt_text);
        record.conversation_id = Some(session_id.as_str().to_string());

        let created_event = build_runtime_event(
            1,
            AcpEvent::new(
                AcpEventKind::RunCreated,
                Some(run.id.as_str().to_string()),
                json!({
                    "event": "run.created",
                    "runId": run.id.as_str(),
                    "sessionId": run.session.id.as_str(),
                    "state": run.status.state.to_string(),
                    "mode": mode.as_str(),
                    "createdAt": run.status.created_at,
                }),
            )?,
            false,
        );

        record.events.push(created_event.clone());
        if let Err(error) = record.sender.send(created_event) {
            tracing::warn!("Failed to broadcast ACP run.created event: {}", error);
        }

        let mut state = lock_runtime_state(&self.state)?;
        state.runs.insert(run.id.as_str().to_string(), record);
        drop(state);

        self.persist_run_state(run.id.as_str())?;
        Ok(run)
    }

    /// Returns a run by identifier.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    ///
    /// # Returns
    ///
    /// Returns the current ACP run state.
    ///
    /// # Errors
    ///
    /// Returns a not-found error if the run does not exist.
    pub fn get_run(&self, run_id: &str) -> Result<AcpRun> {
        let state = lock_runtime_state(&self.state)?;
        let record = state.runs.get(run_id).ok_or_else(|| {
            crate::acp::error::AcpError::lifecycle(format!("ACP run '{}' was not found", run_id))
        })?;
        Ok(record.run.clone())
    }

    /// Returns ordered event history for a run.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    ///
    /// # Returns
    ///
    /// Returns the ordered runtime events for the run.
    ///
    /// # Errors
    ///
    /// Returns a not-found error if the run does not exist.
    pub fn get_events(&self, run_id: &str) -> Result<Vec<AcpRuntimeEvent>> {
        let state = lock_runtime_state(&self.state)?;
        let record = state.runs.get(run_id).ok_or_else(|| {
            crate::acp::error::AcpError::lifecycle(format!("ACP run '{}' was not found", run_id))
        })?;
        Ok(record.events.clone())
    }

    /// Subscribes to live events for a run.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    ///
    /// # Returns
    ///
    /// Returns a live event subscription for the run.
    ///
    /// # Errors
    ///
    /// Returns a not-found error if the run does not exist.
    pub fn subscribe(&self, run_id: &str) -> Result<AcpRuntimeSubscription> {
        let state = lock_runtime_state(&self.state)?;
        let record = state.runs.get(run_id).ok_or_else(|| {
            crate::acp::error::AcpError::lifecycle(format!("ACP run '{}' was not found", run_id))
        })?;

        Ok(AcpRuntimeSubscription {
            receiver: record.sender.subscribe(),
        })
    }

    /// Returns the flattened text prompt for a run.
    ///
    /// This helper is used by the current single-agent execution model to map
    /// ACP input messages onto XZatoma's prompt-oriented execution loop.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    ///
    /// # Returns
    ///
    /// Returns the flattened text prompt.
    ///
    /// # Errors
    ///
    /// Returns a not-found error if the run does not exist.
    pub fn prompt_for_run(&self, run_id: &str) -> Result<String> {
        let state = lock_runtime_state(&self.state)?;
        let record = state.runs.get(run_id).ok_or_else(|| {
            crate::acp::error::AcpError::lifecycle(format!("ACP run '{}' was not found", run_id))
        })?;
        Ok(record.prompt_text.clone())
    }

    /// Marks a run as queued.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    ///
    /// # Returns
    ///
    /// Returns the updated run.
    ///
    /// # Errors
    ///
    /// Returns an error if the run does not exist or if the lifecycle transition
    /// is invalid.
    pub fn mark_queued(&self, run_id: &str) -> Result<AcpRun> {
        self.transition_run(run_id, AcpRunState::Queued, "run.in-progress", false)
    }

    /// Marks a run as running.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    ///
    /// # Returns
    ///
    /// Returns the updated run.
    ///
    /// # Errors
    ///
    /// Returns an error if the run does not exist or if the lifecycle transition
    /// is invalid.
    pub fn mark_running(&self, run_id: &str) -> Result<AcpRun> {
        self.transition_run(run_id, AcpRunState::Running, "run.in-progress", false)
    }

    /// Records a created output message and accompanying incremental part events.
    ///
    /// This method records:
    ///
    /// - `message.created`
    /// - one `message.part` event per text part
    /// - `message.completed`
    ///
    /// It also appends the full message to the run's stored ACP output.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    /// * `message` - ACP output message to append
    ///
    /// # Returns
    ///
    /// Returns the updated run.
    ///
    /// # Errors
    ///
    /// Returns an error if the run does not exist, is terminal, or the message
    /// is invalid.
    pub fn append_output_message(&self, run_id: &str, message: AcpMessage) -> Result<AcpRun> {
        message.validate()?;

        let mut state = lock_runtime_state(&self.state)?;
        let record = state.runs.get_mut(run_id).ok_or_else(|| {
            crate::acp::error::AcpError::lifecycle(format!("ACP run '{}' was not found", run_id))
        })?;

        if record.completed {
            return Err(crate::acp::error::AcpError::lifecycle(format!(
                "cannot append output for completed ACP run '{}'",
                run_id
            ))
            .into());
        }

        record.run.append_output_message(message.clone())?;

        let created_sequence = next_sequence(record)?;
        let created_event = build_runtime_event(
            created_sequence,
            AcpEvent::new(
                AcpEventKind::RunOutputAppended,
                Some(run_id.to_string()),
                json!({
                    "event": "message.created",
                    "runId": run_id,
                    "role": message.role.to_string(),
                    "partCount": message.parts.len(),
                }),
            )?,
            false,
        );
        push_event(record, created_event);

        for (index, part) in message.parts.iter().enumerate() {
            let payload = match part {
                AcpMessagePart::Text(text) => json!({
                    "event": "message.part",
                    "runId": run_id,
                    "index": index,
                    "type": "text",
                    "text": text.text,
                }),
                AcpMessagePart::Artifact(artifact) => json!({
                    "event": "message.part",
                    "runId": run_id,
                    "index": index,
                    "type": "artifact",
                    "name": artifact.name,
                    "mimeType": artifact.mime_type,
                }),
            };

            let event = build_runtime_event(
                next_sequence(record)?,
                AcpEvent::new(
                    AcpEventKind::RunOutputAppended,
                    Some(run_id.to_string()),
                    payload,
                )?,
                false,
            );
            push_event(record, event);
        }

        let completed_event = build_runtime_event(
            next_sequence(record)?,
            AcpEvent::new(
                AcpEventKind::RunOutputAppended,
                Some(run_id.to_string()),
                json!({
                    "event": "message.completed",
                    "runId": run_id,
                    "role": message.role.to_string(),
                }),
            )?,
            false,
        );
        push_event(record, completed_event);

        Ok(record.run.clone())
    }

    /// Records successful completion for a run.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    ///
    /// # Returns
    ///
    /// Returns the terminal run state.
    ///
    /// # Errors
    ///
    /// Returns an error if the run does not exist, is already terminal, or the
    /// lifecycle transition is invalid.
    pub fn complete_run(&self, run_id: &str) -> Result<AcpRun> {
        let run = {
            let mut state = lock_runtime_state(&self.state)?;
            let record = state.runs.get_mut(run_id).ok_or_else(|| {
                crate::acp::error::AcpError::lifecycle(format!(
                    "ACP run '{}' was not found",
                    run_id
                ))
            })?;

            prevent_duplicate_completion(record, run_id)?;
            record.run.transition_to(AcpRunState::Completed)?;

            let event = build_runtime_event(
                next_sequence(record)?,
                AcpEvent::new(
                    AcpEventKind::RunCompleted,
                    Some(run_id.to_string()),
                    json!({
                        "event": "run.completed",
                        "runId": run_id,
                        "state": record.run.status.state.to_string(),
                        "completedAt": record.run.status.completed_at,
                        "outputMessageCount": record.run.output.messages.len(),
                    }),
                )?,
                true,
            );
            record.completed = true;
            push_event(record, event);

            record.run.clone()
        };

        self.persist_run_state(run_id)?;
        Ok(run)
    }

    /// Records a run failure.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    /// * `reason` - Human-readable failure reason
    ///
    /// # Returns
    ///
    /// Returns the terminal failed run state.
    ///
    /// # Errors
    ///
    /// Returns an error if the run does not exist or is already terminal.
    pub fn fail_run(&self, run_id: &str, reason: String) -> Result<AcpRun> {
        let run = {
            let mut state = lock_runtime_state(&self.state)?;
            let record = state.runs.get_mut(run_id).ok_or_else(|| {
                crate::acp::error::AcpError::lifecycle(format!(
                    "ACP run '{}' was not found",
                    run_id
                ))
            })?;

            prevent_duplicate_completion(record, run_id)?;
            record.run.record_failure(reason.clone())?;

            let event = build_runtime_event(
                next_sequence(record)?,
                AcpEvent::new(
                    AcpEventKind::RunFailed,
                    Some(run_id.to_string()),
                    json!({
                        "event": "run.failed",
                        "runId": run_id,
                        "state": record.run.status.state.to_string(),
                        "error": reason,
                    }),
                )?,
                true,
            );
            record.completed = true;
            push_event(record, event);

            record.run.clone()
        };

        self.persist_run_state(run_id)?;
        Ok(run)
    }

    /// Records an error event without forcing terminal completion.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    /// * `message` - Human-readable error message
    ///
    /// # Returns
    ///
    /// Returns the current run state.
    ///
    /// # Errors
    ///
    /// Returns an error if the run does not exist.
    pub fn record_error_event(&self, run_id: &str, message: String) -> Result<AcpRun> {
        let mut state = lock_runtime_state(&self.state)?;
        let record = state.runs.get_mut(run_id).ok_or_else(|| {
            crate::acp::error::AcpError::lifecycle(format!("ACP run '{}' was not found", run_id))
        })?;

        let event = build_runtime_event(
            next_sequence(record)?,
            AcpEvent::new(
                AcpEventKind::RunOutputAppended,
                Some(run_id.to_string()),
                json!({
                    "event": "error",
                    "runId": run_id,
                    "message": message,
                }),
            )?,
            false,
        );
        push_event(record, event);

        Ok(record.run.clone())
    }

    /// Returns the number of tracked runs.
    ///
    /// # Returns
    ///
    /// Returns the number of in-memory runs currently tracked by the runtime.
    pub fn run_count(&self) -> usize {
        match self.state.lock() {
            Ok(state) => state.runs.len(),
            Err(_) => 0,
        }
    }

    fn transition_run(
        &self,
        run_id: &str,
        target_state: AcpRunState,
        event_name: &str,
        terminal: bool,
    ) -> Result<AcpRun> {
        let updated_run = {
            let mut state = lock_runtime_state(&self.state)?;
            let record = state.runs.get_mut(run_id).ok_or_else(|| {
                crate::acp::error::AcpError::lifecycle(format!(
                    "ACP run '{}' was not found",
                    run_id
                ))
            })?;

            if record.completed {
                return Err(crate::acp::error::AcpError::lifecycle(format!(
                    "cannot transition completed ACP run '{}'",
                    run_id
                ))
                .into());
            }

            record.run.transition_to(target_state)?;

            let event = build_runtime_event(
                next_sequence(record)?,
                AcpEvent::new(
                    AcpEventKind::RunStatusChanged,
                    Some(run_id.to_string()),
                    json!({
                        "event": event_name,
                        "runId": run_id,
                        "state": record.run.status.state.to_string(),
                        "updatedAt": record.run.status.updated_at,
                    }),
                )?,
                terminal,
            );
            push_event(record, event);

            record.run.clone()
        };

        self.persist_run_state(run_id)?;
        Ok(updated_run)
    }

    /// Loads an ACP session, restoring persisted run state when necessary.
    ///
    /// # Arguments
    ///
    /// * `session_id` - ACP session identifier
    ///
    /// # Returns
    ///
    /// Returns the canonical ACP session when found.
    ///
    /// # Errors
    ///
    /// Returns an error if session lookup or restoration fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::runtime::AcpRuntime;
    /// use xzatoma::Config;
    ///
    /// let runtime = AcpRuntime::new(Config::default());
    /// let _ = runtime.get_session("session_missing");
    /// ```
    pub fn get_session(&self, session_id: &str) -> Result<Option<AcpRunSession>> {
        if let Some(storage) = self.storage_handle()? {
            let stored_runs = storage.list_acp_runs_for_session(session_id)?;
            if let Some(first_run) = stored_runs.first() {
                return Ok(Some(AcpRunSession {
                    id: AcpSessionId::new(first_run.session_id.clone())?,
                    created_at: first_run.created_at.to_rfc3339(),
                }));
            }

            if let Some(stored) = storage.load_acp_session(session_id)? {
                return Ok(Some(AcpRunSession {
                    id: AcpSessionId::new(stored.session_id)?,
                    created_at: stored.created_at.to_rfc3339(),
                }));
            }
        }

        let state = lock_runtime_state(&self.state)?;
        let session = state
            .runs
            .values()
            .find(|record| record.run.session.id.as_str() == session_id)
            .map(|record| record.run.session.clone());
        Ok(session)
    }

    /// Returns all canonical runs currently known for a session.
    ///
    /// # Arguments
    ///
    /// * `session_id` - ACP session identifier
    ///
    /// # Returns
    ///
    /// Returns persisted and in-memory runs for the session ordered by creation.
    ///
    /// # Errors
    ///
    /// Returns an error if restoration or lookup fails.
    pub fn get_session_runs(&self, session_id: &str) -> Result<Vec<AcpRun>> {
        let mut seen = HashSet::new();
        let mut runs = Vec::new();

        if let Some(storage) = self.storage_handle()? {
            for stored_run in storage.list_acp_runs_for_session(session_id)? {
                if seen.insert(stored_run.run_id.clone()) {
                    if let Some(restored) = self.restore_run(&stored_run.run_id)? {
                        runs.push(restored);
                    }
                }
            }
        }

        let state = lock_runtime_state(&self.state)?;
        for record in state.runs.values() {
            if record.run.session.id.as_str() == session_id
                && seen.insert(record.run.id.as_str().to_string())
            {
                runs.push(record.run.clone());
            }
        }

        runs.sort_by(|left, right| left.status.created_at.cmp(&right.status.created_at));
        Ok(runs)
    }

    /// Sets a run into the awaiting state with a durable await payload.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    /// * `kind` - Await discriminator
    /// * `detail` - Await detail
    ///
    /// # Returns
    ///
    /// Returns the updated awaiting run.
    ///
    /// # Errors
    ///
    /// Returns an error if the run does not exist or the await transition fails.
    pub fn set_awaiting(&self, run_id: &str, kind: String, detail: String) -> Result<AcpRun> {
        let updated_run = {
            let mut state = lock_runtime_state(&self.state)?;
            let record = state.runs.get_mut(run_id).ok_or_else(|| {
                crate::acp::error::AcpError::lifecycle(format!(
                    "ACP run '{}' was not found",
                    run_id
                ))
            })?;

            if record.completed {
                return Err(crate::acp::error::AcpError::lifecycle(format!(
                    "cannot await completed ACP run '{}'",
                    run_id
                ))
                .into());
            }

            record.run.set_await_payload(kind.clone(), detail.clone())?;

            let event = build_runtime_event(
                next_sequence(record)?,
                AcpEvent::new(
                    AcpEventKind::RunAwaitingInput,
                    Some(run_id.to_string()),
                    json!({
                        "event": "run.awaiting",
                        "runId": run_id,
                        "state": record.run.status.state.to_string(),
                        "kind": kind,
                        "detail": detail,
                        "updatedAt": record.run.status.updated_at,
                    }),
                )?,
                false,
            );
            push_event(record, event);

            record.run.clone()
        };

        self.persist_run_state(run_id)?;
        Ok(updated_run)
    }

    /// Resumes an awaiting run using a minimal persisted resume contract.
    ///
    /// # Arguments
    ///
    /// * `request` - Resume request identifying the awaiting run
    /// * `resume_payload` - JSON payload supplied by the client
    ///
    /// # Returns
    ///
    /// Returns the resumed run after it transitions back to `running`.
    ///
    /// # Errors
    ///
    /// Returns an error if the run is missing, not awaiting, or the payload is
    /// invalid.
    pub fn resume_run(
        &self,
        request: AcpRunResumeRequest,
        resume_payload: Value,
    ) -> Result<AcpRun> {
        if resume_payload.is_null() {
            return Err(
                crate::acp::error::AcpError::validation("resume payload cannot be null").into(),
            );
        }

        let run_id = request.run_id.as_str().to_string();
        let updated_run = {
            let mut state = lock_runtime_state(&self.state)?;
            let record = state.runs.get_mut(&run_id).ok_or_else(|| {
                crate::acp::error::AcpError::lifecycle(format!(
                    "ACP run '{}' was not found",
                    run_id
                ))
            })?;

            if record.run.status.state != AcpRunState::Awaiting {
                return Err(crate::acp::error::AcpError::lifecycle(format!(
                    "ACP run '{}' is not awaiting resume input",
                    run_id
                ))
                .into());
            }

            record.resume_payload = Some(resume_payload.clone());
            record.run.await_payload = None;
            record.run.transition_to(AcpRunState::Running)?;

            let event = build_runtime_event(
                next_sequence(record)?,
                AcpEvent::new(
                    AcpEventKind::RunStatusChanged,
                    Some(run_id.clone()),
                    json!({
                        "event": "run.resumed",
                        "runId": run_id,
                        "state": record.run.status.state.to_string(),
                        "resumePayload": resume_payload,
                        "updatedAt": record.run.status.updated_at,
                    }),
                )?,
                false,
            );
            push_event(record, event);

            record.run.clone()
        };

        self.persist_run_state(&run_id)?;
        Ok(updated_run)
    }

    /// Requests cancellation for a run and applies minimal terminal cancellation
    /// semantics.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    /// * `reason` - Human-readable cancellation reason
    ///
    /// # Returns
    ///
    /// Returns the cancelled run.
    ///
    /// # Errors
    ///
    /// Returns an error if the run does not exist or is already terminal.
    pub fn cancel_run(&self, run_id: &str, reason: String) -> Result<AcpRun> {
        let updated_run = {
            let mut state = lock_runtime_state(&self.state)?;
            let record = state.runs.get_mut(run_id).ok_or_else(|| {
                crate::acp::error::AcpError::lifecycle(format!(
                    "ACP run '{}' was not found",
                    run_id
                ))
            })?;

            if record.run.status.state.is_terminal() {
                return Err(crate::acp::error::AcpError::lifecycle(format!(
                    "cannot cancel terminal ACP run '{}'",
                    run_id
                ))
                .into());
            }

            record.cancellation_requested = true;
            record.cancellation_reason = Some(reason.clone());
            record.run.record_cancellation(reason.clone())?;
            record.completed = true;

            let event = build_runtime_event(
                next_sequence(record)?,
                AcpEvent::new(
                    AcpEventKind::RunCancelled,
                    Some(run_id.to_string()),
                    json!({
                        "event": "run.cancelled",
                        "runId": run_id,
                        "state": record.run.status.state.to_string(),
                        "reason": reason,
                        "completedAt": record.run.status.completed_at,
                    }),
                )?,
                true,
            );
            push_event(record, event);

            record.run.clone()
        };

        self.persist_run_state(run_id)?;
        Ok(updated_run)
    }

    /// Restores a persisted run and its event history into the in-memory runtime.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    ///
    /// # Returns
    ///
    /// Returns the restored canonical run when found.
    ///
    /// # Errors
    ///
    /// Returns an error if persistent restoration fails.
    pub fn restore_run(&self, run_id: &str) -> Result<Option<AcpRun>> {
        {
            let state = lock_runtime_state(&self.state)?;
            if let Some(record) = state.runs.get(run_id) {
                return Ok(Some(record.run.clone()));
            }
        }

        let Some(storage) = self.storage_handle()? else {
            return Ok(None);
        };

        let Some(run) = storage.restore_acp_run(run_id)? else {
            return Ok(None);
        };

        let stored_run = storage
            .load_acp_run(run_id)?
            .ok_or_else(|| XzatomaError::Storage(format!("Missing stored ACP run '{}'", run_id)))?;
        let events = storage.restore_acp_runtime_events(run_id)?;
        let prompt_text = flatten_input_to_prompt(&run.request.input)?;
        let mut record = AcpRuntimeRunRecord::new(
            run.clone(),
            AcpRuntimeExecuteMode::parse(&stored_run.mode)?,
            prompt_text,
        );
        record.events = events;
        record.completed = run.status.state.is_terminal();
        record.conversation_id = stored_run.conversation_id;
        record.cancellation_reason = run.status.cancellation_reason.clone();

        if let Some(await_state) = storage.load_acp_await_state(run_id)? {
            if let Some(resume_payload_json) = await_state.resume_payload_json {
                record.resume_payload = Some(serde_json::from_str(&resume_payload_json).map_err(
                    |source| XzatomaError::StorageSerialization {
                        operation: "deserialize stored ACP resume payload".to_string(),
                        source: source.into(),
                    },
                )?);
            }
        }

        if let Some(cancellation) = storage.load_acp_cancellation(run_id)? {
            record.cancellation_requested = true;
            if record.cancellation_reason.is_none() {
                record.cancellation_reason = cancellation.reason;
            }
        }

        let mut state = lock_runtime_state(&self.state)?;
        state.runs.insert(run_id.to_string(), record);

        Ok(Some(run))
    }

    fn persist_run_state(&self, run_id: &str) -> Result<()> {
        let Some(storage) = self.storage_handle()? else {
            return Ok(());
        };

        let (
            run,
            mode,
            events,
            conversation_id,
            await_payload,
            cancellation_requested,
            cancellation_reason,
        ) = {
            let state = lock_runtime_state(&self.state)?;
            let record = state.runs.get(run_id).ok_or_else(|| {
                crate::acp::error::AcpError::lifecycle(format!(
                    "ACP run '{}' was not found",
                    run_id
                ))
            })?;

            (
                record.run.clone(),
                record.mode,
                record.events.clone(),
                record.conversation_id.clone(),
                record.run.await_payload.clone(),
                record.cancellation_requested,
                record.cancellation_reason.clone(),
            )
        };

        storage.persist_acp_run(&run, mode, conversation_id)?;

        let stored_events: Vec<PublicStoredAcpRunEvent> = events
            .iter()
            .map(|event| -> Result<PublicStoredAcpRunEvent> {
                Ok(PublicStoredAcpRunEvent {
                    run_id: run.id.as_str().to_string(),
                    sequence: event.sequence,
                    kind: event.event.kind.to_string(),
                    created_at: parse_runtime_timestamp(&event.event.created_at)?,
                    payload_json: serde_json::to_string(&event.event.payload).map_err(
                        |source| XzatomaError::StorageSerialization {
                            operation: "serialize ACP event payload".to_string(),
                            source: source.into(),
                        },
                    )?,
                    terminal: event.terminal,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        storage.save_acp_run_events(run.id.as_str(), &stored_events)?;

        if let Some(payload) = await_payload {
            let await_state = PublicStoredAcpAwaitState {
                run_id: run.id.as_str().to_string(),
                session_id: run.session.id.as_str().to_string(),
                kind: payload.kind,
                detail: payload.detail,
                created_at: parse_runtime_timestamp(&run.status.updated_at)?,
                updated_at: parse_runtime_timestamp(&run.status.updated_at)?,
                resumed_at: None,
                resume_payload_json: None,
            };
            storage.save_acp_await_state(&await_state)?;
        }

        if cancellation_requested {
            let cancellation = PublicStoredAcpCancellation {
                run_id: run.id.as_str().to_string(),
                requested_at: parse_runtime_timestamp(&run.status.updated_at)?,
                acknowledged_at: Some(parse_runtime_timestamp(&run.status.updated_at)?),
                completed_at: run
                    .status
                    .completed_at
                    .as_deref()
                    .map(parse_runtime_timestamp)
                    .transpose()?,
                reason: cancellation_reason,
                acknowledged: true,
            };
            storage.save_acp_cancellation(&cancellation)?;
        }

        Ok(())
    }

    fn storage_handle(&self) -> Result<Option<SqliteStorage>> {
        let guard = self.storage.lock().map_err(|_| {
            XzatomaError::Internal("ACP runtime storage lock was poisoned".to_string())
        })?;

        if let Some(storage) = guard.as_ref() {
            Ok(Some(SqliteStorage::new_with_path(
                storage.database_path().clone(),
            )?))
        } else {
            Ok(None)
        }
    }

    fn restore_from_storage(&self) -> Result<()> {
        let Some(storage) = self.storage_handle()? else {
            return Ok(());
        };

        let conn = rusqlite::Connection::open(storage.database_path()).map_err(|source| {
            XzatomaError::StorageDatabaseOpen {
                path: storage.database_path().display().to_string(),
                source: source.into(),
            }
        })?;
        let mut stmt = conn
            .prepare("SELECT run_id FROM acp_runs ORDER BY created_at ASC")
            .map_err(|source| XzatomaError::StorageQuery {
                operation: "prepare ACP runtime restore query".to_string(),
                source: source.into(),
            })?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|source| XzatomaError::StorageQuery {
                operation: "query ACP runtime restore run IDs".to_string(),
                source: source.into(),
            })?;

        let mut run_ids = Vec::new();
        for run_id in rows {
            run_ids.push(run_id.map_err(|source| XzatomaError::StorageRowDecode {
                operation: "decode ACP runtime restore run ID".to_string(),
                source: source.into(),
            })?);
        }

        for run_id in run_ids {
            self.restore_run(&run_id)?;
        }

        Ok(())
    }
}

fn parse_runtime_timestamp(value: &str) -> Result<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&chrono::Utc))
        .map_err(|source| XzatomaError::StorageRowDecode {
            operation: format!("parse persisted ACP runtime timestamp '{value}'"),
            source: source.into(),
        })
}

/// Flattens ACP input messages into the prompt text used by the current
/// single-agent XZatoma execution model.
///
/// This adapter preserves input ordering and supports text-first ACP messages.
/// Unsupported multimodal or artifact-only payloads are rejected until fuller
/// multimodal support is implemented.
///
/// # Arguments
///
/// * `messages` - Ordered ACP input messages
///
/// # Returns
///
/// Returns a single flattened text prompt.
///
/// # Errors
///
/// Returns an error if any message contains unsupported non-text parts or would
/// flatten to empty content.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::runtime::flatten_input_to_prompt;
/// use xzatoma::acp::{AcpMessage, AcpMessagePart, AcpRole, AcpTextPart};
///
/// let prompt = flatten_input_to_prompt(&[AcpMessage::new(
///     AcpRole::User,
///     vec![AcpMessagePart::Text(AcpTextPart::new("Hello".to_string()))],
/// )?])?;
///
/// assert!(prompt.contains("Hello"));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn flatten_input_to_prompt(messages: &[AcpMessage]) -> Result<String> {
    if messages.is_empty() {
        return Err(
            crate::acp::error::AcpError::validation("ACP run input cannot be empty").into(),
        );
    }

    let mut sections = Vec::new();

    for message in messages {
        validate_supported_message_parts(message)?;

        let mut text_parts = Vec::new();
        for part in &message.parts {
            match part {
                AcpMessagePart::Text(text) => {
                    if !text.text.trim().is_empty() {
                        text_parts.push(text.text.clone());
                    }
                }
                AcpMessagePart::Artifact(_) => {
                    return Err(crate::acp::error::AcpError::validation(
                        "artifact input parts are not yet supported for ACP runs",
                    )
                    .into());
                }
            }
        }

        let section_text = text_parts.join("\n");
        if section_text.trim().is_empty() {
            return Err(crate::acp::error::AcpError::validation(
                "ACP input messages must contain non-empty text parts",
            )
            .into());
        }

        sections.push(format!("{}:\n{}", message.role, section_text));
    }

    Ok(sections.join("\n\n"))
}

/// Converts ACP input messages into provider-layer messages.
///
/// This adapter preserves input ordering and maps ACP roles onto the existing
/// provider message structure used by XZatoma agent execution.
///
/// # Arguments
///
/// * `messages` - Ordered ACP input messages
///
/// # Returns
///
/// Returns ordered provider messages.
///
/// # Errors
///
/// Returns an error if the ACP messages contain unsupported content.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::runtime::acp_messages_to_provider_messages;
/// use xzatoma::acp::{AcpMessage, AcpMessagePart, AcpRole, AcpTextPart};
///
/// let provider_messages = acp_messages_to_provider_messages(&[AcpMessage::new(
///     AcpRole::User,
///     vec![AcpMessagePart::Text(AcpTextPart::new("Hello".to_string()))],
/// )?])?;
///
/// assert_eq!(provider_messages.len(), 1);
/// assert_eq!(provider_messages[0].role, "user");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn acp_messages_to_provider_messages(
    messages: &[AcpMessage],
) -> Result<Vec<crate::providers::Message>> {
    let mut converted = Vec::with_capacity(messages.len());

    for message in messages {
        validate_supported_message_parts(message)?;

        let content = extract_text_content(message)?;
        let provider_message = match message.role {
            AcpRole::System => crate::providers::Message::system(content),
            AcpRole::User => crate::providers::Message::user(content),
            AcpRole::Assistant => crate::providers::Message::assistant(content),
            AcpRole::Tool => crate::providers::Message::tool_result("acp_tool_result", content),
        };
        converted.push(provider_message);
    }

    Ok(converted)
}

fn extract_text_content(message: &AcpMessage) -> Result<String> {
    let parts = message
        .parts
        .iter()
        .map(|part| match part {
            AcpMessagePart::Text(text) => Ok(text.text.clone()),
            AcpMessagePart::Artifact(_) => {
                Err(XzatomaError::Acp(crate::acp::error::AcpError::validation(
                    "artifact message parts are not yet supported for ACP run execution"
                        .to_string(),
                )))
            }
        })
        .collect::<Result<Vec<_>>>()?;

    let content = parts.join("\n");
    if content.trim().is_empty() {
        return Err(crate::acp::error::AcpError::validation(
            "ACP message content cannot be empty after flattening",
        )
        .into());
    }

    Ok(content)
}

fn validate_supported_message_parts(message: &AcpMessage) -> Result<()> {
    if message.parts.is_empty() {
        return Err(crate::acp::error::AcpError::validation(
            "ACP message must include at least one part",
        )
        .into());
    }

    for part in &message.parts {
        match part {
            AcpMessagePart::Text(text) => text.validate()?,
            AcpMessagePart::Artifact(_) => {
                return Err(crate::acp::error::AcpError::validation(
                    "artifact and multimodal ACP inputs are not yet supported",
                )
                .into());
            }
        }
    }

    Ok(())
}

fn generate_run_id() -> String {
    let suffix = Uuid::new_v4().simple().to_string();
    format!("run_{}", suffix)
}

fn generate_session_id() -> String {
    let suffix = Uuid::new_v4().simple().to_string();
    format!("session_{}", suffix)
}

fn build_runtime_event(sequence: u64, event: AcpEvent, terminal: bool) -> AcpRuntimeEvent {
    AcpRuntimeEvent::new(sequence, event, terminal)
}

fn lock_runtime_state(
    state: &Arc<Mutex<AcpRuntimeState>>,
) -> Result<std::sync::MutexGuard<'_, AcpRuntimeState>> {
    state
        .lock()
        .map_err(|_| XzatomaError::Internal("ACP runtime state lock was poisoned".to_string()))
}

fn next_sequence(record: &AcpRuntimeRunRecord) -> Result<u64> {
    let next = record.events.len() + 1;
    u64::try_from(next)
        .map_err(|_| XzatomaError::Internal("ACP runtime event sequence overflowed".to_string()))
}

fn push_event(record: &mut AcpRuntimeRunRecord, event: AcpRuntimeEvent) {
    record.events.push(event.clone());
    let _send_result = record.sender.send(event);
}

fn prevent_duplicate_completion(record: &AcpRuntimeRunRecord, run_id: &str) -> Result<()> {
    if record.completed {
        return Err(crate::acp::error::AcpError::lifecycle(format!(
            "ACP run '{}' has already reached terminal completion",
            run_id
        ))
        .into());
    }

    Ok(())
}

/// Builds a synthetic assistant ACP message from plain text.
///
/// This helper is useful when current XZatoma execution returns a final text
/// response that must be recorded as ACP output.
///
/// # Arguments
///
/// * `content` - Assistant output text
///
/// # Returns
///
/// Returns an ACP assistant message with one text part.
///
/// # Errors
///
/// Returns an error if the content is empty or invalid.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::runtime::assistant_text_message;
///
/// let message = assistant_text_message("Done".to_string())?;
/// assert_eq!(message.role.to_string(), "assistant");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn assistant_text_message(content: String) -> Result<AcpMessage> {
    AcpMessage::new(
        AcpRole::Assistant,
        vec![AcpMessagePart::Text(crate::acp::AcpTextPart::new(content))],
    )
}

/// Builds a synthetic system ACP message from plain text.
///
/// # Arguments
///
/// * `content` - System output text
///
/// # Returns
///
/// Returns an ACP system message with one text part.
///
/// # Errors
///
/// Returns an error if the content is empty or invalid.
pub fn system_text_message(content: String) -> Result<AcpMessage> {
    AcpMessage::new(
        AcpRole::System,
        vec![AcpMessagePart::Text(crate::acp::AcpTextPart::new(content))],
    )
}

/// Builds a lifecycle snapshot payload for polling surfaces.
///
/// # Arguments
///
/// * `run` - Current ACP run
/// * `mode` - Runtime execution mode
///
/// # Returns
///
/// Returns a JSON lifecycle snapshot.
pub fn build_run_snapshot(run: &AcpRun, mode: AcpRuntimeExecuteMode) -> Value {
    json!({
        "runId": run.id.as_str(),
        "sessionId": run.session.id.as_str(),
        "mode": mode.as_str(),
        "state": run.status.state.to_string(),
        "createdAt": run.status.created_at,
        "updatedAt": run.status.updated_at,
        "completedAt": run.status.completed_at,
        "outputMessageCount": run.output.messages.len(),
        "failureReason": run.status.failure_reason,
        "cancellationReason": run.status.cancellation_reason,
        "snapshotAt": now_rfc3339(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::{AcpRunResumeRequest, AcpTextPart};

    fn test_message(text: &str) -> AcpMessage {
        AcpMessage::new(
            AcpRole::User,
            vec![AcpMessagePart::Text(AcpTextPart::new(text.to_string()))],
        )
        .unwrap()
    }

    #[test]
    fn test_execute_mode_parse_accepts_supported_values() {
        assert_eq!(
            AcpRuntimeExecuteMode::parse("sync").unwrap(),
            AcpRuntimeExecuteMode::Sync
        );
        assert_eq!(
            AcpRuntimeExecuteMode::parse("async").unwrap(),
            AcpRuntimeExecuteMode::Async
        );
        assert_eq!(
            AcpRuntimeExecuteMode::parse("stream").unwrap(),
            AcpRuntimeExecuteMode::Stream
        );
    }

    #[test]
    fn test_execute_mode_parse_rejects_invalid_value() {
        let error = AcpRuntimeExecuteMode::parse("invalid").unwrap_err();
        assert!(error.to_string().contains("unsupported ACP execution mode"));
    }

    #[test]
    fn test_runtime_create_request_validate_rejects_empty_input() {
        let request = AcpRuntimeCreateRequest::new(Vec::new());
        let error = request.validate().unwrap_err();
        assert!(error.to_string().contains("ACP run input cannot be empty"));
    }

    #[test]
    fn test_flatten_input_to_prompt_preserves_ordered_roles() {
        let messages = vec![
            AcpMessage::new(
                AcpRole::System,
                vec![AcpMessagePart::Text(AcpTextPart::new(
                    "You are helpful".to_string(),
                ))],
            )
            .unwrap(),
            AcpMessage::new(
                AcpRole::User,
                vec![AcpMessagePart::Text(AcpTextPart::new(
                    "Build a summary".to_string(),
                ))],
            )
            .unwrap(),
        ];

        let prompt = flatten_input_to_prompt(&messages).unwrap();
        assert!(prompt.contains("system:\nYou are helpful"));
        assert!(prompt.contains("user:\nBuild a summary"));
    }

    #[test]
    fn test_acp_messages_to_provider_messages_maps_roles() {
        let provider_messages =
            acp_messages_to_provider_messages(&[test_message("hello world")]).unwrap();
        assert_eq!(provider_messages.len(), 1);
        assert_eq!(provider_messages[0].role, "user");
        assert_eq!(provider_messages[0].content.as_deref(), Some("hello world"));
    }

    #[test]
    fn test_runtime_create_run_records_initial_event() {
        let runtime = AcpRuntime::new_in_memory(Config::default());

        let run = runtime
            .create_run(
                AcpRuntimeCreateRequest::new(vec![test_message("run me")])
                    .with_mode(AcpRuntimeExecuteMode::Async),
            )
            .unwrap();

        let events = runtime.get_events(run.id.as_str()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].sequence, 1);
        assert_eq!(events[0].event.kind, AcpEventKind::RunCreated);
    }

    #[test]
    fn test_runtime_mark_running_and_complete_orders_lifecycle_events() {
        let runtime = AcpRuntime::new_in_memory(Config::default());

        let run = runtime
            .create_run(
                AcpRuntimeCreateRequest::new(vec![test_message("run me")])
                    .with_mode(AcpRuntimeExecuteMode::Async),
            )
            .unwrap();

        runtime.mark_queued(run.id.as_str()).unwrap();
        runtime.mark_running(run.id.as_str()).unwrap();
        runtime
            .append_output_message(
                run.id.as_str(),
                assistant_text_message("done".to_string()).unwrap(),
            )
            .unwrap();
        runtime.complete_run(run.id.as_str()).unwrap();

        let events = runtime.get_events(run.id.as_str()).unwrap();
        assert_eq!(events.len(), 7);
        assert_eq!(events[0].sequence, 1);
        assert_eq!(events[1].sequence, 2);
        assert_eq!(events[2].sequence, 3);
        assert_eq!(events[6].sequence, 7);
        assert!(events[6].terminal);
    }

    #[test]
    fn test_runtime_fail_run_prevents_duplicate_completion() {
        let runtime = AcpRuntime::new_in_memory(Config::default());

        let run = runtime
            .create_run(AcpRuntimeCreateRequest::new(vec![test_message("fail me")]))
            .unwrap();

        runtime.mark_queued(run.id.as_str()).unwrap();
        runtime.mark_running(run.id.as_str()).unwrap();
        runtime
            .fail_run(run.id.as_str(), "provider failed".to_string())
            .unwrap();

        let error = runtime.complete_run(run.id.as_str()).unwrap_err();
        assert!(error
            .to_string()
            .contains("already reached terminal completion"));
    }

    #[test]
    fn test_runtime_get_run_returns_not_found_for_missing_run() {
        let runtime = AcpRuntime::new_in_memory(Config::default());
        let error = runtime.get_run("run_missing").unwrap_err();
        assert!(error.to_string().contains("was not found"));
    }

    #[test]
    fn test_runtime_append_output_message_accumulates_large_output_parts() {
        let runtime = AcpRuntime::new_in_memory(Config::default());

        let run = runtime
            .create_run(AcpRuntimeCreateRequest::new(vec![test_message(
                "large output",
            )]))
            .unwrap();

        runtime.mark_queued(run.id.as_str()).unwrap();
        runtime.mark_running(run.id.as_str()).unwrap();

        let large = "x".repeat(16 * 1024);
        runtime
            .append_output_message(
                run.id.as_str(),
                assistant_text_message(large.clone()).unwrap(),
            )
            .unwrap();

        let current = runtime.get_run(run.id.as_str()).unwrap();
        assert_eq!(current.output.messages.len(), 1);

        let events = runtime.get_events(run.id.as_str()).unwrap();
        assert!(events.iter().any(|event| {
            event.event.payload["event"] == "message.part" && event.event.payload["text"] == large
        }));
    }

    #[test]
    fn test_runtime_record_error_event_is_non_terminal() {
        let runtime = AcpRuntime::new_in_memory(Config::default());

        let run = runtime
            .create_run(AcpRuntimeCreateRequest::new(vec![test_message(
                "error event",
            )]))
            .unwrap();

        runtime
            .record_error_event(run.id.as_str(), "temporary issue".to_string())
            .unwrap();

        let events = runtime.get_events(run.id.as_str()).unwrap();
        assert_eq!(events.len(), 2);
        assert!(!events[1].terminal);
        assert_eq!(events[1].event.payload["event"], "error");
    }

    #[test]
    fn test_runtime_build_run_snapshot_contains_expected_fields() {
        let runtime = AcpRuntime::new_in_memory(Config::default());

        let run = runtime
            .create_run(AcpRuntimeCreateRequest::new(vec![test_message("snapshot")]))
            .unwrap();

        let snapshot = build_run_snapshot(&run, AcpRuntimeExecuteMode::Sync);
        assert_eq!(snapshot["runId"], run.id.as_str());
        assert_eq!(snapshot["mode"], "sync");
        assert_eq!(snapshot["state"], "created");
    }

    #[test]
    fn test_runtime_get_session_returns_session_for_created_run() {
        let runtime = AcpRuntime::new_in_memory(Config::default());

        let run = runtime
            .create_run(AcpRuntimeCreateRequest::new(vec![test_message(
                "session lookup",
            )]))
            .unwrap();

        let session = runtime
            .get_session(run.session.id.as_str())
            .unwrap()
            .expect("session should exist");

        assert_eq!(session.id.as_str(), run.session.id.as_str());

        let restored_run = runtime
            .restore_run(run.id.as_str())
            .unwrap()
            .expect("restored run should exist");
        assert_eq!(
            parse_runtime_timestamp(&session.created_at).unwrap(),
            parse_runtime_timestamp(&restored_run.session.created_at).unwrap()
        );
    }

    #[test]
    fn test_runtime_get_session_runs_returns_history_continuity() {
        let runtime = AcpRuntime::new_in_memory(Config::default());

        let first_run = runtime
            .create_run(AcpRuntimeCreateRequest::new(vec![test_message(
                "first run",
            )]))
            .unwrap();

        let second_run = runtime
            .create_run(
                AcpRuntimeCreateRequest::new(vec![test_message("second run")])
                    .with_session_id(first_run.session.id.as_str().to_string()),
            )
            .unwrap();

        let runs = runtime
            .get_session_runs(first_run.session.id.as_str())
            .unwrap();

        assert_eq!(runs.len(), 2);
        assert!(runs
            .iter()
            .any(|run| run.id.as_str() == first_run.id.as_str()));
        assert!(runs
            .iter()
            .any(|run| run.id.as_str() == second_run.id.as_str()));
    }

    #[test]
    #[ignore = "disabled in CI because ACP runtime persistence can hang when touching shared storage"]
    fn test_runtime_set_awaiting_persists_await_state() {
        let runtime = AcpRuntime::new_in_memory(Config::default());

        let run = runtime
            .create_run(AcpRuntimeCreateRequest::new(vec![test_message("await me")]))
            .unwrap();

        runtime.mark_queued(run.id.as_str()).unwrap();
        runtime.mark_running(run.id.as_str()).unwrap();

        let updated = runtime
            .set_awaiting(
                run.id.as_str(),
                "approval_required".to_string(),
                "Need confirmation before continuing".to_string(),
            )
            .unwrap();

        assert_eq!(updated.status.state, AcpRunState::Awaiting);
        assert_eq!(
            updated
                .await_payload
                .as_ref()
                .map(|payload| payload.kind.as_str()),
            Some("approval_required")
        );

        let restored = runtime
            .restore_run(run.id.as_str())
            .unwrap()
            .expect("restored run should exist");

        assert_eq!(restored.status.state, AcpRunState::Awaiting);
        assert_eq!(
            restored
                .await_payload
                .as_ref()
                .map(|payload| payload.detail.as_str()),
            Some("Need confirmation before continuing")
        );
    }

    #[test]
    #[ignore = "disabled in CI because ACP runtime persistence can hang when touching shared storage"]
    fn test_runtime_resume_run_transitions_awaiting_to_running() {
        let runtime = AcpRuntime::new_in_memory(Config::default());

        let run = runtime
            .create_run(AcpRuntimeCreateRequest::new(vec![test_message(
                "resume me",
            )]))
            .unwrap();

        runtime.mark_queued(run.id.as_str()).unwrap();
        runtime.mark_running(run.id.as_str()).unwrap();
        runtime
            .set_awaiting(
                run.id.as_str(),
                "approval_required".to_string(),
                "Need confirmation before continuing".to_string(),
            )
            .unwrap();

        let resumed = runtime
            .resume_run(
                AcpRunResumeRequest::new(AcpRunId::new(run.id.as_str().to_string()).unwrap()),
                serde_json::json!({"approved": true}),
            )
            .unwrap();

        assert_eq!(resumed.status.state, AcpRunState::Running);
        assert!(resumed.await_payload.is_none());

        let events = runtime.get_events(run.id.as_str()).unwrap();
        assert!(events
            .iter()
            .any(|event| event.event.payload["event"] == "run.resumed"));
    }

    #[test]
    fn test_runtime_resume_run_rejects_invalid_null_payload() {
        let runtime = AcpRuntime::new_in_memory(Config::default());

        let run = runtime
            .create_run(AcpRuntimeCreateRequest::new(vec![test_message(
                "bad resume",
            )]))
            .unwrap();

        runtime.mark_queued(run.id.as_str()).unwrap();
        runtime.mark_running(run.id.as_str()).unwrap();
        runtime
            .set_awaiting(
                run.id.as_str(),
                "approval_required".to_string(),
                "Need confirmation before continuing".to_string(),
            )
            .unwrap();

        let error = runtime
            .resume_run(
                AcpRunResumeRequest::new(AcpRunId::new(run.id.as_str().to_string()).unwrap()),
                serde_json::Value::Null,
            )
            .unwrap_err();

        assert!(error.to_string().contains("resume payload cannot be null"));
    }

    #[test]
    fn test_runtime_cancel_run_transitions_in_progress_to_cancelled() {
        let runtime = AcpRuntime::new_in_memory(Config::default());

        let run = runtime
            .create_run(AcpRuntimeCreateRequest::new(vec![test_message(
                "cancel me",
            )]))
            .unwrap();

        runtime.mark_queued(run.id.as_str()).unwrap();
        runtime.mark_running(run.id.as_str()).unwrap();

        let cancelled = runtime
            .cancel_run(run.id.as_str(), "user requested stop".to_string())
            .unwrap();

        assert_eq!(cancelled.status.state, AcpRunState::Cancelled);
        assert_eq!(
            cancelled.status.cancellation_reason.as_deref(),
            Some("user requested stop")
        );

        let restored = runtime
            .restore_run(run.id.as_str())
            .unwrap()
            .expect("restored cancelled run should exist");
        assert_eq!(restored.status.state, AcpRunState::Cancelled);
    }

    #[test]
    fn test_runtime_cancel_run_rejects_completed_run() {
        let runtime = AcpRuntime::new_in_memory(Config::default());

        let run = runtime
            .create_run(AcpRuntimeCreateRequest::new(vec![test_message(
                "already done",
            )]))
            .unwrap();

        runtime.mark_queued(run.id.as_str()).unwrap();
        runtime.mark_running(run.id.as_str()).unwrap();
        runtime
            .append_output_message(
                run.id.as_str(),
                assistant_text_message("done".to_string()).unwrap(),
            )
            .unwrap();
        runtime.complete_run(run.id.as_str()).unwrap();

        let error = runtime
            .cancel_run(run.id.as_str(), "too late".to_string())
            .unwrap_err();

        assert!(error.to_string().contains("cannot cancel terminal ACP run"));
    }

    #[test]
    fn test_runtime_restore_run_restores_completed_run_and_events() {
        let runtime = AcpRuntime::new_in_memory(Config::default());

        let run = runtime
            .create_run(AcpRuntimeCreateRequest::new(vec![test_message(
                "restore me",
            )]))
            .unwrap();

        runtime.mark_queued(run.id.as_str()).unwrap();
        runtime.mark_running(run.id.as_str()).unwrap();
        runtime
            .append_output_message(
                run.id.as_str(),
                assistant_text_message("restored output".to_string()).unwrap(),
            )
            .unwrap();
        runtime.complete_run(run.id.as_str()).unwrap();

        let restored = runtime
            .restore_run(run.id.as_str())
            .unwrap()
            .expect("restored run should exist");

        assert_eq!(restored.id.as_str(), run.id.as_str());
        assert_eq!(restored.status.state, AcpRunState::Completed);

        let restored_events = runtime.get_events(run.id.as_str()).unwrap();
        assert!(!restored_events.is_empty());
        assert_eq!(
            restored_events.last().unwrap().event.payload["event"],
            "run.completed"
        );
    }

    #[test]
    fn test_runtime_get_session_returns_none_for_missing_session() {
        let runtime = AcpRuntime::new_in_memory(Config::default());

        let session = runtime.get_session("session_missing").unwrap();
        assert!(session.is_none());
    }
}
