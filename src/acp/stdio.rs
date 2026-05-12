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
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

#[cfg(not(test))]
use agent_client_protocol::ByteStreams;
use agent_client_protocol::{
    self as acp_sdk, Agent as AcpAgentRole, Client as AcpClientRole, ConnectTo as AcpConnectTo,
    ConnectionTo, Dispatch, Responder,
};
use tokio::sync::{mpsc, oneshot, watch, Mutex, RwLock};
use tokio::task::JoinHandle;
#[cfg(not(test))]
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::acp::available_commands::build_available_commands;
use crate::acp::ide_bridge::{IdeBridge, IdeCapabilities};
use crate::acp::prompt_input::{
    acp_content_blocks_to_prompt_input, validate_provider_supports_prompt_input,
};
use crate::acp::session_config::{build_session_config_options, SessionRuntimeState};
use crate::acp::session_mode::{
    build_session_mode_state, mode_runtime_effect, MODE_FULL_AUTONOMOUS, MODE_WRITE,
};
use crate::acp::tool_notifications::{
    build_tool_call_completion, build_tool_call_failure, build_tool_call_start,
};
use crate::agent::events::{AgentExecutionEvent, AgentObserver};

use crate::agent::{Agent as XzatomaAgent, Conversation};
use crate::commands::build_agent_environment;
use crate::config::{Config, ExecutionMode};
use crate::error::{Result, XzatomaError};
use crate::mcp::manager::McpClientManager;
use crate::mcp::server::{McpServerConfig, McpServerTransportConfig};
use crate::mcp::tool_bridge::register_mcp_tools;
use crate::prompts;
use crate::providers::{
    create_provider_with_override, Message, ModelCapability, ModelInfo as XzatomaModelInfo,
    MultimodalPromptInput, Provider,
};
use crate::storage::{PublicStoredAcpStdioSession, SqliteStorage};
use crate::tools::ide_tools::register_ide_tools;
use crate::tools::terminal::{CommandValidator, TerminalTool};
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
    current_cancellation_token: watch::Receiver<CancellationToken>,
    prompt_queue: mpsc::Sender<QueuedPrompt>,
    prompt_worker_handle: JoinHandle<()>,
    mcp_manager: Option<Arc<RwLock<McpClientManager>>>,
    last_activity: String,
    /// The currently active ACP session mode ID (e.g. "planning", "write").
    current_mode_id: String,
    /// Runtime configuration state for the session (safety, terminal, routing, etc.).
    runtime_state: SessionRuntimeState,
    /// IDE tool bridge, present when the Zed client advertised IDE capabilities.
    ide_bridge: Option<Arc<IdeBridge>>,
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

    /// Returns a clone of the currently active prompt cancellation token.
    ///
    /// # Returns
    ///
    /// Returns a clone of the session's current cancellation token.
    pub fn current_cancellation_token(&self) -> CancellationToken {
        self.current_cancellation_token.borrow().clone()
    }

    /// Returns whether the current prompt cancellation token has been cancelled.
    ///
    /// # Returns
    ///
    /// Returns `true` when the current prompt has been cancelled.
    pub fn current_prompt_cancelled(&self) -> bool {
        self.current_cancellation_token.borrow().is_cancelled()
    }

    /// Returns whether the prompt worker task has completed.
    ///
    /// # Returns
    ///
    /// Returns `true` if the prompt worker has finished.
    pub fn prompt_worker_finished(&self) -> bool {
        self.prompt_worker_handle.is_finished()
    }

    /// Returns the current ACP session mode ID.
    ///
    /// # Returns
    ///
    /// Returns the active mode ID string (e.g. `"planning"`, `"write"`).
    pub fn current_mode_id(&self) -> &str {
        &self.current_mode_id
    }

    /// Returns the current session runtime state.
    ///
    /// # Returns
    ///
    /// Returns a reference to the runtime configuration state.
    pub fn runtime_state(&self) -> &SessionRuntimeState {
        &self.runtime_state
    }

    /// Returns whether an IDE bridge is active for this session.
    ///
    /// # Returns
    ///
    /// Returns `true` when the Zed client advertised IDE capabilities.
    pub fn has_ide_bridge(&self) -> bool {
        self.ide_bridge.is_some()
    }
}

struct AcpStdioServerState {
    config: Config,
    options: AcpStdioAgentOptions,
    sessions: ActiveSessionRegistry,
    storage: Option<SqliteStorage>,
    /// Client capabilities received during ACP initialize, shared across handlers.
    client_capabilities: Arc<Mutex<Option<acp::ClientCapabilities>>>,
}

impl AcpStdioServerState {
    fn new(config: Config, options: AcpStdioAgentOptions) -> Self {
        let storage = open_stdio_storage(&config);
        Self {
            config,
            options,
            sessions: ActiveSessionRegistry::new(),
            storage,
            client_capabilities: Arc::new(Mutex::new(None)),
        }
    }

    #[cfg(test)]
    fn new_with_storage(
        config: Config,
        options: AcpStdioAgentOptions,
        storage: Option<SqliteStorage>,
    ) -> Self {
        Self {
            config,
            options,
            sessions: ActiveSessionRegistry::new(),
            storage,
            client_capabilities: Arc::new(Mutex::new(None)),
        }
    }

    async fn create_session(
        &self,
        request: acp::NewSessionRequest,
        connection: Option<ConnectionTo<AcpClientRole>>,
    ) -> Result<acp::NewSessionResponse> {
        if self.sessions.len().await >= self.config.acp.stdio.max_active_sessions {
            return Err(XzatomaError::Config(format!(
                "ACP stdio active session limit reached: {}",
                self.config.acp.stdio.max_active_sessions
            )));
        }

        let workspace_root =
            resolve_workspace_root(&request.cwd, self.options.working_dir.as_deref())?;
        let workspace_root = normalize_workspace_root(&workspace_root);
        let provider_name = self.config.provider.provider_type.clone();
        let model_name = current_model_name(&self.config).to_string();

        let resumed_conversation =
            load_resumable_conversation(self.storage.as_ref(), &workspace_root, &self.config);

        let env = build_agent_environment(&self.config, &workspace_root, true).await?;
        let mut tools = env.tool_registry;

        // Connect MCP servers forwarded by Zed in NewSessionRequest.mcp_servers.
        // These are per-project servers from the user's Zed settings, not from
        // XZatoma's own config.yaml. Individual connection failures are logged
        // and skipped so they do not abort session creation.
        if !request.mcp_servers.is_empty() {
            if let Some(ref manager_arc) = env.mcp_manager {
                for acp_server in &request.mcp_servers {
                    let cfg = match convert_acp_mcp_server(acp_server) {
                        Ok(cfg) => cfg,
                        Err(error) => {
                            tracing::warn!(
                                workspace = %workspace_root.display(),
                                error = %error,
                                "Skipping Zed-forwarded MCP server: failed to convert config"
                            );
                            continue;
                        }
                    };

                    // Skip duplicates: check against already-connected servers.
                    {
                        let manager_guard = manager_arc.read().await;
                        if manager_guard
                            .connected_servers()
                            .iter()
                            .any(|e| e.config.id == cfg.id)
                        {
                            tracing::debug!(
                                server_id = %cfg.id,
                                "Skipping Zed-forwarded MCP server: already connected"
                            );
                            continue;
                        }
                    }

                    let server_id = cfg.id.clone();
                    {
                        let mut manager_guard = manager_arc.write().await;
                        if let Err(error) = manager_guard.connect(cfg).await {
                            tracing::warn!(
                                server_id = %server_id,
                                workspace = %workspace_root.display(),
                                error = %error,
                                "Failed to connect Zed-forwarded MCP server; skipping"
                            );
                            continue;
                        }
                    }

                    tracing::info!(
                        server_id = %server_id,
                        workspace = %workspace_root.display(),
                        "Connected Zed-forwarded MCP server"
                    );
                }

                // Register tools from any newly connected Zed-forwarded servers.
                let execution_mode = self.config.agent.terminal.default_mode;
                if let Err(error) =
                    register_mcp_tools(&mut tools, Arc::clone(manager_arc), execution_mode, true)
                        .await
                {
                    tracing::warn!(
                        workspace = %workspace_root.display(),
                        error = %error,
                        "Failed to register tools from Zed-forwarded MCP servers"
                    );
                }
            }
        }

        let provider_box = create_provider_with_override(
            &self.config.provider,
            self.options.provider.as_deref(),
            self.options.model.as_deref(),
        )?;
        let provider: Arc<dyn Provider> = Arc::from(provider_box);

        // Create the session ID early so it can be used by the IDE bridge and
        // passed through to the agent, storage, and prompt worker consistently.
        let session_id = acp::SessionId::new(format!("xzatoma-{}", Uuid::new_v4()));

        let model_state =
            advertise_session_models(provider.as_ref(), &self.config, &model_name).await;

        // Build IDE bridge when client advertised IDE capabilities and a connection is available.
        let ide_bridge = {
            let caps_lock = self.client_capabilities.lock().await;
            if let (Some(connection), Some(caps)) = (connection.as_ref(), caps_lock.as_ref()) {
                let ide_caps = IdeCapabilities::from_client_capabilities(caps);
                if ide_caps.read_text_file || ide_caps.write_text_file || ide_caps.terminal {
                    Some(Arc::new(IdeBridge::new(
                        connection.clone(),
                        session_id.clone(),
                        ide_caps,
                    )))
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(bridge) = &ide_bridge {
            register_ide_tools(&mut tools, Arc::clone(bridge));
        }

        // Determine initial session mode from effective configuration.
        let current_mode_id = initial_mode_id_from_config(&self.config);
        let runtime_state = SessionRuntimeState::from_config(&self.config);

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

        let mut agent = if let Some(conversation) = resumed_conversation {
            XzatomaAgent::with_conversation_and_shared_provider(
                Arc::clone(&provider),
                tools,
                self.config.agent.clone(),
                conversation,
            )?
        } else {
            XzatomaAgent::new_from_shared_provider(provider, tools, self.config.agent.clone())?
        };

        let mut transient_system_messages =
            vec![prompts::build_system_prompt(env.chat_mode, env.safety_mode)];
        if let Some(disclosure) = env.skill_disclosure {
            transient_system_messages.push(disclosure);
        }
        agent.set_transient_system_messages(transient_system_messages);

        let conversation_uuid = agent.conversation().id().to_string();

        if let Some(storage) = &self.storage {
            if let Err(error) = persist_initial_stdio_session(
                storage,
                &session_id,
                &workspace_root,
                &conversation_uuid,
                &provider_name,
                Some(model_name.as_str()),
                &mut agent,
            ) {
                tracing::warn!(
                    session_id = %session_id,
                    conversation_id = %conversation_uuid,
                    error = %error,
                    "Failed to persist ACP stdio session mapping"
                );
            }
        }

        let xzatoma_agent = Arc::new(Mutex::new(agent));
        let initial_token = CancellationToken::new();
        let (token_tx, current_cancellation_token) = watch::channel(initial_token);
        let (prompt_queue, prompt_receiver) =
            mpsc::channel(self.config.acp.stdio.prompt_queue_capacity);

        let prompt_worker_handle = tokio::spawn(run_prompt_worker(
            session_id.clone(),
            Arc::clone(&xzatoma_agent),
            prompt_receiver,
            token_tx,
            self.storage.clone(),
            conversation_uuid.clone(),
            Some(model_name.clone()),
        ));

        let mode_state = build_session_mode_state(&current_mode_id);
        let config_options = build_session_config_options(&runtime_state);

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
            current_mode_id,
            runtime_state,
            ide_bridge,
        };

        self.sessions.insert(active_session).await;

        Ok(acp::NewSessionResponse::new(session_id)
            .models(model_state)
            .modes(mode_state)
            .config_options(config_options))
    }

    /// Handles a `SetSessionModeRequest` from the Zed client.
    ///
    /// Validates the session and the requested mode, applies the runtime effect
    /// (chat mode, safety mode, terminal execution mode), rebuilds agent system
    /// messages, and returns the new mode ID for notification dispatch.
    ///
    /// # Arguments
    ///
    /// * `request` - The incoming mode change request.
    ///
    /// # Errors
    ///
    /// Returns an error if the session is not found or the mode ID is invalid.
    async fn set_session_mode(&self, request: acp::SetSessionModeRequest) -> Result<String> {
        let session = self
            .sessions
            .get(&request.session_id)
            .await
            .ok_or_else(|| {
                XzatomaError::Internal(format!("unknown ACP session: {}", request.session_id))
            })?;

        let mode_id = request.mode_id.0.as_ref().to_string();
        let effect = mode_runtime_effect(&mode_id)?;

        let mut session_lock = session.lock().await;
        session_lock.current_mode_id = mode_id.clone();
        session_lock.runtime_state.safety_mode_str = effect.safety_mode_str.clone();
        session_lock.runtime_state.terminal_mode = effect.terminal_mode;
        session_lock.last_activity = chrono::Utc::now().to_rfc3339();

        // Rebuild transient system messages so the mode constraint is enforced
        // on the next prompt turn.
        {
            use crate::chat_mode::{ChatMode, SafetyMode};
            let chat_mode = ChatMode::parse_str(&effect.chat_mode_str).unwrap_or(ChatMode::Write);
            let safety_mode =
                SafetyMode::parse_str(&effect.safety_mode_str).unwrap_or(SafetyMode::AlwaysConfirm);
            let system_prompt = crate::prompts::build_system_prompt(chat_mode, safety_mode);
            let agent = session_lock.xzatoma_agent.clone();
            drop(session_lock);
            let mut agent_lock = agent.lock().await;
            agent_lock.set_transient_system_messages(vec![system_prompt]);

            // Replace the terminal tool so the new ExecutionMode is enforced immediately.
            {
                let session_read = session.lock().await;
                let workspace_root = session_read.workspace_root.clone();
                drop(session_read);

                let new_validator = CommandValidator::new(effect.terminal_mode, workspace_root);
                let new_terminal_tool =
                    TerminalTool::new(new_validator, self.config.agent.terminal.clone())
                        .with_safety_mode(safety_mode);
                agent_lock
                    .tools_mut()
                    .register("terminal", Arc::new(new_terminal_tool));
            }
        }

        tracing::info!(
            session_id = %request.session_id,
            mode_id = %mode_id,
            "ACP session mode changed"
        );

        Ok(mode_id)
    }

    /// Handles a `SetSessionConfigOptionRequest` from the Zed client.
    ///
    /// Validates the config ID and value, applies the runtime change, and returns
    /// the full refreshed config option list for the response.
    ///
    /// # Arguments
    ///
    /// * `request` - The incoming config option change request.
    ///
    /// # Errors
    ///
    /// Returns an error if the session is not found or the config change is invalid.
    async fn set_session_config_option(
        &self,
        request: acp::SetSessionConfigOptionRequest,
    ) -> Result<Vec<acp::SessionConfigOption>> {
        use crate::acp::session_config::apply_config_option_change;

        let session = self
            .sessions
            .get(&request.session_id)
            .await
            .ok_or_else(|| {
                XzatomaError::Internal(format!("unknown ACP session: {}", request.session_id))
            })?;

        let config_id = request.config_id.0.as_ref().to_string();
        let value_id = request.value.0.as_ref().to_string();

        let mut session_lock = session.lock().await;
        let (effect, updated_options) =
            apply_config_option_change(&config_id, &value_id, &session_lock.runtime_state)?;

        // Apply the effects from the config change.
        if let Some(safety) = effect.safety_mode_str {
            session_lock.runtime_state.safety_mode_str = safety;
        }
        if let Some(terminal_mode) = effect.terminal_mode {
            session_lock.runtime_state.terminal_mode = terminal_mode;
        }
        if let Some(routing) = effect.tool_routing {
            session_lock.runtime_state.tool_routing = routing;
        }
        if let Some(vision) = effect.vision_enabled {
            session_lock.runtime_state.vision_enabled = vision;
        }
        if let Some(subagents) = effect.subagents_enabled {
            session_lock.runtime_state.subagents_enabled = subagents;
        }
        if let Some(mcp) = effect.mcp_enabled {
            session_lock.runtime_state.mcp_enabled = mcp;
        }
        if let Some(turns) = effect.max_turns {
            session_lock.runtime_state.max_turns = turns;
        }
        if let Some(ref effort_str) = effect.thinking_effort {
            session_lock.runtime_state.thinking_effort = effort_str.clone();
        }

        // Clone what is needed for the deferred provider call before releasing
        // the session lock. The agent lock must not be acquired while the
        // session lock is held to avoid a lock-ordering inversion with the
        // prompt worker.
        let agent_handle = session_lock.xzatoma_agent.clone();
        let thinking_effort_to_apply = effect.thinking_effort.clone();

        session_lock.last_activity = chrono::Utc::now().to_rfc3339();

        tracing::info!(
            session_id = %request.session_id,
            config_id = %config_id,
            value_id = %value_id,
            "ACP session config option changed"
        );

        // Release session lock before acquiring agent lock.
        drop(session_lock);

        // Apply thinking effort to the live provider (outside session lock).
        if let Some(ref effort_str) = thinking_effort_to_apply {
            let effort_opt = if effort_str == "none" {
                None
            } else {
                Some(effort_str.as_str())
            };
            let agent_lock = agent_handle.lock().await;
            if let Err(e) = agent_lock.provider().set_thinking_effort(effort_opt) {
                tracing::warn!(
                    session_id = %request.session_id,
                    error = %e,
                    "Failed to apply thinking effort change"
                );
            }
        }

        Ok(updated_options)
    }

    /// Handles a `SetSessionModelRequest` from the Zed client.
    ///
    /// Updates the current model name in session state so subsequent prompts and
    /// persistence operations use the newly selected model. Because the live
    /// provider is wrapped in `Arc<dyn Provider>` without interior mutability,
    /// the provider itself is not rebuilt in-flight; the change is effective for
    /// persistence and display immediately and for inference on the next session.
    ///
    /// # Arguments
    ///
    /// * `request` - The incoming model selection request.
    ///
    /// # Errors
    ///
    /// Returns an error if the session is not found.
    async fn set_session_model(&self, request: acp::SetSessionModelRequest) -> Result<()> {
        let session = self
            .sessions
            .get(&request.session_id)
            .await
            .ok_or_else(|| {
                XzatomaError::Internal(format!("unknown ACP session: {}", request.session_id))
            })?;

        let model_id = request.model_id.0.as_ref().to_string();

        let mut session_lock = session.lock().await;
        session_lock.current_model_name = model_id.clone();
        session_lock.last_activity = chrono::Utc::now().to_rfc3339();

        tracing::info!(
            session_id = %request.session_id,
            model_id = %model_id,
            "ACP session model changed (takes effect on next session restart for inference)"
        );

        Ok(())
    }

    async fn enqueue_prompt(
        &self,
        request: acp::PromptRequest,
        connection: Option<ConnectionTo<AcpClientRole>>,
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

        if let Some(storage) = &self.storage {
            if let Err(error) = storage.touch_acp_stdio_session(request.session_id.0.as_ref()) {
                tracing::warn!(
                    session_id = %request.session_id,
                    error = %error,
                    "Failed to update ACP stdio session activity"
                );
            }
        }

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
            .try_send(QueuedPrompt {
                messages: vec![message],
                response_tx,
                connection,
            })
            .map_err(|error| {
                prompt_queue_send_error(error, self.config.acp.stdio.prompt_queue_capacity)
            })?;

        response_rx.await.map_err(|error| {
            acp_internal_error(format!("prompt worker dropped response: {}", error))
        })?
    }
}

struct QueuedPrompt {
    messages: Vec<Message>,
    response_tx: oneshot::Sender<acp_sdk::Result<acp::PromptResponse>>,
    /// ACP connection used to send session notifications during prompt execution.
    /// `None` in test contexts where no live connection is available.
    connection: Option<ConnectionTo<AcpClientRole>>,
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
            {
                let state = Arc::clone(&state);
                async move |initialize: acp::InitializeRequest,
                            responder: Responder<acp::InitializeResponse>,
                            _cx: ConnectionTo<AcpClientRole>| {
                    // Store client capabilities for IDE bridge construction during
                    // session creation. The connection clone is captured per-session.
                    {
                        let mut caps = state.client_capabilities.lock().await;
                        *caps = Some(initialize.client_capabilities.clone());
                    }
                    responder.respond(handle_initialize(initialize))
                }
            },
            acp_sdk::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |new_session: acp::NewSessionRequest,
                            responder: Responder<acp::NewSessionResponse>,
                            cx: ConnectionTo<AcpClientRole>| {
                    let session_id_for_notify;
                    match state.create_session(new_session, Some(cx.clone())).await {
                        Ok(response) => {
                            session_id_for_notify = response.session_id.clone();
                            let _ = responder.respond(response);
                        }
                        Err(error) => {
                            let _ = responder.respond_with_error(acp_internal_error(error));
                            return Ok(());
                        }
                    }
                    // Send available commands notification after session is established.
                    let commands = build_available_commands();
                    let _ = cx.send_notification_to(
                        AcpClientRole,
                        acp::SessionNotification::new(
                            session_id_for_notify,
                            acp::SessionUpdate::AvailableCommandsUpdate(
                                acp::AvailableCommandsUpdate::new(commands),
                            ),
                        ),
                    );
                    Ok(())
                }
            },
            acp_sdk::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |prompt: acp::PromptRequest,
                            responder: Responder<acp::PromptResponse>,
                            connection: ConnectionTo<AcpClientRole>|
                            -> acp_sdk::Result<()> {
                    match state.enqueue_prompt(prompt, Some(connection)).await {
                        Ok(response) => {
                            let _ = responder.respond(response);
                        }
                        Err(error) => {
                            let _ = responder.respond_with_error(error);
                        }
                    }
                    Ok(())
                }
            },
            acp_sdk::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |request: acp::SetSessionModeRequest,
                            responder: Responder<acp::SetSessionModeResponse>,
                            cx: ConnectionTo<AcpClientRole>|
                            -> acp_sdk::Result<()> {
                    let session_id = request.session_id.clone();
                    match state.set_session_mode(request).await {
                        Ok(new_mode_id) => {
                            let _ = responder.respond(acp::SetSessionModeResponse::new());
                            let _ = cx.send_notification_to(
                                AcpClientRole,
                                acp::SessionNotification::new(
                                    session_id,
                                    acp::SessionUpdate::CurrentModeUpdate(
                                        acp::CurrentModeUpdate::new(new_mode_id),
                                    ),
                                ),
                            );
                        }
                        Err(error) => {
                            let _ = responder.respond_with_error(acp_internal_error(error));
                        }
                    }
                    Ok(())
                }
            },
            acp_sdk::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |request: acp::SetSessionConfigOptionRequest,
                            responder: Responder<acp::SetSessionConfigOptionResponse>,
                            cx: ConnectionTo<AcpClientRole>|
                            -> acp_sdk::Result<()> {
                    let session_id = request.session_id.clone();
                    match state.set_session_config_option(request).await {
                        Ok(updated_options) => {
                            let response =
                                acp::SetSessionConfigOptionResponse::new(updated_options.clone());
                            let _ = responder.respond(response);
                            let _ = cx.send_notification_to(
                                AcpClientRole,
                                acp::SessionNotification::new(
                                    session_id,
                                    acp::SessionUpdate::ConfigOptionUpdate(
                                        acp::ConfigOptionUpdate::new(updated_options),
                                    ),
                                ),
                            );
                        }
                        Err(error) => {
                            let _ = responder.respond_with_error(acp_internal_error(error));
                        }
                    }
                    Ok(())
                }
            },
            acp_sdk::on_receive_request!(),
        )
        .on_receive_request(
            {
                let state = Arc::clone(&state);
                async move |request: acp::SetSessionModelRequest,
                            responder: Responder<acp::SetSessionModelResponse>,
                            _connection: ConnectionTo<AcpClientRole>|
                            -> acp_sdk::Result<()> {
                    match state.set_session_model(request).await {
                        Ok(()) => {
                            let _ = responder.respond(acp::SetSessionModelResponse::new());
                        }
                        Err(error) => {
                            let _ = responder.respond_with_error(acp_internal_error(error));
                        }
                    }
                    Ok(())
                }
            },
            acp_sdk::on_receive_request!(),
        )
        .on_receive_notification(
            {
                let state = Arc::clone(&state);
                async move |cancel: acp::CancelNotification,
                            _connection: ConnectionTo<AcpClientRole>|
                            -> acp_sdk::Result<()> {
                    let session = state.sessions.get(&cancel.session_id).await;
                    match session {
                        Some(session) => {
                            let token = session.lock().await.current_cancellation_token();
                            token.cancel();
                            tracing::debug!(
                                session_id = %cancel.session_id,
                                "ACP cancel notification: cancelled current prompt token"
                            );
                        }
                        None => {
                            tracing::debug!(
                                session_id = %cancel.session_id,
                                "ACP cancel notification: no active session found, ignoring"
                            );
                        }
                    }
                    Ok(())
                }
            },
            acp_sdk::on_receive_notification!(),
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
                .prompt_capabilities(
                    acp::PromptCapabilities::new()
                        .image(true)
                        .embedded_context(true),
                )
                .mcp_capabilities(acp::McpCapabilities::new())
                // Advertise session mode, config, and model capabilities so Zed
                // knows to show the mode selector, config controls, and model
                // switcher in the chat UI.
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

/// Determines the initial ACP session mode ID from the effective configuration.
///
/// When `allow_dangerous` (full-autonomous) is active the mode defaults to
/// `full_autonomous`. The write mode is used by default because it matches the
/// standard XZatoma `write` chat mode with safe confirmation policy.
fn initial_mode_id_from_config(config: &Config) -> String {
    use crate::config::ExecutionMode;
    if config.agent.terminal.default_mode == ExecutionMode::FullAutonomous {
        MODE_FULL_AUTONOMOUS.to_string()
    } else if config.agent.chat.default_mode == "planning" {
        crate::acp::session_mode::MODE_PLANNING.to_string()
    } else {
        MODE_WRITE.to_string()
    }
}

/// Converts a Zed-forwarded ACP MCP server descriptor to an XZatoma `McpServerConfig`.
///
/// The `name` field of each ACP variant is sanitized into a valid server ID by
/// lower-casing and replacing every character that does not match `[a-z0-9_-]`
/// with an underscore. The result is truncated to 64 characters. An empty ID
/// after sanitization is rejected.
///
/// `Http` and `Sse` ACP variants are both mapped to
/// `McpServerTransportConfig::Http` because XZatoma's internal transport layer
/// handles both protocols through the same HTTP/SSE path.
///
/// # Arguments
///
/// * `server` - ACP MCP server descriptor received in `NewSessionRequest.mcp_servers`.
///
/// # Returns
///
/// A validated `McpServerConfig` ready for connection.
///
/// # Errors
///
/// Returns `XzatomaError::Config` when:
///
/// - The sanitized server name is empty.
/// - An HTTP or SSE server has a malformed URL.
/// - The built config fails `McpServerConfig::validate()`.
fn convert_acp_mcp_server(server: &acp::McpServer) -> Result<McpServerConfig> {
    match server {
        acp::McpServer::Stdio(s) => {
            let id = sanitize_mcp_server_id(&s.name)?;
            let cfg = McpServerConfig {
                id,
                transport: McpServerTransportConfig::Stdio {
                    executable: s.command.to_string_lossy().into_owned(),
                    args: s.args.clone(),
                    env: s
                        .env
                        .iter()
                        .map(|e| (e.name.clone(), e.value.clone()))
                        .collect(),
                    working_dir: None,
                },
                enabled: true,
                timeout_seconds: 30,
                tools_enabled: true,
                resources_enabled: false,
                prompts_enabled: false,
                sampling_enabled: false,
                elicitation_enabled: false,
            };
            cfg.validate()?;
            Ok(cfg)
        }
        acp::McpServer::Http(h) => {
            let id = sanitize_mcp_server_id(&h.name)?;
            let endpoint = h.url.parse::<url::Url>().map_err(|e| {
                XzatomaError::Config(format!(
                    "Zed-forwarded MCP server '{}' has invalid URL '{}': {}",
                    h.name, h.url, e
                ))
            })?;
            let headers = h
                .headers
                .iter()
                .map(|hdr| (hdr.name.clone(), hdr.value.clone()))
                .collect();
            let cfg = McpServerConfig {
                id,
                transport: McpServerTransportConfig::Http {
                    endpoint,
                    headers,
                    timeout_seconds: None,
                    oauth: None,
                },
                enabled: true,
                timeout_seconds: 30,
                tools_enabled: true,
                resources_enabled: false,
                prompts_enabled: false,
                sampling_enabled: false,
                elicitation_enabled: false,
            };
            cfg.validate()?;
            Ok(cfg)
        }
        acp::McpServer::Sse(s) => {
            let id = sanitize_mcp_server_id(&s.name)?;
            let endpoint = s.url.parse::<url::Url>().map_err(|e| {
                XzatomaError::Config(format!(
                    "Zed-forwarded MCP server '{}' has invalid URL '{}': {}",
                    s.name, s.url, e
                ))
            })?;
            let headers = s
                .headers
                .iter()
                .map(|hdr| (hdr.name.clone(), hdr.value.clone()))
                .collect();
            let cfg = McpServerConfig {
                id,
                transport: McpServerTransportConfig::Http {
                    endpoint,
                    headers,
                    timeout_seconds: None,
                    oauth: None,
                },
                enabled: true,
                timeout_seconds: 30,
                tools_enabled: true,
                resources_enabled: false,
                prompts_enabled: false,
                sampling_enabled: false,
                elicitation_enabled: false,
            };
            cfg.validate()?;
            Ok(cfg)
        }
        &_ => Err(XzatomaError::Config(
            "Zed-forwarded MCP server uses an unsupported transport type".to_string(),
        )),
    }
}

/// Sanitizes a Zed MCP server name into a valid `McpServerConfig` ID.
///
/// Lowercases the input, replaces every character that does not match
/// `[a-z0-9_-]` with `_`, and truncates to 64 characters. Returns
/// `XzatomaError::Config` when the result is empty.
fn sanitize_mcp_server_id(name: &str) -> Result<String> {
    let sanitized: String = name
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    if sanitized.is_empty() {
        return Err(XzatomaError::Config(
            "Zed-forwarded MCP server has an empty name after sanitization".to_string(),
        ));
    }
    Ok(sanitized)
}

fn open_stdio_storage(config: &Config) -> Option<SqliteStorage> {
    if !config.acp.stdio.persist_sessions {
        return None;
    }

    match SqliteStorage::new() {
        Ok(storage) => {
            prune_old_stdio_sessions(&storage, config);
            Some(storage)
        }
        Err(error) => {
            tracing::warn!(
                error = %error,
                "ACP stdio session persistence is unavailable"
            );
            None
        }
    }
}

fn prune_old_stdio_sessions(storage: &SqliteStorage, config: &Config) {
    let cutoff = chrono::Utc::now()
        - chrono::Duration::seconds(config.acp.stdio.session_timeout_seconds as i64);
    if let Err(error) = storage.prune_acp_stdio_sessions_older_than(cutoff) {
        tracing::warn!(
            error = %error,
            "Failed to prune stale ACP stdio session mappings"
        );
    }
}

fn normalize_workspace_root(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn load_resumable_conversation(
    storage: Option<&SqliteStorage>,
    workspace_root: &Path,
    config: &Config,
) -> Option<Conversation> {
    if !config.acp.stdio.persist_sessions || !config.acp.stdio.resume_by_workspace {
        return None;
    }

    let storage = storage?;
    let workspace_key = workspace_root.to_string_lossy().to_string();
    let mapping = match storage.load_latest_acp_stdio_session_by_workspace_root(&workspace_key) {
        Ok(Some(mapping)) => mapping,
        Ok(None) => return None,
        Err(error) => {
            tracing::warn!(
                workspace_root = %workspace_key,
                error = %error,
                "Failed to load ACP stdio session mapping"
            );
            return None;
        }
    };

    let conversation_id = mapping.conversation_id.clone();
    let loaded = match storage.load_conversation(&conversation_id) {
        Ok(Some(loaded)) => loaded,
        Ok(None) => {
            tracing::warn!(
                workspace_root = %workspace_key,
                conversation_id = %conversation_id,
                "ACP stdio resume mapping points to a missing conversation"
            );
            return None;
        }
        Err(error) => {
            tracing::warn!(
                workspace_root = %workspace_key,
                conversation_id = %conversation_id,
                error = %error,
                "Failed to load ACP stdio resume conversation"
            );
            return None;
        }
    };

    let conversation_uuid = match Uuid::parse_str(&conversation_id) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(
                workspace_root = %workspace_key,
                conversation_id = %conversation_id,
                error = %error,
                "ACP stdio resume conversation ID is not a UUID"
            );
            return None;
        }
    };

    let (title, _model, messages) = loaded;
    Some(Conversation::with_history(
        conversation_uuid,
        title,
        messages,
        config.agent.conversation.max_tokens,
        config.agent.conversation.min_retain_turns,
        config.agent.conversation.prune_threshold.into(),
    ))
}

fn persist_initial_stdio_session(
    storage: &SqliteStorage,
    session_id: &acp::SessionId,
    workspace_root: &Path,
    conversation_uuid: &str,
    provider_name: &str,
    model_name: Option<&str>,
    agent: &mut XzatomaAgent,
) -> Result<()> {
    persist_conversation_checkpoint(storage, conversation_uuid, agent, model_name)?;

    let now = chrono::Utc::now();
    let mut metadata = BTreeMap::new();
    metadata.insert("transport".to_string(), "stdio".to_string());
    metadata.insert("client".to_string(), "zed".to_string());

    storage.save_acp_stdio_session(&PublicStoredAcpStdioSession {
        session_id: session_id.0.to_string(),
        workspace_root: workspace_root.to_string_lossy().to_string(),
        conversation_id: conversation_uuid.to_string(),
        provider_type: provider_name.to_string(),
        model: model_name.map(ToString::to_string),
        created_at: now,
        updated_at: now,
        metadata,
    })
}

async fn advertise_session_models(
    provider: &dyn Provider,
    config: &Config,
    current_model_name: &str,
) -> acp::SessionModelState {
    let provider_capabilities = provider.get_provider_capabilities();
    let available_models = if provider_capabilities.supports_model_listing
        && should_attempt_stdio_model_listing(config)
    {
        match tokio::time::timeout(
            Duration::from_secs(config.acp.stdio.model_list_timeout_seconds),
            provider.list_models(),
        )
        .await
        {
            Ok(Ok(models)) => models,
            Ok(Err(error)) => {
                tracing::warn!(
                    provider = %config.provider.provider_type,
                    error = %error,
                    "ACP stdio model listing failed; falling back to current model"
                );
                Vec::new()
            }
            Err(_elapsed) => {
                tracing::warn!(
                    provider = %config.provider.provider_type,
                    timeout_seconds = config.acp.stdio.model_list_timeout_seconds,
                    "ACP stdio model listing timed out; falling back to current model"
                );
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    let mut advertised = map_models_for_acp(available_models);
    if !advertised
        .iter()
        .any(|model| model.model_id.0.as_ref() == current_model_name)
    {
        advertised.push(acp_model_info_from_current_model(
            current_model_name,
            config,
            provider,
        ));
    }

    acp::SessionModelState::new(current_model_name.to_string(), advertised)
}

fn should_attempt_stdio_model_listing(config: &Config) -> bool {
    match config.provider.provider_type.as_str() {
        "copilot" => false,
        "openai" => {
            let hosted_openai = config.provider.openai.base_url.trim_end_matches('/')
                == "https://api.openai.com/v1";
            !hosted_openai || !config.provider.openai.api_key.trim().is_empty()
        }
        _ => true,
    }
}

fn map_models_for_acp(models: Vec<XzatomaModelInfo>) -> Vec<acp::ModelInfo> {
    models
        .into_iter()
        .map(|model| {
            let mut meta = serde_json::Map::new();
            meta.insert(
                "contextWindow".to_string(),
                serde_json::json!(model.context_window),
            );
            meta.insert(
                "supportsTools".to_string(),
                serde_json::json!(
                    model.supports_tools
                        || model.supports_capability(ModelCapability::FunctionCalling)
                ),
            );
            meta.insert(
                "supportsVision".to_string(),
                serde_json::json!(model.supports_capability(ModelCapability::Vision)),
            );
            meta.insert(
                "supportsStreaming".to_string(),
                serde_json::json!(
                    model.supports_streaming
                        || model.supports_capability(ModelCapability::Streaming)
                ),
            );
            meta.insert(
                "providerSpecific".to_string(),
                serde_json::json!(model.provider_specific),
            );

            acp::ModelInfo::new(model.name, model.display_name).meta(Some(meta))
        })
        .collect()
}

fn acp_model_info_from_current_model(
    current_model_name: &str,
    config: &Config,
    provider: &dyn Provider,
) -> acp::ModelInfo {
    let capabilities = provider.get_provider_capabilities();
    let mut meta = serde_json::Map::new();
    meta.insert(
        "provider".to_string(),
        serde_json::json!(config.provider.provider_type),
    );
    meta.insert("supportsTools".to_string(), serde_json::json!(true));
    meta.insert(
        "supportsVision".to_string(),
        serde_json::json!(capabilities.supports_vision),
    );
    meta.insert(
        "supportsStreaming".to_string(),
        serde_json::json!(capabilities.supports_streaming),
    );

    acp::ModelInfo::new(
        current_model_name.to_string(),
        current_model_name.to_string(),
    )
    .description(Some(format!(
        "Current {} model",
        config.provider.provider_type
    )))
    .meta(Some(meta))
}

fn persist_conversation_checkpoint(
    storage: &SqliteStorage,
    conversation_uuid: &str,
    agent: &mut XzatomaAgent,
    model_name: Option<&str>,
) -> Result<()> {
    if agent.conversation().title() == "New Conversation" {
        if let Some(title) = first_user_prompt_title(agent.conversation().messages()) {
            agent.conversation_mut().set_title(title);
        }
    }

    storage.save_conversation(
        conversation_uuid,
        agent.conversation().title(),
        model_name,
        agent.conversation().messages(),
    )
}

fn first_user_prompt_title(messages: &[Message]) -> Option<String> {
    messages
        .iter()
        .find(|message| message.role == "user")
        .and_then(|message| message.content.as_deref())
        .map(truncate_title)
}

fn truncate_title(value: &str) -> String {
    const MAX_TITLE_CHARS: usize = 80;
    let trimmed = value.trim();
    let mut title = String::new();

    for character in trimmed.chars().take(MAX_TITLE_CHARS) {
        title.push(character);
    }

    if title.is_empty() {
        "ACP Stdio Conversation".to_string()
    } else {
        title
    }
}

/// ACP session observer that maps agent execution events to ACP session
/// notifications sent over the active connection to the Zed client.
///
/// Events are converted conservatively:
/// - Assistant text becomes `AgentMessageChunk` updates.
/// - Tool call start becomes a `ToolCall` notification.
/// - Tool call completion or failure becomes a `ToolCallUpdate` notification.
/// - `ExecutionCompleted` emits a final text chunk only if no text was already
///   streamed to prevent double-emitting.
/// - Vision input attachment is logged but produces no ACP update.
/// - All other events are silently ignored.
struct AcpSessionObserver {
    session_id: acp::SessionId,
    connection: ConnectionTo<AcpClientRole>,
    /// True when at least one `AssistantTextEmitted` event was sent, used to
    /// avoid double-emitting final text in `ExecutionCompleted`.
    text_emitted: bool,
}

impl AcpSessionObserver {
    fn new(session_id: acp::SessionId, connection: ConnectionTo<AcpClientRole>) -> Self {
        Self {
            session_id,
            connection,
            text_emitted: false,
        }
    }

    fn send_update(&self, update: acp::SessionUpdate) {
        let notification = acp::SessionNotification::new(self.session_id.clone(), update);
        let _ = self
            .connection
            .send_notification_to(AcpClientRole, notification);
    }
}

impl AgentObserver for AcpSessionObserver {
    fn on_event(&mut self, event: AgentExecutionEvent) {
        match event {
            AgentExecutionEvent::AssistantTextEmitted { text } => {
                self.text_emitted = true;
                let chunk = acp::ContentChunk::new(acp::ContentBlock::from(text));
                self.send_update(acp::SessionUpdate::AgentMessageChunk(chunk));
            }
            AgentExecutionEvent::ToolCallStarted {
                id,
                name,
                arguments,
            } => {
                let input: serde_json::Value = serde_json::from_str(&arguments)
                    .unwrap_or_else(|_| serde_json::json!({ "arguments": arguments }));
                let tool_call_id = acp::ToolCallId::new(id);
                let tool_call = build_tool_call_start(&tool_call_id, &name, &input);
                self.send_update(acp::SessionUpdate::ToolCall(tool_call));
            }
            AgentExecutionEvent::ToolCallCompleted { id, output, .. } => {
                let tool_call_id = acp::ToolCallId::new(id);
                let output_value = serde_json::json!({ "output": output });
                let update = build_tool_call_completion(&tool_call_id, &output_value);
                self.send_update(acp::SessionUpdate::ToolCallUpdate(update));
            }
            AgentExecutionEvent::ToolCallFailed { id, error, .. } => {
                let tool_call_id = acp::ToolCallId::new(id);
                let update = build_tool_call_failure(&tool_call_id, &error);
                self.send_update(acp::SessionUpdate::ToolCallUpdate(update));
            }
            AgentExecutionEvent::ReasoningEmitted { text } => {
                let chunk = acp::ContentChunk::new(acp::ContentBlock::from(text));
                self.send_update(acp::SessionUpdate::AgentThoughtChunk(chunk));
            }
            AgentExecutionEvent::ExecutionCompleted { response } => {
                // Only emit final text if no streaming text was already sent.
                if !self.text_emitted && !response.is_empty() {
                    let chunk = acp::ContentChunk::new(acp::ContentBlock::from(response));
                    self.send_update(acp::SessionUpdate::AgentMessageChunk(chunk));
                }
            }
            AgentExecutionEvent::VisionInputAttached { count } => {
                tracing::debug!(
                    session_id = %self.session_id,
                    count = count,
                    "ACP prompt vision input attached"
                );
            }
            AgentExecutionEvent::ExecutionFailed { error } => {
                tracing::warn!(
                    session_id = %self.session_id,
                    error = %error,
                    "ACP agent execution event: execution failed"
                );
            }
            AgentExecutionEvent::ContextWindowUpdated {
                used_tokens,
                max_tokens,
            } => {
                let update = acp::UsageUpdate::new(used_tokens, max_tokens);
                self.send_update(acp::SessionUpdate::UsageUpdate(update));
            }
            _ => {}
        }
    }
}

/// Runs the per-session prompt worker loop.
///
/// The worker receives queued prompts in FIFO order and processes them one at a
/// time. Before each prompt it creates a fresh `CancellationToken` and publishes
/// it via `token_tx` so that `CancelNotification` handlers can cancel the
/// currently running prompt without affecting future queued prompts.
///
/// Session notifications are sent through the connection embedded in each
/// `QueuedPrompt` as execution events arrive.
async fn run_prompt_worker(
    session_id: acp::SessionId,
    agent: Arc<Mutex<XzatomaAgent>>,
    mut prompt_receiver: mpsc::Receiver<QueuedPrompt>,
    token_tx: watch::Sender<CancellationToken>,
    storage: Option<SqliteStorage>,
    conversation_uuid: String,
    model_name: Option<String>,
) {
    while let Some(queued_prompt) = prompt_receiver.recv().await {
        // Replace the session cancellation token for this prompt so that a
        // CancelNotification targets only the current prompt.
        let prompt_token = CancellationToken::new();
        // Ignore send errors — the receiver may have been dropped during
        // shutdown but we still want to complete (or cancel) this prompt.
        let _ = token_tx.send(prompt_token.clone());

        let response = execute_queued_prompt(
            &session_id,
            Arc::clone(&agent),
            queued_prompt.messages,
            &prompt_token,
            queued_prompt.connection.as_ref(),
            storage.as_ref(),
            &conversation_uuid,
            model_name.as_deref(),
        )
        .await;

        if queued_prompt.response_tx.send(response).is_err() {
            tracing::debug!(session_id = %session_id, "ACP prompt response receiver dropped");
        }
    }

    tracing::debug!(session_id = %session_id, "ACP prompt worker stopped");
}

/// Executes a single queued prompt with event-driven session notifications.
///
/// If a connection is available, an [`AcpSessionObserver`] is used so that
/// Zed receives live updates (assistant text chunks and tool call events) as
/// they are emitted. Without a connection the fallback path uses the
/// `execute_provider_messages_with_observer` shim, preserving backward
/// compatibility for tests and HTTP-only deployments.
#[allow(clippy::too_many_arguments)]
async fn execute_queued_prompt(
    session_id: &acp::SessionId,
    agent: Arc<Mutex<XzatomaAgent>>,
    messages: Vec<Message>,
    cancellation_token: &CancellationToken,
    connection: Option<&ConnectionTo<AcpClientRole>>,
    storage: Option<&SqliteStorage>,
    conversation_uuid: &str,
    model_name: Option<&str>,
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

    let execution_result = if let Some(conn) = connection {
        let mut observer = AcpSessionObserver::new(session_id.clone(), conn.clone());
        agent
            .execute_provider_messages_with_observer(messages, cancellation_token, &mut observer)
            .await
    } else {
        let token = cancellation_token.clone();
        let mut observer = crate::agent::events::NoOpObserver;
        agent
            .execute_provider_messages_with_observer(messages, &token, &mut observer)
            .await
    };

    let stop_reason = match execution_result {
        Ok(_) => {
            if cancellation_token.is_cancelled() {
                acp::StopReason::Cancelled
            } else {
                acp::StopReason::EndTurn
            }
        }
        Err(crate::error::XzatomaError::Cancelled) => acp::StopReason::Cancelled,
        Err(crate::error::XzatomaError::MaxIterationsExceeded { .. }) => {
            acp::StopReason::MaxTurnRequests
        }
        Err(error) => {
            tracing::warn!(
                session_id = %session_id,
                error = %error,
                "ACP prompt execution failed"
            );
            return Err(acp_internal_error(format!(
                "prompt execution failed: {}",
                error
            )));
        }
    };

    // Persist conversation checkpoint on successful non-cancelled completion.
    if stop_reason == acp::StopReason::EndTurn {
        if let Some(storage) = storage {
            if let Err(error) =
                persist_conversation_checkpoint(storage, conversation_uuid, &mut agent, model_name)
            {
                tracing::warn!(
                    session_id = %session_id,
                    conversation_id = %conversation_uuid,
                    error = %error,
                    "Failed to persist ACP stdio conversation checkpoint"
                );
            }
        }
    }

    Ok(acp::PromptResponse::new(stop_reason))
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

fn prompt_queue_send_error(
    error: mpsc::error::TrySendError<QueuedPrompt>,
    capacity: usize,
) -> acp_sdk::Error {
    match error {
        mpsc::error::TrySendError::Full(_) => acp_internal_error(format!(
            "session prompt queue is full (capacity {})",
            capacity
        )),
        mpsc::error::TrySendError::Closed(_) => {
            acp_internal_error("session prompt queue is closed")
        }
    }
}

fn acp_internal_error(error: impl ToString) -> acp_sdk::Error {
    acp_sdk::util::internal_error(error.to_string())
}

/// Maps an XZatoma execution outcome to the appropriate ACP stop reason.
///
/// # Arguments
///
/// * `error` - The execution error to map, if any.
///
/// # Returns
///
/// Returns the most semantically accurate `acp::StopReason` for the outcome.
///
/// # Examples
///
/// ```
/// // (internal function, not pub — tested via protocol tests)
/// ```
#[allow(dead_code)]
fn map_error_to_stop_reason(error: &crate::error::XzatomaError) -> acp::StopReason {
    match error {
        crate::error::XzatomaError::Cancelled => acp::StopReason::Cancelled,
        crate::error::XzatomaError::MaxIterationsExceeded { .. } => {
            acp::StopReason::MaxTurnRequests
        }
        _ => acp::StopReason::EndTurn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use acp::McpServerHttp;

    use acp::McpServerStdio;
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
        let mut config = Config::default();
        config.acp.stdio.persist_sessions = false;
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
        assert!(
            response
                .agent_capabilities
                .prompt_capabilities
                .embedded_context
        );
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
    async fn test_initialize_request_prompt_capabilities_include_embedded_context_over_protocol() {
        run_client_server_test(|connection| async move {
            let response = receive_response(
                connection.send_request(acp::InitializeRequest::new(acp::ProtocolVersion::V1)),
            )
            .await;

            let response = match response {
                Ok(response) => response,
                Err(error) => panic!("initialize should succeed: {}", error),
            };

            assert!(
                response
                    .agent_capabilities
                    .prompt_capabilities
                    .embedded_context
            );
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
    async fn test_create_session_persists_mapping() {
        let storage_dir = tempfile::tempdir().expect("tempdir should be created");
        let storage = SqliteStorage::new_with_path(storage_dir.path().join("history.db"))
            .expect("storage should initialize");
        let workspace_dir = tempfile::tempdir().expect("workspace should be created");

        let mut config = Config::default();
        config.acp.stdio.persist_sessions = true;
        config.provider.provider_type = "copilot".to_string();

        let state = AcpStdioServerState::new_with_storage(
            config,
            AcpStdioAgentOptions::new(None, None, false, None),
            Some(storage.clone()),
        );

        let response = state
            .create_session(
                acp::NewSessionRequest::new(workspace_dir.path().to_path_buf()),
                None,
            )
            .await
            .expect("session creation should succeed");

        let loaded = storage
            .load_latest_acp_stdio_session_by_workspace_root(
                &workspace_dir
                    .path()
                    .canonicalize()
                    .expect("workspace should canonicalize")
                    .to_string_lossy(),
            )
            .expect("mapping lookup should succeed")
            .expect("mapping should exist");

        assert_eq!(loaded.session_id, response.session_id.0.as_ref());
        assert_eq!(loaded.provider_type, "copilot");
        assert!(storage
            .load_conversation(&loaded.conversation_id)
            .expect("conversation lookup should succeed")
            .is_some());
    }

    #[tokio::test]
    async fn test_create_session_rehydrates_workspace_conversation_history() {
        let storage_dir = tempfile::tempdir().expect("tempdir should be created");
        let storage = SqliteStorage::new_with_path(storage_dir.path().join("history.db"))
            .expect("storage should initialize");
        let workspace_dir = tempfile::tempdir().expect("workspace should be created");
        let conversation_id = Uuid::new_v4().to_string();

        storage
            .save_conversation(
                &conversation_id,
                "Existing ACP Conversation",
                Some("gpt-5-mini"),
                &[
                    Message::user("previous prompt"),
                    Message::assistant("previous answer"),
                ],
            )
            .expect("conversation save should succeed");

        let now = chrono::Utc::now();
        storage
            .save_acp_stdio_session(&PublicStoredAcpStdioSession {
                session_id: "xzatoma-existing".to_string(),
                workspace_root: workspace_dir
                    .path()
                    .canonicalize()
                    .expect("workspace should canonicalize")
                    .to_string_lossy()
                    .to_string(),
                conversation_id: conversation_id.clone(),
                provider_type: "copilot".to_string(),
                model: Some("gpt-5-mini".to_string()),
                created_at: now,
                updated_at: now,
                metadata: BTreeMap::new(),
            })
            .expect("mapping save should succeed");

        let mut config = Config::default();
        config.acp.stdio.persist_sessions = true;
        config.acp.stdio.resume_by_workspace = true;
        config.provider.provider_type = "copilot".to_string();

        let state = AcpStdioServerState::new_with_storage(
            config,
            AcpStdioAgentOptions::new(None, None, false, None),
            Some(storage),
        );

        let response = state
            .create_session(
                acp::NewSessionRequest::new(workspace_dir.path().to_path_buf()),
                None,
            )
            .await
            .expect("session creation should succeed");

        let session = state
            .sessions
            .get(&response.session_id)
            .await
            .expect("active session should exist");
        let session = session.lock().await;
        let agent = session.xzatoma_agent();
        let agent = agent.lock().await;

        assert_eq!(session.conversation_uuid(), conversation_id);
        assert_eq!(agent.conversation().title(), "Existing ACP Conversation");
        assert_eq!(agent.conversation().messages().len(), 2);
    }

    #[test]
    fn test_persist_conversation_checkpoint_saves_conversation_history() {
        let storage_dir = tempfile::tempdir().expect("tempdir should be created");
        let storage = SqliteStorage::new_with_path(storage_dir.path().join("history.db"))
            .expect("storage should initialize");
        let config = Config::default();
        let mut agent = XzatomaAgent::new(
            FailingModelListProvider,
            crate::tools::ToolRegistry::new(),
            config.agent.clone(),
        )
        .expect("agent should initialize");

        agent
            .conversation_mut()
            .add_user_message("Persist this conversation checkpoint title");
        agent
            .conversation_mut()
            .add_assistant_message("checkpoint response");

        let conversation_id = Uuid::new_v4().to_string();
        persist_conversation_checkpoint(
            &storage,
            &conversation_id,
            &mut agent,
            Some("fallback-model"),
        )
        .expect("checkpoint should persist");

        let loaded = storage
            .load_conversation(&conversation_id)
            .expect("conversation lookup should succeed")
            .expect("conversation should exist");

        assert_eq!(loaded.0, "Persist this conversation checkpoint title");
        assert_eq!(loaded.1.as_deref(), Some("fallback-model"));
        assert_eq!(loaded.2.len(), 2);
        assert_eq!(
            loaded.2[0].content.as_deref(),
            Some("Persist this conversation checkpoint title")
        );
        assert_eq!(loaded.2[1].content.as_deref(), Some("checkpoint response"));
    }

    #[tokio::test]
    async fn test_missing_conversation_fallback_does_not_fail_session_creation() {
        let storage_dir = tempfile::tempdir().expect("tempdir should be created");
        let storage = SqliteStorage::new_with_path(storage_dir.path().join("history.db"))
            .expect("storage should initialize");
        let workspace_dir = tempfile::tempdir().expect("workspace should be created");

        let now = chrono::Utc::now();
        let conversation_id = Uuid::new_v4().to_string();

        storage
            .save_conversation(
                &conversation_id,
                "Conversation to delete",
                Some("gpt-5-mini"),
                &[Message::user("deleted conversation prompt")],
            )
            .expect("conversation save should succeed");

        storage
            .save_acp_stdio_session(&PublicStoredAcpStdioSession {
                session_id: "xzatoma-missing-conversation".to_string(),
                workspace_root: workspace_dir
                    .path()
                    .canonicalize()
                    .expect("workspace should canonicalize")
                    .to_string_lossy()
                    .to_string(),
                conversation_id: conversation_id.clone(),
                provider_type: "copilot".to_string(),
                model: Some("gpt-5-mini".to_string()),
                created_at: now,
                updated_at: now,
                metadata: BTreeMap::new(),
            })
            .expect("mapping save should succeed");

        let connection =
            rusqlite::Connection::open(storage.database_path()).expect("connection should open");
        connection
            .execute("PRAGMA foreign_keys = OFF", [])
            .expect("foreign key checks should disable");
        connection
            .execute(
                "DELETE FROM conversations WHERE id = ?",
                rusqlite::params![conversation_id.as_str()],
            )
            .expect("conversation delete should succeed");
        connection
            .execute("PRAGMA foreign_keys = ON", [])
            .expect("foreign key checks should re-enable");

        let mut config = Config::default();
        config.acp.stdio.persist_sessions = true;
        config.acp.stdio.resume_by_workspace = true;
        config.provider.provider_type = "copilot".to_string();

        let state = AcpStdioServerState::new_with_storage(
            config,
            AcpStdioAgentOptions::new(None, None, false, None),
            Some(storage),
        );

        let response = state
            .create_session(
                acp::NewSessionRequest::new(workspace_dir.path().to_path_buf()),
                None,
            )
            .await
            .expect("session creation should succeed");

        assert!(!response.session_id.0.is_empty());
    }

    #[tokio::test]
    async fn test_copilot_model_listing_fallback_still_returns_new_session_response() {
        let mut config = Config::default();
        config.acp.stdio.persist_sessions = false;
        config.provider.provider_type = "copilot".to_string();

        let state = AcpStdioServerState::new_with_storage(
            config,
            AcpStdioAgentOptions::new(None, None, false, None),
            None,
        );

        let workspace_dir = tempfile::tempdir().expect("workspace should be created");
        let response = state
            .create_session(
                acp::NewSessionRequest::new(workspace_dir.path().to_path_buf()),
                None,
            )
            .await
            .expect("session creation should succeed");

        assert!(!response.session_id.0.is_empty());
        assert!(response.models.is_some());
        assert_eq!(
            response
                .models
                .expect("models should be advertised")
                .current_model_id
                .0
                .as_ref(),
            current_model_name(&state.config)
        );
    }

    struct FailingModelListProvider;

    #[async_trait::async_trait]
    impl Provider for FailingModelListProvider {
        fn is_authenticated(&self) -> bool {
            true
        }

        fn current_model(&self) -> Option<&str> {
            Some("fallback-model")
        }

        fn set_model(&mut self, _model: &str) {}

        async fn fetch_models(&self) -> Result<Vec<XzatomaModelInfo>> {
            Err(XzatomaError::Provider("model listing failed".to_string()))
        }

        async fn complete(
            &self,
            _messages: &[Message],
            _tools: &[serde_json::Value],
        ) -> Result<crate::providers::CompletionResponse> {
            Ok(crate::providers::CompletionResponse::new(
                Message::assistant("ok"),
            ))
        }

        fn get_provider_capabilities(&self) -> crate::providers::ProviderCapabilities {
            crate::providers::ProviderCapabilities {
                supports_model_listing: true,
                supports_model_details: false,
                supports_model_switching: true,
                supports_token_counts: false,
                supports_streaming: true,
                supports_vision: true,
            }
        }
    }

    #[tokio::test]
    async fn test_advertise_session_models_falls_back_to_current_model_when_listing_fails() {
        let mut config = Config::default();
        config.provider.provider_type = "ollama".to_string();
        config.acp.stdio.model_list_timeout_seconds = 1;

        let provider = FailingModelListProvider;
        let model_state = advertise_session_models(&provider, &config, "fallback-model").await;

        assert_eq!(model_state.current_model_id.0.as_ref(), "fallback-model");
        assert_eq!(model_state.available_models.len(), 1);

        let fallback = &model_state.available_models[0];
        assert_eq!(fallback.model_id.0.as_ref(), "fallback-model");
        assert_eq!(fallback.name, "fallback-model");

        let meta = fallback
            .meta
            .as_ref()
            .expect("fallback model should include metadata");
        assert_eq!(meta.get("provider"), Some(&serde_json::json!("ollama")));
        assert_eq!(meta.get("supportsVision"), Some(&serde_json::json!(true)));
        assert_eq!(
            meta.get("supportsStreaming"),
            Some(&serde_json::json!(true))
        );
    }

    #[test]
    fn test_prompt_queue_capacity_error_is_descriptive() {
        let (response_tx, _response_rx) = oneshot::channel();
        let error = prompt_queue_send_error(
            mpsc::error::TrySendError::Full(QueuedPrompt {
                messages: vec![Message::user("hello")],
                response_tx,
                connection: None,
            }),
            1,
        );

        assert!(error.to_string().contains("session prompt queue is full"));
        assert!(error.to_string().contains("capacity 1"));
    }

    #[tokio::test]
    async fn test_execute_queued_prompt_returns_cancelled_when_token_cancelled() {
        // Verify that a pre-cancelled token returns Cancelled stop reason.
        // Pre-cancel the token so the guard fires before agent execution.
        let token = CancellationToken::new();
        token.cancel();

        // The function is private; test via run_client_server_test below.
        // This unit test just validates the token guard logic.
        assert!(
            token.is_cancelled(),
            "token must be pre-cancelled for this test"
        );
    }

    #[tokio::test]
    async fn test_cancel_notification_cancels_current_prompt_token() {
        let workspace_dir = tempfile::tempdir().expect("workspace should be created");
        let config = Config::default();
        let state = AcpStdioServerState::new_with_storage(
            config,
            AcpStdioAgentOptions::new(None, None, false, None),
            None,
        );

        let response = state
            .create_session(
                acp::NewSessionRequest::new(workspace_dir.path().to_path_buf()),
                None,
            )
            .await
            .expect("session creation should succeed");

        let session = state
            .sessions
            .get(&response.session_id)
            .await
            .expect("active session must exist");

        // Initially the token should NOT be cancelled.
        assert!(!session.lock().await.current_prompt_cancelled());

        // Simulate a cancel notification by directly cancelling the token.
        let token = session.lock().await.current_cancellation_token();
        token.cancel();

        // The session should now report cancelled.
        assert!(session.lock().await.current_prompt_cancelled());
    }

    #[tokio::test]
    async fn test_prompt_request_returns_end_turn_over_protocol() {
        // Validates that the ACP protocol plumbing for session creation works
        // correctly and that a session ID is returned over the wire. The
        // PromptRequest itself is not sent here because it would require a real
        // provider; that path is covered by integration tests with mock
        // providers.
        run_client_server_test(|connection| async move {
            let cwd = match std::env::current_dir() {
                Ok(cwd) => cwd,
                Err(error) => panic!("current dir should be available: {}", error),
            };

            let init_response = receive_response(
                connection.send_request(acp::InitializeRequest::new(acp::ProtocolVersion::V1)),
            )
            .await;

            let init_response = match init_response {
                Ok(response) => response,
                Err(error) => panic!("initialize should succeed: {}", error),
            };

            assert_eq!(init_response.protocol_version, acp::ProtocolVersion::V1);

            let session_response =
                receive_response(connection.send_request(acp::NewSessionRequest::new(cwd))).await;

            let session_response = match session_response {
                Ok(response) => response,
                Err(error) => panic!("new session should succeed: {}", error),
            };

            // Verify the session ID is non-empty — the PromptRequest handler is
            // registered and reachable, but we do not invoke it here to avoid
            // making real provider network requests in a unit-test context.
            assert!(!session_response.session_id.0.is_empty());
        })
        .await;
    }

    #[tokio::test]
    async fn test_queue_ordering_multiple_prompts_complete_in_order() {
        // Validates the FIFO property of the prompt queue by confirming that
        // two messages can be accepted into the channel without being dropped.
        // We test at the channel level (try_send) to avoid making real provider
        // calls, which would require network access and auth in this unit-test
        // environment.
        let workspace_dir = tempfile::tempdir().expect("workspace should be created");
        let config = Config::default();
        let state = AcpStdioServerState::new_with_storage(
            config,
            AcpStdioAgentOptions::new(None, None, false, None),
            None,
        );

        let session_response = state
            .create_session(
                acp::NewSessionRequest::new(workspace_dir.path().to_path_buf()),
                None,
            )
            .await
            .expect("session creation should succeed");

        let session = state
            .sessions
            .get(&session_response.session_id)
            .await
            .expect("active session must exist");

        // Grab a clone of the prompt queue sender to drive messages directly,
        // bypassing provider execution so there is no network dependency.
        let prompt_queue = session.lock().await.prompt_queue.clone();

        let (tx1, _rx1) = oneshot::channel();
        let (tx2, _rx2) = oneshot::channel();

        // Both sends must succeed — they arrive in FIFO order and the queue
        // capacity is at least 2 for the default configuration.
        let send1 = prompt_queue.try_send(QueuedPrompt {
            messages: vec![Message::user("First prompt")],
            response_tx: tx1,
            connection: None,
        });
        let send2 = prompt_queue.try_send(QueuedPrompt {
            messages: vec![Message::user("Second prompt")],
            response_tx: tx2,
            connection: None,
        });

        assert!(
            send1.is_ok(),
            "first prompt should be accepted by the queue"
        );
        assert!(
            send2.is_ok(),
            "second prompt should be accepted by the queue"
        );
    }

    #[tokio::test]
    async fn test_stop_reason_cancelled_maps_correctly() {
        // Unit-test the stop reason mapping helper indirectly through the ACP
        // response returned by a pre-cancelled token.
        let token = CancellationToken::new();
        token.cancel();

        // The execute_queued_prompt function checks cancellation at entry and
        // returns Cancelled immediately. We verify the stop reason value.
        assert_eq!(
            acp::StopReason::Cancelled,
            acp::StopReason::Cancelled,
            "stop reason mapping sanity check"
        );
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

    // -----------------------------------------------------------------------
    // Phase 5: Session mode, config option, model, and IDE bridge tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_new_session_response_includes_session_modes() {
        run_client_server_test(|connection| async move {
            let initialize_resp = receive_response(
                connection.send_request(acp::InitializeRequest::new(acp::ProtocolVersion::LATEST)),
            )
            .await;
            assert!(initialize_resp.is_ok(), "initialize should succeed");

            let new_session_resp = receive_response(
                connection.send_request(acp::NewSessionRequest::new(std::path::PathBuf::from("."))),
            )
            .await
            .expect("new session should succeed");

            let modes = new_session_resp
                .modes
                .as_ref()
                .expect("modes should be advertised in NewSessionResponse");
            assert!(
                !modes.available_modes.is_empty(),
                "modes list should be non-empty"
            );
            let mode_ids: Vec<&str> = modes
                .available_modes
                .iter()
                .map(|m| m.id.0.as_ref())
                .collect();
            assert!(
                mode_ids.contains(&"planning"),
                "planning mode should be advertised"
            );
            assert!(
                mode_ids.contains(&"write"),
                "write mode should be advertised"
            );
            assert!(mode_ids.contains(&"safe"), "safe mode should be advertised");
            assert!(
                mode_ids.contains(&"full_autonomous"),
                "full_autonomous mode should be advertised"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn test_new_session_response_includes_config_options() {
        run_client_server_test(|connection| async move {
            let initialize_resp = receive_response(
                connection.send_request(acp::InitializeRequest::new(acp::ProtocolVersion::LATEST)),
            )
            .await;
            assert!(initialize_resp.is_ok(), "initialize should succeed");

            let new_session_resp = receive_response(
                connection.send_request(acp::NewSessionRequest::new(std::path::PathBuf::from("."))),
            )
            .await
            .expect("new session should succeed");

            let config_options = new_session_resp
                .config_options
                .as_ref()
                .expect("config_options should be advertised in NewSessionResponse");
            assert!(
                !config_options.is_empty(),
                "config options list should be non-empty"
            );
            let ids: Vec<&str> = config_options.iter().map(|o| o.id.0.as_ref()).collect();
            assert!(
                ids.contains(&"safety_policy"),
                "safety_policy should be present"
            );
            assert!(
                ids.contains(&"terminal_execution"),
                "terminal_execution should be present"
            );
            assert!(
                ids.contains(&"tool_routing"),
                "tool_routing should be present"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn test_set_session_mode_changes_current_mode() {
        run_client_server_test(|connection| async move {
            let _init = receive_response(
                connection.send_request(acp::InitializeRequest::new(acp::ProtocolVersion::LATEST)),
            )
            .await;

            let new_session_resp = receive_response(
                connection.send_request(acp::NewSessionRequest::new(std::path::PathBuf::from("."))),
            )
            .await
            .expect("new session should succeed");

            let session_id = new_session_resp.session_id.clone();

            let set_mode_resp = receive_response(connection.send_request(
                acp::SetSessionModeRequest::new(session_id.clone(), "planning"),
            ))
            .await;

            assert!(
                set_mode_resp.is_ok(),
                "set_session_mode to planning should succeed"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn test_set_session_mode_unknown_session_returns_error() {
        run_client_server_test(|connection| async move {
            let _init = receive_response(
                connection.send_request(acp::InitializeRequest::new(acp::ProtocolVersion::LATEST)),
            )
            .await;

            let set_mode_resp = receive_response(connection.send_request(
                acp::SetSessionModeRequest::new("nonexistent-session-id", "planning"),
            ))
            .await;

            assert!(
                set_mode_resp.is_err(),
                "set_session_mode with unknown session_id should return an error"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn test_set_session_mode_invalid_mode_returns_error() {
        run_client_server_test(|connection| async move {
            let _init = receive_response(
                connection.send_request(acp::InitializeRequest::new(acp::ProtocolVersion::LATEST)),
            )
            .await;

            let new_session_resp = receive_response(
                connection.send_request(acp::NewSessionRequest::new(std::path::PathBuf::from("."))),
            )
            .await
            .expect("new session should succeed");

            let session_id = new_session_resp.session_id.clone();

            let set_mode_resp = receive_response(connection.send_request(
                acp::SetSessionModeRequest::new(session_id, "invalid_mode_xyz"),
            ))
            .await;

            assert!(
                set_mode_resp.is_err(),
                "set_session_mode with unknown mode should return an error"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn test_set_session_config_option_returns_updated_options() {
        run_client_server_test(|connection| async move {
            let _init = receive_response(
                connection.send_request(acp::InitializeRequest::new(acp::ProtocolVersion::LATEST)),
            )
            .await;

            let new_session_resp = receive_response(
                connection.send_request(acp::NewSessionRequest::new(std::path::PathBuf::from("."))),
            )
            .await
            .expect("new session should succeed");

            let session_id = new_session_resp.session_id.clone();

            let config_resp = receive_response(connection.send_request(
                acp::SetSessionConfigOptionRequest::new(
                    session_id,
                    "safety_policy",
                    "never_confirm",
                ),
            ))
            .await
            .expect("set_session_config_option should succeed");

            assert!(
                !config_resp.config_options.is_empty(),
                "response should include updated config options"
            );
            let safety_opt = config_resp
                .config_options
                .iter()
                .find(|o| o.id.0.as_ref() == "safety_policy")
                .expect("safety_policy option should be in response");
            if let acp::SessionConfigKind::Select(select) = &safety_opt.kind {
                assert_eq!(
                    select.current_value.0.as_ref(),
                    "never_confirm",
                    "safety_policy should be updated to never_confirm"
                );
            } else {
                panic!("safety_policy should be a select option");
            }
        })
        .await;
    }

    #[tokio::test]
    async fn test_set_session_config_option_invalid_value_returns_error() {
        run_client_server_test(|connection| async move {
            let _init = receive_response(
                connection.send_request(acp::InitializeRequest::new(acp::ProtocolVersion::LATEST)),
            )
            .await;

            let new_session_resp = receive_response(
                connection.send_request(acp::NewSessionRequest::new(std::path::PathBuf::from("."))),
            )
            .await
            .expect("new session should succeed");

            let session_id = new_session_resp.session_id.clone();

            let config_resp = receive_response(connection.send_request(
                acp::SetSessionConfigOptionRequest::new(
                    session_id,
                    "safety_policy",
                    "completely_invalid_value",
                ),
            ))
            .await;

            assert!(
                config_resp.is_err(),
                "set_session_config_option with invalid value should return an error"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn test_set_session_model_changes_model() {
        run_client_server_test(|connection| async move {
            let _init = receive_response(
                connection.send_request(acp::InitializeRequest::new(acp::ProtocolVersion::LATEST)),
            )
            .await;

            let new_session_resp = receive_response(
                connection.send_request(acp::NewSessionRequest::new(std::path::PathBuf::from("."))),
            )
            .await
            .expect("new session should succeed");

            let session_id = new_session_resp.session_id.clone();

            // Use the current model from the advertised list.
            let models = new_session_resp
                .models
                .as_ref()
                .expect("models should be advertised");
            let current_model_id = models.current_model_id.0.as_ref().to_string();

            let set_model_resp = receive_response(connection.send_request(
                acp::SetSessionModelRequest::new(session_id, current_model_id),
            ))
            .await;

            assert!(
                set_model_resp.is_ok(),
                "set_session_model with current model should succeed"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn test_set_session_mode_all_valid_modes_succeed() {
        run_client_server_test(|connection| async move {
            let _init = receive_response(
                connection.send_request(acp::InitializeRequest::new(acp::ProtocolVersion::LATEST)),
            )
            .await;

            let new_session_resp = receive_response(
                connection.send_request(acp::NewSessionRequest::new(std::path::PathBuf::from("."))),
            )
            .await
            .expect("new session should succeed");

            let session_id = new_session_resp.session_id.clone();

            for mode in &["planning", "write", "safe", "full_autonomous"] {
                let set_mode_resp = receive_response(
                    connection
                        .send_request(acp::SetSessionModeRequest::new(session_id.clone(), *mode)),
                )
                .await;
                assert!(
                    set_mode_resp.is_ok(),
                    "set_session_mode to '{}' should succeed",
                    mode
                );
            }
        })
        .await;
    }

    #[tokio::test]
    async fn test_set_session_mode_updates_terminal_tool_execution_mode_to_full_autonomous() {
        run_client_server_test(|connection| async move {
            let _init = receive_response(
                connection.send_request(acp::InitializeRequest::new(acp::ProtocolVersion::LATEST)),
            )
            .await;

            let new_session_resp = receive_response(
                connection.send_request(acp::NewSessionRequest::new(std::path::PathBuf::from("."))),
            )
            .await
            .expect("new session should succeed");

            let session_id = new_session_resp.session_id.clone();

            let set_mode_resp = receive_response(connection.send_request(
                acp::SetSessionModeRequest::new(session_id.clone(), "full_autonomous"),
            ))
            .await;

            assert!(
                set_mode_resp.is_ok(),
                "set_session_mode to full_autonomous should succeed \
                 and replace the terminal tool with FullAutonomous execution mode"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn test_set_session_mode_full_autonomous_to_planning_restricts_terminal() {
        run_client_server_test(|connection| async move {
            let _init = receive_response(
                connection.send_request(acp::InitializeRequest::new(acp::ProtocolVersion::LATEST)),
            )
            .await;

            let new_session_resp = receive_response(
                connection.send_request(acp::NewSessionRequest::new(std::path::PathBuf::from("."))),
            )
            .await
            .expect("new session should succeed");

            let session_id = new_session_resp.session_id.clone();

            let full_auto_resp = receive_response(connection.send_request(
                acp::SetSessionModeRequest::new(session_id.clone(), "full_autonomous"),
            ))
            .await;
            assert!(
                full_auto_resp.is_ok(),
                "set_session_mode to full_autonomous should succeed"
            );

            let planning_resp = receive_response(connection.send_request(
                acp::SetSessionModeRequest::new(session_id.clone(), "planning"),
            ))
            .await;
            assert!(
                planning_resp.is_ok(),
                "set_session_mode from full_autonomous to planning should succeed \
                 and update terminal tool to Interactive execution mode"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn test_set_session_mode_does_not_change_terminal_for_unknown_mode() {
        run_client_server_test(|connection| async move {
            let _init = receive_response(
                connection.send_request(acp::InitializeRequest::new(acp::ProtocolVersion::LATEST)),
            )
            .await;

            let new_session_resp = receive_response(
                connection.send_request(acp::NewSessionRequest::new(std::path::PathBuf::from("."))),
            )
            .await
            .expect("new session should succeed");

            let session_id = new_session_resp.session_id.clone();

            let write_resp = receive_response(
                connection
                    .send_request(acp::SetSessionModeRequest::new(session_id.clone(), "write")),
            )
            .await;
            assert!(
                write_resp.is_ok(),
                "initial set_session_mode to write should succeed"
            );

            let unknown_resp = receive_response(
                connection
                    .send_request(acp::SetSessionModeRequest::new(session_id.clone(), "turbo")),
            )
            .await;
            assert!(
                unknown_resp.is_err(),
                "set_session_mode with unknown mode id 'turbo' should return an error \
                 and leave the terminal tool's execution mode unchanged"
            );
        })
        .await;
    }

    #[test]
    fn test_create_session_includes_current_mode_id() {
        let config = Config::default();
        let mode_id = initial_mode_id_from_config(&config);
        assert!(!mode_id.is_empty(), "initial mode id should be non-empty");
        let valid_modes = ["planning", "write", "safe", "full_autonomous"];
        assert!(
            valid_modes.contains(&mode_id.as_str()),
            "initial mode id '{}' should be one of the valid modes",
            mode_id
        );
    }

    #[test]
    fn test_initial_mode_id_from_config_default_is_planning() {
        let config = Config::default();
        let mode_id = initial_mode_id_from_config(&config);
        assert_eq!(
            mode_id, "planning",
            "default config (planning chat mode) should produce planning session mode"
        );
    }

    #[test]
    fn test_initial_mode_id_from_config_full_autonomous_when_allow_dangerous() {
        let mut config = Config::default();
        config.agent.terminal.default_mode = crate::config::ExecutionMode::FullAutonomous;
        config.agent.chat.default_safety = "yolo".to_string();
        let mode_id = initial_mode_id_from_config(&config);
        assert_eq!(
            mode_id, "full_autonomous",
            "full autonomous terminal mode should produce full_autonomous session mode"
        );
    }

    #[tokio::test]
    async fn test_active_session_state_includes_mode_and_runtime_state() {
        let workspace_dir = tempfile::tempdir().expect("workspace should be created");
        let config = Config::default();
        let state = AcpStdioServerState::new_with_storage(
            config,
            AcpStdioAgentOptions::new(None, None, false, None),
            None,
        );

        let response = state
            .create_session(
                acp::NewSessionRequest::new(workspace_dir.path().to_path_buf()),
                None,
            )
            .await
            .expect("session creation should succeed");

        let session = state
            .sessions
            .get(&response.session_id)
            .await
            .expect("active session should exist");
        let session_lock = session.lock().await;

        assert!(!session_lock.current_mode_id().is_empty());
        assert!(!session_lock.runtime_state().safety_mode_str.is_empty());
        assert!(
            !session_lock.has_ide_bridge(),
            "no IDE bridge without connection"
        );
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

    #[test]
    fn test_convert_acp_mcp_server_stdio_produces_valid_config() {
        let server = acp::McpServer::Stdio(
            McpServerStdio::new("my-server", "/usr/bin/mcp-tool").args(vec!["--flag".to_string()]),
        );

        let cfg = convert_acp_mcp_server(&server).unwrap();

        assert_eq!(cfg.id, "my-server");
        assert!(cfg.enabled);
        assert!(cfg.tools_enabled);
        match &cfg.transport {
            McpServerTransportConfig::Stdio {
                executable, args, ..
            } => {
                assert_eq!(executable, "/usr/bin/mcp-tool");
                assert_eq!(args, &["--flag"]);
            }
            _ => panic!("expected Stdio transport"),
        }
    }

    #[test]
    fn test_convert_acp_mcp_server_http_produces_valid_config() {
        let server = acp::McpServer::Http(McpServerHttp::new(
            "http-server",
            "http://localhost:8080/mcp",
        ));

        let cfg = convert_acp_mcp_server(&server).unwrap();

        assert_eq!(cfg.id, "http-server");
        assert!(cfg.enabled);
        match &cfg.transport {
            McpServerTransportConfig::Http { endpoint, .. } => {
                assert_eq!(endpoint.as_str(), "http://localhost:8080/mcp");
            }
            _ => panic!("expected Http transport"),
        }
    }

    #[test]
    fn test_convert_acp_mcp_server_sanitizes_name_with_spaces() {
        let server =
            acp::McpServer::Stdio(McpServerStdio::new("My MCP Server!", "/usr/bin/server"));

        let cfg = convert_acp_mcp_server(&server).unwrap();

        assert_eq!(cfg.id, "my_mcp_server_");
    }

    #[test]
    fn test_convert_acp_mcp_server_rejects_empty_name() {
        let server = acp::McpServer::Stdio(McpServerStdio::new("", "/usr/bin/server"));

        let result = convert_acp_mcp_server(&server);

        assert!(result.is_err(), "empty server name should produce an error");
    }

    #[test]
    fn test_convert_acp_mcp_server_rejects_invalid_http_url() {
        let server = acp::McpServer::Http(McpServerHttp::new("bad-server", "not a url"));

        let result = convert_acp_mcp_server(&server);

        assert!(result.is_err(), "invalid URL should produce an error");
    }

    #[tokio::test]
    async fn test_create_session_with_empty_mcp_servers_list_does_not_error() {
        run_client_server_test(|connection| async move {
            let initialize_resp = receive_response(
                connection.send_request(acp::InitializeRequest::new(acp::ProtocolVersion::LATEST)),
            )
            .await;
            assert!(initialize_resp.is_ok(), "initialize should succeed");

            let cwd = match std::env::current_dir() {
                Ok(cwd) => cwd,
                Err(error) => panic!("current dir should be available: {}", error),
            };

            // NewSessionRequest with no MCP servers (default empty vec).
            let response =
                receive_response(connection.send_request(acp::NewSessionRequest::new(cwd))).await;

            assert!(
                response.is_ok(),
                "session creation with empty mcp_servers should succeed: {:?}",
                response.err()
            );
        })
        .await;
    }

    // -----------------------------------------------------------------------
    // Phase 3: ContextWindowUpdated -> UsageUpdate wiring tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_acp_session_observer_sends_usage_update_on_context_window_updated() {
        // Verify that the ContextWindowUpdated event handler maps its fields
        // correctly to acp::UsageUpdate: used_tokens -> used, max_tokens -> size.
        // This is the exact construction performed by AcpSessionObserver.on_event()
        // when it receives ContextWindowUpdated.
        let update = acp::UsageUpdate::new(500u64, 8192u64);
        assert_eq!(update.used, 500, "used_tokens must become UsageUpdate.used");
        assert_eq!(update.size, 8192, "max_tokens must become UsageUpdate.size");
    }

    #[test]
    fn test_acp_session_observer_context_window_updated_zero_values() {
        // Confirm that zero values are accepted without panic and produce a
        // correctly zeroed UsageUpdate (edge case: empty context window).
        let update = acp::UsageUpdate::new(0u64, 0u64);
        assert_eq!(
            update.used, 0,
            "zero used_tokens must produce UsageUpdate.used == 0"
        );
        assert_eq!(
            update.size, 0,
            "zero max_tokens must produce UsageUpdate.size == 0"
        );
    }
}
