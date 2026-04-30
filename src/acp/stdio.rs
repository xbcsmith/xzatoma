//! ACP stdio transport for Zed-compatible subprocess integration.
//!
//! Zed launches custom ACP agents as child processes and communicates with them
//! over stdin/stdout using newline-delimited JSON-RPC. This module owns the
//! stdio transport boundary, initialization handshake, in-memory session
//! registry, and per-session prompt queue scaffolding for `xzatoma agent`.
//!
//! # Examples
//!
//! ```no_run
//! use xzatoma::acp::stdio::{run_stdio_agent, AcpStdioAgentOptions};
//! use xzatoma::Config;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let options = AcpStdioAgentOptions::new(None, None, false, None);
//! run_stdio_agent(Config::default(), options).await?;
//! # Ok(())
//! # }
//! ```
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(not(test))]
use agent_client_protocol::ByteStreams;
use agent_client_protocol::{
    self as acp_sdk, Agent as AcpAgentRole, Client as AcpClientRole, ConnectTo as AcpConnectTo,
    ConnectionTo, Dispatch, Responder,
};
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tokio::task::JoinHandle;
#[cfg(not(test))]
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::acp::prompt_input::{
    acp_content_blocks_to_prompt_input, validate_provider_supports_prompt_input,
};
use crate::agent::Agent as XzatomaAgent;
use crate::commands::build_agent_environment;
use crate::config::{Config, ExecutionMode};
use crate::error::{Result, XzatomaError};
use crate::mcp::manager::McpClientManager;
use crate::prompts;
use crate::providers::{create_provider_with_override, Message, MultimodalPromptInput, Provider};
use crate::tools::SubagentTool;

use acp_sdk::schema as acp;

/// Runtime options for the ACP stdio agent command.
///
/// These options are derived from `xzatoma agent` CLI flags and are applied to
/// the loaded configuration for the current subprocess only. They do not persist
/// to configuration files.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
/// use xzatoma::acp::stdio::AcpStdioAgentOptions;
///
/// let options = AcpStdioAgentOptions::new(
///     Some("ollama".to_string()),
///     Some("llama3.2:latest".to_string()),
///     true,
///     Some(PathBuf::from("/tmp/workspace")),
/// );
///
/// assert_eq!(options.provider.as_deref(), Some("ollama"));
/// assert_eq!(options.model.as_deref(), Some("llama3.2:latest"));
/// assert!(options.allow_dangerous);
/// assert_eq!(options.working_dir, Some(PathBuf::from("/tmp/workspace")));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpStdioAgentOptions {
    /// Optional provider override, such as `copilot`, `ollama`, or `openai`.
    pub provider: Option<String>,
    /// Optional model override for the selected provider.
    pub model: Option<String>,
    /// Whether dangerous terminal commands should run without confirmation.
    pub allow_dangerous: bool,
    /// Optional fallback workspace root when the ACP client omits one.
    pub working_dir: Option<PathBuf>,
}

impl AcpStdioAgentOptions {
    /// Creates ACP stdio agent runtime options.
    ///
    /// # Arguments
    ///
    /// * `provider` - Optional provider override.
    /// * `model` - Optional model override.
    /// * `allow_dangerous` - Whether to allow dangerous terminal commands.
    /// * `working_dir` - Optional fallback workspace root.
    ///
    /// # Returns
    ///
    /// Returns initialized runtime options for the stdio agent.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::stdio::AcpStdioAgentOptions;
    ///
    /// let options = AcpStdioAgentOptions::new(None, None, false, None);
    /// assert!(options.provider.is_none());
    /// assert!(!options.allow_dangerous);
    /// ```
    pub fn new(
        provider: Option<String>,
        model: Option<String>,
        allow_dangerous: bool,
        working_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            provider,
            model,
            allow_dangerous,
            working_dir,
        }
    }
}

/// In-memory registry of active ACP stdio sessions.
///
/// The registry is shared by protocol handlers and contains lightweight session
/// handles guarded by Tokio synchronization primitives. It intentionally avoids a
/// complex actor hierarchy.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::stdio::ActiveSessionRegistry;
///
/// let registry = ActiveSessionRegistry::new();
/// # async fn check(registry: ActiveSessionRegistry) {
/// assert_eq!(registry.len().await, 0);
/// # }
/// ```
#[derive(Clone, Default)]
pub struct ActiveSessionRegistry {
    sessions: Arc<RwLock<HashMap<acp::SessionId, Arc<Mutex<ActiveSessionState>>>>>,
}

impl ActiveSessionRegistry {
    /// Creates an empty active session registry.
    ///
    /// # Returns
    ///
    /// Returns a registry ready to share across ACP protocol handlers.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::stdio::ActiveSessionRegistry;
    ///
    /// let registry = ActiveSessionRegistry::new();
    /// # async fn check(registry: ActiveSessionRegistry) {
    /// assert_eq!(registry.len().await, 0);
    /// # }
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of active sessions in the registry.
    ///
    /// # Returns
    ///
    /// Returns the current active session count.
    pub async fn len(&self) -> usize {
        self.sessions.read().await.len()
    }

    /// Returns `true` when no active sessions are registered.
    ///
    /// # Returns
    ///
    /// Returns whether the registry is empty.
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Returns whether a session ID exists in the registry.
    ///
    /// # Arguments
    ///
    /// * `session_id` - ACP session ID to look up.
    ///
    /// # Returns
    ///
    /// Returns `true` when the session is active.
    pub async fn contains(&self, session_id: &acp::SessionId) -> bool {
        self.sessions.read().await.contains_key(session_id)
    }

    async fn insert(&self, session: ActiveSessionState) {
        self.sessions
            .write()
            .await
            .insert(session.session_id.clone(), Arc::new(Mutex::new(session)));
    }

    async fn get(&self, session_id: &acp::SessionId) -> Option<Arc<Mutex<ActiveSessionState>>> {
        self.sessions.read().await.get(session_id).cloned()
    }
}

/// Active state for one ACP stdio session.
///
/// Each session owns a workspace root, a fresh XZatoma conversation ID, a
/// mutable XZatoma agent, a prompt queue, cancellation scaffolding, and any MCP
/// manager handle required to keep registered MCP tools alive.
pub struct ActiveSessionState {
    session_id: acp::SessionId,
    workspace_root: PathBuf,
    conversation_uuid: String,
    xzatoma_agent: Arc<Mutex<XzatomaAgent>>,
    provider_name: String,
    current_model_name: String,
    current_cancellation_token: CancellationToken,
    prompt_queue: mpsc::Sender<QueuedPrompt>,
    prompt_worker_handle: JoinHandle<()>,
    mcp_manager: Option<Arc<RwLock<McpClientManager>>>,
    last_activity: String,
}

impl ActiveSessionState {
    /// Returns the ACP session ID.
    ///
    /// # Returns
    ///
    /// Returns a borrowed ACP session ID for this active session.
    pub fn session_id(&self) -> &acp::SessionId {
        &self.session_id
    }

    /// Returns the workspace root for the session.
    ///
    /// # Returns
    ///
    /// Returns the resolved workspace root used for tools and skill discovery.
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Returns the XZatoma conversation UUID associated with the session.
    ///
    /// # Returns
    ///
    /// Returns the internal conversation UUID as a string slice.
    pub fn conversation_uuid(&self) -> &str {
        &self.conversation_uuid
    }

    /// Returns the provider name configured for the session.
    ///
    /// # Returns
    ///
    /// Returns the provider name as a string slice.
    pub fn provider_name(&self) -> &str {
        &self.provider_name
    }

    /// Returns the current model name configured for the session.
    ///
    /// # Returns
    ///
    /// Returns the model name as a string slice.
    pub fn current_model_name(&self) -> &str {
        &self.current_model_name
    }

    /// Returns the last activity timestamp.
    ///
    /// # Returns
    ///
    /// Returns an RFC 3339 timestamp string for the last observed session
    /// activity.
    pub fn last_activity(&self) -> &str {
        &self.last_activity
    }

    /// Returns whether an MCP manager is kept alive for this session.
    ///
    /// # Returns
    ///
    /// Returns `true` when MCP tools are registered and require a live manager.
    pub fn has_mcp_manager(&self) -> bool {
        self.mcp_manager.is_some()
    }

    /// Returns a shared handle to the mutable XZatoma agent for this session.
    ///
    /// # Returns
    ///
    /// Returns an `Arc` clone of the session agent handle. Callers must lock the
    /// returned mutex before inspecting or mutating the agent.
    pub fn xzatoma_agent(&self) -> Arc<Mutex<XzatomaAgent>> {
        Arc::clone(&self.xzatoma_agent)
    }

    /// Returns the cancellation token for the currently running prompt.
    ///
    /// # Returns
    ///
    /// Returns a clone of the session's current cancellation token.
    pub fn current_cancellation_token(&self) -> CancellationToken {
        self.current_cancellation_token.clone()
    }

    /// Returns whether the current prompt cancellation token is cancelled.
    ///
    /// # Returns
    ///
    /// Returns `true` when the current prompt has been cancelled.
    pub fn current_prompt_cancelled(&self) -> bool {
        self.current_cancellation_token.is_cancelled()
    }

    /// Returns whether the prompt worker task has completed.
    ///
    /// # Returns
    ///
    /// Returns `true` if the prompt worker has finished.
    pub fn prompt_worker_finished(&self) -> bool {
        self.prompt_worker_handle.is_finished()
    }
}

struct AcpStdioServerState {
    config: Config,
    options: AcpStdioAgentOptions,
    sessions: ActiveSessionRegistry,
}

impl AcpStdioServerState {
    fn new(config: Config, options: AcpStdioAgentOptions) -> Self {
        Self {
            config,
            options,
            sessions: ActiveSessionRegistry::new(),
        }
    }

    async fn create_session(
        &self,
        request: acp::NewSessionRequest,
    ) -> Result<acp::NewSessionResponse> {
        let workspace_root =
            resolve_workspace_root(&request.cwd, self.options.working_dir.as_deref())?;
        let provider_name = self.config.provider.provider_type.clone();
        let model_name = current_model_name(&self.config).to_string();

        let env = build_agent_environment(&self.config, &workspace_root, true).await?;
        let mut tools = env.tool_registry;

        let provider_box = create_provider_with_override(
            &self.config.provider,
            self.options.provider.as_deref(),
            self.options.model.as_deref(),
        )?;
        let provider: Arc<dyn Provider> = Arc::from(provider_box);

        if tools.get("subagent").is_none() {
            let subagent_tool = SubagentTool::new_with_config(
                Arc::clone(&provider),
                &self.config.provider,
                self.config.agent.clone(),
                tools.clone(),
                0,
            )?;
            tools.register("subagent", Arc::new(subagent_tool));
        }

        let mut agent =
            XzatomaAgent::new_from_shared_provider(provider, tools, self.config.agent.clone())?;

        let mut transient_system_messages =
            vec![prompts::build_system_prompt(env.chat_mode, env.safety_mode)];
        if let Some(disclosure) = env.skill_disclosure {
            transient_system_messages.push(disclosure);
        }
        agent.set_transient_system_messages(transient_system_messages);

        let session_id = acp::SessionId::new(format!("xzatoma-{}", Uuid::new_v4()));
        let conversation_uuid = Uuid::new_v4().to_string();
        let xzatoma_agent = Arc::new(Mutex::new(agent));
        let current_cancellation_token = CancellationToken::new();
        let (prompt_queue, prompt_receiver) = mpsc::channel(32);

        let prompt_worker_handle = tokio::spawn(run_prompt_worker(
            session_id.clone(),
            Arc::clone(&xzatoma_agent),
            prompt_receiver,
            current_cancellation_token.clone(),
        ));

        let active_session = ActiveSessionState {
            session_id: session_id.clone(),
            workspace_root,
            conversation_uuid,
            xzatoma_agent,
            provider_name,
            current_model_name: model_name,
            current_cancellation_token,
            prompt_queue,
            prompt_worker_handle,
            mcp_manager: env.mcp_manager,
            last_activity: chrono::Utc::now().to_rfc3339(),
        };

        self.sessions.insert(active_session).await;

        Ok(acp::NewSessionResponse::new(session_id))
    }

    async fn enqueue_prompt(
        &self,
        request: acp::PromptRequest,
    ) -> acp_sdk::Result<acp::PromptResponse> {
        let session = self
            .sessions
            .get(&request.session_id)
            .await
            .ok_or_else(|| {
                acp_internal_error(format!("unknown ACP session: {}", request.session_id))
            })?;

        let (response_tx, response_rx) = oneshot::channel();
        let (prompt_queue, workspace_root, provider_name, model_name) = {
            let mut session = session.lock().await;
            session.last_activity = chrono::Utc::now().to_rfc3339();
            (
                session.prompt_queue.clone(),
                session.workspace_root.clone(),
                session.provider_name.clone(),
                session.current_model_name.clone(),
            )
        };

        let prompt_input = acp_content_blocks_to_prompt_input(
            &request.prompt,
            &self.config.acp.stdio,
            &workspace_root,
        )
        .map_err(|error| acp_internal_error(error.to_string()))?;

        validate_provider_supports_prompt_input(&provider_name, &model_name, &prompt_input)
            .map_err(|error| acp_internal_error(error.to_string()))?;

        let message = prompt_input_to_user_message(prompt_input).map_err(acp_internal_error)?;

        prompt_queue
            .send(QueuedPrompt {
                messages: vec![message],
                response_tx,
            })
            .await
            .map_err(|error| {
                acp_internal_error(format!("session prompt queue is closed: {}", error))
            })?;

        response_rx.await.map_err(|error| {
            acp_internal_error(format!("prompt worker dropped response: {}", error))
        })?
    }
}

struct QueuedPrompt {
    messages: Vec<Message>,
    response_tx: oneshot::Sender<acp_sdk::Result<acp::PromptResponse>>,
}

/// Runs the ACP stdio agent subprocess entry point.
///
/// This function applies CLI overrides, validates the effective configuration,
/// constructs the ACP newline-delimited JSON-RPC transport over stdin/stdout,
/// and serves the agent role until the ACP connection closes.
///
/// # Arguments
///
/// * `config` - Loaded XZatoma configuration.
/// * `options` - CLI-derived stdio agent runtime options.
///
/// # Errors
///
/// Returns an error if the effective configuration is invalid or if the ACP
/// stdio server exits with a protocol or transport error.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::acp::stdio::{run_stdio_agent, AcpStdioAgentOptions};
/// use xzatoma::Config;
///
/// # async fn example() -> anyhow::Result<()> {
/// run_stdio_agent(Config::default(), AcpStdioAgentOptions::new(None, None, false, None)).await?;
/// # Ok(())
/// # }
/// ```
pub async fn run_stdio_agent(mut config: Config, options: AcpStdioAgentOptions) -> Result<()> {
    prepare_stdio_config(&mut config, &options)?;

    tracing::info!(
        provider = %config.provider.provider_type,
        model = %current_model_name(&config),
        allow_dangerous = options.allow_dangerous,
        working_dir = ?options.working_dir,
        "ACP stdio agent initialized"
    );

    #[cfg(test)]
    {
        Ok(())
    }

    #[cfg(not(test))]
    {
        let transport = ByteStreams::new(
            tokio::io::stdout().compat_write(),
            tokio::io::stdin().compat(),
        );
        run_stdio_agent_with_transport(config, options, transport).await
    }
}

/// Runs the ACP stdio agent against an arbitrary ACP transport.
///
/// This is primarily used by in-memory protocol tests with
/// [`agent_client_protocol::Channel::duplex`]. Production code should call
/// [`run_stdio_agent`], which binds the server to process stdin/stdout.
///
/// # Arguments
///
/// * `config` - Loaded XZatoma configuration.
/// * `options` - CLI-derived stdio agent runtime options.
/// * `transport` - ACP transport to serve.
///
/// # Errors
///
/// Returns an error if configuration validation or ACP protocol serving fails.
pub async fn run_stdio_agent_with_transport<T>(
    mut config: Config,
    options: AcpStdioAgentOptions,
    transport: T,
) -> Result<()>
where
    T: AcpConnectTo<AcpAgentRole>,
{
    prepare_stdio_config(&mut config, &options)?;

    let state = Arc::new(AcpStdioServerState::new(config, options));

    AcpAgentRole
        .builder()
        .name("xzatoma")
        .on_receive_request(
            async move |initialize: acp::InitializeRequest,
                        responder: Responder<acp::InitializeResponse>,
                        _connection: ConnectionTo<AcpClientRole>| {
                responder.respond(handle_initialize(initialize))
            },
            acp_sdk::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |new_session: acp::NewSessionRequest,
                            responder: Responder<acp::NewSessionResponse>,
                            _connection: ConnectionTo<AcpClientRole>| {
                    match state.create_session(new_session).await {
                        Ok(response) => responder.respond(response),
                        Err(error) => responder.respond_with_error(acp_internal_error(error)),
                    }
                }
            },
            acp_sdk::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |prompt: acp::PromptRequest,
                            responder: Responder<acp::PromptResponse>,
                            _connection: ConnectionTo<AcpClientRole>| {
                    match state.enqueue_prompt(prompt).await {
                        Ok(response) => responder.respond(response),
                        Err(error) => responder.respond_with_error(error),
                    }
                }
            },
            acp_sdk::on_receive_request!(),
        )
        .on_receive_dispatch(
            async move |message: Dispatch, connection: ConnectionTo<AcpClientRole>| {
                message.respond_with_error(acp_sdk::Error::method_not_found(), connection)
            },
            acp_sdk::on_receive_dispatch!(),
        )
        .connect_to(transport)
        .await
        .map_err(|error| XzatomaError::Internal(format!("ACP stdio server failed: {}", error)))
}

/// Applies ACP stdio agent CLI options to a configuration clone.
///
/// Provider and model overrides are applied to the in-memory configuration used
/// by the current subprocess. When `allow_dangerous` is set, terminal execution
/// mode is escalated to full autonomous operation for this subprocess only.
///
/// # Arguments
///
/// * `config` - Mutable configuration to update.
/// * `options` - CLI-derived options to apply.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::stdio::{apply_stdio_agent_options, AcpStdioAgentOptions};
/// use xzatoma::config::{Config, ExecutionMode};
///
/// let mut config = Config::default();
/// let options = AcpStdioAgentOptions::new(
///     Some("ollama".to_string()),
///     Some("llama3.2:latest".to_string()),
///     true,
///     None,
/// );
///
/// apply_stdio_agent_options(&mut config, &options);
///
/// assert_eq!(config.provider.provider_type, "ollama");
/// assert_eq!(config.provider.ollama.model, "llama3.2:latest");
/// assert_eq!(config.agent.terminal.default_mode, ExecutionMode::FullAutonomous);
/// ```
pub fn apply_stdio_agent_options(config: &mut Config, options: &AcpStdioAgentOptions) {
    if let Some(provider) = &options.provider {
        config.provider.provider_type = provider.clone();
    }

    if let Some(model) = &options.model {
        match config.provider.provider_type.as_str() {
            "copilot" => config.provider.copilot.model = model.clone(),
            "ollama" => config.provider.ollama.model = model.clone(),
            "openai" => config.provider.openai.model = model.clone(),
            _ => {}
        }
    }

    if options.allow_dangerous {
        config.agent.terminal.default_mode = ExecutionMode::FullAutonomous;
        config.agent.chat.default_safety = "yolo".to_string();
    }
}

/// Resolves the workspace root for a new ACP session.
///
/// The ACP request `cwd` is preferred when it is non-empty and absolute.
/// Otherwise this function falls back to the CLI `--working-dir`, then to the
/// process current directory.
///
/// # Arguments
///
/// * `request_cwd` - Workspace path from `NewSessionRequest`.
/// * `fallback_working_dir` - Optional CLI fallback path.
///
/// # Returns
///
/// Returns an absolute workspace root.
///
/// # Errors
///
/// Returns an I/O error if the process current directory cannot be read.
pub fn resolve_workspace_root(
    request_cwd: &Path,
    fallback_working_dir: Option<&Path>,
) -> Result<PathBuf> {
    if !request_cwd.as_os_str().is_empty() {
        if request_cwd.is_absolute() {
            return Ok(request_cwd.to_path_buf());
        }

        return Ok(std::env::current_dir()?.join(request_cwd));
    }

    if let Some(working_dir) = fallback_working_dir {
        if working_dir.is_absolute() {
            return Ok(working_dir.to_path_buf());
        }

        return Ok(std::env::current_dir()?.join(working_dir));
    }

    Ok(std::env::current_dir()?)
}

/// Returns the ACP initialize response for XZatoma.
///
/// # Arguments
///
/// * `request` - Client initialization request.
///
/// # Returns
///
/// Returns the negotiated protocol version, XZatoma implementation metadata,
/// prompt capabilities, and currently implemented session capabilities.
pub fn handle_initialize(request: acp::InitializeRequest) -> acp::InitializeResponse {
    let protocol_version = negotiate_protocol_version(&request.protocol_version);

    acp::InitializeResponse::new(protocol_version)
        .agent_info(acp::Implementation::new(
            "xzatoma",
            env!("CARGO_PKG_VERSION"),
        ))
        .agent_capabilities(
            acp::AgentCapabilities::new()
                .load_session(false)
                .prompt_capabilities(acp::PromptCapabilities::new().image(true))
                .mcp_capabilities(acp::McpCapabilities::new())
                .session_capabilities(acp::SessionCapabilities::new()),
        )
        .auth_methods(Vec::new())
}

fn negotiate_protocol_version(requested: &acp::ProtocolVersion) -> acp::ProtocolVersion {
    if requested <= &acp::ProtocolVersion::LATEST {
        requested.clone()
    } else {
        acp::ProtocolVersion::LATEST
    }
}

fn prepare_stdio_config(config: &mut Config, options: &AcpStdioAgentOptions) -> Result<()> {
    apply_stdio_agent_options(config, options);
    config.validate()
}

fn current_model_name(config: &Config) -> &str {
    match config.provider.provider_type.as_str() {
        "copilot" => &config.provider.copilot.model,
        "ollama" => &config.provider.ollama.model,
        "openai" => &config.provider.openai.model,
        _ => "unknown",
    }
}

async fn run_prompt_worker(
    session_id: acp::SessionId,
    agent: Arc<Mutex<XzatomaAgent>>,
    mut prompt_receiver: mpsc::Receiver<QueuedPrompt>,
    cancellation_token: CancellationToken,
) {
    while let Some(queued_prompt) = prompt_receiver.recv().await {
        let response = execute_queued_prompt(
            &session_id,
            Arc::clone(&agent),
            queued_prompt.messages,
            &cancellation_token,
        )
        .await;

        if queued_prompt.response_tx.send(response).is_err() {
            tracing::debug!(session_id = %session_id, "ACP prompt response receiver dropped");
        }
    }

    tracing::debug!(session_id = %session_id, "ACP prompt worker stopped");
}

async fn execute_queued_prompt(
    session_id: &acp::SessionId,
    agent: Arc<Mutex<XzatomaAgent>>,
    messages: Vec<Message>,
    cancellation_token: &CancellationToken,
) -> acp_sdk::Result<acp::PromptResponse> {
    if cancellation_token.is_cancelled() {
        return Ok(acp::PromptResponse::new(acp::StopReason::Cancelled));
    }

    tracing::debug!(
        session_id = %session_id,
        message_count = messages.len(),
        "Processing ACP queued multimodal prompt"
    );

    let mut agent = agent.lock().await;
    agent
        .execute_provider_messages(messages)
        .await
        .map_err(|error| acp_internal_error(format!("prompt execution failed: {}", error)))?;

    if cancellation_token.is_cancelled() {
        Ok(acp::PromptResponse::new(acp::StopReason::Cancelled))
    } else {
        Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
    }
}

fn prompt_input_to_user_message(
    input: MultimodalPromptInput,
) -> std::result::Result<Message, String> {
    if input.has_images() {
        Message::try_user_from_multimodal_input(input)
    } else {
        Message::try_user_from_text_input(input)
    }
}

fn acp_internal_error(error: impl ToString) -> acp_sdk::Error {
    acp_sdk::util::internal_error(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::{
        Channel, JsonRpcMessage, JsonRpcRequest, JsonRpcResponse, SentRequest, UntypedMessage,
    };
    use serde::{Deserialize, Serialize};

    async fn receive_response<T: JsonRpcResponse + Send>(
        response: SentRequest<T>,
    ) -> std::result::Result<T, acp_sdk::Error> {
        let (tx, rx) = oneshot::channel();
        response.on_receiving_result(async move |result| {
            tx.send(result).map_err(|send_error| {
                acp_internal_error(format!("response receiver failed: {:?}", send_error))
            })
        })?;

        rx.await.map_err(|receive_error| {
            acp_internal_error(format!("response channel closed: {}", receive_error))
        })?
    }

    async fn run_client_server_test<F, Fut>(client_operation: F)
    where
        F: FnOnce(ConnectionTo<AcpAgentRole>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let (server_channel, client_channel) = Channel::duplex();
        let config = Config::default();
        let options = AcpStdioAgentOptions::new(None, None, false, None);

        let server = tokio::spawn(async move {
            run_stdio_agent_with_transport(config, options, server_channel).await
        });

        let client_result = AcpClientRole
            .builder()
            .name("xzatoma-stdio-test-client")
            .connect_with(client_channel, async move |connection| {
                client_operation(connection).await;
                Ok(())
            })
            .await;

        assert!(
            client_result.is_ok(),
            "ACP test client should complete successfully: {:?}",
            client_result.err()
        );

        server.abort();
    }

    #[test]
    fn test_options_new_sets_fields() {
        let options = AcpStdioAgentOptions::new(
            Some("openai".to_string()),
            Some("gpt-4o".to_string()),
            true,
            Some(PathBuf::from("/tmp/workspace")),
        );

        assert_eq!(options.provider.as_deref(), Some("openai"));
        assert_eq!(options.model.as_deref(), Some("gpt-4o"));
        assert!(options.allow_dangerous);
        assert_eq!(options.working_dir, Some(PathBuf::from("/tmp/workspace")));
    }

    #[test]
    fn test_apply_stdio_agent_options_overrides_copilot_model() {
        let mut config = Config::default();
        config.provider.provider_type = "copilot".to_string();

        let options = AcpStdioAgentOptions::new(None, Some("gpt-4o".to_string()), false, None);

        apply_stdio_agent_options(&mut config, &options);

        assert_eq!(config.provider.provider_type, "copilot");
        assert_eq!(config.provider.copilot.model, "gpt-4o");
        assert_eq!(
            config.agent.terminal.default_mode,
            ExecutionMode::RestrictedAutonomous
        );
    }

    #[test]
    fn test_apply_stdio_agent_options_overrides_ollama_model() {
        let mut config = Config::default();

        let options = AcpStdioAgentOptions::new(
            Some("ollama".to_string()),
            Some("llama3.2:latest".to_string()),
            false,
            None,
        );

        apply_stdio_agent_options(&mut config, &options);

        assert_eq!(config.provider.provider_type, "ollama");
        assert_eq!(config.provider.ollama.model, "llama3.2:latest");
    }

    #[test]
    fn test_apply_stdio_agent_options_overrides_openai_model() {
        let mut config = Config::default();

        let options = AcpStdioAgentOptions::new(
            Some("openai".to_string()),
            Some("gpt-4o-mini".to_string()),
            false,
            None,
        );

        apply_stdio_agent_options(&mut config, &options);

        assert_eq!(config.provider.provider_type, "openai");
        assert_eq!(config.provider.openai.model, "gpt-4o-mini");
    }

    #[test]
    fn test_apply_stdio_agent_options_allow_dangerous_sets_full_autonomous_and_yolo() {
        let mut config = Config::default();
        let options = AcpStdioAgentOptions::new(None, None, true, None);

        apply_stdio_agent_options(&mut config, &options);

        assert_eq!(
            config.agent.terminal.default_mode,
            ExecutionMode::FullAutonomous
        );
        assert_eq!(config.agent.chat.default_safety, "yolo");
    }

    #[test]
    fn test_handle_initialize_returns_xzatoma_metadata() {
        let response = handle_initialize(acp::InitializeRequest::new(acp::ProtocolVersion::V1));

        assert_eq!(response.protocol_version, acp::ProtocolVersion::V1);
        let agent_info = match response.agent_info {
            Some(agent_info) => agent_info,
            None => panic!("initialize response should include agent info"),
        };
        assert_eq!(agent_info.name, "xzatoma");
        assert_eq!(agent_info.version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_handle_initialize_advertises_text_and_vision_prompt_capabilities() {
        let response = handle_initialize(acp::InitializeRequest::new(acp::ProtocolVersion::V1));

        assert!(response.agent_capabilities.prompt_capabilities.image);
        assert!(!response.agent_capabilities.prompt_capabilities.audio);
        assert!(!response.agent_capabilities.load_session);
    }

    #[test]
    fn test_resolve_workspace_root_uses_absolute_request_cwd() {
        let request_cwd = PathBuf::from("/tmp/xzatoma-acp-request");

        let result = resolve_workspace_root(&request_cwd, Some(Path::new("/tmp/fallback")));

        assert_eq!(result.ok(), Some(request_cwd));
    }

    #[test]
    fn test_prompt_input_to_user_message_converts_text_input() {
        let message = prompt_input_to_user_message(MultimodalPromptInput::text("hello"));

        assert!(matches!(
            message,
            Ok(ref value) if value.content.as_deref() == Some("hello")
        ));
    }

    #[tokio::test]
    async fn test_run_stdio_agent_accepts_default_config() {
        let config = Config::default();
        let options = AcpStdioAgentOptions::new(None, None, false, None);

        let result = run_stdio_agent(config, options).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_stdio_agent_rejects_invalid_provider() {
        let config = Config::default();
        let options = AcpStdioAgentOptions::new(Some("invalid".to_string()), None, false, None);

        let result = run_stdio_agent(config, options).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_initialize_request_returns_xzatoma_metadata_over_protocol() {
        run_client_server_test(|connection| async move {
            let response = receive_response(
                connection.send_request(acp::InitializeRequest::new(acp::ProtocolVersion::V1)),
            )
            .await;

            let response = match response {
                Ok(response) => response,
                Err(error) => panic!("initialize should succeed: {}", error),
            };

            assert_eq!(response.protocol_version, acp::ProtocolVersion::V1);
            let agent_info = match response.agent_info {
                Some(agent_info) => agent_info,
                None => panic!("initialize should include agent info"),
            };
            assert_eq!(agent_info.name, "xzatoma");
        })
        .await;
    }

    #[tokio::test]
    async fn test_initialize_request_prompt_capabilities_include_vision_over_protocol() {
        run_client_server_test(|connection| async move {
            let response = receive_response(
                connection.send_request(acp::InitializeRequest::new(acp::ProtocolVersion::V1)),
            )
            .await;

            let response = match response {
                Ok(response) => response,
                Err(error) => panic!("initialize should succeed: {}", error),
            };

            assert!(response.agent_capabilities.prompt_capabilities.image);
            assert!(!response.agent_capabilities.prompt_capabilities.audio);
        })
        .await;
    }

    #[tokio::test]
    async fn test_new_session_request_returns_non_empty_session_id() {
        run_client_server_test(|connection| async move {
            let cwd = match std::env::current_dir() {
                Ok(cwd) => cwd,
                Err(error) => panic!("current dir should be available: {}", error),
            };

            let response =
                receive_response(connection.send_request(acp::NewSessionRequest::new(cwd))).await;

            let response = match response {
                Ok(response) => response,
                Err(error) => panic!("new session should succeed: {}", error),
            };

            assert!(!response.session_id.0.is_empty());
        })
        .await;
    }

    #[tokio::test]
    async fn test_two_new_session_requests_return_distinct_session_ids() {
        run_client_server_test(|connection| async move {
            let cwd = match std::env::current_dir() {
                Ok(cwd) => cwd,
                Err(error) => panic!("current dir should be available: {}", error),
            };

            let first =
                receive_response(connection.send_request(acp::NewSessionRequest::new(cwd.clone())))
                    .await;
            let second =
                receive_response(connection.send_request(acp::NewSessionRequest::new(cwd))).await;

            let first = match first {
                Ok(response) => response,
                Err(error) => panic!("first new session should succeed: {}", error),
            };
            let second = match second {
                Ok(response) => response,
                Err(error) => panic!("second new session should succeed: {}", error),
            };

            assert_ne!(first.session_id, second.session_id);
        })
        .await;
    }

    #[tokio::test]
    async fn test_unsupported_method_returns_protocol_error() {
        run_client_server_test(|connection| async move {
            let response = receive_response(connection.send_request(UnsupportedRequest {
                value: "test".to_string(),
            }))
            .await;

            assert!(
                response.is_err(),
                "unsupported methods should return protocol errors"
            );
        })
        .await;
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct UnsupportedRequest {
        value: String,
    }

    impl JsonRpcMessage for UnsupportedRequest {
        fn matches_method(method: &str) -> bool {
            method == "xzatoma/unsupported"
        }

        fn method(&self) -> &str {
            "xzatoma/unsupported"
        }

        fn to_untyped_message(&self) -> std::result::Result<UntypedMessage, acp_sdk::Error> {
            UntypedMessage::new(self.method(), self)
        }

        fn parse_message(
            method: &str,
            params: &impl serde::Serialize,
        ) -> std::result::Result<Self, acp_sdk::Error> {
            if !Self::matches_method(method) {
                return Err(acp_sdk::Error::method_not_found());
            }
            acp_sdk::util::json_cast_params(params)
        }
    }

    impl JsonRpcRequest for UnsupportedRequest {
        type Response = UnsupportedResponse;
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct UnsupportedResponse {
        value: String,
    }

    impl JsonRpcResponse for UnsupportedResponse {
        fn into_json(
            self,
            _method: &str,
        ) -> std::result::Result<serde_json::Value, acp_sdk::Error> {
            serde_json::to_value(self).map_err(acp_sdk::Error::into_internal_error)
        }

        fn from_value(
            _method: &str,
            value: serde_json::Value,
        ) -> std::result::Result<Self, acp_sdk::Error> {
            acp_sdk::util::json_cast(&value)
        }
    }
}
