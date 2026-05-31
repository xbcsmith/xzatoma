//! Shared plan execution logic for watcher backends.
//!
//! This module provides [`TaskOutcome`] and [`execute_tasks_sequentially`],
//! which are used by both the generic watcher and the XZepr watcher to drive
//! a shared agent session through a task-based [`crate::tools::plan::Plan`]
//! one task at a time.
//!
//! # Per-task execution model
//!
//! When a plan carries a non-empty `tasks` field, the watcher does not send
//! the entire plan to the LLM as a single prompt. Instead it calls
//! [`execute_tasks_sequentially`], which:
//!
//! 1. Resolves task execution order using [`crate::tools::plan::resolve_task_order`]
//!    (topological sort respecting `dependencies`).
//! 2. For each task in order, calls `agent.execute(task.description)`.
//! 3. Records each outcome in a [`TaskOutcome`] regardless of success or failure
//!    (execution is not aborted on first failure).
//! 4. Returns the full `Vec<TaskOutcome>` to the caller.
//!
//! The caller is responsible for deriving an overall `success` flag (e.g.
//! `outcomes.iter().all(|o| o.success)`) and requesting a final summary from
//! the agent.
//!
//! # Examples
//!
//! ```
//! use xzatoma::watcher::plan_executor::TaskOutcome;
//!
//! let outcome = TaskOutcome {
//!     id: "setup".to_string(),
//!     success: true,
//!     summary: "Created tmp directory".to_string(),
//!     iterations: 2,
//! };
//! assert!(outcome.success);
//! assert_eq!(outcome.id, "setup");
//! ```

use crate::error::Result;
use crate::tools::plan::Plan;
use tracing::{info, warn};

/// The outcome of executing a single plan task through the agent.
///
/// Produced by [`execute_tasks_sequentially`] for each task in a task-based
/// plan. On success, `summary` contains the agent's final response text. On
/// failure, `summary` contains a description of the error.
///
/// # Examples
///
/// ```
/// use xzatoma::watcher::plan_executor::TaskOutcome;
///
/// let outcome = TaskOutcome {
///     id: "build".to_string(),
///     success: false,
///     summary: "Task failed: provider timeout".to_string(),
///     iterations: 0,
/// };
/// assert!(!outcome.success);
/// assert!(outcome.summary.contains("timeout"));
/// ```
#[derive(Debug, Clone)]
pub struct TaskOutcome {
    /// The task identifier from [`crate::tools::plan::PlanTask::id`].
    pub id: String,
    /// Whether the agent completed the task without returning an error.
    pub success: bool,
    /// Agent response text on success, or error description on failure.
    pub summary: String,
    /// Number of LLM provider round-trips the agent performed for this task.
    ///
    /// Captured from [`crate::agent::Agent::iteration_count`] immediately after
    /// `agent.execute` returns. Zero if the task failed before the agent loop ran.
    pub iterations: usize,
}

/// Execute all tasks in a plan sequentially within a single shared agent session.
///
/// Tasks are ordered by [`crate::tools::plan::resolve_task_order`] before
/// execution. Each task description is sent to `agent.execute` as an
/// independent user message; the agent retains conversation history between
/// tasks so later tasks can reference outputs from earlier ones.
///
/// On task failure the error is recorded as a [`TaskOutcome`] and execution
/// continues with the next task — the plan is not aborted on first failure.
///
/// # Arguments
///
/// * `plan`  - The plan whose `tasks` field will be executed in order.
/// * `agent` - The already-constructed watcher agent to drive.
///
/// # Returns
///
/// A `Vec<TaskOutcome>` with one entry per task, in execution order.
///
/// # Errors
///
/// Returns `Err` if dependency resolution fails (unknown task ID or cycle
/// detected in `tasks[*].dependencies`).
///
/// # Examples
///
/// ```no_run
/// # use xzatoma::watcher::plan_executor::execute_tasks_sequentially;
/// # use xzatoma::tools::plan::Plan;
/// # async fn example(plan: &Plan, agent: &mut xzatoma::agent::Agent) {
/// let outcomes = execute_tasks_sequentially(plan, agent).await.unwrap();
/// let all_ok = outcomes.iter().all(|o| o.success);
/// println!("All tasks succeeded: {}", all_ok);
/// # }
/// ```
pub async fn execute_tasks_sequentially(
    plan: &Plan,
    agent: &mut crate::agent::Agent,
) -> Result<Vec<TaskOutcome>> {
    let ordered = crate::tools::plan::resolve_task_order(&plan.tasks)?;

    let total = ordered.len();
    let mut outcomes = Vec::with_capacity(total);

    for (idx, task) in ordered.iter().enumerate() {
        info!(
            task_id = %task.id,
            task_index = idx + 1,
            total_tasks = total,
            "Starting task execution"
        );

        let outcome = match agent.execute(task.description.clone()).await {
            Ok(response) => {
                let iterations = agent.iteration_count();
                info!(
                    task_id = %task.id,
                    success = true,
                    iterations,
                    "Task execution complete"
                );
                TaskOutcome {
                    id: task.id.clone(),
                    success: true,
                    summary: response,
                    iterations,
                }
            }
            Err(e) => {
                warn!(
                    task_id = %task.id,
                    error = %e,
                    "Task execution failed; continuing to next task"
                );
                TaskOutcome {
                    id: task.id.clone(),
                    success: false,
                    summary: format!("Task failed: {}", e),
                    iterations: 0,
                }
            }
        };

        outcomes.push(outcome);
    }

    Ok(outcomes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Agent;
    use crate::config::{AgentConfig, WatcherPlanExecutionMode};
    use crate::providers::{CompletionResponse, Message, ModelInfo};
    use crate::tools::plan::PlanParser;
    use crate::tools::ToolRegistry;
    use async_trait::async_trait;

    #[derive(Clone)]
    struct SuccessProvider {
        response: String,
    }

    #[async_trait]
    impl crate::providers::Provider for SuccessProvider {
        fn is_authenticated(&self) -> bool {
            true
        }

        fn current_model(&self) -> Option<&str> {
            Some("test-model")
        }

        fn set_model(&mut self, _model: &str) {}

        async fn fetch_models(&self) -> crate::error::Result<Vec<ModelInfo>> {
            Ok(vec![])
        }

        async fn complete(
            &self,
            _messages: &[Message],
            _tools: &[serde_json::Value],
        ) -> crate::error::Result<CompletionResponse> {
            Ok(CompletionResponse::new(Message::assistant(
                self.response.clone(),
            )))
        }
    }

    #[derive(Clone)]
    struct ErrorProvider;

    #[async_trait]
    impl crate::providers::Provider for ErrorProvider {
        fn is_authenticated(&self) -> bool {
            true
        }

        fn current_model(&self) -> Option<&str> {
            Some("error-model")
        }

        fn set_model(&mut self, _model: &str) {}

        async fn fetch_models(&self) -> crate::error::Result<Vec<ModelInfo>> {
            Ok(vec![])
        }

        async fn complete(
            &self,
            _messages: &[Message],
            _tools: &[serde_json::Value],
        ) -> crate::error::Result<CompletionResponse> {
            Err(crate::error::XzatomaError::Provider(
                "simulated provider error".to_string(),
            ))
        }
    }

    fn make_plan_with_tasks(task_ids: &[&str]) -> Plan {
        if task_ids.is_empty() {
            // A plan with steps (no tasks) passes validation and yields an empty tasks vec.
            return PlanParser::from_yaml(
                "name: test-plan\nsteps:\n  - name: placeholder\n    action: noop\n",
            )
            .unwrap();
        }
        let tasks_yaml: String = task_ids
            .iter()
            .map(|id| format!("  - id: {}\n    description: Execute task {}\n", id, id))
            .collect();
        let yaml = format!("name: test-plan\ntasks:\n{}", tasks_yaml);
        PlanParser::from_yaml(&yaml).unwrap()
    }

    #[test]
    fn test_task_outcome_fields() {
        let outcome = TaskOutcome {
            id: "t1".to_string(),
            success: true,
            summary: "done".to_string(),
            iterations: 3,
        };
        assert_eq!(outcome.id, "t1");
        assert!(outcome.success);
        assert_eq!(outcome.summary, "done");
        assert_eq!(outcome.iterations, 3);
    }

    #[test]
    fn test_task_outcome_failure() {
        let outcome = TaskOutcome {
            id: "t2".to_string(),
            success: false,
            summary: "Task failed: timeout".to_string(),
            iterations: 0,
        };
        assert!(!outcome.success);
        assert!(outcome.summary.contains("timeout"));
        assert_eq!(outcome.iterations, 0);
    }

    #[test]
    fn test_task_outcome_clone() {
        let original = TaskOutcome {
            id: "t1".to_string(),
            success: true,
            summary: "ok".to_string(),
            iterations: 5,
        };
        let cloned = original.clone();
        assert_eq!(original.id, cloned.id);
        assert_eq!(original.success, cloned.success);
        assert_eq!(original.summary, cloned.summary);
        assert_eq!(original.iterations, cloned.iterations);
    }

    #[tokio::test]
    async fn test_execute_tasks_sequentially_all_tasks_succeed() {
        let plan = make_plan_with_tasks(&["t1", "t2", "t3"]);
        let provider = SuccessProvider {
            response: "task done".to_string(),
        };
        let mut agent = Agent::new(provider, ToolRegistry::new(), AgentConfig::default()).unwrap();

        let outcomes = execute_tasks_sequentially(&plan, &mut agent).await.unwrap();

        assert_eq!(outcomes.len(), 3);
        assert_eq!(outcomes[0].id, "t1");
        assert!(outcomes[0].success);
        assert_eq!(outcomes[1].id, "t2");
        assert!(outcomes[1].success);
        assert_eq!(outcomes[2].id, "t3");
        assert!(outcomes[2].success);
    }

    #[tokio::test]
    async fn test_execute_tasks_sequentially_records_failure_and_continues_to_next_task() {
        let plan = make_plan_with_tasks(&["t1", "t2", "t3"]);
        let mut agent =
            Agent::new(ErrorProvider, ToolRegistry::new(), AgentConfig::default()).unwrap();

        let outcomes = execute_tasks_sequentially(&plan, &mut agent).await.unwrap();

        assert_eq!(outcomes.len(), 3);
        assert!(!outcomes[0].success);
        assert!(!outcomes[1].success);
        assert!(!outcomes[2].success);
        assert!(outcomes[0].summary.contains("Task failed"));
        assert_eq!(outcomes[0].iterations, 0);
    }

    #[tokio::test]
    async fn test_execute_tasks_sequentially_empty_plan_returns_empty_outcomes() {
        let plan = make_plan_with_tasks(&[]);
        let provider = SuccessProvider {
            response: "done".to_string(),
        };
        let mut agent = Agent::new(provider, ToolRegistry::new(), AgentConfig::default()).unwrap();

        let outcomes = execute_tasks_sequentially(&plan, &mut agent).await.unwrap();

        assert!(outcomes.is_empty());
    }

    #[test]
    fn test_use_per_task_is_false_when_execution_mode_is_single_shot() {
        let plan = make_plan_with_tasks(&["t1", "t2"]);
        let execution_mode = WatcherPlanExecutionMode::SingleShot;
        let use_per_task =
            matches!(execution_mode, WatcherPlanExecutionMode::PerTask) && !plan.tasks.is_empty();
        assert!(!use_per_task);
    }

    #[test]
    fn test_use_per_task_is_false_when_plan_has_no_tasks() {
        let plan = make_plan_with_tasks(&[]);
        let execution_mode = WatcherPlanExecutionMode::PerTask;
        let use_per_task =
            matches!(execution_mode, WatcherPlanExecutionMode::PerTask) && !plan.tasks.is_empty();
        assert!(!use_per_task);
    }

    #[test]
    fn test_use_per_task_is_true_when_per_task_mode_and_tasks_present() {
        let plan = make_plan_with_tasks(&["t1", "t2"]);
        let execution_mode = WatcherPlanExecutionMode::PerTask;
        let use_per_task =
            matches!(execution_mode, WatcherPlanExecutionMode::PerTask) && !plan.tasks.is_empty();
        assert!(use_per_task);
    }
}
