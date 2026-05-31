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
}
