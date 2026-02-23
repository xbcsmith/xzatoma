//! Integration tests for Phase 5: Advanced Execution Patterns
//!
//! This test suite validates:
//! - Quota tracking and enforcement
//! - Metrics collection
//! - Configuration integration

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use xzatoma::agent::quota::{QuotaLimits, QuotaTracker};
    use xzatoma::agent::SubagentMetrics;

    // Tests for Task 5.2: Quota Tracking

    #[test]
    fn test_quota_tracker_unlimited() {
        let limits = QuotaLimits {
            max_executions: None,
            max_total_tokens: None,
            max_total_time: None,
        };

        let tracker = QuotaTracker::new(limits);

        // Should allow unlimited executions
        for _ in 0..100 {
            assert!(tracker.check_and_reserve().is_ok());
            assert!(tracker.record_execution(1000).is_ok());
        }
    }

    #[test]
    fn test_quota_tracker_execution_limit() {
        let limits = QuotaLimits {
            max_executions: Some(3),
            max_total_tokens: None,
            max_total_time: None,
        };

        let tracker = QuotaTracker::new(limits);

        // First 3 should succeed
        for i in 0..3 {
            assert!(
                tracker.check_and_reserve().is_ok(),
                "Execution {} should be allowed",
                i
            );
            assert!(tracker.record_execution(100).is_ok());
        }

        // 4th should fail
        assert!(
            tracker.check_and_reserve().is_err(),
            "4th execution should exceed limit"
        );
    }

    #[test]
    fn test_quota_tracker_token_limit() {
        let limits = QuotaLimits {
            max_executions: None,
            max_total_tokens: Some(5000),
            max_total_time: None,
        };

        let tracker = QuotaTracker::new(limits);

        // Execute with 2000 tokens each
        assert!(tracker.check_and_reserve().is_ok());
        assert!(tracker.record_execution(2000).is_ok());

        assert!(tracker.check_and_reserve().is_ok());
        assert!(tracker.record_execution(2000).is_ok());

        // This would exceed 5000 (2000 + 2000 + 2000 = 6000 > 5000)
        assert!(tracker.check_and_reserve().is_ok());
        assert!(tracker.record_execution(2000).is_err());
    }

    #[test]
    fn test_quota_tracker_time_limit() {
        let limits = QuotaLimits {
            max_executions: None,
            max_total_tokens: None,
            max_total_time: Some(Duration::from_millis(100)),
        };

        let tracker = QuotaTracker::new(limits);

        // First should succeed
        assert!(tracker.check_and_reserve().is_ok());

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(120));

        // Second should fail
        assert!(tracker.check_and_reserve().is_err());
    }

    #[test]
    fn test_quota_tracker_multiple_limits() {
        let limits = QuotaLimits {
            max_executions: Some(5),
            max_total_tokens: Some(10000),
            max_total_time: Some(Duration::from_secs(60)),
        };

        let tracker = QuotaTracker::new(limits);

        // Test execution limit first
        for i in 0..5 {
            assert!(tracker.check_and_reserve().is_ok(), "Exec {}", i);
            assert!(tracker.record_execution(1500).is_ok());
        }

        // Should hit execution limit
        assert!(tracker.check_and_reserve().is_err());
    }

    #[test]
    fn test_quota_remaining_executions() {
        let limits = QuotaLimits {
            max_executions: Some(10),
            max_total_tokens: None,
            max_total_time: None,
        };

        let tracker = QuotaTracker::new(limits);

        assert_eq!(tracker.remaining_executions(), Some(10));

        tracker.check_and_reserve().ok();
        tracker.record_execution(100).ok();
        assert_eq!(tracker.remaining_executions(), Some(9));

        tracker.check_and_reserve().ok();
        tracker.record_execution(100).ok();
        assert_eq!(tracker.remaining_executions(), Some(8));
    }

    #[test]
    fn test_quota_remaining_tokens() {
        let limits = QuotaLimits {
            max_executions: None,
            max_total_tokens: Some(10000),
            max_total_time: None,
        };

        let tracker = QuotaTracker::new(limits);

        assert_eq!(tracker.remaining_tokens(), Some(10000));

        tracker.check_and_reserve().ok();
        tracker.record_execution(3000).ok();
        assert_eq!(tracker.remaining_tokens(), Some(7000));

        tracker.check_and_reserve().ok();
        tracker.record_execution(2000).ok();
        assert_eq!(tracker.remaining_tokens(), Some(5000));
    }

    #[test]
    fn test_quota_tracker_clone() {
        let limits = QuotaLimits {
            max_executions: Some(5),
            max_total_tokens: None,
            max_total_time: None,
        };

        let tracker1 = QuotaTracker::new(limits);
        let tracker2 = tracker1.clone();

        // Use tracker1
        tracker1.check_and_reserve().ok();
        tracker1.record_execution(100).ok();

        // tracker2 should see same usage
        let usage = tracker2.get_usage();
        assert_eq!(usage.executions, 1);
        assert_eq!(usage.total_tokens, 100);

        // tracker2 should also be affected
        tracker2.check_and_reserve().ok();
        tracker2.record_execution(100).ok();

        let usage1 = tracker1.get_usage();
        assert_eq!(usage1.executions, 2);
    }

    // Tests for Task 5.3: Metrics

    #[test]
    fn test_metrics_creation() {
        let metrics = SubagentMetrics::new("task1".to_string(), 1);

        assert_eq!(metrics.label(), "task1");
        assert_eq!(metrics.depth(), 1);
    }

    #[test]
    fn test_metrics_elapsed_time() {
        let metrics = SubagentMetrics::new("task".to_string(), 0);
        let elapsed = metrics.elapsed();

        // Should be very small, close to 0
        assert!(elapsed.as_millis() < 50);
    }

    #[test]
    fn test_metrics_elapsed_time_after_delay() {
        let metrics = SubagentMetrics::new("task".to_string(), 0);

        std::thread::sleep(Duration::from_millis(10));

        let elapsed = metrics.elapsed();
        assert!(elapsed.as_millis() >= 10);
        assert!(elapsed.as_millis() < 100);
    }

    #[test]
    fn test_metrics_record_completion() {
        let metrics = SubagentMetrics::new("analyze".to_string(), 1);

        // Should not panic
        metrics.record_completion(5, 1250, "success");
    }

    #[test]
    fn test_metrics_record_completion_various_statuses() {
        let m1 = SubagentMetrics::new("task1".to_string(), 0);
        let m2 = SubagentMetrics::new("task2".to_string(), 0);
        let m3 = SubagentMetrics::new("task3".to_string(), 0);

        m1.record_completion(3, 500, "success");
        m2.record_completion(10, 5000, "truncated");
        m3.record_completion(1, 100, "partial");

        // All should complete without panic
    }

    #[test]
    fn test_metrics_record_error() {
        let metrics = SubagentMetrics::new("task".to_string(), 1);

        // Should not panic
        metrics.record_error("timeout");
    }

    #[test]
    fn test_metrics_record_error_various_types() {
        let m1 = SubagentMetrics::new("task1".to_string(), 1);
        let m2 = SubagentMetrics::new("task2".to_string(), 1);
        let m3 = SubagentMetrics::new("task3".to_string(), 1);

        m1.record_error("depth_limit");
        m2.record_error("execution_timeout");
        m3.record_error("provider_error");

        // All should complete without panic
    }

    #[test]
    fn test_metrics_drop() {
        {
            let _metrics = SubagentMetrics::new("task".to_string(), 0);
            // Dropped without explicit record_*
        }
        // Should not panic
    }

    #[test]
    fn test_metrics_multiple_same_depth() {
        let m1 = SubagentMetrics::new("task1".to_string(), 1);
        let m2 = SubagentMetrics::new("task2".to_string(), 1);
        let m3 = SubagentMetrics::new("task3".to_string(), 1);

        m1.record_completion(3, 1000, "success");
        m2.record_completion(5, 2000, "success");
        m3.record_error("failed");

        // All should work independently
    }

    #[test]
    fn test_metrics_different_depths() {
        let m0 = SubagentMetrics::new("root".to_string(), 0);
        let m1 = SubagentMetrics::new("sub1".to_string(), 1);
        let m2 = SubagentMetrics::new("sub2".to_string(), 2);

        m0.record_completion(10, 5000, "success");
        m1.record_completion(5, 2500, "success");
        m2.record_error("depth_exceeded");

        // All should be tracked separately
    }

    #[test]
    fn test_metrics_init_exporter() {
        // Should not panic
        xzatoma::agent::init_metrics_exporter();
    }

    // Configuration tests

    #[test]
    fn test_quota_config_from_subagent_config() {
        use xzatoma::config::SubagentConfig;

        let config = SubagentConfig {
            max_depth: 3,
            default_max_turns: 10,
            output_max_size: 4096,
            telemetry_enabled: true,
            persistence_enabled: false,
            persistence_path: "/tmp/db.sled".to_string(),
            max_executions: Some(5),
            max_total_tokens: Some(50000),
            max_total_time: Some(300),
            provider: None,
            model: None,
            chat_enabled: false,
        };

        let limits = QuotaLimits {
            max_executions: config.max_executions,
            max_total_tokens: config.max_total_tokens,
            max_total_time: config.max_total_time.map(Duration::from_secs),
        };

        let tracker = QuotaTracker::new(limits);
        assert_eq!(tracker.remaining_executions(), Some(5));
        assert_eq!(tracker.remaining_tokens(), Some(50000));
    }

    #[test]
    fn test_quota_config_with_no_limits() {
        use xzatoma::config::SubagentConfig;

        let config = SubagentConfig::default();

        let limits = QuotaLimits {
            max_executions: config.max_executions,
            max_total_tokens: config.max_total_tokens,
            max_total_time: config.max_total_time.map(Duration::from_secs),
        };

        let tracker = QuotaTracker::new(limits);
        assert!(tracker.remaining_executions().is_none());
        assert!(tracker.remaining_tokens().is_none());
        assert!(tracker.remaining_time().is_none());
    }
}
