/// ACP executor for sync and async run execution.
///
/// This module bridges ACP run lifecycle coordination with the existing XZatoma
/// agent execution loop. It keeps HTTP handlers transport-focused while the
/// executor handles:
///
/// - loading ACP run input from the runtime
/// - building the existing provider and tool stack
/// - running the current single-agent execution path
/// - recording ACP lifecycle transitions and output events
/// - supporting synchronous and background asynchronous execution
///
/// Phase 3 intentionally keeps execution simple and in-process. The runtime
/// remains the source of truth for ACP run state and event history, while this
/// executor delegates actual agent behavior to the existing XZatoma agent.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::executor::{AcpExecutor, AcpExecutorOutcome};
/// use xzatoma::acp::runtime::{AcpRuntime, AcpRuntimeCreateRequest, AcpRuntimeExecuteMode};
/// use xzatoma::acp::{AcpMessage, AcpMessagePart, AcpRole, AcpTextPart};
/// use xzatoma::Config;
///
/// # async fn example() -> anyhow::Result<()> {
/// let config = Config::default();
/// let runtime = AcpRuntime::new(config.clone());
/// let executor = AcpExecutor::new(config, runtime.clone());
///
/// let run = runtime.create_run(
///     AcpRuntimeCreateRequest::new(vec![AcpMessage::new(
///         AcpRole::User,
///         vec![AcpMessagePart::Text(AcpTextPart::new("Say hello".to_string()))],
///     )?])
///     .with_mode(AcpRuntimeExecuteMode::Async),
/// )?;
///
/// let outcome = executor.spawn_background(run.id.as_str().to_string()).await?;
/// assert!(matches!(outcome, AcpExecutorOutcome::Accepted));
/// # Ok(())
/// # }
/// ```
use std::path::Path;
use std::sync::Arc;

use crate::acp::runtime::{
    assistant_text_message, AcpRuntime, AcpRuntimeCreateRequest, AcpRuntimeExecuteMode,
};
use crate::agent::Agent;
use crate::chat_mode::{ChatMode, SafetyMode};
use crate::commands::{
    build_startup_skill_disclosure, build_visible_skill_catalog, register_activate_skill_tool,
};
use crate::config::Config;
use crate::error::{Result, XzatomaError};
use crate::mcp::auth::token_store::TokenStore;
use crate::mcp::manager::McpClientManager;
use crate::mcp::tool_bridge::register_mcp_tools;
use crate::providers::{create_provider, Provider};
use crate::skills::ActiveSkillRegistry;
use crate::tools::registry_builder::ToolRegistryBuilder;
use tokio::sync::RwLock;

/// ACP executor outcome.
///
/// Sync execution returns a completed or failed run immediately. Async execution
/// returns `Accepted` once the background task has been spawned successfully.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::executor::AcpExecutorOutcome;
///
/// let outcome = AcpExecutorOutcome::Accepted;
/// assert!(matches!(outcome, AcpExecutorOutcome::Accepted));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcpExecutorOutcome {
    /// The run was accepted for background processing.
    Accepted,
    /// The run completed during synchronous execution.
    Completed(crate::acp::AcpRun),
    /// The run failed during synchronous execution.
    Failed(crate::acp::AcpRun),
}

/// ACP run executor.
///
/// This type owns the configuration and ACP runtime handle needed to execute
/// ACP runs using the existing single-agent XZatoma flow.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::executor::AcpExecutor;
/// use xzatoma::acp::runtime::AcpRuntime;
/// use xzatoma::Config;
///
/// let config = Config::default();
/// let runtime = AcpRuntime::new(config.clone());
/// let executor = AcpExecutor::new(config, runtime);
/// let _ = executor;
/// ```
#[derive(Clone)]
pub struct AcpExecutor {
    config: Config,
    runtime: AcpRuntime,
    mock_success_response: Option<String>,
}

impl std::fmt::Debug for AcpExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcpExecutor").finish_non_exhaustive()
    }
}

impl AcpExecutor {
    /// Creates a new ACP executor.
    ///
    /// # Arguments
    ///
    /// * `config` - Application configuration
    /// * `runtime` - ACP runtime coordinator
    ///
    /// # Returns
    ///
    /// Returns a new ACP executor.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::executor::AcpExecutor;
    /// use xzatoma::acp::runtime::AcpRuntime;
    /// use xzatoma::Config;
    ///
    /// let config = Config::default();
    /// let runtime = AcpRuntime::new(config.clone());
    /// let executor = AcpExecutor::new(config, runtime);
    /// let _ = executor;
    /// ```
    pub fn new(config: Config, runtime: AcpRuntime) -> Self {
        Self {
            config,
            runtime,
            mock_success_response: None,
        }
    }

    /// Creates a new ACP executor with a mocked successful response.
    ///
    /// This constructor is intended for tests that need deterministic ACP run
    /// execution without invoking a real provider or requiring external
    /// authentication.
    ///
    /// # Arguments
    ///
    /// * `config` - Application configuration
    /// * `runtime` - ACP runtime coordinator
    /// * `response` - Mock assistant response to record for each executed run
    ///
    /// # Returns
    ///
    /// Returns a new ACP executor configured to bypass provider execution and
    /// return the supplied response.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::executor::AcpExecutor;
    /// use xzatoma::acp::runtime::AcpRuntime;
    /// use xzatoma::Config;
    ///
    /// let config = Config::default();
    /// let runtime = AcpRuntime::new(config.clone());
    /// let executor = AcpExecutor::new_mock_success(
    ///     config,
    ///     runtime,
    ///     "mock response".to_string(),
    /// );
    /// let _ = executor;
    /// ```
    pub fn new_mock_success(config: Config, runtime: AcpRuntime, response: String) -> Self {
        Self {
            config,
            runtime,
            mock_success_response: Some(response),
        }
    }

    /// Returns the shared runtime handle used by the executor.
    ///
    /// # Returns
    ///
    /// Returns a clone of the executor runtime handle.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::executor::AcpExecutor;
    /// use xzatoma::acp::runtime::AcpRuntime;
    /// use xzatoma::Config;
    ///
    /// let config = Config::default();
    /// let runtime = AcpRuntime::new(config.clone());
    /// let executor = AcpExecutor::new(config, runtime.clone());
    ///
    /// assert_eq!(executor.runtime().run_count(), runtime.run_count());
    /// ```
    pub fn runtime(&self) -> AcpRuntime {
        self.runtime.clone()
    }

    /// Executes a run according to the requested mode.
    ///
    /// `sync` executes immediately and returns the final run state. `async`
    /// spawns a background task and returns `Accepted`. `stream` also executes in
    /// the background because the streaming transport consumes runtime events
    /// separately.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    /// * `mode` - Requested ACP execution mode
    ///
    /// # Returns
    ///
    /// Returns an executor outcome describing whether the run completed
    /// synchronously or was accepted for background processing.
    ///
    /// # Errors
    ///
    /// Returns an error if the run cannot be found or execution initialization
    /// fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::executor::{AcpExecutor, AcpExecutorOutcome};
    /// use xzatoma::acp::runtime::{AcpRuntime, AcpRuntimeCreateRequest, AcpRuntimeExecuteMode};
    /// use xzatoma::acp::{AcpMessage, AcpMessagePart, AcpRole, AcpTextPart};
    /// use xzatoma::Config;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let config = Config::default();
    /// let runtime = AcpRuntime::new(config.clone());
    /// let executor = AcpExecutor::new(config, runtime.clone());
    ///
    /// let run = runtime.create_run(
    ///     AcpRuntimeCreateRequest::new(vec![AcpMessage::new(
    ///         AcpRole::User,
    ///         vec![AcpMessagePart::Text(AcpTextPart::new("Hello".to_string()))],
    ///     )?]),
    /// )?;
    ///
    /// let outcome = executor
    ///     .execute(run.id.as_str(), AcpRuntimeExecuteMode::Async)
    ///     .await?;
    /// assert!(matches!(outcome, AcpExecutorOutcome::Accepted));
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute(
        &self,
        run_id: &str,
        mode: AcpRuntimeExecuteMode,
    ) -> Result<AcpExecutorOutcome> {
        match mode {
            AcpRuntimeExecuteMode::Sync => self.execute_sync(run_id).await,
            AcpRuntimeExecuteMode::Async | AcpRuntimeExecuteMode::Stream => {
                self.spawn_background(run_id.to_string()).await
            }
        }
    }

    /// Executes a run synchronously and returns the final run state.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    ///
    /// # Returns
    ///
    /// Returns the terminal run state wrapped in an executor outcome.
    ///
    /// # Errors
    ///
    /// Returns an error if the run cannot be executed.
    pub async fn execute_sync(&self, run_id: &str) -> Result<AcpExecutorOutcome> {
        self.execute_run_internal(run_id).await?;
        let run = self.runtime.get_run(run_id)?;

        if run.status.state == crate::acp::AcpRunState::Completed {
            Ok(AcpExecutorOutcome::Completed(run))
        } else {
            Ok(AcpExecutorOutcome::Failed(run))
        }
    }

    /// Spawns background execution for a run.
    ///
    /// # Arguments
    ///
    /// * `run_id` - ACP run identifier
    ///
    /// # Returns
    ///
    /// Returns `Accepted` once the task has been spawned.
    ///
    /// # Errors
    ///
    /// Returns an error if the run cannot be loaded before spawning.
    pub async fn spawn_background(&self, run_id: String) -> Result<AcpExecutorOutcome> {
        self.runtime.get_run(&run_id)?;

        let executor = self.clone();
        tokio::spawn(async move {
            let result = executor.execute_run_internal(&run_id).await;
            if let Err(error) = result {
                if let Err(record_error) = executor.runtime.record_error_event(
                    &run_id,
                    format!("background ACP execution failed: {}", error),
                ) {
                    tracing::warn!(
                        run_id = %run_id,
                        error = %record_error,
                        "Failed to record background ACP execution error event"
                    );
                }

                if let Err(fail_error) = executor.runtime.fail_run(&run_id, error.to_string()) {
                    tracing::warn!(
                        run_id = %run_id,
                        error = %fail_error,
                        "Failed to mark background ACP run as failed"
                    );
                }
            }
        });

        Ok(AcpExecutorOutcome::Accepted)
    }

    /// Creates and executes a run in one step.
    ///
    /// This is a convenience helper for callers that want run creation and
    /// execution together.
    ///
    /// # Arguments
    ///
    /// * `request` - Runtime create request
    ///
    /// # Returns
    ///
    /// Returns the created run and execution outcome.
    ///
    /// # Errors
    ///
    /// Returns an error if creation or execution fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::executor::AcpExecutor;
    /// use xzatoma::acp::runtime::{AcpRuntime, AcpRuntimeCreateRequest, AcpRuntimeExecuteMode};
    /// use xzatoma::acp::{AcpMessage, AcpMessagePart, AcpRole, AcpTextPart};
    /// use xzatoma::Config;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let config = Config::default();
    /// let runtime = AcpRuntime::new(config.clone());
    /// let executor = AcpExecutor::new(config, runtime);
    ///
    /// let request = AcpRuntimeCreateRequest::new(vec![AcpMessage::new(
    ///     AcpRole::User,
    ///     vec![AcpMessagePart::Text(AcpTextPart::new("Hello".to_string()))],
    /// )?])
    /// .with_mode(AcpRuntimeExecuteMode::Async);
    ///
    /// let (_run, outcome) = executor.create_and_execute(request).await?;
    /// let _ = outcome;
    /// # Ok::<(), anyhow::Error>(())
    /// # }
    /// ```
    pub async fn create_and_execute(
        &self,
        request: AcpRuntimeCreateRequest,
    ) -> Result<(crate::acp::AcpRun, AcpExecutorOutcome)> {
        let mode = request.mode;
        let run = self.runtime.create_run(request)?;
        let outcome = self.execute(run.id.as_str(), mode).await?;
        Ok((run, outcome))
    }

    async fn execute_run_internal(&self, run_id: &str) -> Result<()> {
        self.runtime.mark_queued(run_id)?;
        self.runtime.mark_running(run_id)?;

        let prompt = self.runtime.prompt_for_run(run_id)?;
        let execution = self.execute_prompt(&prompt).await;

        match execution {
            Ok(output) => {
                let message = assistant_text_message(output)?;
                self.runtime.append_output_message(run_id, message)?;
                self.runtime.complete_run(run_id)?;
                Ok(())
            }
            Err(error) => {
                if let Err(record_error) = self
                    .runtime
                    .record_error_event(run_id, format!("ACP executor error: {}", error))
                {
                    tracing::warn!(
                        run_id = %run_id,
                        error = %record_error,
                        "Failed to record ACP executor error event"
                    );
                }

                self.runtime.fail_run(run_id, error.to_string())?;
                Err(error)
            }
        }
    }

    async fn execute_prompt(&self, prompt: &str) -> Result<String> {
        if let Some(response) = &self.mock_success_response {
            tracing::debug!(
                prompt_length = prompt.len(),
                "Using mock ACP execution response"
            );
            return Ok(response.clone());
        }

        let working_dir = std::env::current_dir()?;
        let mut tools = self.build_tools(&working_dir).await?;
        let provider_box =
            create_provider(&self.config.provider.provider_type, &self.config.provider)?;
        let provider: Arc<dyn Provider> = Arc::from(provider_box);

        let subagent_tool = crate::tools::SubagentTool::new_with_config(
            Arc::clone(&provider),
            &self.config.provider,
            self.config.agent.clone(),
            tools.clone(),
            0,
        )?;
        tools.register("subagent", Arc::new(subagent_tool));

        let mut agent =
            Agent::new_from_shared_provider(provider, tools, self.config.agent.clone())?;
        agent.execute(prompt.to_string()).await
    }

    async fn build_tools(&self, working_dir: &Path) -> Result<crate::tools::ToolRegistry> {
        let chat_mode =
            ChatMode::parse_str(&self.config.agent.chat.default_mode).unwrap_or(ChatMode::Planning);

        let safety_mode = match self
            .config
            .agent
            .chat
            .default_safety
            .to_lowercase()
            .as_str()
        {
            "yolo" => SafetyMode::NeverConfirm,
            _ => SafetyMode::AlwaysConfirm,
        };

        let visible_skill_catalog = build_visible_skill_catalog(&self.config, working_dir)?;
        let active_skill_registry = Arc::new(std::sync::Mutex::new(ActiveSkillRegistry::new()));

        let mut tools = ToolRegistryBuilder::new(chat_mode, safety_mode, working_dir.to_path_buf())
            .with_tools_config(self.config.agent.tools.clone())
            .with_terminal_config(self.config.agent.terminal.clone())
            .build()?;

        let _activate_skill_registered = register_activate_skill_tool(
            &mut tools,
            &self.config,
            visible_skill_catalog,
            Arc::clone(&active_skill_registry),
        )?;

        if let Some(disclosure) = build_startup_skill_disclosure(&self.config, working_dir)? {
            tracing::debug!(
                disclosure_length = disclosure.len(),
                "Built ACP skill disclosure for run execution"
            );
        }

        if let Some(manager) = self.build_mcp_manager().await? {
            let execution_mode = self.config.agent.terminal.default_mode;
            let count = register_mcp_tools(&mut tools, manager, execution_mode, true)
                .await
                .map_err(|error| {
                    XzatomaError::Config(format!("Failed to register MCP tools for ACP: {}", error))
                })?;

            if count > 0 {
                tracing::info!(count = count, "Registered MCP tools for ACP executor");
            }
        }

        Ok(tools)
    }

    async fn build_mcp_manager(&self) -> Result<Option<Arc<RwLock<McpClientManager>>>> {
        if !self.config.mcp.auto_connect || self.config.mcp.servers.is_empty() {
            return Ok(None);
        }

        let http_client = Arc::new(reqwest::Client::new());
        let token_store = Arc::new(TokenStore);
        let mut manager = McpClientManager::new(http_client, token_store);

        for server_config in self
            .config
            .mcp
            .servers
            .iter()
            .filter(|server| server.enabled)
        {
            if let Err(error) = manager.connect(server_config.clone()).await {
                tracing::warn!(
                    server_id = %server_config.id,
                    error = %error,
                    "Failed to connect MCP server during ACP executor startup"
                );
            }
        }

        Ok(Some(Arc::new(RwLock::new(manager))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::runtime::AcpRuntimeCreateRequest;
    use crate::acp::{AcpMessage, AcpMessagePart, AcpRole, AcpTextPart};

    fn test_request(mode: AcpRuntimeExecuteMode) -> AcpRuntimeCreateRequest {
        AcpRuntimeCreateRequest::new(vec![AcpMessage::new(
            AcpRole::User,
            vec![AcpMessagePart::Text(AcpTextPart::new(
                "Test ACP executor".to_string(),
            ))],
        )
        .unwrap()])
        .with_mode(mode)
    }

    #[tokio::test]
    async fn test_spawn_background_returns_accepted_for_existing_run() {
        let mut config = Config::default();
        config.provider.provider_type = "ollama".to_string();
        let runtime = AcpRuntime::new(config.clone());
        let executor = AcpExecutor::new_mock_success(
            config,
            runtime.clone(),
            "mock async response".to_string(),
        );

        let run = runtime
            .create_run(test_request(AcpRuntimeExecuteMode::Async))
            .unwrap();
        let outcome = executor
            .spawn_background(run.id.as_str().to_string())
            .await
            .unwrap();

        assert!(matches!(outcome, AcpExecutorOutcome::Accepted));
    }

    #[tokio::test]
    async fn test_execute_async_returns_accepted() {
        let mut config = Config::default();
        config.provider.provider_type = "ollama".to_string();
        let runtime = AcpRuntime::new(config.clone());
        let executor = AcpExecutor::new_mock_success(
            config,
            runtime.clone(),
            "mock async response".to_string(),
        );

        let run = runtime
            .create_run(test_request(AcpRuntimeExecuteMode::Async))
            .unwrap();
        let outcome = executor
            .execute(run.id.as_str(), AcpRuntimeExecuteMode::Async)
            .await
            .unwrap();

        assert!(matches!(outcome, AcpExecutorOutcome::Accepted));
    }

    #[tokio::test]
    async fn test_execute_with_missing_run_returns_error() {
        let mut config = Config::default();
        config.provider.provider_type = "ollama".to_string();
        let runtime = AcpRuntime::new(config.clone());
        let executor =
            AcpExecutor::new_mock_success(config, runtime, "mock missing response".to_string());

        let error = executor
            .execute("run_missing", AcpRuntimeExecuteMode::Async)
            .await
            .unwrap_err();

        assert!(error.to_string().contains("was not found"));
    }

    #[test]
    fn test_executor_runtime_returns_clone() {
        let mut config = Config::default();
        config.provider.provider_type = "ollama".to_string();
        let runtime = AcpRuntime::new(config.clone());
        let executor =
            AcpExecutor::new_mock_success(config, runtime.clone(), "mock runtime".to_string());

        assert_eq!(executor.runtime().run_count(), runtime.run_count());
    }

    #[tokio::test]
    async fn test_execute_sync_with_mock_success_returns_completed_run() {
        let mut config = Config::default();
        config.provider.provider_type = "ollama".to_string();
        let runtime = AcpRuntime::new(config.clone());
        let executor = AcpExecutor::new_mock_success(
            config,
            runtime.clone(),
            "mock sync response".to_string(),
        );

        let run = runtime
            .create_run(test_request(AcpRuntimeExecuteMode::Sync))
            .unwrap();

        let outcome = executor.execute_sync(run.id.as_str()).await.unwrap();

        match outcome {
            AcpExecutorOutcome::Completed(updated_run) => {
                assert_eq!(updated_run.status.state, crate::acp::AcpRunState::Completed);
                assert_eq!(updated_run.output.messages.len(), 1);
            }
            other => panic!("expected completed run outcome, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_create_and_execute_sync_with_mock_success_returns_completed_run() {
        let mut config = Config::default();
        config.provider.provider_type = "ollama".to_string();
        let runtime = AcpRuntime::new(config.clone());
        let executor = AcpExecutor::new_mock_success(
            config,
            runtime,
            "mock create and execute response".to_string(),
        );

        let (run, outcome) = executor
            .create_and_execute(test_request(AcpRuntimeExecuteMode::Sync))
            .await
            .unwrap();

        assert!(!run.id.as_str().is_empty());

        match outcome {
            AcpExecutorOutcome::Completed(updated_run) => {
                assert_eq!(updated_run.status.state, crate::acp::AcpRunState::Completed);
                assert_eq!(updated_run.output.messages.len(), 1);
            }
            other => panic!("expected completed run outcome, got {:?}", other),
        }
    }
}
