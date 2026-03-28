/// ACP HTTP server bootstrap and discovery surface.
///
/// This module provides the Phase 2 HTTP transport for ACP discovery. It keeps
/// HTTP concerns isolated from the transport-independent ACP domain model while
/// exposing the first read-only ACP endpoints:
///
/// - `GET /ping`
/// - `GET /agents`
/// - `GET /agents/{name}`
///
/// The route layout is configurable through [`crate::config::AcpConfig`].
/// XZatoma defaults to a versioned base path such as `/api/v1/acp`, but it can
/// also expose ACP root-compatible paths like `/ping` and `/agents` when
/// `acp.compatibility_mode = "root_compatible"`.
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
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::acp::manifest::{AcpAgentCapability, AcpAgentManifest, AcpManifestLink};
use crate::config::{AcpCompatibilityMode, AcpConfig, Config};
use crate::error::{Result, XzatomaError};

/// Shared ACP HTTP server state.
///
/// This state contains the generated ACP discovery manifests and the effective
/// path strategy chosen from configuration. The state is intentionally small and
/// read-only for Phase 2.
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
        Ok(Self {
            manifests: Arc::new(vec![manifest]),
            path_strategy: AcpPathStrategy::from_config(&config.acp),
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
}

/// Effective ACP path exposure strategy.
///
/// This captures the chosen Phase 2 route layout so it can be surfaced in
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
/// Phase 2 supports a simple list shape with optional offset/limit filtering so
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
/// This function wires the Phase 2 discovery handlers according to the selected
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
    let discovery_router = Router::new()
        .route("/ping", get(handle_ping))
        .route("/agents", get(handle_agents))
        .route("/agents/:name", get(handle_agent_by_name))
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
///     run_server(config).await
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
        XzatomaError::Internal(format!("ACP server terminated with error: {}", error)).into()
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

fn build_primary_manifest(config: &Config) -> Result<AcpAgentManifest> {
    let state_preview = AcpServerState {
        manifests: Arc::new(Vec::new()),
        path_strategy: AcpPathStrategy::from_config(&config.acp),
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

    #[test]
    fn test_bind_address_with_default_config() {
        let address = bind_address(&AcpConfig::default()).unwrap();
        assert_eq!(address.ip().to_string(), "127.0.0.1");
        assert_eq!(address.port(), 8765);
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
    fn test_acp_server_state_generates_primary_manifest() {
        let state = AcpServerState::from_config(&test_config()).unwrap();
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
        let state = AcpServerState::from_config(&config).unwrap();
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

        let state = AcpServerState::from_config(&config).unwrap();
        let app = build_router(state, &config.acp);

        let response = app
            .oneshot(Request::builder().uri("/ping").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_agents_endpoint_returns_list_shape() {
        let config = test_config();
        let state = AcpServerState::from_config(&config).unwrap();
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
    async fn test_agent_by_name_endpoint_returns_success() {
        let config = test_config();
        let state = AcpServerState::from_config(&config).unwrap();

        let result = handle_agent_by_name(State(state), Path("xzatoma".to_string())).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().0.name, "xzatoma");
    }

    #[tokio::test]
    async fn test_agent_by_name_endpoint_returns_not_found() {
        let config = test_config();
        let state = AcpServerState::from_config(&config).unwrap();

        let result = handle_agent_by_name(State(state), Path("missing".to_string())).await;
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().status, StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_manifest_contains_required_phase2_metadata() {
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
}
