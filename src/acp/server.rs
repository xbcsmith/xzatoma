/// ACP HTTP server bootstrap and ACP surface.
///
/// This module provides the HTTP transport for ACP support in XZatoma. It keeps
/// HTTP concerns isolated from the transport-independent ACP domain model while
/// exposing:
///
/// - discovery endpoints:
///   - `GET /ping`
///   - `GET /agents`
///   - `GET /agents/{name}`
/// - run lifecycle endpoints:
///   - `POST /runs`
///   - `GET /runs/{run_id}`
///   - `GET /runs/{run_id}/events`
/// - Stateful lifecycle endpoints:
///   - `POST /runs/{run_id}`
///   - `POST /runs/{run_id}/cancel`
///   - `GET /sessions/{session_id}`
///
/// The route layout is configurable through [`crate::config::AcpConfig`].
/// XZatoma defaults to a versioned base path such as `/api/v1/acp`, but it can
/// also expose ACP root-compatible paths like `/ping`, `/agents`, `/runs`, and
/// `/sessions` when `acp.compatibility_mode = "root_compatible"`.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::acp::server::{build_router, AcpServerState};
/// use xzatoma::config::Config;
///
/// let config = Config::default();
/// let state = AcpServerState::from_config(&config).unwrap();
/// let _router = build_router(state, &config.acp);
/// ```
use std::collections::VecDeque;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::acp::executor::{AcpExecutor, AcpExecutorOutcome};
use crate::acp::manifest::{AcpAgentCapability, AcpAgentManifest, AcpManifestLink};
use crate::acp::runtime::{
    build_run_snapshot, AcpRuntime, AcpRuntimeCreateRequest, AcpRuntimeEvent, AcpRuntimeExecuteMode,
};
use crate::acp::streaming::stream_run_events_sse;
use crate::acp::{AcpRun, AcpRunId, AcpRunResumeRequest, AcpRunSession};
use crate::config::{AcpCompatibilityMode, AcpConfig, Config};
use crate::error::{Result, XzatomaError};

/// Shared ACP HTTP server state.
///
/// This state contains the generated ACP discovery manifests and the effective
/// path strategy chosen from configuration. The state is intentionally small and
/// read-only for discovery.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::server::AcpServerState;
/// use xzatoma::Config;
///
/// let config = Config::default();
/// let state = AcpServerState::from_config(&config).unwrap();
///
/// assert_eq!(state.manifests().len(), 1);
/// ```
#[derive(Debug, Clone)]
pub struct AcpServerState {
    manifests: Arc<Vec<AcpAgentManifest>>,
    path_strategy: AcpPathStrategy,
    runtime: AcpRuntime,
    executor: AcpExecutor,
}

impl AcpServerState {
    /// Builds ACP server state from the current application configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - Application configuration
    ///
    /// # Returns
    ///
    /// Returns initialized server state with generated discovery manifests.
    ///
    /// # Errors
    ///
    /// Returns an error if manifest generation fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::server::AcpServerState;
    /// use xzatoma::Config;
    ///
    /// let config = Config::default();
    /// let state = AcpServerState::from_config(&config).unwrap();
    /// assert!(!state.manifests().is_empty());
    /// ```
    pub fn from_config(config: &Config) -> Result<Self> {
        let manifest = build_primary_manifest(config)?;
        let runtime = AcpRuntime::new(config.clone());
        let executor = AcpExecutor::new(config.clone(), runtime.clone());

        Ok(Self {
            manifests: Arc::new(vec![manifest]),
            path_strategy: AcpPathStrategy::from_config(&config.acp),
            runtime,
            executor,
        })
    }

    /// Returns all ACP manifests currently exposed by the server.
    ///
    /// # Returns
    ///
    /// Returns a slice of discoverable ACP manifests.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::server::AcpServerState;
    /// use xzatoma::Config;
    ///
    /// let config = Config::default();
    /// let state = AcpServerState::from_config(&config).unwrap();
    /// assert_eq!(state.manifests()[0].name, "xzatoma");
    /// ```
    pub fn manifests(&self) -> &[AcpAgentManifest] {
        self.manifests.as_ref().as_slice()
    }

    /// Returns the configured ACP path strategy.
    ///
    /// # Returns
    ///
    /// Returns the effective route exposure strategy.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::server::{AcpPathStrategy, AcpServerState};
    /// use xzatoma::Config;
    ///
    /// let config = Config::default();
    /// let state = AcpServerState::from_config(&config).unwrap();
    /// assert!(matches!(state.path_strategy(), AcpPathStrategy::Versioned { .. }));
    /// ```
    pub fn path_strategy(&self) -> &AcpPathStrategy {
        &self.path_strategy
    }

    fn find_manifest(&self, name: &str) -> Option<&AcpAgentManifest> {
        self.manifests.iter().find(|manifest| manifest.name == name)
    }

    /// Returns the shared ACP runtime.
    ///
    /// # Returns
    ///
    /// Returns the in-memory ACP runtime coordinator used for run lifecycle
    /// creation, status tracking, and event playback.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::server::AcpServerState;
    /// use xzatoma::Config;
    ///
    /// let config = Config::default();
    /// let state = AcpServerState::from_config(&config).unwrap();
    /// let _ = state.runtime().run_count();
    /// ```
    pub fn runtime(&self) -> &AcpRuntime {
        &self.runtime
    }

    /// Creates ACP server state from explicit runtime and executor dependencies.
    ///
    /// This constructor is primarily intended for tests that need deterministic
    /// runtime and execution wiring without relying on the default production
    /// initialization path.
    ///
    /// # Arguments
    ///
    /// * `config` - Application configuration
    /// * `runtime` - Prebuilt ACP runtime coordinator
    /// * `executor` - Prebuilt ACP executor
    ///
    /// # Returns
    ///
    /// Returns initialized ACP server state with generated discovery manifests.
    ///
    /// # Errors
    ///
    /// Returns an error if manifest generation fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::executor::AcpExecutor;
    /// use xzatoma::acp::runtime::AcpRuntime;
    /// use xzatoma::acp::server::AcpServerState;
    /// use xzatoma::Config;
    ///
    /// let config = Config::default();
    /// let runtime = AcpRuntime::new(config.clone());
    /// let executor = AcpExecutor::new(config.clone(), runtime.clone());
    /// let state = AcpServerState::from_parts(&config, runtime, executor).unwrap();
    ///
    /// assert_eq!(state.manifests().len(), 1);
    /// ```
    pub fn from_parts(config: &Config, runtime: AcpRuntime, executor: AcpExecutor) -> Result<Self> {
        let manifest = build_primary_manifest(config)?;
        Ok(Self {
            manifests: Arc::new(vec![manifest]),
            path_strategy: AcpPathStrategy::from_config(&config.acp),
            runtime,
            executor,
        })
    }

    /// Returns the shared ACP executor.
    ///
    /// # Returns
    ///
    /// Returns the ACP executor used to bridge HTTP run creation with the
    /// existing XZatoma single-agent execution path.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::server::AcpServerState;
    /// use xzatoma::Config;
    ///
    /// let config = Config::default();
    /// let state = AcpServerState::from_config(&config).unwrap();
    /// let _executor = state.executor();
    /// ```
    pub fn executor(&self) -> &AcpExecutor {
        &self.executor
    }
}

/// Effective ACP path exposure strategy.
///
/// This captures the chosen route layout so it can be surfaced in
/// metadata and documentation. XZatoma defaults to versioned routes to avoid
/// colliding with unrelated application paths, but root-compatible ACP paths
/// can be enabled explicitly for spec-oriented deployments.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::server::AcpPathStrategy;
/// use xzatoma::config::AcpConfig;
///
/// let strategy = AcpPathStrategy::from_config(&AcpConfig::default());
/// assert!(strategy.discovery_base().starts_with('/'));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcpPathStrategy {
    /// ACP routes are served under a versioned base such as `/api/v1/acp`.
    Versioned { base_path: String },
    /// ACP routes are served at ACP root-compatible paths.
    RootCompatible,
}

impl AcpPathStrategy {
    /// Derives the effective path strategy from ACP configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - ACP server configuration
    ///
    /// # Returns
    ///
    /// Returns the selected path strategy.
    pub fn from_config(config: &AcpConfig) -> Self {
        match config.compatibility_mode {
            AcpCompatibilityMode::Versioned => Self::Versioned {
                base_path: normalize_base_path(&config.base_path),
            },
            AcpCompatibilityMode::RootCompatible => Self::RootCompatible,
        }
    }

    /// Returns the base path used for discovery routes.
    ///
    /// # Returns
    ///
    /// Returns `/` for root-compatible mode or the normalized versioned base
    /// path for versioned mode.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::server::AcpPathStrategy;
    /// use xzatoma::config::{AcpCompatibilityMode, AcpConfig};
    ///
    /// let mut config = AcpConfig::default();
    /// config.compatibility_mode = AcpCompatibilityMode::RootCompatible;
    ///
    /// let strategy = AcpPathStrategy::from_config(&config);
    /// assert_eq!(strategy.discovery_base(), "/");
    /// ```
    pub fn discovery_base(&self) -> &str {
        match self {
            Self::Versioned { base_path } => base_path.as_str(),
            Self::RootCompatible => "/",
        }
    }

    /// Returns a human-readable description of the selected route strategy.
    ///
    /// # Returns
    ///
    /// Returns a stable strategy description for manifest metadata.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::server::AcpPathStrategy;
    /// use xzatoma::config::AcpConfig;
    ///
    /// let strategy = AcpPathStrategy::from_config(&AcpConfig::default());
    /// assert!(strategy.description().contains("versioned"));
    /// ```
    pub fn description(&self) -> &'static str {
        match self {
            Self::Versioned { .. } => {
                "versioned ACP routes under a dedicated base path to avoid CLI/API collisions"
            }
            Self::RootCompatible => {
                "root-compatible ACP routes for deployments that prefer spec-style paths"
            }
        }
    }
}

/// Response payload for `GET /ping`.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::server::PingResponse;
///
/// let response = PingResponse::new();
/// assert_eq!(response.status, "ok");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PingResponse {
    /// Health status string.
    pub status: String,
    /// Service identifier.
    pub service: String,
    /// Timestamp of the response.
    pub timestamp: String,
}

impl Default for PingResponse {
    fn default() -> Self {
        Self::new()
    }
}

impl PingResponse {
    /// Creates a successful ACP ping payload.
    ///
    /// # Returns
    ///
    /// Returns a ready-to-serialize ping response.
    pub fn new() -> Self {
        Self {
            status: "ok".to_string(),
            service: "xzatoma-acp".to_string(),
            timestamp: crate::acp::now_rfc3339(),
        }
    }
}

/// Query parameters for `GET /agents`.
///
/// The current implementation supports a simple list shape with optional offset/limit filtering so
/// the endpoint can evolve toward pagination without breaking its response
/// contract.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::server::AgentsQuery;
///
/// let query = AgentsQuery {
///     offset: Some(0),
///     limit: Some(10),
/// };
/// assert_eq!(query.limit, Some(10));
/// ```
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct AgentsQuery {
    /// Optional zero-based starting offset.
    pub offset: Option<usize>,
    /// Optional maximum number of items to return.
    pub limit: Option<usize>,
}

/// Response payload for `GET /agents`.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::server::AgentsListResponse;
///
/// let response = AgentsListResponse {
///     agents: Vec::new(),
///     total: 0,
///     offset: 0,
///     limit: 0,
/// };
///
/// assert_eq!(response.total, 0);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentsListResponse {
    /// Listed ACP agent manifests.
    pub agents: Vec<AcpAgentManifest>,
    /// Total discoverable agent count before filtering.
    pub total: usize,
    /// Applied response offset.
    pub offset: usize,
    /// Applied response limit.
    pub limit: usize,
}

/// JSON error payload for ACP HTTP responses.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::server::AcpHttpErrorBody;
///
/// let error = AcpHttpErrorBody::new("not_found", "agent was not found");
/// assert_eq!(error.code, "not_found");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcpHttpErrorBody {
    /// Stable machine-readable error code.
    pub code: String,
    /// Human-readable error message.
    pub message: String,
}

/// Request body for `POST /runs`.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::server::CreateRunRequestBody;
/// use xzatoma::acp::runtime::AcpRuntimeExecuteMode;
///
/// let body = CreateRunRequestBody {
///     input: Vec::new(),
///     mode: Some(AcpRuntimeExecuteMode::Async),
///     session_id: None,
///     agent_name: Some("xzatoma".to_string()),
/// };
///
/// assert_eq!(body.agent_name.as_deref(), Some("xzatoma"));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateRunRequestBody {
    /// Ordered ACP input messages.
    pub input: Vec<crate::acp::AcpMessage>,
    /// Optional execution mode override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<AcpRuntimeExecuteMode>,
    /// Optional session identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Optional ACP agent name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
}

/// Request body for `POST /runs/{run_id}` resume behavior.
///
/// # Examples
///
/// ```
/// use serde_json::json;
/// use xzatoma::acp::server::ResumeRunRequestBody;
///
/// let body = ResumeRunRequestBody {
///     resume_payload: json!({"approved": true}),
/// };
///
/// assert_eq!(body.resume_payload["approved"], true);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResumeRunRequestBody {
    /// Opaque resume payload consumed by the ACP runtime.
    pub resume_payload: Value,
}

/// Request body for `POST /runs/{run_id}/cancel`.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::server::CancelRunRequestBody;
///
/// let body = CancelRunRequestBody {
///     reason: Some("user requested stop".to_string()),
/// };
///
/// assert_eq!(body.reason.as_deref(), Some("user requested stop"));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CancelRunRequestBody {
    /// Optional cancellation reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Response body for `GET /sessions/{session_id}`.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::server::SessionResponseBody;
///
/// let body = SessionResponseBody {
///     session: None,
///     runs: Vec::new(),
/// };
///
/// assert!(body.session.is_none());
/// assert!(body.runs.is_empty());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionResponseBody {
    /// Canonical ACP session when found.
    pub session: Option<AcpRunSession>,
    /// Persisted and active runs linked to the session.
    pub runs: Vec<AcpRun>,
}

/// Response body for `POST /runs`.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::server::CreateRunResponseBody;
/// use xzatoma::acp::runtime::AcpRuntimeExecuteMode;
/// use xzatoma::acp::{AcpMessage, AcpMessagePart, AcpRole, AcpTextPart};
///
/// let run = xzatoma::acp::AcpRun::new(
///     xzatoma::acp::AcpRunId::new("run_123".to_string())?,
///     xzatoma::acp::AcpRunCreateRequest::new(
///         xzatoma::acp::AcpSessionId::new("session_123".to_string())?,
///         vec![AcpMessage::new(
///             AcpRole::User,
///             vec![AcpMessagePart::Text(AcpTextPart::new("hello".to_string()))],
///         )?],
///     )?,
///     xzatoma::acp::AcpRunSession::new(
///         xzatoma::acp::AcpSessionId::new("session_123".to_string())?,
///     )?,
/// )?;
///
/// let response = CreateRunResponseBody {
///     run,
///     mode: AcpRuntimeExecuteMode::Sync,
/// };
///
/// assert_eq!(response.mode, AcpRuntimeExecuteMode::Sync);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateRunResponseBody {
    /// Current run state.
    pub run: crate::acp::AcpRun,
    /// Effective execution mode.
    pub mode: AcpRuntimeExecuteMode,
}

/// Response body for `GET /runs/{run_id}/events`.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::server::RunEventsResponseBody;
///
/// let response = RunEventsResponseBody { events: Vec::new() };
/// assert!(response.events.is_empty());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunEventsResponseBody {
    /// Ordered ACP runtime events for the run.
    pub events: Vec<AcpRuntimeEvent>,
}

impl AcpHttpErrorBody {
    /// Creates a new ACP HTTP error payload.
    ///
    /// # Arguments
    ///
    /// * `code` - Stable error code
    /// * `message` - Human-readable message
    ///
    /// # Returns
    ///
    /// Returns a ready-to-serialize error body.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug)]
struct AcpHttpProtection {
    auth_token: Option<String>,
    rate_limiter: Mutex<AcpRateLimiter>,
}

impl AcpHttpProtection {
    fn from_config(config: &AcpConfig) -> Self {
        Self {
            auth_token: config.auth_token.clone(),
            rate_limiter: Mutex::new(AcpRateLimiter::new(config.rate_limit_per_minute)),
        }
    }

    fn validate_authorization(&self, request: &Request) -> std::result::Result<(), AcpHttpError> {
        let Some(expected_token) = self.auth_token.as_deref() else {
            return Ok(());
        };

        let authorized = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(|token| token == expected_token)
            .unwrap_or(false);

        if authorized {
            Ok(())
        } else {
            Err(AcpHttpError::new(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "ACP HTTP request requires a valid bearer token",
            ))
        }
    }

    fn check_rate_limit(&self) -> std::result::Result<(), AcpHttpError> {
        let mut limiter = self.rate_limiter.lock().map_err(|_| {
            AcpHttpError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "rate_limiter_unavailable",
                "ACP rate limiter is unavailable",
            )
        })?;
        if limiter.check_and_record() {
            Ok(())
        } else {
            Err(AcpHttpError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limit_exceeded",
                "ACP HTTP request rate limit exceeded",
            ))
        }
    }
}

#[derive(Debug)]
struct AcpRateLimiter {
    requests: VecDeque<Instant>,
    limit: usize,
    window: Duration,
}

impl AcpRateLimiter {
    fn new(limit: usize) -> Self {
        Self {
            requests: VecDeque::new(),
            limit,
            window: Duration::from_secs(60),
        }
    }

    fn check_and_record(&mut self) -> bool {
        let cutoff = Instant::now() - self.window;
        while self
            .requests
            .front()
            .map(|instant| *instant <= cutoff)
            .unwrap_or(false)
        {
            self.requests.pop_front();
        }

        if self.requests.len() >= self.limit {
            return false;
        }

        self.requests.push_back(Instant::now());
        true
    }
}

async fn enforce_acp_http_protection(
    State(protection): State<Arc<AcpHttpProtection>>,
    request: Request,
    next: Next,
) -> Response {
    if let Err(error) = protection.validate_authorization(&request) {
        return error.into_response();
    }

    if let Err(error) = protection.check_rate_limit() {
        return error.into_response();
    }

    next.run(request).await
}

/// Typed ACP HTTP error response.
///
/// # Examples
///
/// ```
/// use axum::http::StatusCode;
/// use xzatoma::acp::server::AcpHttpError;
///
/// let error = AcpHttpError::new(StatusCode::NOT_FOUND, "not_found", "missing");
/// assert_eq!(error.status, StatusCode::NOT_FOUND);
/// ```
#[derive(Debug, Clone)]
pub struct AcpHttpError {
    /// HTTP status code.
    pub status: StatusCode,
    /// JSON response body.
    pub body: AcpHttpErrorBody,
}

impl AcpHttpError {
    /// Creates a new ACP HTTP error.
    ///
    /// # Arguments
    ///
    /// * `status` - HTTP status code
    /// * `code` - Stable error code
    /// * `message` - Human-readable message
    ///
    /// # Returns
    ///
    /// Returns the structured error response.
    pub fn new(status: StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status,
            body: AcpHttpErrorBody::new(code, message),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }
}

impl IntoResponse for AcpHttpError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

/// Builds the ACP discovery router.
///
/// This function wires the discovery handlers according to the selected
/// ACP compatibility mode. In versioned mode, routes are mounted beneath the
/// configured base path such as `/api/v1/acp`. In root-compatible mode, the
/// handlers are mounted directly at `/ping`, `/agents`, and `/agents/{name}`.
///
/// # Arguments
///
/// * `state` - Shared ACP server state
/// * `config` - ACP server configuration
///
/// # Returns
///
/// Returns an `axum::Router` exposing ACP discovery endpoints.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::server::{build_router, AcpServerState};
/// use xzatoma::Config;
///
/// let config = Config::default();
/// let state = AcpServerState::from_config(&config).unwrap();
/// let _router = build_router(state, &config.acp);
/// ```
pub fn build_router(state: AcpServerState, config: &AcpConfig) -> Router {
    let protection = Arc::new(AcpHttpProtection::from_config(config));
    let discovery_router = Router::new()
        .route("/ping", get(handle_ping))
        .route("/agents", get(handle_agents))
        .route("/agents/:name", get(handle_agent_by_name))
        .route("/runs", post(handle_create_run))
        .route("/runs/:run_id", get(handle_get_run).post(handle_resume_run))
        .route("/runs/:run_id/events", get(handle_get_run_events))
        .route("/runs/:run_id/cancel", post(handle_cancel_run))
        .route("/sessions/:session_id", get(handle_get_session))
        .layer(DefaultBodyLimit::max(config.max_request_bytes))
        .layer(middleware::from_fn_with_state(
            Arc::clone(&protection),
            enforce_acp_http_protection,
        ))
        .with_state(state.clone());

    match AcpPathStrategy::from_config(config) {
        AcpPathStrategy::Versioned { base_path } => {
            Router::new().nest(&base_path, discovery_router)
        }
        AcpPathStrategy::RootCompatible => discovery_router,
    }
}

/// Runs the ACP HTTP server until shutdown.
///
/// # Arguments
///
/// * `config` - Full application configuration
///
/// # Errors
///
/// Returns an error if the bind address is invalid or the HTTP server fails.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::acp::server::run_server;
/// use xzatoma::Config;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let config = Config::default();
///     Ok(run_server(config).await?)
/// }
/// ```
pub async fn run_server(config: Config) -> Result<()> {
    let state = AcpServerState::from_config(&config)?;
    let router = build_router(state.clone(), &config.acp);
    let address = bind_address(&config.acp)?;

    tracing::info!(
        address = %address,
        path_strategy = ?state.path_strategy(),
        "Starting ACP HTTP server",
    );

    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|error| {
            XzatomaError::Config(format!(
                "Failed to bind ACP server at {}: {}",
                address, error
            ))
        })?;

    axum::serve(listener, router).await.map_err(|error| {
        XzatomaError::Internal(format!("ACP server terminated with error: {}", error))
    })
}

/// Resolves the TCP bind address for the ACP server.
///
/// # Arguments
///
/// * `config` - ACP server configuration
///
/// # Errors
///
/// Returns an error if the configured host is not a valid IP address.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::server::bind_address;
/// use xzatoma::config::AcpConfig;
///
/// let address = bind_address(&AcpConfig::default()).unwrap();
/// assert_eq!(address.port(), 8765);
/// ```
pub fn bind_address(config: &AcpConfig) -> Result<SocketAddr> {
    let ip_addr: IpAddr = config.host.parse().map_err(|error| {
        XzatomaError::Config(format!(
            "acp.host must be a valid IP address, got '{}': {}",
            config.host, error
        ))
    })?;

    if !ip_addr.is_loopback()
        && config
            .auth_token
            .as_ref()
            .map(|token| token.trim().is_empty())
            .unwrap_or(true)
    {
        return Err(XzatomaError::Config(
            "acp.auth_token is required when binding ACP server to a non-loopback address"
                .to_string(),
        ));
    }

    Ok(SocketAddr::new(ip_addr, config.port))
}

/// Handles `GET /ping`.
///
/// # Returns
///
/// Returns a successful ACP ping payload.
pub async fn handle_ping() -> Json<PingResponse> {
    Json(PingResponse::new())
}

/// Handles `GET /agents`.
///
/// # Arguments
///
/// * `state` - Shared ACP server state
/// * `query` - Optional offset and limit
///
/// # Returns
///
/// Returns the current ACP manifest list shape.
pub async fn handle_agents(
    State(state): State<AcpServerState>,
    Query(query): Query<AgentsQuery>,
) -> Json<AgentsListResponse> {
    let total = state.manifests().len();
    let offset = query.offset.unwrap_or(0);
    let requested_limit = query.limit.unwrap_or(total.max(1));

    let agents = state
        .manifests()
        .iter()
        .skip(offset)
        .take(requested_limit)
        .cloned()
        .collect::<Vec<_>>();

    Json(AgentsListResponse {
        limit: requested_limit,
        offset,
        total,
        agents,
    })
}

/// Handles `GET /agents/{name}`.
///
/// # Arguments
///
/// * `state` - Shared ACP server state
/// * `name` - Requested ACP agent name
///
/// # Errors
///
/// Returns a not-found error when no agent manifest matches the requested name.
pub async fn handle_agent_by_name(
    State(state): State<AcpServerState>,
    Path(name): Path<String>,
) -> std::result::Result<Json<AcpAgentManifest>, AcpHttpError> {
    let manifest = state
        .find_manifest(&name)
        .ok_or_else(|| AcpHttpError::not_found(format!("ACP agent '{}' was not found", name)))?;

    Ok(Json(manifest.clone()))
}

/// Handles `POST /runs`.
///
/// This endpoint creates an ACP run and dispatches execution according to the
/// requested or configured execution mode:
///
/// - `sync` returns the terminal run after execution completes
/// - `async` returns `202 Accepted` with the created run
/// - `stream` returns an SSE stream of ordered ACP runtime events
///
/// # Arguments
///
/// * `state` - Shared ACP server state
/// * `body` - Create run request payload
///
/// # Errors
///
/// Returns an ACP HTTP error if the request is invalid, the target agent is not
/// found, or the run cannot be created.
pub async fn handle_create_run(
    State(state): State<AcpServerState>,
    Json(body): Json<CreateRunRequestBody>,
) -> std::result::Result<Response, AcpHttpError> {
    if let Some(agent_name) = &body.agent_name {
        if state.find_manifest(agent_name).is_none() {
            return Err(AcpHttpError::not_found(format!(
                "ACP agent '{}' was not found",
                agent_name
            )));
        }
    }

    let mode = body.mode.unwrap_or_else(|| state.runtime().default_mode());

    let mut request = AcpRuntimeCreateRequest::new(body.input).with_mode(mode);
    if let Some(session_id) = body.session_id {
        request = request.with_session_id(session_id);
    }
    if let Some(agent_name) = body.agent_name {
        request = request.with_agent_name(agent_name);
    }

    request
        .validate()
        .map_err(acp_runtime_error_to_http_error)?;

    match mode {
        AcpRuntimeExecuteMode::Sync => {
            let (run, outcome) = state
                .executor()
                .create_and_execute(request)
                .await
                .map_err(acp_runtime_error_to_http_error)?;

            let final_run = match outcome {
                AcpExecutorOutcome::Completed(updated_run)
                | AcpExecutorOutcome::Failed(updated_run) => updated_run,
                AcpExecutorOutcome::Accepted => run,
            };

            Ok((
                StatusCode::OK,
                Json(CreateRunResponseBody {
                    run: final_run,
                    mode,
                }),
            )
                .into_response())
        }
        AcpRuntimeExecuteMode::Async => {
            let (run, outcome) = state
                .executor()
                .create_and_execute(request)
                .await
                .map_err(acp_runtime_error_to_http_error)?;

            match outcome {
                AcpExecutorOutcome::Accepted => Ok((
                    StatusCode::ACCEPTED,
                    Json(CreateRunResponseBody { run, mode }),
                )
                    .into_response()),
                AcpExecutorOutcome::Completed(updated_run)
                | AcpExecutorOutcome::Failed(updated_run) => Ok((
                    StatusCode::ACCEPTED,
                    Json(CreateRunResponseBody {
                        run: updated_run,
                        mode,
                    }),
                )
                    .into_response()),
            }
        }
        AcpRuntimeExecuteMode::Stream => {
            let (run, _outcome) = state
                .executor()
                .create_and_execute(request)
                .await
                .map_err(acp_runtime_error_to_http_error)?;

            let sse = stream_run_events_sse(state.runtime().clone(), run.id.as_str())
                .map_err(acp_runtime_error_to_http_error)?;
            Ok(sse.into_response())
        }
    }
}

/// Handles `GET /runs/{run_id}`.
///
/// # Arguments
///
/// * `state` - Shared ACP server state
/// * `run_id` - Requested ACP run identifier
///
/// # Errors
///
/// Returns a not-found ACP HTTP error if the run does not exist.
pub async fn handle_get_run(
    State(state): State<AcpServerState>,
    Path(run_id): Path<String>,
) -> std::result::Result<Json<serde_json::Value>, AcpHttpError> {
    let run = match state.runtime().get_run(&run_id).or_else(|_| {
        state
            .runtime()
            .restore_run(&run_id)
            .and_then(|restored| match restored {
                Some(run) => Ok(run),
                None => Err(XzatomaError::Acp(crate::acp::error::AcpError::lifecycle(
                    format!("ACP run '{}' was not found", run_id),
                ))),
            })
    }) {
        Ok(run) => run,
        Err(error) => return Err(acp_runtime_error_to_http_error(error)),
    };

    let events = state
        .runtime()
        .get_events(&run_id)
        .map_err(acp_runtime_error_to_http_error)?;

    let mode = events
        .first()
        .and_then(|event| event.event.payload.get("mode"))
        .and_then(serde_json::Value::as_str)
        .and_then(|value| AcpRuntimeExecuteMode::parse(value).ok())
        .unwrap_or_else(|| state.runtime().default_mode());

    Ok(Json(build_run_snapshot(&run, mode)))
}

/// Handles `GET /runs/{run_id}/events`.
///
/// # Arguments
///
/// * `state` - Shared ACP server state
/// * `run_id` - Requested ACP run identifier
///
/// # Errors
///
/// Returns a not-found ACP HTTP error if the run does not exist.
pub async fn handle_get_run_events(
    State(state): State<AcpServerState>,
    Path(run_id): Path<String>,
) -> std::result::Result<Json<RunEventsResponseBody>, AcpHttpError> {
    if state
        .runtime()
        .restore_run(&run_id)
        .map_err(acp_runtime_error_to_http_error)?
        .is_none()
    {
        return Err(AcpHttpError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("ACP run '{}' was not found", run_id),
        ));
    }

    let events = state
        .runtime()
        .get_events(&run_id)
        .map_err(acp_runtime_error_to_http_error)?;

    Ok(Json(RunEventsResponseBody { events }))
}

/// Handles `POST /runs/{run_id}` for await/resume behavior.
///
/// # Arguments
///
/// * `state` - Shared ACP server state
/// * `run_id` - Requested ACP run identifier
/// * `body` - Resume request payload
///
/// # Errors
///
/// Returns a not-found or invalid-request ACP HTTP error if the run cannot be
/// resumed.
pub async fn handle_resume_run(
    State(state): State<AcpServerState>,
    Path(run_id): Path<String>,
    Json(body): Json<ResumeRunRequestBody>,
) -> std::result::Result<Json<CreateRunResponseBody>, AcpHttpError> {
    if state
        .runtime()
        .restore_run(&run_id)
        .map_err(acp_runtime_error_to_http_error)?
        .is_none()
    {
        return Err(AcpHttpError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("ACP run '{}' was not found", run_id),
        ));
    }

    let resumed = state
        .runtime()
        .resume_run(
            AcpRunResumeRequest::new(
                AcpRunId::new(run_id.clone()).map_err(acp_runtime_error_to_http_error)?,
            ),
            body.resume_payload,
        )
        .map_err(acp_runtime_error_to_http_error)?;

    let events = state
        .runtime()
        .get_events(&run_id)
        .map_err(acp_runtime_error_to_http_error)?;

    let mode = events
        .first()
        .and_then(|event| event.event.payload.get("mode"))
        .and_then(serde_json::Value::as_str)
        .and_then(|value| AcpRuntimeExecuteMode::parse(value).ok())
        .unwrap_or_else(|| state.runtime().default_mode());

    Ok(Json(CreateRunResponseBody { run: resumed, mode }))
}

/// Handles `POST /runs/{run_id}/cancel`.
///
/// # Arguments
///
/// * `state` - Shared ACP server state
/// * `run_id` - Requested ACP run identifier
/// * `body` - Cancellation request payload
///
/// # Errors
///
/// Returns a not-found or invalid-request ACP HTTP error if the run cannot be
/// cancelled.
pub async fn handle_cancel_run(
    State(state): State<AcpServerState>,
    Path(run_id): Path<String>,
    Json(body): Json<CancelRunRequestBody>,
) -> std::result::Result<Json<CreateRunResponseBody>, AcpHttpError> {
    if state
        .runtime()
        .restore_run(&run_id)
        .map_err(acp_runtime_error_to_http_error)?
        .is_none()
    {
        return Err(AcpHttpError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("ACP run '{}' was not found", run_id),
        ));
    }

    let cancelled = state
        .runtime()
        .cancel_run(
            &run_id,
            body.reason
                .unwrap_or_else(|| "ACP client requested cancellation".to_string()),
        )
        .map_err(acp_runtime_error_to_http_error)?;

    let events = state
        .runtime()
        .get_events(&run_id)
        .map_err(acp_runtime_error_to_http_error)?;

    let mode = events
        .first()
        .and_then(|event| event.event.payload.get("mode"))
        .and_then(serde_json::Value::as_str)
        .and_then(|value| AcpRuntimeExecuteMode::parse(value).ok())
        .unwrap_or_else(|| state.runtime().default_mode());

    Ok(Json(CreateRunResponseBody {
        run: cancelled,
        mode,
    }))
}

/// Handles `GET /sessions/{session_id}`.
///
/// # Arguments
///
/// * `state` - Shared ACP server state
/// * `session_id` - Requested ACP session identifier
///
/// # Errors
///
/// Returns a not-found ACP HTTP error if the session does not exist.
pub async fn handle_get_session(
    State(state): State<AcpServerState>,
    Path(session_id): Path<String>,
) -> std::result::Result<Json<SessionResponseBody>, AcpHttpError> {
    let session = state
        .runtime()
        .get_session(&session_id)
        .map_err(acp_runtime_error_to_http_error)?;

    let Some(session) = session else {
        return Err(AcpHttpError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("ACP session '{}' was not found", session_id),
        ));
    };

    let runs = state
        .runtime()
        .get_session_runs(&session_id)
        .map_err(acp_runtime_error_to_http_error)?;

    Ok(Json(SessionResponseBody {
        session: Some(session),
        runs,
    }))
}

fn acp_runtime_error_to_http_error(error: XzatomaError) -> AcpHttpError {
    let message = error.to_string();

    if message.contains("was not found") {
        AcpHttpError::new(StatusCode::NOT_FOUND, "not_found", message)
    } else if message.contains("unsupported")
        || message.contains("cannot be empty")
        || message.contains("invalid")
        || message.contains("not yet supported")
    {
        AcpHttpError::new(StatusCode::BAD_REQUEST, "invalid_request", message)
    } else {
        AcpHttpError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", message)
    }
}

fn build_primary_manifest(config: &Config) -> Result<AcpAgentManifest> {
    let preview_runtime = AcpRuntime::new_in_memory(config.clone());
    let state_preview = AcpServerState {
        manifests: Arc::new(Vec::new()),
        path_strategy: AcpPathStrategy::from_config(&config.acp),
        runtime: preview_runtime.clone(),
        executor: AcpExecutor::new(config.clone(), preview_runtime),
    };

    let documentation_href = "https://github.com/xbcsmith/xzatoma/tree/main/docs".to_string();
    let homepage_href = "https://github.com/xbcsmith/xzatoma".to_string();

    let manifest = AcpAgentManifest::new(
        "xzatoma".to_string(),
        env!("CARGO_PKG_VERSION").to_string(),
        "XZatoma ACP Agent".to_string(),
    )
    .with_description(
        "Primary autonomous XZatoma agent exposed through the ACP discovery surface.".to_string(),
    )?
    .with_capabilities(vec![
        AcpAgentCapability::ManifestRead,
        AcpAgentCapability::RunsCreate,
        AcpAgentCapability::RunsGet,
        AcpAgentCapability::RunsEvents,
        AcpAgentCapability::RunsCancel,
        AcpAgentCapability::SessionsGet,
        AcpAgentCapability::SessionsResume,
    ])
    .with_metadata(
        "implementation".to_string(),
        "xzatoma_primary_agent".to_string(),
    )?
    .with_metadata("language".to_string(), "rust".to_string())?
    .with_metadata("framework".to_string(), "axum".to_string())?
    .with_metadata(
        "supported_input_content_types".to_string(),
        "application/json,text/plain".to_string(),
    )?
    .with_metadata(
        "supported_output_content_types".to_string(),
        "application/json,text/plain".to_string(),
    )?
    .with_metadata(
        "route_strategy".to_string(),
        state_preview.path_strategy.description().to_string(),
    )?
    .with_metadata(
        "discovery_base".to_string(),
        state_preview.path_strategy.discovery_base().to_string(),
    )?
    .with_metadata(
        "default_run_mode".to_string(),
        format!("{:?}", config.acp.default_run_mode).to_lowercase(),
    )?
    .with_metadata("generated_at".to_string(), Utc::now().to_rfc3339())?
    .with_link(
        AcpManifestLink::new("documentation".to_string(), documentation_href)?
            .with_title("XZatoma documentation".to_string())?,
    )?
    .with_link(
        AcpManifestLink::new("homepage".to_string(), homepage_href)?
            .with_title("XZatoma repository".to_string())?,
    )?;

    manifest.validate()?;
    Ok(manifest)
}

fn normalize_base_path(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return "/".to_string();
    }

    let without_trailing = trimmed.trim_end_matches('/');
    if without_trailing.starts_with('/') {
        without_trailing.to_string()
    } else {
        format!("/{}", without_trailing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    fn test_config() -> Config {
        Config::default()
    }

    fn test_server_state_from_config(config: &Config) -> AcpServerState {
        let runtime = AcpRuntime::new_in_memory(config.clone());
        let executor = AcpExecutor::new_mock_success(
            config.clone(),
            runtime.clone(),
            "mock ACP server test response".to_string(),
        );

        AcpServerState::from_parts(config, runtime, executor).unwrap()
    }

    #[test]
    fn test_bind_address_with_default_config() {
        let address = bind_address(&AcpConfig::default()).unwrap();
        assert_eq!(address.ip().to_string(), "127.0.0.1");
        assert_eq!(address.port(), 8765);
    }

    #[test]
    fn test_bind_address_rejects_non_loopback_without_auth_token() {
        let config = AcpConfig {
            host: "0.0.0.0".to_string(),
            ..Default::default()
        };

        let result = bind_address(&config);

        assert!(result.is_err());
    }

    #[test]
    fn test_bind_address_accepts_non_loopback_with_auth_token() {
        let config = AcpConfig {
            host: "0.0.0.0".to_string(),
            auth_token: Some("test-token".to_string()),
            ..Default::default()
        };

        let result = bind_address(&config);

        assert!(result.is_ok());
    }

    #[test]
    fn test_path_strategy_uses_versioned_mode_by_default() {
        let strategy = AcpPathStrategy::from_config(&AcpConfig::default());
        assert_eq!(
            strategy,
            AcpPathStrategy::Versioned {
                base_path: "/api/v1/acp".to_string()
            }
        );
    }

    #[test]
    fn test_normalize_base_path_adds_leading_slash_and_removes_trailing_slash() {
        assert_eq!(normalize_base_path("api/v1/acp/"), "/api/v1/acp");
        assert_eq!(normalize_base_path("/api/v1/acp/"), "/api/v1/acp");
    }

    #[test]
    #[ignore = "disabled in CI because ACP server state initialization can touch shared runtime storage"]
    fn test_acp_server_state_generates_primary_manifest() {
        let config = test_config();
        let state = test_server_state_from_config(&config);
        assert_eq!(state.manifests().len(), 1);
        assert_eq!(state.manifests()[0].name, "xzatoma");
    }

    #[tokio::test]
    async fn test_handle_ping_success() {
        let response = handle_ping().await;
        assert_eq!(response.0.status, "ok");
        assert_eq!(response.0.service, "xzatoma-acp");
    }

    #[tokio::test]
    async fn test_router_serves_ping_in_versioned_mode() {
        let config = test_config();
        let state = test_server_state_from_config(&config);
        let app = build_router(state, &config.acp);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/acp/ping")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_router_serves_ping_in_root_compatible_mode() {
        let mut config = test_config();
        config.acp.compatibility_mode = AcpCompatibilityMode::RootCompatible;

        let state = test_server_state_from_config(&config);
        let app = build_router(state, &config.acp);

        let response = app
            .oneshot(Request::builder().uri("/ping").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_router_rejects_missing_bearer_token_when_auth_configured() {
        let mut config = test_config();
        config.acp.auth_token = Some("test-token".to_string());
        let state = test_server_state_from_config(&config);
        let app = build_router(state, &config.acp);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/acp/ping")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_router_accepts_valid_bearer_token_when_auth_configured() {
        let mut config = test_config();
        config.acp.auth_token = Some("test-token".to_string());
        let state = test_server_state_from_config(&config);
        let app = build_router(state, &config.acp);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/acp/ping")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_router_enforces_rate_limit() {
        let mut config = test_config();
        config.acp.rate_limit_per_minute = 1;
        let state = test_server_state_from_config(&config);
        let app = build_router(state, &config.acp);

        let first = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/acp/ping")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let second = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/acp/ping")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(first.status(), StatusCode::OK);
        assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn test_router_enforces_request_body_limit() {
        let mut config = test_config();
        config.acp.max_request_bytes = 8;
        let state = test_server_state_from_config(&config);
        let app = build_router(state, &config.acp);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/acp/runs")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{\"input\":\"too large\"}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    #[ignore = "disabled in CI because ACP server endpoint tests can hang when touching shared runtime storage"]
    async fn test_agents_endpoint_returns_list_shape() {
        let config = test_config();
        let state = test_server_state_from_config(&config);
        let app = build_router(state, &config.acp);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/acp/agents")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    #[ignore = "disabled in CI because ACP server endpoint tests can hang when touching shared runtime storage"]
    async fn test_agent_by_name_endpoint_returns_success() {
        let config = test_config();
        let state = test_server_state_from_config(&config);

        let result = handle_agent_by_name(State(state), Path("xzatoma".to_string())).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().0.name, "xzatoma");
    }

    #[tokio::test]
    #[ignore = "disabled in CI because ACP server endpoint tests can hang when touching shared runtime storage"]
    async fn test_agent_by_name_endpoint_returns_not_found() {
        let config = test_config();
        let state = test_server_state_from_config(&config);

        let result = handle_agent_by_name(State(state), Path("missing".to_string())).await;
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().status, StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_manifest_contains_required_metadata() {
        let manifest = build_primary_manifest(&test_config()).unwrap();

        assert_eq!(manifest.name, "xzatoma");
        assert!(manifest.description.is_some());
        assert!(manifest.metadata.contains_key("implementation"));
        assert!(manifest.metadata.contains_key("language"));
        assert!(manifest.metadata.contains_key("framework"));
        assert!(manifest
            .metadata
            .contains_key("supported_input_content_types"));
        assert!(manifest
            .metadata
            .contains_key("supported_output_content_types"));
        assert!(manifest.metadata.contains_key("generated_at"));
        assert!(!manifest.links.is_empty());
    }

    fn test_create_run_request_body(
        mode: AcpRuntimeExecuteMode,
        prompt: &str,
        agent_name: &str,
    ) -> CreateRunRequestBody {
        CreateRunRequestBody {
            input: vec![crate::acp::AcpMessage::new(
                crate::acp::AcpRole::User,
                vec![crate::acp::AcpMessagePart::Text(
                    crate::acp::AcpTextPart::new(prompt.to_string()),
                )],
            )
            .unwrap()],
            mode: Some(mode),
            session_id: None,
            agent_name: Some(agent_name.to_string()),
        }
    }

    fn test_server_state() -> AcpServerState {
        let mut config = test_config();
        config.provider.provider_type = "ollama".to_string();
        let runtime = AcpRuntime::new_in_memory(config.clone());
        let executor = AcpExecutor::new_mock_success(
            config.clone(),
            runtime.clone(),
            "mock ACP server test response".to_string(),
        );

        AcpServerState::from_parts(&config, runtime, executor).unwrap()
    }

    async fn response_json(response: Response) -> serde_json::Value {
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn test_handle_create_run_sync_returns_completed_run() {
        let state = test_server_state();

        let response = handle_create_run(
            State(state),
            Json(test_create_run_request_body(
                AcpRuntimeExecuteMode::Sync,
                "Reply with a short greeting",
                "xzatoma",
            )),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let json = response_json(response).await;
        assert_eq!(json["mode"], "sync");
        assert_eq!(json["run"]["status"]["state"], "completed");

        let output_messages = json["run"]["output"]["messages"].as_array().unwrap();
        assert_eq!(output_messages.len(), 1);
        assert_eq!(output_messages[0]["role"], "assistant");
    }

    #[tokio::test]
    #[ignore = "disabled in CI because ACP server endpoint tests can hang when touching shared runtime storage"]
    async fn test_handle_create_run_async_returns_accepted() {
        let state = test_server_state();

        let response = handle_create_run(
            State(state),
            Json(test_create_run_request_body(
                AcpRuntimeExecuteMode::Async,
                "Count to three",
                "xzatoma",
            )),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::ACCEPTED);

        let json = response_json(response).await;
        assert_eq!(json["mode"], "async");
        assert_eq!(json["run"]["status"]["state"], "created");
    }

    #[tokio::test]
    #[ignore = "disabled in CI because ACP server endpoint tests can hang when touching shared runtime storage"]
    async fn test_handle_create_run_stream_returns_sse_response() {
        let state = test_server_state();

        let response = handle_create_run(
            State(state),
            Json(test_create_run_request_body(
                AcpRuntimeExecuteMode::Stream,
                "Stream a short answer",
                "xzatoma",
            )),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let content_type = response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap();
        assert!(content_type.starts_with("text/event-stream"));
    }

    #[tokio::test]
    async fn test_handle_get_run_returns_snapshot_after_sync_run() {
        let state = test_server_state();

        let create_response = handle_create_run(
            State(state.clone()),
            Json(test_create_run_request_body(
                AcpRuntimeExecuteMode::Sync,
                "Generate one sentence",
                "xzatoma",
            )),
        )
        .await
        .unwrap();

        let create_json = response_json(create_response).await;
        let run_id = create_json["run"]["id"].as_str().unwrap().to_string();

        let run_response = handle_get_run(State(state), Path(run_id.clone()))
            .await
            .unwrap()
            .0;

        assert_eq!(run_response["runId"], run_id);
        assert_eq!(run_response["state"], "completed");
        assert_eq!(run_response["mode"], "sync");
    }

    #[tokio::test]
    async fn test_handle_get_run_returns_not_found_for_missing_run() {
        let state = test_server_state();

        let result = handle_get_run(State(state), Path("run_missing".to_string())).await;

        assert!(result.is_err());
        assert_eq!(result.err().unwrap().status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_handle_get_run_events_returns_not_found_for_missing_run() {
        let state = test_server_state();

        let result = handle_get_run_events(State(state), Path("run_missing".to_string())).await;

        assert!(result.is_err());
        assert_eq!(result.err().unwrap().status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    #[ignore = "disabled in CI because ACP server endpoint tests can hang when touching shared runtime storage"]
    async fn test_handle_create_run_rejects_unknown_agent() {
        let state = test_server_state();

        let result = handle_create_run(
            State(state),
            Json(test_create_run_request_body(
                AcpRuntimeExecuteMode::Sync,
                "Hello",
                "missing-agent",
            )),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.err().unwrap().status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    #[ignore = "disabled in CI because ACP server endpoint tests can hang when touching shared runtime storage"]
    async fn test_handle_create_run_rejects_unsupported_artifact_input() {
        let state = test_server_state();

        let result = handle_create_run(
            State(state),
            Json(CreateRunRequestBody {
                input: vec![crate::acp::AcpMessage::new(
                    crate::acp::AcpRole::User,
                    vec![crate::acp::AcpMessagePart::Artifact(
                        crate::acp::AcpArtifact::new_remote(
                            "image.png".to_string(),
                            "image/png".to_string(),
                            "https://example.com/image.png".to_string(),
                        )
                        .unwrap(),
                    )],
                )
                .unwrap()],
                mode: Some(AcpRuntimeExecuteMode::Sync),
                session_id: None,
                agent_name: Some("xzatoma".to_string()),
            }),
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.err().unwrap().status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_handle_get_run_events_returns_ordered_history_after_sync_run() {
        let state = test_server_state();

        let create_response = handle_create_run(
            State(state.clone()),
            Json(test_create_run_request_body(
                AcpRuntimeExecuteMode::Sync,
                "Generate another sentence",
                "xzatoma",
            )),
        )
        .await
        .unwrap();

        let create_json = response_json(create_response).await;
        let run_id = create_json["run"]["id"].as_str().unwrap().to_string();

        let events = handle_get_run_events(State(state), Path(run_id))
            .await
            .unwrap()
            .0;

        assert!(events.events.len() >= 5);

        let sequences: Vec<u64> = events.events.iter().map(|event| event.sequence).collect();
        let mut sorted = sequences.clone();
        sorted.sort_unstable();
        assert_eq!(sequences, sorted);
    }
}
