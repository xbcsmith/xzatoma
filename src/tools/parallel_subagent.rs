//! Parallel subagent execution for independent tasks
//!
//! This module provides infrastructure for executing multiple independent
//! subagent tasks concurrently with configurable concurrency limits,
//! fail-fast behavior, and comprehensive result aggregation.

use crate::agent::{quota::QuotaTracker, Agent, SubagentMetrics};
use crate::config::AgentConfig;
use crate::error::Result;
use crate::providers::Provider;
use crate::tools::{ToolExecutor, ToolRegistry, ToolResult};
use async_trait::async_trait;
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info, warn};

/// Input specification for parallel subagent execution
///
/// Defines a batch of independent tasks to execute concurrently,
/// along with execution parameters like concurrency limits and fail-fast behavior.
#[derive(Debug, Clone, Deserialize)]
pub struct ParallelSubagentInput {
    /// Tasks to execute in parallel
    pub tasks: Vec<ParallelTask>,

    /// Maximum concurrent executions (default: 5)
    #[serde(default)]
    pub max_concurrent: Option<usize>,

    /// Fail fast on first error (default: false)
    #[serde(default)]
    pub fail_fast: Option<bool>,
}

/// Individual task specification for parallel execution
///
/// Defines a single task to be executed by a subagent,
/// including the task prompt, optional summary prompt, and resource limits.
#[derive(Debug, Clone, Deserialize)]
pub struct ParallelTask {
    /// Task identifier (must be unique within a batch)
    pub label: String,

    /// Task description/prompt for the subagent
    pub task_prompt: String,

    /// Optional prompt for summarizing task results
    #[serde(default)]
    pub summary_prompt: Option<String>,

    /// Optional list of allowed tools (for security isolation)
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,

    /// Optional maximum conversation turns
    #[serde(default)]
    pub max_turns: Option<usize>,
}

/// Results from parallel subagent execution
///
/// Contains the results of all executed tasks along with
/// aggregate metrics about the execution.
#[derive(Debug, Clone, Serialize)]
pub struct ParallelSubagentOutput {
    /// Results from all executed tasks
    pub results: Vec<TaskResult>,

    /// Total time for entire parallel execution (milliseconds)
    pub total_duration_ms: u64,

    /// Number of successfully completed tasks
    pub successful: usize,

    /// Number of failed tasks
    pub failed: usize,
}

/// Result from a single parallel task execution
///
/// Contains the outcome of executing a single task within
/// a parallel batch, including timing and error information.
#[derive(Debug, Clone, Serialize)]
pub struct TaskResult {
    /// Task identifier (matches ParallelTask::label)
    pub label: String,

    /// True if task completed successfully
    pub success: bool,

    /// Task output (content if successful, empty if failed)
    pub output: String,

    /// Time taken for this task (milliseconds)
    pub duration_ms: u64,

    /// Error message if task failed (None if successful)
    pub error: Option<String>,

    /// Tokens consumed by this task (0 if unavailable)
    #[serde(default)]
    pub tokens_used: usize,
}

/// Parallel subagent execution tool
///
/// Executes multiple independent tasks concurrently, managing
/// concurrency limits and collecting results.
pub struct ParallelSubagentTool {
    /// AI provider for creating subagents
    provider: Arc<dyn Provider>,

    /// Agent configuration for subagent creation
    config: AgentConfig,

    /// Parent tool registry for filtering
    parent_registry: Arc<ToolRegistry>,

    /// Current recursion depth
    current_depth: usize,

    /// Optional quota tracker for resource management
    ///
    /// When Some, enforces limits on execution count, total tokens,
    /// and wall-clock time across all parallel task executions.
    /// When None, no resource limits are enforced.
    quota_tracker: Option<QuotaTracker>,
}

impl ParallelSubagentTool {
    /// Creates a new parallel subagent tool
    ///
    /// # Arguments
    ///
    /// * `provider` - AI provider for subagent execution
    /// * `config` - Agent configuration
    /// * `parent_registry` - Parent tool registry for filtering
    /// * `current_depth` - Current recursion depth
    ///
    /// # Returns
    ///
    /// A new `ParallelSubagentTool` instance
    pub fn new(
        provider: Arc<dyn Provider>,
        config: AgentConfig,
        parent_registry: Arc<ToolRegistry>,
        current_depth: usize,
    ) -> Self {
        Self {
            provider,
            config,
            parent_registry,
            current_depth,
            quota_tracker: None,
        }
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

#[async_trait]
impl ToolExecutor for ParallelSubagentTool {
    fn tool_definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "parallel_subagent",
                "description": "Execute multiple independent tasks in parallel using subagents. Each task runs concurrently with configurable limits.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "tasks": {
                            "type": "array",
                            "description": "List of tasks to execute in parallel",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "label": {
                                        "type": "string",
                                        "description": "Unique identifier for this task"
                                    },
                                    "task_prompt": {
                                        "type": "string",
                                        "description": "The task description/prompt for the subagent"
                                    },
                                    "summary_prompt": {
                                        "type": "string",
                                        "description": "Optional prompt for summarizing results"
                                    },
                                    "allowed_tools": {
                                        "type": "array",
                                        "items": { "type": "string" },
                                        "description": "Optional list of tools this task can use (for security)"
                                    },
                                    "max_turns": {
                                        "type": "number",
                                        "description": "Optional maximum conversation turns for this task"
                                    }
                                },
                                "required": ["label", "task_prompt"]
                            }
                        },
                        "max_concurrent": {
                            "type": "number",
                            "description": "Maximum concurrent executions (default: 5)"
                        },
                        "fail_fast": {
                            "type": "boolean",
                            "description": "Stop on first error (default: false)"
                        }
                    },
                    "required": ["tasks"]
                }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        // Create metrics tracker for parallel batch execution
        let batch_metrics = SubagentMetrics::new("parallel_batch".to_string(), self.current_depth);

        // Check quota availability before starting parallel execution
        if let Some(quota_tracker) = &self.quota_tracker {
            if let Err(e) = quota_tracker.check_and_reserve() {
                warn!(
                    parallel.event = "quota_exceeded",
                    parallel.error = %e,
                    "Parallel execution quota exceeded"
                );
                batch_metrics.record_error("quota_exceeded");
                return Ok(ToolResult::error(format!("Resource quota exceeded: {}", e)));
            }
        }

        // Parse input
        let input: ParallelSubagentInput = match serde_json::from_value(args) {
            Ok(input) => input,
            Err(e) => {
                batch_metrics.record_error("invalid_input");
                return Ok(ToolResult::error(format!("Invalid input: {}", e)));
            }
        };

        // Validate input
        if input.tasks.is_empty() {
            batch_metrics.record_error("empty_tasks");
            return Ok(ToolResult::error("At least one task required"));
        }

        // Check depth limit
        if self.current_depth >= self.config.subagent.max_depth {
            batch_metrics.record_error("max_depth_reached");
            return Ok(ToolResult::error(format!(
                "Maximum subagent depth {} reached",
                self.config.subagent.max_depth
            )));
        }

        let max_concurrent = input.max_concurrent.unwrap_or(5);
        let fail_fast = input.fail_fast.unwrap_or(false);
        let task_count = input.tasks.len();

        info!(
            parallel.event = "start",
            parallel.task_count = task_count,
            parallel.max_concurrent = max_concurrent,
            parallel.fail_fast = fail_fast,
            "Starting parallel subagent execution"
        );

        let start = Instant::now();

        // Create semaphore for concurrency control
        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent));

        // Spawn all tasks
        let task_handles: Vec<_> = input
            .tasks
            .into_iter()
            .map(|task| {
                let provider = Arc::clone(&self.provider);
                let config = self.config.clone();
                let parent_registry = Arc::clone(&self.parent_registry);
                let current_depth = self.current_depth;
                let sem = Arc::clone(&semaphore);

                tokio::spawn(async move {
                    let _permit = match sem.acquire().await {
                        Ok(p) => p,
                        Err(e) => {
                            error!("Failed to acquire semaphore permit: {}", e);
                            panic!("Semaphore closed");
                        }
                    };

                    execute_task(task, provider, config, parent_registry, current_depth).await
                })
            })
            .collect();

        // Collect results
        let mut results = Vec::new();
        let mut successful = 0;
        let mut failed = 0;

        for (idx, task_result) in join_all(task_handles).await.into_iter().enumerate() {
            match task_result {
                Ok(result) => {
                    if result.success {
                        successful += 1;
                    } else {
                        failed += 1;
                        if fail_fast {
                            warn!(
                                parallel.event = "fail_fast",
                                parallel.task_index = idx,
                                parallel.task_label = &result.label,
                                "Parallel execution stopped on error"
                            );
                            break;
                        }
                    }
                    results.push(result);
                }
                Err(e) => {
                    warn!(
                        parallel.event = "task_panic",
                        parallel.task_index = idx,
                        parallel.error = %e,
                        "Task panicked"
                    );
                    failed += 1;
                    if fail_fast {
                        break;
                    }
                }
            }
        }

        let total_duration_ms = start.elapsed().as_millis() as u64;

        // Calculate total tokens consumed across all tasks
        let total_tokens: usize = results.iter().map(|r| r.tokens_used).sum();

        // Record metrics for parallel batch completion
        let batch_status = if failed == 0 && successful > 0 {
            "complete"
        } else if successful > 0 {
            "partial"
        } else {
            "failed"
        };
        // Use total successful tasks as a proxy for "turns" in the batch context
        batch_metrics.record_completion(successful, total_tokens, batch_status);

        // Record quota usage if tracker available
        if let Some(quota_tracker) = &self.quota_tracker {
            if let Err(e) = quota_tracker.record_execution(total_tokens) {
                warn!(
                    parallel.event = "quota_recording_failed",
                    parallel.error = %e,
                    "Failed to record quota usage"
                );
                // Log warning but don't fail the execution
                // All tasks completed successfully
            }
        }

        info!(
            parallel.event = "complete",
            parallel.successful = successful,
            parallel.failed = failed,
            parallel.total_tokens = total_tokens,
            parallel.duration_ms = total_duration_ms,
            "Parallel execution complete"
        );

        let output = ParallelSubagentOutput {
            results,
            total_duration_ms,
            successful,
            failed,
        };

        match serde_json::to_string(&output) {
            Ok(json) => Ok(ToolResult::success(json)),
            Err(e) => Ok(ToolResult::error(format!(
                "Failed to serialize output: {}",
                e
            ))),
        }
    }
}

/// Executes a single task as a subagent
///
/// Creates a new subagent with the specified configuration and
/// executes the task prompt, optionally summarizing the result.
async fn execute_task(
    task: ParallelTask,
    provider: Arc<dyn Provider>,
    config: AgentConfig,
    parent_registry: Arc<ToolRegistry>,
    _current_depth: usize,
) -> TaskResult {
    let start = Instant::now();
    let label = task.label.clone();

    // Create filtered registry
    let registry = if let Some(allowed_tools) = &task.allowed_tools {
        create_filtered_registry(&parent_registry, allowed_tools)
    } else {
        // Exclude parallel_subagent from subagent's registry
        parent_registry.clone_without_parallel()
    };

    // Create subagent
    let mut agent =
        match Agent::new_from_shared_provider(Arc::clone(&provider), registry, config.clone()) {
            Ok(agent) => agent,
            Err(e) => {
                error!("Failed to create subagent for {}: {}", label, e);
                return TaskResult {
                    label,
                    success: false,
                    output: String::new(),
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: Some(format!("Failed to create subagent: {}", e)),
                    tokens_used: 0,
                };
            }
        };

    // Execute task
    match agent.execute(&task.task_prompt).await {
        Ok(output) => {
            // Get summary if requested
            let final_output = if let Some(summary_prompt) = task.summary_prompt {
                match agent.execute(&summary_prompt).await {
                    Ok(summary) => summary,
                    Err(e) => {
                        warn!(
                            parallel_task.label = &label,
                            parallel_task.error = "summary_failed",
                            "Failed to generate summary: {}",
                            e
                        );
                        output
                    }
                }
            } else {
                output
            };

            // Capture token usage
            let tokens_used = agent.get_token_usage().map(|u| u.total_tokens).unwrap_or(0);

            TaskResult {
                label,
                success: true,
                output: final_output,
                duration_ms: start.elapsed().as_millis() as u64,
                error: None,
                tokens_used,
            }
        }
        Err(e) => {
            let error_msg = format!("{}", e);
            error!(
                parallel_task.label = &label,
                parallel_task.error = %e,
                "Task execution failed"
            );

            TaskResult {
                label,
                success: false,
                output: String::new(),
                duration_ms: start.elapsed().as_millis() as u64,
                error: Some(error_msg),
                tokens_used: 0,
            }
        }
    }
}

/// Creates a filtered registry with only allowed tools
fn create_filtered_registry(parent: &ToolRegistry, allowed: &[String]) -> ToolRegistry {
    parent.clone_with_filter(allowed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parallel_task_creation() {
        let task = ParallelTask {
            label: "test_task".to_string(),
            task_prompt: "Do something".to_string(),
            summary_prompt: None,
            allowed_tools: None,
            max_turns: None,
        };

        assert_eq!(task.label, "test_task");
        assert_eq!(task.task_prompt, "Do something");
    }

    #[test]
    fn test_parallel_task_with_options() {
        let task = ParallelTask {
            label: "advanced_task".to_string(),
            task_prompt: "Complex task".to_string(),
            summary_prompt: Some("Summarize".to_string()),
            allowed_tools: Some(vec!["read_file".to_string(), "terminal".to_string()]),
            max_turns: Some(10),
        };

        assert!(task.summary_prompt.is_some());
        assert_eq!(task.allowed_tools.as_ref().unwrap().len(), 2);
        assert_eq!(task.max_turns, Some(10));
    }

    #[test]
    fn test_task_result_success() {
        let result = TaskResult {
            label: "task1".to_string(),
            success: true,
            output: "Success output".to_string(),
            duration_ms: 100,
            error: None,
            tokens_used: 150,
        };

        assert!(result.success);
        assert_eq!(result.output, "Success output");
        assert!(result.error.is_none());
        assert_eq!(result.tokens_used, 150);
    }

    #[test]
    fn test_task_result_failure() {
        let result = TaskResult {
            label: "task1".to_string(),
            success: false,
            output: String::new(),
            duration_ms: 100,
            error: Some("Task failed".to_string()),
            tokens_used: 0,
        };

        assert!(!result.success);
        assert!(result.error.is_some());
        assert_eq!(result.tokens_used, 0);
    }

    #[test]
    fn test_parallel_subagent_output_serialization() {
        let output = ParallelSubagentOutput {
            results: vec![TaskResult {
                label: "task1".to_string(),
                success: true,
                output: "Result".to_string(),
                duration_ms: 100,
                error: None,
                tokens_used: 200,
            }],
            total_duration_ms: 150,
            successful: 1,
            failed: 0,
        };

        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("task1"));
        assert!(json.contains("150"));
        assert!(json.contains("200"));
    }

    #[test]
    fn test_parallel_subagent_input_deserialization() {
        let json = serde_json::json!({
            "tasks": [
                {
                    "label": "task1",
                    "task_prompt": "Do something"
                }
            ],
            "max_concurrent": 2,
            "fail_fast": true
        });

        let input: ParallelSubagentInput = serde_json::from_value(json).unwrap();
        assert_eq!(input.tasks.len(), 1);
        assert_eq!(input.max_concurrent, Some(2));
        assert_eq!(input.fail_fast, Some(true));
    }

    #[test]
    fn test_parallel_subagent_input_defaults() {
        let json = serde_json::json!({
            "tasks": [
                {
                    "label": "task1",
                    "task_prompt": "Do something"
                }
            ]
        });

        let input: ParallelSubagentInput = serde_json::from_value(json).unwrap();
        assert_eq!(input.max_concurrent, None);
        assert_eq!(input.fail_fast, None);
    }

    #[test]
    fn test_task_result_serialization() {
        let result = TaskResult {
            label: "task1".to_string(),
            success: true,
            output: "Output data".to_string(),
            duration_ms: 250,
            error: None,
            tokens_used: 350,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("task1"));
        assert!(json.contains("250"));
        assert!(json.contains("350"));
    }

    #[test]
    fn test_parallel_input_with_allowed_tools() {
        let json = serde_json::json!({
            "tasks": [
                {
                    "label": "task1",
                    "task_prompt": "Do something",
                    "allowed_tools": ["read_file", "fetch"]
                }
            ]
        });

        let input: ParallelSubagentInput = serde_json::from_value(json).unwrap();
        assert_eq!(input.tasks[0].allowed_tools.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_parallel_output_stats() {
        let output = ParallelSubagentOutput {
            results: vec![
                TaskResult {
                    label: "task1".to_string(),
                    success: true,
                    output: "Done".to_string(),
                    duration_ms: 100,
                    error: None,
                    tokens_used: 250,
                },
                TaskResult {
                    label: "task2".to_string(),
                    success: false,
                    output: String::new(),
                    duration_ms: 50,
                    error: Some("Failed".to_string()),
                    tokens_used: 0,
                },
            ],
            total_duration_ms: 150,
            successful: 1,
            failed: 1,
        };

        assert_eq!(output.successful, 1);
        assert_eq!(output.failed, 1);
        assert_eq!(output.results.len(), 2);

        // Verify token aggregation
        let total_tokens: usize = output.results.iter().map(|r| r.tokens_used).sum();
        assert_eq!(total_tokens, 250);
    }
}
