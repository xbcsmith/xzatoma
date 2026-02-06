//! Performance metrics for subagent execution
//!
//! This module provides comprehensive metrics collection for subagent executions,
//! tracking completion and errors with detailed telemetry for performance analysis.
//! Metrics are labeled by recursion depth to enable analysis of performance
//! characteristics at different nesting levels.
//!
//! # Metrics
//!
//! - `subagent_executions_total`: Counter of total subagent executions
//! - `subagent_duration_seconds`: Histogram of execution duration
//! - `subagent_turns_used`: Histogram of conversation turns consumed
//! - `subagent_tokens_consumed`: Histogram of tokens consumed
//! - `subagent_completions_total`: Counter of completions by status
//! - `subagent_errors_total`: Counter of errors by type
//! - `subagent_active_count`: Gauge of currently active subagent executions
//!
//! # Examples
//!
//! ```
//! use xzatoma::agent::metrics::SubagentMetrics;
//!
//! let metrics = SubagentMetrics::new("my_task".to_string(), 1);
//! metrics.record_completion(5, 1500, "success");
//! ```

use metrics::{counter, decrement_gauge, histogram, increment_counter, increment_gauge};
use std::cell::Cell;
use std::time::Instant;

/// Metrics collection for a single subagent execution
///
/// Tracks metrics for a subagent execution including timing, token consumption,
/// turns used, and completion status. Metrics are automatically labeled by
/// recursion depth for performance analysis.
///
/// Uses interior mutability (Cell) to allow recording metrics through
/// immutable references, making it easy to use in async contexts.
///
/// # Thread Safety
///
/// `SubagentMetrics` is not Send/Sync due to the Cell. However, it's designed
/// to be created and used within a single async task scope, which is the
/// typical usage pattern.
#[derive(Debug)]
pub struct SubagentMetrics {
    /// Label identifying the subagent execution
    label: String,

    /// Recursion depth of this subagent
    depth: usize,

    /// When the execution started
    start: Instant,

    /// Whether metrics have been recorded to prevent double-recording
    recorded: Cell<bool>,
}

impl SubagentMetrics {
    /// Creates a new metrics tracker for a subagent execution
    ///
    /// Increments the active subagent count gauge and records the execution start.
    ///
    /// # Arguments
    ///
    /// * `label` - Identifier for this subagent execution
    /// * `depth` - Recursion depth (0 for root agent, 1+ for nested subagents)
    ///
    /// # Returns
    ///
    /// A new `SubagentMetrics` instance that tracks the execution
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::metrics::SubagentMetrics;
    ///
    /// let metrics = SubagentMetrics::new("analysis_task".to_string(), 1);
    /// assert_eq!(metrics.label(), "analysis_task");
    /// assert_eq!(metrics.depth(), 1);
    /// ```
    pub fn new(label: String, depth: usize) -> Self {
        increment_counter!("subagent_executions_total", "depth" => depth.to_string());
        increment_gauge!("subagent_active_count", 1.0, "depth" => depth.to_string());

        Self {
            label,
            depth,
            start: Instant::now(),
            recorded: Cell::new(false),
        }
    }

    /// Records successful completion of the subagent execution
    ///
    /// Records metrics including duration, turns used, tokens consumed,
    /// and completion status. Updates the active count gauge.
    ///
    /// # Arguments
    ///
    /// * `turns` - Number of conversation turns used
    /// * `tokens` - Number of tokens consumed
    /// * `status` - Completion status ("success", "timeout", "incomplete", etc)
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::metrics::SubagentMetrics;
    ///
    /// let metrics = SubagentMetrics::new("task".to_string(), 0);
    /// metrics.record_completion(5, 1500, "success");
    /// ```
    pub fn record_completion(&self, turns: usize, tokens: usize, status: &str) {
        // Prevent double-recording using interior mutability
        if self.recorded.get() {
            return;
        }
        self.recorded.set(true);

        let duration = self.start.elapsed();

        histogram!(
            "subagent_duration_seconds",
            duration.as_secs_f64(),
            "depth" => self.depth.to_string(),
            "status" => status.to_string()
        );

        histogram!(
            "subagent_turns_used",
            turns as f64,
            "depth" => self.depth.to_string()
        );

        histogram!(
            "subagent_tokens_consumed",
            tokens as f64,
            "depth" => self.depth.to_string()
        );

        increment_counter!(
            "subagent_completions_total",
            "depth" => self.depth.to_string(),
            "status" => status.to_string()
        );

        decrement_gauge!("subagent_active_count", 1.0, "depth" => self.depth.to_string());
    }

    /// Records an error during subagent execution
    ///
    /// Increments the error counter with the error type label
    /// and decrements the active count gauge.
    ///
    /// # Arguments
    ///
    /// * `error_type` - Description of the error type (e.g., "timeout", "quota_exceeded")
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::metrics::SubagentMetrics;
    ///
    /// let metrics = SubagentMetrics::new("task".to_string(), 0);
    /// metrics.record_error("timeout");
    /// ```
    pub fn record_error(&self, error_type: &str) {
        // Prevent double-recording using interior mutability
        if self.recorded.get() {
            return;
        }
        self.recorded.set(true);

        increment_counter!(
            "subagent_errors_total",
            "depth" => self.depth.to_string(),
            "error_type" => error_type.to_string()
        );

        decrement_gauge!("subagent_active_count", 1.0, "depth" => self.depth.to_string());
    }

    /// Returns the execution label
    ///
    /// # Returns
    ///
    /// Reference to the label string
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the recursion depth
    ///
    /// # Returns
    ///
    /// The depth of this subagent
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// Returns elapsed time since execution started
    ///
    /// # Returns
    ///
    /// Duration elapsed
    pub fn elapsed(&self) -> std::time::Duration {
        self.start.elapsed()
    }
}

impl Drop for SubagentMetrics {
    /// Ensures cleanup on drop - decrements active count even on panic
    ///
    /// If metrics were not explicitly recorded (via `record_completion`
    /// or `record_error`), this ensures the active count is still decremented
    /// to maintain accurate gauge values even in case of panics.
    fn drop(&mut self) {
        if !self.recorded.get() {
            decrement_gauge!("subagent_active_count", 1.0, "depth" => self.depth.to_string());
        }
    }
}

/// Initializes the metrics exporter for Prometheus
///
/// When the `prometheus` feature is enabled, this function
/// sets up the Prometheus metrics exporter to expose metrics
/// on the standard Prometheus endpoint. When disabled, it's a no-op.
///
/// # Feature Gate
///
/// This function only has an effect when compiled with the `prometheus`
/// feature enabled. In other configurations, it does nothing but is
/// still safe to call.
///
/// # Examples
///
/// ```
/// use xzatoma::agent::metrics::init_metrics_exporter;
///
/// // Initialize metrics (only does something with prometheus feature)
/// init_metrics_exporter();
/// ```
pub fn init_metrics_exporter() {
    #[cfg(feature = "prometheus")]
    {
        use metrics_exporter_prometheus::PrometheusBuilder;
        let builder = PrometheusBuilder::new();
        let _ = builder.install().map_err(|e| {
            tracing::warn!("Failed to install Prometheus exporter: {}", e);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subagent_metrics_creation() {
        let metrics = SubagentMetrics::new("test_task".to_string(), 1);
        assert_eq!(metrics.label(), "test_task");
        assert_eq!(metrics.depth(), 1);
    }

    #[test]
    fn test_subagent_metrics_elapsed() {
        let metrics = SubagentMetrics::new("test".to_string(), 0);
        let elapsed = metrics.elapsed();
        assert!(elapsed.as_millis() < 100);
    }

    #[test]
    fn test_subagent_metrics_record_completion() {
        let metrics = SubagentMetrics::new("test".to_string(), 0);
        metrics.record_completion(5, 1500, "success");
        // Verify recorded flag is set
        assert!(metrics.recorded.get());
    }

    #[test]
    fn test_subagent_metrics_record_error() {
        let metrics = SubagentMetrics::new("test".to_string(), 0);
        metrics.record_error("timeout");
        // Verify recorded flag is set
        assert!(metrics.recorded.get());
    }

    #[test]
    fn test_subagent_metrics_drop_without_recording() {
        {
            let _metrics = SubagentMetrics::new("test".to_string(), 0);
            // Metrics will decrement active count on drop
        }
    }

    #[test]
    fn test_subagent_metrics_drop_after_recording() {
        {
            let metrics = SubagentMetrics::new("test".to_string(), 0);
            metrics.record_completion(3, 1000, "complete");
            // Metrics will NOT double-decrement on drop because recorded flag is set
        }
    }

    #[test]
    fn test_init_metrics_exporter() {
        init_metrics_exporter();
        // Should not panic
    }

    #[test]
    fn test_multiple_metrics_same_depth() {
        let m1 = SubagentMetrics::new("task1".to_string(), 1);
        let m2 = SubagentMetrics::new("task2".to_string(), 1);

        m1.record_completion(3, 1000, "complete");
        m2.record_error("failed");

        assert!(m1.recorded.get());
        assert!(m2.recorded.get());
    }

    #[test]
    fn test_metrics_different_depths() {
        let m0 = SubagentMetrics::new("root".to_string(), 0);
        let m1 = SubagentMetrics::new("sub1".to_string(), 1);
        let m2 = SubagentMetrics::new("sub2".to_string(), 2);

        m0.record_completion(10, 5000, "complete");
        m1.record_completion(5, 2500, "complete");
        m2.record_error("depth_exceeded");

        assert_eq!(m0.depth(), 0);
        assert_eq!(m1.depth(), 1);
        assert_eq!(m2.depth(), 2);
    }

    #[test]
    fn test_metrics_double_record_prevention() {
        let metrics = SubagentMetrics::new("test".to_string(), 0);
        metrics.record_completion(5, 1500, "success");
        // Second call should be ignored
        metrics.record_completion(10, 3000, "timeout");
        // Only first completion should be recorded
        assert!(metrics.recorded.get());
    }

    #[test]
    fn test_metrics_various_statuses() {
        let m1 = SubagentMetrics::new("task1".to_string(), 1);
        let m2 = SubagentMetrics::new("task2".to_string(), 1);
        let m3 = SubagentMetrics::new("task3".to_string(), 1);

        m1.record_completion(5, 1500, "success");
        m2.record_completion(10, 5000, "timeout");
        m3.record_completion(3, 800, "incomplete");

        assert!(m1.recorded.get());
        assert!(m2.recorded.get());
        assert!(m3.recorded.get());
    }

    #[test]
    fn test_metrics_various_error_types() {
        let m1 = SubagentMetrics::new("task1".to_string(), 1);
        let m2 = SubagentMetrics::new("task2".to_string(), 1);
        let m3 = SubagentMetrics::new("task3".to_string(), 1);

        m1.record_error("timeout");
        m2.record_error("quota_exceeded");
        m3.record_error("depth_limit");

        assert!(m1.recorded.get());
        assert!(m2.recorded.get());
        assert!(m3.recorded.get());
    }

    #[test]
    fn test_metrics_zero_tokens() {
        let metrics = SubagentMetrics::new("test".to_string(), 0);
        metrics.record_completion(0, 0, "failed");
        assert!(metrics.recorded.get());
    }

    #[test]
    fn test_metrics_high_values() {
        let metrics = SubagentMetrics::new("test".to_string(), 5);
        metrics.record_completion(50, 100000, "success");
        assert_eq!(metrics.depth(), 5);
        assert!(metrics.recorded.get());
    }

    #[test]
    fn test_metrics_elapsed_increases() {
        let metrics = SubagentMetrics::new("test".to_string(), 0);
        let t1 = metrics.elapsed();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let t2 = metrics.elapsed();
        assert!(t2 > t1);
    }

    #[test]
    fn test_metrics_error_then_completion_ignored() {
        let metrics = SubagentMetrics::new("test".to_string(), 1);
        metrics.record_error("quota_exceeded");
        // Second call ignored
        metrics.record_completion(3, 1000, "success");
        // Only error should be recorded
        assert!(metrics.recorded.get());
    }

    #[test]
    fn test_metrics_label_preservation() {
        let label = "complex_analysis_task".to_string();
        let metrics = SubagentMetrics::new(label.clone(), 2);
        assert_eq!(metrics.label(), label);
    }
}
