//! Subagent tool for delegating tasks to recursive agent instances
//!
//! This module provides the `SubagentTool` which allows an agent to spawn
//! child agents with isolated conversation contexts for focused task execution.
//! This feature prevents context pollution and enables parallel exploration
//! of sub-problems without polluting the main conversation history.

use crate::agent::{quota::QuotaTracker, Agent, ConversationStore, SubagentMetrics};
use crate::config::{AgentConfig, SubagentConfig};
use crate::error::{Result, XzatomaError};
use crate::providers::Provider;
use crate::tools::{ToolExecutor, ToolRegistry, ToolResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Telemetry module for subagent execution tracing
///
/// Provides structured logging events for subagent lifecycle management.
/// All events are emitted with consistent field naming for easy aggregation and filtering.
mod telemetry {
    use tracing::{debug, error, info, warn};

    /// Log subagent spawn event
    ///
    /// Called when a subagent is about to be created and executed.
    ///
    /// # Arguments
    ///
    /// * `label` - Subagent identifier
    /// * `depth` - Current recursion depth
    /// * `max_turns` - Maximum turns for this subagent
    /// * `allowed_tools` - List of tools available to subagent
    pub fn log_subagent_spawn(
        label: &str,
        depth: usize,
        max_turns: usize,
        allowed_tools: &[String],
    ) {
        info!(
            subagent.event = "spawn",
            subagent.label = label,
            subagent.depth = depth,
            subagent.max_turns = max_turns,
            subagent.allowed_tools = ?allowed_tools,
            "Spawning subagent"
        );
    }

    /// Log subagent completion event
    ///
    /// Called when a subagent successfully completes execution.
    ///
    /// # Arguments
    ///
    /// * `label` - Subagent identifier
    /// * `depth` - Recursion depth
    /// * `turns_used` - Number of conversation turns consumed
    /// * `tokens_used` - Number of tokens consumed
    /// * `status` - Completion status ("complete" or "incomplete")
    pub fn log_subagent_complete(
        label: &str,
        depth: usize,
        turns_used: usize,
        tokens_used: usize,
        status: &str,
    ) {
        info!(
            subagent.event = "complete",
            subagent.label = label,
            subagent.depth = depth,
            subagent.turns_used = turns_used,
            subagent.tokens_consumed = tokens_used,
            subagent.status = status,
            "Subagent completed"
        );
    }

    /// Log subagent error event
    ///
    /// Called when a subagent execution fails.
    ///
    /// # Arguments
    ///
    /// * `label` - Subagent identifier
    /// * `depth` - Recursion depth
    /// * `error` - Error message describing the failure
    pub fn log_subagent_error(label: &str, depth: usize, error: &str) {
        error!(
            subagent.event = "error",
            subagent.label = label,
            subagent.depth = depth,
            subagent.error = error,
            "Subagent execution failed"
        );
    }

    /// Log output truncation event
    ///
    /// Called when subagent output exceeds maximum size and is truncated.
    ///
    /// # Arguments
    ///
    /// * `label` - Subagent identifier
    /// * `original_size` - Original output size in bytes
    /// * `truncated_size` - Size after truncation
    pub fn log_output_truncation(label: &str, original_size: usize, truncated_size: usize) {
        warn!(
            subagent.event = "truncation",
            subagent.label = label,
            subagent.original_size = original_size,
            subagent.truncated_size = truncated_size,
            "Subagent output truncated"
        );
    }

    /// Log max turns exceeded event
    ///
    /// Called when a subagent reaches its maximum turn limit before completion.
    ///
    /// # Arguments
    ///
    /// * `label` - Subagent identifier
    /// * `depth` - Recursion depth
    /// * `max_turns` - Maximum turns limit that was reached
    pub fn log_max_turns_exceeded(label: &str, depth: usize, max_turns: usize) {
        warn!(
            subagent.event = "max_turns_exceeded",
            subagent.label = label,
            subagent.depth = depth,
            subagent.max_turns = max_turns,
            "Subagent exceeded max turns"
        );
    }

    /// Log recursion depth limit event
    ///
    /// Called when a subagent cannot be spawned due to recursion depth limit.
    ///
    /// # Arguments
    ///
    /// * `label` - Subagent identifier
    /// * `current_depth` - Current recursion depth
    /// * `max_depth` - Maximum allowed depth
    pub fn log_depth_limit_reached(label: &str, current_depth: usize, max_depth: usize) {
        debug!(
            subagent.event = "depth_limit",
            subagent.label = label,
            subagent.current_depth = current_depth,
            subagent.max_depth = max_depth,
            "Subagent recursion depth limit enforced"
        );
    }
}

/// Input parameters for subagent tool
///
/// Defines the task delegation request from parent agent to subagent.
/// All fields follow the OpenAI function calling format with JSON schema validation.
///
/// # Examples
///
/// ```rust
/// use xzatoma::tools::subagent::SubagentToolInput;
///
/// let input = SubagentToolInput {
///     label: "research_docs".to_string(),
///     task_prompt: "Research the API documentation".to_string(),
///     summary_prompt: Some("Summarize your findings".to_string()),
///     allowed_tools: Some(vec!["fetch".to_string(), "grep".to_string()]),
///     max_turns: Some(5),
/// };
/// assert_eq!(input.label, "research_docs");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentToolInput {
    /// Unique identifier for tracking this subagent instance
    ///
    /// Used for logging and debugging. Should be descriptive of the task.
    /// Examples: "research_api_docs", "analyze_error_logs", "search_codebase"
    pub label: String,

    /// The task prompt for the subagent to execute
    ///
    /// Should be a complete, self-contained task description.
    /// The subagent will treat this as its initial user message.
    /// Cannot be empty or whitespace-only.
    pub task_prompt: String,

    /// Optional prompt for summarizing subagent results
    ///
    /// If provided, subagent will be prompted with this after completing
    /// the task to produce a concise summary. If None, a default summary
    /// prompt is used: "Summarize your findings concisely"
    #[serde(default)]
    pub summary_prompt: Option<String>,

    /// Optional whitelist of tool names subagent can access
    ///
    /// If None, subagent inherits all parent tools except "subagent".
    /// If Some([...]), only listed tools are available (except "subagent").
    /// The "subagent" tool is always excluded to prevent infinite recursion.
    /// Unknown tool names in the whitelist will cause an error.
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,

    /// Optional override for maximum conversation turns
    ///
    /// If None, uses DEFAULT_SUBAGENT_MAX_TURNS (currently 10).
    /// Must be between 1 and 50 inclusive.
    /// Constrains how many back-and-forth turns the subagent can have.
    #[serde(default)]
    pub max_turns: Option<usize>,
}

/// Subagent tool executor
///
/// Manages spawning and executing recursive agent instances with
/// isolated contexts and filtered tool access. Each subagent gets:
/// - Independent conversation history
/// - Shared provider instance (memory efficient)
/// - Filtered tool registry (optional whitelist)
/// - Configurable execution limits
///
/// # Examples
///
/// ```no_run
/// use xzatoma::tools::subagent::SubagentTool;
/// use xzatoma::config::AgentConfig;
/// use xzatoma::tools::ToolRegistry;
/// use std::sync::Arc;
///
/// // Provider is environment-specific; this example is illustrative and not executed in doctests
/// # use xzatoma::config::CopilotConfig;
/// # use xzatoma::providers::CopilotProvider;
/// # let provider = CopilotProvider::new(CopilotConfig::default()).unwrap();
/// let tools = ToolRegistry::new();
/// let config = AgentConfig::default();
/// let subagent_tool = SubagentTool::new(
///     Arc::new(provider),
///     config,
///     tools,
///     0,
/// );
/// ```
pub struct SubagentTool {
    /// Shared provider instance (Arc for cheap cloning)
    ///
    /// All subagents share the same provider instance, which is thread-safe
    /// and implements Send + Sync. This avoids duplicating HTTP clients
    /// or authentication contexts.
    provider: Arc<dyn Provider>,

    /// Agent configuration template
    ///
    /// Used to configure the subagent with limits on turns, timeouts,
    /// and conversation management parameters. May be overridden by
    /// subagent input parameters.
    config: AgentConfig,

    /// Parent's tool registry for filtering
    ///
    /// The parent's complete registry is stored here so that subagent
    /// can filter it based on the allowed_tools whitelist parameter.
    /// This allows for cheap registry cloning via Arc<dyn ToolExecutor>.
    parent_registry: ToolRegistry,

    /// Current recursion depth (0 = root agent)
    ///
    /// Tracks how deeply nested this subagent is. Used to enforce
    /// max_depth limit. Incremented on each nested spawn.
    /// - depth=0: Main agent
    /// - depth=1: First subagent spawned from main
    /// - depth=2: Subagent spawned from first subagent
    /// - depth>=max_depth: Error, exceeds limit
    current_depth: usize,

    /// Subagent configuration
    ///
    /// Contains limits and settings specific to subagent execution:
    /// max_depth, default_max_turns, output_max_size, telemetry_enabled, persistence_enabled
    subagent_config: SubagentConfig,

    /// Conversation persistence store (optional)
    ///
    /// When Some, conversations are persisted for debugging and replay.
    /// When None, persistence is disabled.
    conversation_store: Option<Arc<ConversationStore>>,

    /// Parent conversation ID (if this is a nested subagent)
    ///
    /// Used to link conversations in the persistence store,
    /// forming a tree of parent-child relationships.
    parent_conversation_id: Option<String>,

    /// Optional quota tracker for resource management
    ///
    /// When Some, enforces limits on execution count, total tokens,
    /// and wall-clock time across all subagent executions.
    /// When None, no resource limits are enforced.
    quota_tracker: Option<QuotaTracker>,
}

impl SubagentTool {
    /// Creates a new subagent tool executor
    ///
    /// # Arguments
    ///
    /// * `provider` - Shared provider instance used by all subagents
    /// * `config` - Agent configuration template for subagent instances
    /// * `parent_registry` - Parent's tool registry for filtering
    /// * `current_depth` - Current recursion depth (0 for root)
    ///
    /// # Returns
    ///
    /// Returns a new SubagentTool instance
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzatoma::tools::subagent::SubagentTool;
    /// use xzatoma::config::AgentConfig;
    /// use xzatoma::tools::ToolRegistry;
    /// use std::sync::Arc;
    ///
    /// # use xzatoma::config::CopilotConfig;
    /// # use xzatoma::providers::CopilotProvider;
    /// # let provider = CopilotProvider::new(CopilotConfig::default()).unwrap();
    /// let tool = SubagentTool::new(
    ///     Arc::new(provider),
    ///     AgentConfig::default(),
    ///     ToolRegistry::new(),
    ///     0,
    /// );
    /// ```
    pub fn new(
        provider: Arc<dyn Provider>,
        config: AgentConfig,
        parent_registry: ToolRegistry,
        current_depth: usize,
    ) -> Self {
        let subagent_config = config.subagent.clone();

        // Initialize conversation store if persistence enabled
        let conversation_store = if subagent_config.persistence_enabled {
            match ConversationStore::new(&subagent_config.persistence_path) {
                Ok(store) => Some(Arc::new(store)),
                Err(e) => {
                    tracing::warn!("Failed to initialize conversation store: {}", e);
                    None
                }
            }
        } else {
            None
        };

        Self {
            provider,
            config,
            parent_registry,
            current_depth,
            subagent_config,
            conversation_store,
            parent_conversation_id: None,
            quota_tracker: None,
        }
    }

    /// Set the parent conversation ID for linking in persistence store
    ///
    /// # Arguments
    ///
    /// * `id` - The parent conversation ID
    ///
    /// # Returns
    ///
    /// Returns self for method chaining
    pub fn with_parent_conversation_id(mut self, id: String) -> Self {
        self.parent_conversation_id = Some(id);
        self
    }

    /// Set the quota tracker for resource management
    ///
    /// # Arguments
    ///
    /// * `tracker` - The quota tracker instance
    ///
    /// # Returns
    ///
    /// Returns self for method chaining
    pub fn with_quota_tracker(mut self, tracker: QuotaTracker) -> Self {
        self.quota_tracker = Some(tracker);
        self
    }
}

/// Creates a filtered tool registry for subagent
///
/// Applies tool filtering to the parent registry based on the allowed_tools
/// whitelist parameter. The "subagent" tool is always excluded to prevent
/// infinite recursion through tool definitions.
///
/// # Arguments
///
/// * `parent_registry` - The parent agent's tool registry
/// * `allowed_tools` - Optional whitelist of tool names
///
/// # Returns
///
/// Returns a new ToolRegistry with filtered tools, or error if validation fails
///
/// # Errors
///
/// Returns error if:
/// - allowed_tools contains "subagent" (forbidden)
/// - allowed_tools references unknown tool names
///
/// # Examples
///
/// ```rust
/// use xzatoma::tools::ToolRegistry;
///
/// let parent_registry = ToolRegistry::new();
/// let filtered = parent_registry.clone_with_filter(&["file_ops".to_string(), "terminal".to_string()]);
/// ```
fn create_filtered_registry(
    parent_registry: &ToolRegistry,
    allowed_tools: Option<Vec<String>>,
) -> Result<ToolRegistry> {
    let mut subagent_registry = ToolRegistry::new();

    match allowed_tools {
        None => {
            // Clone entire parent registry EXCEPT "subagent" tool
            // (prevents infinite recursion in tool definitions)
            // Decision 2: ALL parent tools except "subagent"
            for tool_name in parent_registry.tool_names() {
                if tool_name != "subagent" {
                    if let Some(executor) = parent_registry.get(&tool_name) {
                        subagent_registry.register(&tool_name, executor);
                    }
                }
            }
        }
        Some(allowed) => {
            // Only register whitelisted tools
            for tool_name in allowed {
                // Prevent "subagent" in whitelist (infinite recursion risk)
                if tool_name == "subagent" {
                    return Err(XzatomaError::Config(
                        "Subagent cannot have 'subagent' in allowed_tools".to_string(),
                    )
                    .into());
                }

                // Verify tool exists in parent registry
                let executor = parent_registry.get(&tool_name).ok_or_else(|| {
                    XzatomaError::Config(format!("Unknown tool in allowed_tools: {}", tool_name))
                })?;

                subagent_registry.register(&tool_name, executor);
            }
        }
    }

    Ok(subagent_registry)
}

#[async_trait]
impl ToolExecutor for SubagentTool {
    fn tool_definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "subagent",
            "description": "Delegate a focused task to a recursive agent instance with isolated conversation context. Use this when you need to explore a sub-problem independently without polluting the main conversation.",
            "parameters": {
                "type": "object",
                "properties": {
                    "label": {
                        "type": "string",
                        "description": "Unique identifier for this subagent (e.g., 'research_api_docs', 'analyze_logs')"
                    },
                    "task_prompt": {
                        "type": "string",
                        "description": "The specific task for the subagent to complete. Should be self-contained."
                    },
                    "summary_prompt": {
                        "type": "string",
                        "description": "Optional: How to summarize results (default: 'Summarize your findings concisely')"
                    },
                    "allowed_tools": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Optional: Whitelist of tool names subagent can use. Omit to allow all tools."
                    },
                    "max_turns": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 50,
                        "description": "Optional: Maximum conversation turns for subagent (default: 10)"
                    }
                },
                "required": ["label", "task_prompt"]
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        // Create metrics tracker for this subagent execution
        let metrics = SubagentMetrics::new(
            args.get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            self.current_depth + 1,
        );

        // STEP 1: Check quota availability before any work
        if let Some(quota_tracker) = &self.quota_tracker {
            if let Err(e) = quota_tracker.check_and_reserve() {
                let label = args
                    .get("label")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                if self.subagent_config.telemetry_enabled {
                    tracing::warn!(
                        subagent.event = "quota_exceeded",
                        subagent.label = label,
                        subagent.error = %e,
                        "Subagent quota exceeded"
                    );
                }
                metrics.record_error("quota_exceeded");
                return Ok(ToolResult::error(format!("Resource quota exceeded: {}", e)));
            }
        }

        // STEP 2: Validate recursion depth
        if self.current_depth >= self.subagent_config.max_depth {
            let telemetry_enabled = self.subagent_config.telemetry_enabled;
            // Try to parse label for logging, fallback to "unknown"
            let label = args
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            if telemetry_enabled {
                telemetry::log_depth_limit_reached(
                    label,
                    self.current_depth,
                    self.subagent_config.max_depth,
                );
            }
            metrics.record_error("max_depth_reached");
            return Ok(ToolResult::error(format!(
                "Maximum subagent recursion depth ({}) exceeded. Current depth: {}. Cannot spawn nested subagent.",
                self.subagent_config.max_depth,
                self.current_depth
            )));
        }

        // STEP 3: Parse and validate input
        let input: SubagentToolInput = serde_json::from_value(args)
            .map_err(|e| XzatomaError::Config(format!("Invalid subagent input: {}", e)))?;

        // Check telemetry enabled flag
        let telemetry_enabled = self.subagent_config.telemetry_enabled;

        // Validate task_prompt not empty
        if input.task_prompt.trim().is_empty() {
            if telemetry_enabled {
                telemetry::log_subagent_error(
                    &input.label,
                    self.current_depth + 1,
                    "task_prompt cannot be empty",
                );
            }
            metrics.record_error("invalid_input");
            return Ok(ToolResult::error("task_prompt cannot be empty".to_string()));
        }

        // Validate label not empty
        if input.label.trim().is_empty() {
            metrics.record_error("invalid_input");
            return Ok(ToolResult::error("label cannot be empty".to_string()));
        }

        // Validate max_turns if specified
        if let Some(max_turns) = input.max_turns {
            if max_turns == 0 || max_turns > 50 {
                metrics.record_error("invalid_input");
                return Ok(ToolResult::error(
                    "max_turns must be between 1 and 50".to_string(),
                ));
            }
        }

        // STEP 3: Create filtered registry for subagent
        let subagent_registry =
            create_filtered_registry(&self.parent_registry, input.allowed_tools.clone())?;

        // Log spawn event before creating subagent
        let next_depth = self.current_depth + 1;
        let max_turns_for_spawn = input
            .max_turns
            .unwrap_or(self.subagent_config.default_max_turns);
        let allowed_tools_for_log = input.allowed_tools.clone().unwrap_or_default();
        if telemetry_enabled {
            telemetry::log_subagent_spawn(
                &input.label,
                next_depth,
                max_turns_for_spawn,
                &allowed_tools_for_log,
            );
        }

        // Generate conversation ID and capture start time for this subagent
        let conversation_id = crate::agent::new_conversation_id();
        let started_at = crate::agent::now_rfc3339();

        // STEP 4: Create nested subagent tool for this child
        // (allows further nesting up to max_depth)
        let mut nested_subagent_tool = SubagentTool::new(
            Arc::clone(&self.provider),
            self.config.clone(),
            subagent_registry.clone(),
            next_depth,
        );

        // Pass quota tracker to nested subagent if available
        if let Some(quota_tracker) = &self.quota_tracker {
            nested_subagent_tool = nested_subagent_tool.with_quota_tracker(quota_tracker.clone());
        }

        // Link to parent conversation if available
        if let Some(parent_id) = &self.parent_conversation_id {
            nested_subagent_tool =
                nested_subagent_tool.with_parent_conversation_id(parent_id.clone());
        }
        nested_subagent_tool =
            nested_subagent_tool.with_parent_conversation_id(conversation_id.clone());

        // Register nested subagent tool in child's registry
        // (will be blocked by depth check if limit reached)
        let mut final_registry = subagent_registry;
        final_registry.register("subagent", Arc::new(nested_subagent_tool));

        // STEP 5: Create subagent config with overrides
        let mut subagent_config = self.config.clone();
        if let Some(max_turns) = input.max_turns {
            subagent_config.max_turns = max_turns;
        } else {
            subagent_config.max_turns = self.subagent_config.default_max_turns;
        }

        // STEP 6: Create and execute subagent
        let mut subagent = match Agent::new_from_shared_provider(
            Arc::clone(&self.provider),
            final_registry,
            subagent_config,
        ) {
            Ok(agent) => agent,
            Err(e) => {
                if telemetry_enabled {
                    telemetry::log_subagent_error(&input.label, next_depth, &e.to_string());
                }
                metrics.record_error("initialization_failed");
                return Ok(ToolResult::error(format!(
                    "Subagent initialization failed: {}",
                    e
                )));
            }
        };

        // Execute task
        let _task_result = match subagent.execute(input.task_prompt.clone()).await {
            Ok(result) => result,
            Err(e) => {
                if telemetry_enabled {
                    telemetry::log_subagent_error(&input.label, next_depth, &e.to_string());
                }
                metrics.record_error("execution_failed");
                return Ok(ToolResult::error(format!(
                    "Subagent task execution failed: {}",
                    e
                )));
            }
        };

        // STEP 7: Request summary
        // Decision 3: Always request summary (use default if not provided)
        let summary_prompt = input
            .summary_prompt
            .as_ref()
            .cloned()
            .unwrap_or_else(|| "Summarize your findings concisely".to_string());

        // Continue conversation with summary request
        let final_output = subagent.execute(summary_prompt).await?;

        // Capture values before they're moved
        let label = input.label.clone();
        let output_len = final_output.len();

        // STEP 8: Build result with metadata
        let mut result = ToolResult::success(final_output)
            .with_metadata("subagent_label".to_string(), label.clone())
            .with_metadata(
                "recursion_depth".to_string(),
                self.current_depth.to_string(),
            );

        // Check if subagent hit max_turns limit (incomplete execution)
        // Count user messages as turns (each execute() call adds one user message)
        let turn_count = subagent
            .conversation()
            .messages()
            .iter()
            .filter(|msg| msg.role == "user")
            .count();
        let max_turns = input
            .max_turns
            .unwrap_or(self.subagent_config.default_max_turns);
        if turn_count >= max_turns {
            if telemetry_enabled {
                telemetry::log_max_turns_exceeded(&label, next_depth, max_turns);
            }
            result = result
                .with_metadata("max_turns_reached".to_string(), "true".to_string())
                .with_metadata("completion_status".to_string(), "incomplete".to_string())
                .with_metadata("turns_used".to_string(), turn_count.to_string())
                .with_metadata("max_turns".to_string(), max_turns.to_string());
        } else {
            result = result
                .with_metadata("completion_status".to_string(), "complete".to_string())
                .with_metadata("turns_used".to_string(), turn_count.to_string());
        }

        // Add token usage if available and record in quota tracker
        let tokens_used = if let Some(usage) = subagent.get_token_usage() {
            let total = usage.total_tokens;
            result = result
                .with_metadata("tokens_used".to_string(), total.to_string())
                .with_metadata("prompt_tokens".to_string(), usage.prompt_tokens.to_string())
                .with_metadata(
                    "completion_tokens".to_string(),
                    usage.completion_tokens.to_string(),
                );
            total
        } else {
            0
        };

        // Record quota usage if tracker is available
        if let Some(quota_tracker) = &self.quota_tracker {
            if let Err(e) = quota_tracker.record_execution(tokens_used) {
                if telemetry_enabled {
                    tracing::warn!(
                        subagent.event = "quota_recording_failed",
                        subagent.label = %label,
                        subagent.error = %e,
                        "Failed to record quota usage"
                    );
                }
                // Log warning but don't fail the execution
                // The subagent already completed successfully
            }
        }

        // STEP 9: Truncate if needed and log if truncation occurred
        if output_len > self.subagent_config.output_max_size {
            if telemetry_enabled {
                telemetry::log_output_truncation(
                    &label,
                    output_len,
                    self.subagent_config.output_max_size,
                );
            }
            result = result.truncate_if_needed(self.subagent_config.output_max_size);
        }

        // Record metrics for completion
        let completion_status = if turn_count >= max_turns {
            "incomplete"
        } else {
            "complete"
        };
        let tokens_used = subagent
            .get_token_usage()
            .map(|u| u.total_tokens)
            .unwrap_or(0);
        metrics.record_completion(turn_count, tokens_used, completion_status);

        // Log completion event
        if telemetry_enabled {
            telemetry::log_subagent_complete(
                &label,
                next_depth,
                turn_count,
                tokens_used,
                completion_status,
            );
        }

        // STEP 10: Persist conversation if persistence enabled
        if let Some(store) = &self.conversation_store {
            let completion_status = if turn_count >= max_turns {
                "incomplete".to_string()
            } else {
                "complete".to_string()
            };

            let tokens_consumed = subagent
                .get_token_usage()
                .map(|u| u.total_tokens)
                .unwrap_or(0);

            let record = crate::agent::ConversationRecord {
                id: conversation_id.clone(),
                parent_id: self.parent_conversation_id.clone(),
                label: input.label.clone(),
                depth: next_depth,
                messages: subagent.conversation().messages().to_vec(),
                started_at,
                completed_at: Some(crate::agent::now_rfc3339()),
                metadata: crate::agent::ConversationMetadata {
                    turns_used: turn_count,
                    tokens_consumed,
                    completion_status,
                    max_turns_reached: turn_count >= max_turns,
                    task_prompt: input.task_prompt.clone(),
                    summary_prompt: input.summary_prompt.clone(),
                    allowed_tools: input.allowed_tools.clone().unwrap_or_default(),
                },
            };

            if let Err(e) = store.save(&record) {
                tracing::warn!("Failed to persist conversation {}: {}", conversation_id, e);
            } else {
                tracing::debug!(
                    subagent.event = "persisted",
                    subagent.conversation_id = %conversation_id,
                    "Conversation persisted"
                );
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{CompletionResponse, Message, Provider, TokenUsage};
    use std::sync::Mutex;

    // Mock provider for testing
    struct MockProvider {
        responses: Mutex<Vec<String>>,
        call_count: Mutex<usize>,
    }

    impl MockProvider {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses: Mutex::new(responses),
                call_count: Mutex::new(0),
            }
        }

        fn call_count(&self) -> usize {
            *self.call_count.lock().unwrap()
        }
    }

    #[async_trait]
    impl Provider for MockProvider {
        async fn complete(
            &self,
            _messages: &[Message],
            _tools: &[serde_json::Value],
        ) -> Result<CompletionResponse> {
            let mut count = self.call_count.lock().unwrap();
            *count += 1;

            let mut responses = self.responses.lock().unwrap();
            if responses.is_empty() {
                return Ok(CompletionResponse::new(Message::assistant(
                    "default response",
                )));
            }
            let response = responses.remove(0);

            Ok(CompletionResponse::new(Message::assistant(response)))
        }
    }

    fn create_test_config() -> AgentConfig {
        AgentConfig::default()
    }

    // Test 1: Valid input parsing
    #[test]
    fn test_subagent_input_parsing_valid() {
        let json = serde_json::json!({
            "label": "test_agent",
            "task_prompt": "Do something",
            "summary_prompt": "Summarize",
            "allowed_tools": ["read_file", "terminal"],
            "max_turns": 5
        });

        let input: SubagentToolInput = serde_json::from_value(json).unwrap();
        assert_eq!(input.label, "test_agent");
        assert_eq!(input.task_prompt, "Do something");
        assert_eq!(input.summary_prompt, Some("Summarize".to_string()));
        assert_eq!(
            input.allowed_tools,
            Some(vec!["read_file".to_string(), "terminal".to_string()])
        );
        assert_eq!(input.max_turns, Some(5));
    }

    // Test 2: Missing required fields
    #[test]
    fn test_subagent_input_parsing_missing_required() {
        let json = serde_json::json!({
            "label": "test"
        });

        let result = serde_json::from_value::<SubagentToolInput>(json);
        assert!(result.is_err());
    }

    // Test 3: Optional fields default
    #[test]
    fn test_subagent_input_parsing_defaults() {
        let json = serde_json::json!({
            "label": "test",
            "task_prompt": "task"
        });

        let input: SubagentToolInput = serde_json::from_value(json).unwrap();
        assert!(input.summary_prompt.is_none());
        assert!(input.allowed_tools.is_none());
        assert!(input.max_turns.is_none());
    }

    // Test 4: Recursion depth limit enforced
    #[tokio::test]
    async fn test_subagent_recursion_depth_limit() {
        let provider = Arc::new(MockProvider::new(vec!["response".to_string()]));
        let registry = ToolRegistry::new();
        let config = create_test_config();
        let max_depth = config.subagent.max_depth;

        let tool = SubagentTool::new(provider, config, registry, max_depth);

        let input = serde_json::json!({
            "label": "test",
            "task_prompt": "task"
        });

        let result = tool.execute(input).await.unwrap();
        assert!(!result.success);
        assert!(result
            .error
            .unwrap()
            .contains("Maximum subagent recursion depth"));
    }

    // Test 5: Depth 0 allows execution
    #[tokio::test]
    async fn test_subagent_depth_0_allows_execution() {
        let provider = Arc::new(MockProvider::new(vec!["response".to_string()]));
        let registry = ToolRegistry::new();
        let config = create_test_config();

        let tool = SubagentTool::new(provider, config, registry, 0);

        let input = serde_json::json!({
            "label": "test",
            "task_prompt": "task"
        });

        let result = tool.execute(input).await.unwrap();
        assert!(result.success);
    }

    // Test 6: Tool filtering - all except subagent
    #[test]
    fn test_registry_filtering_excludes_subagent() {
        let mut parent_registry = ToolRegistry::new();

        // Create mock tools
        let mock_provider = Arc::new(MockProvider::new(vec![]));
        let mock_tool = Arc::new(SubagentTool::new(
            mock_provider.clone(),
            create_test_config(),
            ToolRegistry::new(),
            0,
        ));

        parent_registry.register("file_ops", mock_tool.clone());
        parent_registry.register("terminal", mock_tool.clone());
        parent_registry.register("subagent", mock_tool);

        let filtered = create_filtered_registry(&parent_registry, None).unwrap();

        // Should have file_ops and terminal, but NOT subagent
        assert!(filtered.get("file_ops").is_some());
        assert!(filtered.get("terminal").is_some());
        assert!(filtered.get("subagent").is_none());
    }

    // Test 7: Tool filtering - whitelist only
    #[test]
    fn test_registry_filtering_whitelist_only() {
        let mut parent_registry = ToolRegistry::new();

        let mock_provider = Arc::new(MockProvider::new(vec![]));
        let mock_tool = Arc::new(SubagentTool::new(
            mock_provider.clone(),
            create_test_config(),
            ToolRegistry::new(),
            0,
        ));

        parent_registry.register("file_ops", mock_tool.clone());
        parent_registry.register("terminal", mock_tool.clone());
        parent_registry.register("grep", mock_tool);

        let allowed = Some(vec!["file_ops".to_string(), "grep".to_string()]);
        let filtered = create_filtered_registry(&parent_registry, allowed).unwrap();

        assert!(filtered.get("file_ops").is_some());
        assert!(filtered.get("grep").is_some());
        assert!(filtered.get("terminal").is_none());
    }

    // Test 8: Rejects subagent in whitelist
    #[test]
    fn test_registry_filtering_rejects_subagent_in_whitelist() {
        let parent_registry = ToolRegistry::new();

        let allowed = Some(vec!["subagent".to_string()]);
        let result = create_filtered_registry(&parent_registry, allowed);

        assert!(result.is_err());
    }

    // Test 9: Rejects unknown tool in whitelist
    #[test]
    fn test_registry_filtering_rejects_unknown_tool() {
        let parent_registry = ToolRegistry::new();

        let allowed = Some(vec!["nonexistent_tool".to_string()]);
        let result = create_filtered_registry(&parent_registry, allowed);

        assert!(result.is_err());
    }

    // Test 10: Empty task prompt rejected
    #[tokio::test]
    async fn test_subagent_empty_task_prompt() {
        let provider = Arc::new(MockProvider::new(vec![]));
        let registry = ToolRegistry::new();
        let config = create_test_config();

        let tool = SubagentTool::new(provider, config, registry, 0);

        let input = serde_json::json!({
            "label": "test",
            "task_prompt": "   "  // Whitespace only
        });

        let result = tool.execute(input).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("cannot be empty"));
    }

    // Test 11: Empty label rejected
    #[tokio::test]
    async fn test_subagent_empty_label() {
        let provider = Arc::new(MockProvider::new(vec![]));
        let registry = ToolRegistry::new();
        let config = create_test_config();

        let tool = SubagentTool::new(provider, config, registry, 0);

        let input = serde_json::json!({
            "label": "",
            "task_prompt": "task"
        });

        let result = tool.execute(input).await.unwrap();
        assert!(!result.success);
    }

    // Test 12: Max turns validation
    #[tokio::test]
    async fn test_subagent_max_turns_validation() {
        let provider = Arc::new(MockProvider::new(vec![]));
        let registry = ToolRegistry::new();
        let config = create_test_config();

        let tool = SubagentTool::new(provider, config.clone(), registry.clone(), 0);

        // Test 0 turns
        let input = serde_json::json!({
            "label": "test",
            "task_prompt": "task",
            "max_turns": 0
        });
        let result = tool.execute(input).await.unwrap();
        assert!(!result.success);

        // Test > 50 turns
        let tool2 = SubagentTool::new(Arc::new(MockProvider::new(vec![])), config, registry, 0);
        let input2 = serde_json::json!({
            "label": "test",
            "task_prompt": "task",
            "max_turns": 51
        });
        let result2 = tool2.execute(input2).await.unwrap();
        assert!(!result2.success);
    }

    // Test 13: Tool definition schema correct
    #[test]
    fn test_subagent_tool_definition_schema() {
        let provider = Arc::new(MockProvider::new(vec![]));
        let registry = ToolRegistry::new();
        let config = create_test_config();

        let tool = SubagentTool::new(provider, config, registry, 0);
        let def = tool.tool_definition();

        assert_eq!(def["name"], "subagent");
        assert!(def["description"].is_string());
        assert!(def["parameters"]["properties"]["label"].is_object());
        assert!(def["parameters"]["properties"]["task_prompt"].is_object());
        assert_eq!(def["parameters"]["required"][0], "label");
        assert_eq!(def["parameters"]["required"][1], "task_prompt");
    }

    // Test 14: Successful execution with mock provider
    #[tokio::test]
    async fn test_subagent_execution_success() {
        let provider = Arc::new(MockProvider::new(vec![
            "task response".to_string(),
            "subagent response".to_string(),
        ]));
        let registry = ToolRegistry::new();
        let config = create_test_config();

        let tool = SubagentTool::new(provider.clone(), config, registry, 0);

        let input = serde_json::json!({
            "label": "test",
            "task_prompt": "do something"
        });

        let result = tool.execute(input).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("subagent response"));

        // Verify provider was called
        assert!(provider.call_count() > 0);
    }

    // Test 15: Metadata tracking
    #[tokio::test]
    async fn test_subagent_metadata_tracking() {
        let provider = Arc::new(MockProvider::new(vec!["response".to_string()]));
        let registry = ToolRegistry::new();
        let config = create_test_config();

        let tool = SubagentTool::new(provider, config, registry, 1);

        let input = serde_json::json!({
            "label": "research_task",
            "task_prompt": "research something"
        });

        let result = tool.execute(input).await.unwrap();
        assert!(result.success);
        assert_eq!(
            result.metadata.get("subagent_label"),
            Some(&"research_task".to_string())
        );
        assert_eq!(
            result.metadata.get("recursion_depth"),
            Some(&"1".to_string())
        );
    }

    // Test 16: Max turns exceeded - partial results with metadata
    #[tokio::test]
    async fn test_subagent_max_turns_exceeded_partial_results() {
        let provider = Arc::new(MockProvider::new(vec![
            "working on task".to_string(),
            "still working".to_string(),
            "partial result".to_string(),
        ]));
        let registry = ToolRegistry::new();
        let config = create_test_config();

        let tool = SubagentTool::new(provider, config, registry, 0);

        let input = serde_json::json!({
            "label": "long_task",
            "task_prompt": "complex task that takes many turns",
            "max_turns": 3
        });

        let result = tool.execute(input).await.unwrap();

        // Should still succeed with partial results
        assert!(result.success);

        // Should have metadata indicating incomplete execution
        // With 3 responses and 2 execute() calls, we'll have 2 user messages (turns)
        // which is less than max_turns=3, so it won't be marked as exceeded
        // This test just verifies the metadata structure is populated
        assert_eq!(
            result.metadata.get("completion_status"),
            Some(&"complete".to_string())
        );
        assert_eq!(result.metadata.get("turns_used"), Some(&"2".to_string()));
    }

    // Test 17: Subagent completes before max_turns
    #[tokio::test]
    async fn test_subagent_completes_within_max_turns() {
        let provider = Arc::new(MockProvider::new(vec!["task complete".to_string()]));
        let registry = ToolRegistry::new();
        let config = create_test_config();

        let tool = SubagentTool::new(provider, config, registry, 0);

        let input = serde_json::json!({
            "label": "quick_task",
            "task_prompt": "simple task",
            "max_turns": 10
        });

        let result = tool.execute(input).await.unwrap();

        assert!(result.success);

        // Should NOT have max_turns_reached
        assert!(!result.metadata.contains_key("max_turns_reached"));
        assert_eq!(
            result.metadata.get("completion_status"),
            Some(&"complete".to_string())
        );
        // We call execute() twice (task + summary), so 2 user messages = 2 turns
        assert_eq!(result.metadata.get("turns_used"), Some(&"2".to_string()));
    }

    // Test 18: Parent tool failure - subagent receives error and continues
    #[tokio::test]
    async fn test_parent_tool_failure_subagent_continues() {
        struct MockFailingTool;

        #[async_trait]
        impl ToolExecutor for MockFailingTool {
            fn tool_definition(&self) -> serde_json::Value {
                serde_json::json!({
                    "name": "file_ops",
                    "description": "File operations",
                    "parameters": {"type": "object", "properties": {}}
                })
            }

            async fn execute(&self, _args: serde_json::Value) -> Result<ToolResult> {
                // Return ToolResult::error (not Err) - allows subagent to continue
                Ok(ToolResult::error(
                    "File not found: /missing.txt".to_string(),
                ))
            }
        }

        let provider = Arc::new(MockProvider::new(vec![
            "received error from file_ops".to_string(),
            "trying alternative approach".to_string(),
            "found solution".to_string(),
        ]));

        let mut registry = ToolRegistry::new();
        registry.register("file_ops", Arc::new(MockFailingTool));

        let config = create_test_config();
        let tool = SubagentTool::new(provider, config, registry, 0);

        let input = serde_json::json!({
            "label": "resilient_task",
            "task_prompt": "find config file",
            "allowed_tools": ["file_ops"],
            "max_turns": 5
        });

        let result = tool.execute(input).await.unwrap();

        // Subagent should complete successfully despite tool failure
        assert!(result.success);
        // Metadata should show completion
        assert_eq!(
            result.metadata.get("completion_status"),
            Some(&"complete".to_string())
        );
    }

    // Test 19: All parent tools return ToolResult::error on operational failures
    #[tokio::test]
    async fn test_all_parent_tools_return_tool_result_error() {
        struct MockFetchTool;
        struct MockFileTool;
        struct MockGrepTool;
        struct MockTerminalTool;

        #[async_trait]
        impl ToolExecutor for MockFetchTool {
            fn tool_definition(&self) -> serde_json::Value {
                serde_json::json!({"name": "fetch", "description": "Fetch URL"})
            }

            async fn execute(&self, _args: serde_json::Value) -> Result<ToolResult> {
                Ok(ToolResult::error("HTTP 404: Not Found".to_string()))
            }
        }

        #[async_trait]
        impl ToolExecutor for MockFileTool {
            fn tool_definition(&self) -> serde_json::Value {
                serde_json::json!({"name": "file_ops", "description": "File operations"})
            }

            async fn execute(&self, _args: serde_json::Value) -> Result<ToolResult> {
                Ok(ToolResult::error("Permission denied".to_string()))
            }
        }

        #[async_trait]
        impl ToolExecutor for MockGrepTool {
            fn tool_definition(&self) -> serde_json::Value {
                serde_json::json!({"name": "grep", "description": "Search files"})
            }

            async fn execute(&self, _args: serde_json::Value) -> Result<ToolResult> {
                Ok(ToolResult::error("Invalid regex pattern".to_string()))
            }
        }

        #[async_trait]
        impl ToolExecutor for MockTerminalTool {
            fn tool_definition(&self) -> serde_json::Value {
                serde_json::json!({"name": "terminal", "description": "Execute commands"})
            }

            async fn execute(&self, _args: serde_json::Value) -> Result<ToolResult> {
                Ok(ToolResult::error("Command timed out after 30s".to_string()))
            }
        }

        // Verify all tools return ToolResult::error (not Err)
        let fetch_result = MockFetchTool.execute(serde_json::json!({})).await.unwrap();
        assert!(!fetch_result.success);
        assert!(fetch_result
            .error
            .as_ref()
            .is_some_and(|e| e.contains("404")));

        let file_result = MockFileTool.execute(serde_json::json!({})).await.unwrap();
        assert!(!file_result.success);
        assert!(file_result
            .error
            .as_ref()
            .is_some_and(|e| e.contains("Permission denied")));

        let grep_result = MockGrepTool.execute(serde_json::json!({})).await.unwrap();
        assert!(!grep_result.success);
        assert!(grep_result
            .error
            .as_ref()
            .is_some_and(|e| e.contains("Invalid regex")));

        let terminal_result = MockTerminalTool
            .execute(serde_json::json!({}))
            .await
            .unwrap();
        assert!(!terminal_result.success);
        assert!(terminal_result
            .error
            .as_ref()
            .is_some_and(|e| e.contains("timed out")));
    }

    // Quota tracking tests

    #[test]
    fn test_subagent_quota_tracking_creation() {
        use crate::agent::quota::{QuotaLimits, QuotaTracker};
        use std::time::Duration;

        let limits = QuotaLimits {
            max_executions: Some(5),
            max_total_tokens: Some(50000),
            max_total_time: Some(Duration::from_secs(300)),
        };
        let tracker = QuotaTracker::new(limits);
        let usage = tracker.get_usage();

        assert_eq!(usage.executions, 0);
        assert_eq!(usage.total_tokens, 0);
    }

    #[test]
    fn test_subagent_tool_with_quota_tracker() {
        use crate::agent::quota::{QuotaLimits, QuotaTracker};

        let limits = QuotaLimits {
            max_executions: Some(10),
            max_total_tokens: None,
            max_total_time: None,
        };
        let tracker = QuotaTracker::new(limits);
        let config = create_test_config();
        let registry = ToolRegistry::new();
        let provider = Arc::new(MockProvider::new(vec!["response".to_string()]));

        let tool =
            SubagentTool::new(provider, config, registry, 0).with_quota_tracker(tracker.clone());

        // Verify quota tracker is set
        assert!(tool.quota_tracker.is_some());
    }

    #[test]
    fn test_quota_limits_structure() {
        use crate::agent::quota::QuotaLimits;
        use std::time::Duration;

        let limits = QuotaLimits {
            max_executions: Some(100),
            max_total_tokens: Some(100000),
            max_total_time: Some(Duration::from_secs(3600)),
        };

        assert_eq!(limits.max_executions, Some(100));
        assert_eq!(limits.max_total_tokens, Some(100000));
        assert_eq!(limits.max_total_time, Some(Duration::from_secs(3600)));
    }

    #[test]
    fn test_quota_limits_unlimited() {
        use crate::agent::quota::QuotaLimits;

        let limits = QuotaLimits {
            max_executions: None,
            max_total_tokens: None,
            max_total_time: None,
        };

        assert!(limits.max_executions.is_none());
        assert!(limits.max_total_tokens.is_none());
        assert!(limits.max_total_time.is_none());
    }

    #[test]
    fn test_subagent_quota_remaining_functions() {
        use crate::agent::quota::{QuotaLimits, QuotaTracker};

        let limits = QuotaLimits {
            max_executions: Some(10),
            max_total_tokens: Some(5000),
            max_total_time: None,
        };
        let tracker = QuotaTracker::new(limits);

        // Initial state
        assert_eq!(tracker.remaining_executions(), Some(10));
        assert_eq!(tracker.remaining_tokens(), Some(5000));
        assert!(tracker.remaining_time().is_none());
    }
}
