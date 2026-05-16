//! MCP task lifecycle manager.
//!
//! This module provides [`TaskManager`], which tracks the lifecycle of
//! long-running MCP tasks created via the `tasks` capability introduced in
//! protocol revision `2025-11-25`.
//!
//! A task is created when a server responds to `tools/call` with a
//! `_meta.taskId` field instead of an immediate result. The task manager
//! records state updates delivered via `notifications/tasks/status`
//! notifications and exposes query methods for the current state of each
//! tracked task.
//!
//! # Scope
//!
//! This module provides task registration, state tracking, and removal.
//! Polling `tasks/get` and delivering final results to waiting callers via an
//! async channel requires notification-based wiring that is not yet active.
//! When a `_meta.taskId` is detected in a tool response,
//! [`crate::mcp::manager::McpClientManager::call_tool_as_task`] returns
//! [`crate::error::XzatomaError::McpTask`] so callers receive a stable typed
//! error instead of a partial result.
//!
//! # API
//!
//! ```text
//! manager.register_task(server_id, task_id, ttl)
//! manager.update_task_state(server_id, task_id, new_state)
//! manager.task_state(server_id, task_id) -> Option<&TaskLifecycleState>
//! manager.remove_task(server_id, task_id) -> bool
//! manager.active_task_count() -> usize
//! ```

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// TaskEntry -- internal state for a single in-flight task
// ---------------------------------------------------------------------------

/// Lifecycle state of a single MCP task.
///
/// Mirrors the `TaskStatus` values defined in the MCP `2025-11-25`
/// specification. These states mirror the `TaskStatus` values in the MCP
/// `2025-11-25` specification.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskLifecycleState {
    /// The task has been submitted and the server is working on it.
    Working,
    /// The server is waiting for additional input from the client.
    InputRequired,
    /// The task has finished successfully.
    Completed,
    /// The task ended with a failure.
    Failed,
    /// The task was cancelled before completion.
    Cancelled,
}

/// Internal record for a single tracked task.
///
/// A future iteration could add a `oneshot::Sender<CallToolResponse>` to
/// deliver completion results across async boundaries.
#[derive(Debug)]
#[allow(dead_code)]
struct TaskEntry {
    /// Identifier of the MCP server that owns this task.
    server_id: String,
    /// Opaque task identifier returned by the server.
    task_id: String,
    /// Current lifecycle state.
    state: TaskLifecycleState,
    /// Optional time-to-live in seconds, as requested by the caller.
    ttl: Option<u64>,
}

// ---------------------------------------------------------------------------
// TaskManager
// ---------------------------------------------------------------------------

/// Tracks in-flight MCP tasks and delivers their results to waiters.
///
/// The struct is `Default`-constructible so that
/// [`crate::mcp::manager::McpClientManager`] can hold a shared instance.
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::task_manager::TaskManager;
///
/// let manager = TaskManager::default();
/// assert_eq!(manager.active_task_count(), 0);
/// ```
#[derive(Debug, Default)]
pub struct TaskManager {
    /// In-flight tasks keyed by `"<server_id>/<task_id>"`.
    tasks: HashMap<String, TaskEntry>,
}

impl TaskManager {
    /// Create a new, empty [`TaskManager`].
    ///
    /// Equivalent to [`Default::default`].
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::mcp::task_manager::TaskManager;
    ///
    /// let manager = TaskManager::new();
    /// assert_eq!(manager.active_task_count(), 0);
    /// ```
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
        }
    }

    /// Return the number of tasks currently being tracked.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::mcp::task_manager::TaskManager;
    ///
    /// let manager = TaskManager::new();
    /// assert_eq!(manager.active_task_count(), 0);
    /// ```
    pub fn active_task_count(&self) -> usize {
        self.tasks.len()
    }

    /// Register a newly created task with the manager.
    ///
    /// Called by [`crate::mcp::manager::McpClientManager::call_tool_as_task`]
    /// when the server response contains a `_meta.taskId` field.
    ///
    /// # Arguments
    ///
    /// * `server_id` - Identifier of the MCP server that created the task.
    /// * `task_id` - Opaque task identifier returned by the server.
    /// * `ttl` - Optional time-to-live in seconds for the task.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::mcp::task_manager::TaskManager;
    ///
    /// let mut manager = TaskManager::new();
    /// manager.register_task("my-server", "task-001", Some(60));
    /// assert_eq!(manager.active_task_count(), 1);
    /// ```
    pub fn register_task(&mut self, server_id: &str, task_id: &str, ttl: Option<u64>) {
        let key = Self::task_key(server_id, task_id);
        self.tasks.insert(
            key,
            TaskEntry {
                server_id: server_id.to_string(),
                task_id: task_id.to_string(),
                state: TaskLifecycleState::Working,
                ttl,
            },
        );
    }

    /// Update the state of a tracked task.
    ///
    /// Called when a `notifications/tasks/status` notification is received.
    /// A future iteration can add waiter wake-up logic here.
    ///
    /// # Arguments
    ///
    /// * `server_id` - Identifier of the MCP server.
    /// * `task_id` - Task identifier.
    /// * `new_state` - The new lifecycle state.
    ///
    /// # Returns
    ///
    /// `true` if the task was found and updated, `false` if unknown.
    pub fn update_task_state(
        &mut self,
        server_id: &str,
        task_id: &str,
        new_state: TaskLifecycleState,
    ) -> bool {
        let key = Self::task_key(server_id, task_id);
        if let Some(entry) = self.tasks.get_mut(&key) {
            entry.state = new_state;
            true
        } else {
            false
        }
    }

    /// Remove a task from the manager.
    ///
    /// Called when a task reaches a terminal state and its result has been
    /// delivered to the caller.
    ///
    /// # Arguments
    ///
    /// * `server_id` - Identifier of the MCP server.
    /// * `task_id` - Task identifier.
    ///
    /// # Returns
    ///
    /// `true` if the task was present and removed, `false` if not found.
    pub fn remove_task(&mut self, server_id: &str, task_id: &str) -> bool {
        let key = Self::task_key(server_id, task_id);
        self.tasks.remove(&key).is_some()
    }

    /// Return the current state of a tracked task, if known.
    ///
    /// # Arguments
    ///
    /// * `server_id` - Identifier of the MCP server.
    /// * `task_id` - Task identifier.
    ///
    /// # Returns
    ///
    /// `Some(&TaskLifecycleState)` when the task is tracked, `None` otherwise.
    pub fn task_state(&self, server_id: &str, task_id: &str) -> Option<&TaskLifecycleState> {
        let key = Self::task_key(server_id, task_id);
        self.tasks.get(&key).map(|e| &e.state)
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Build the composite map key `"<server_id>/<task_id>"`.
    fn task_key(server_id: &str, task_id: &str) -> String {
        format!("{}/{}", server_id, task_id)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_manager_is_empty() {
        let manager = TaskManager::new();
        assert_eq!(manager.active_task_count(), 0);
    }

    #[test]
    fn test_default_manager_is_empty() {
        let manager = TaskManager::default();
        assert_eq!(manager.active_task_count(), 0);
    }

    #[test]
    fn test_register_task_increments_count() {
        let mut manager = TaskManager::new();
        manager.register_task("server-a", "t1", None);
        assert_eq!(manager.active_task_count(), 1);
    }

    #[test]
    fn test_register_two_tasks_count_is_two() {
        let mut manager = TaskManager::new();
        manager.register_task("server-a", "t1", None);
        manager.register_task("server-a", "t2", Some(30));
        assert_eq!(manager.active_task_count(), 2);
    }

    #[test]
    fn test_register_same_key_twice_does_not_duplicate() {
        let mut manager = TaskManager::new();
        manager.register_task("srv", "t1", None);
        manager.register_task("srv", "t1", Some(60)); // overwrite
        assert_eq!(manager.active_task_count(), 1);
    }

    #[test]
    fn test_task_state_returns_working_after_register() {
        let mut manager = TaskManager::new();
        manager.register_task("srv", "t1", None);
        assert_eq!(
            manager.task_state("srv", "t1"),
            Some(&TaskLifecycleState::Working)
        );
    }

    #[test]
    fn test_task_state_returns_none_for_unknown_task() {
        let manager = TaskManager::new();
        assert!(manager.task_state("srv", "nonexistent").is_none());
    }

    #[test]
    fn test_update_task_state_returns_true_when_found() {
        let mut manager = TaskManager::new();
        manager.register_task("srv", "t1", None);
        let updated = manager.update_task_state("srv", "t1", TaskLifecycleState::Completed);
        assert!(updated);
        assert_eq!(
            manager.task_state("srv", "t1"),
            Some(&TaskLifecycleState::Completed)
        );
    }

    #[test]
    fn test_update_task_state_returns_false_when_not_found() {
        let mut manager = TaskManager::new();
        let updated = manager.update_task_state("srv", "ghost", TaskLifecycleState::Failed);
        assert!(!updated);
    }

    #[test]
    fn test_remove_task_returns_true_when_found() {
        let mut manager = TaskManager::new();
        manager.register_task("srv", "t1", None);
        assert!(manager.remove_task("srv", "t1"));
        assert_eq!(manager.active_task_count(), 0);
    }

    #[test]
    fn test_remove_task_returns_false_when_not_found() {
        let mut manager = TaskManager::new();
        assert!(!manager.remove_task("srv", "nobody"));
    }

    #[test]
    fn test_lifecycle_state_partial_eq() {
        assert_eq!(TaskLifecycleState::Working, TaskLifecycleState::Working);
        assert_ne!(TaskLifecycleState::Working, TaskLifecycleState::Completed);
        assert_ne!(TaskLifecycleState::Failed, TaskLifecycleState::Cancelled);
    }

    #[test]
    fn test_task_key_format() {
        let key = TaskManager::task_key("my-server", "task-42");
        assert_eq!(key, "my-server/task-42");
    }

    #[test]
    fn test_different_servers_same_task_id_are_independent() {
        let mut manager = TaskManager::new();
        manager.register_task("srv-a", "t1", None);
        manager.register_task("srv-b", "t1", None);
        assert_eq!(manager.active_task_count(), 2);

        manager.update_task_state("srv-a", "t1", TaskLifecycleState::Completed);
        assert_eq!(
            manager.task_state("srv-a", "t1"),
            Some(&TaskLifecycleState::Completed)
        );
        // srv-b's t1 must still be Working.
        assert_eq!(
            manager.task_state("srv-b", "t1"),
            Some(&TaskLifecycleState::Working)
        );
    }
}
