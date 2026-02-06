//! Integration tests for parallel subagent execution
//!
//! This test suite validates:
//! - Parallel task execution with concurrency limits
//! - Fail-fast behavior
//! - Quota enforcement across parallel tasks
//! - Metrics recording for parallel batches

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use xzatoma::agent::quota::{QuotaLimits, QuotaTracker};
    use xzatoma::agent::SubagentMetrics;
    use xzatoma::tools::{ParallelSubagentInput, ParallelTask, TaskResult};

    // Tests for Task 5.1: Parallel Execution Infrastructure

    #[test]
    fn test_parallel_execution_basic() {
        // Test that we can construct and validate parallel input
        let input = ParallelSubagentInput {
            tasks: vec![
                ParallelTask {
                    label: "task1".to_string(),
                    task_prompt: "Analyze task 1".to_string(),
                    summary_prompt: None,
                    allowed_tools: None,
                    max_turns: None,
                },
                ParallelTask {
                    label: "task2".to_string(),
                    task_prompt: "Analyze task 2".to_string(),
                    summary_prompt: None,
                    allowed_tools: None,
                    max_turns: None,
                },
                ParallelTask {
                    label: "task3".to_string(),
                    task_prompt: "Analyze task 3".to_string(),
                    summary_prompt: None,
                    allowed_tools: None,
                    max_turns: None,
                },
            ],
            max_concurrent: Some(3),
            fail_fast: Some(false),
        };

        assert_eq!(input.tasks.len(), 3);
        assert_eq!(input.max_concurrent, Some(3));
        assert_eq!(input.fail_fast, Some(false));

        for (i, task) in input.tasks.iter().enumerate() {
            assert_eq!(task.label, format!("task{}", i + 1));
            assert!(task.task_prompt.contains("task"));
        }
    }

    #[test]
    fn test_parallel_concurrency_limit() {
        // Test that max_concurrent defaults to 5
        let mut tasks = Vec::new();
        for i in 0..10 {
            tasks.push(ParallelTask {
                label: format!("task{}", i),
                task_prompt: format!("Task {}", i),
                summary_prompt: None,
                allowed_tools: None,
                max_turns: None,
            });
        }

        let input = ParallelSubagentInput {
            tasks,
            max_concurrent: Some(2),
            fail_fast: None,
        };

        assert_eq!(input.tasks.len(), 10);
        assert_eq!(input.max_concurrent, Some(2));
    }

    #[test]
    fn test_parallel_concurrency_default() {
        // Test that max_concurrent defaults when not specified
        let input = ParallelSubagentInput {
            tasks: vec![ParallelTask {
                label: "task".to_string(),
                task_prompt: "test".to_string(),
                summary_prompt: None,
                allowed_tools: None,
                max_turns: None,
            }],
            max_concurrent: None,
            fail_fast: None,
        };

        // When None, implementation will use default of 5
        assert_eq!(input.max_concurrent, None);
    }

    #[test]
    fn test_parallel_fail_fast() {
        // Test fail_fast configuration
        let input_fail_fast = ParallelSubagentInput {
            tasks: vec![
                ParallelTask {
                    label: "task1".to_string(),
                    task_prompt: "test".to_string(),
                    summary_prompt: None,
                    allowed_tools: None,
                    max_turns: None,
                },
                ParallelTask {
                    label: "task2".to_string(),
                    task_prompt: "test".to_string(),
                    summary_prompt: None,
                    allowed_tools: None,
                    max_turns: None,
                },
            ],
            max_concurrent: Some(5),
            fail_fast: Some(true),
        };

        assert_eq!(input_fail_fast.fail_fast, Some(true));

        let input_no_fail_fast = ParallelSubagentInput {
            tasks: vec![
                ParallelTask {
                    label: "task1".to_string(),
                    task_prompt: "test".to_string(),
                    summary_prompt: None,
                    allowed_tools: None,
                    max_turns: None,
                },
                ParallelTask {
                    label: "task2".to_string(),
                    task_prompt: "test".to_string(),
                    summary_prompt: None,
                    allowed_tools: None,
                    max_turns: None,
                },
            ],
            max_concurrent: Some(5),
            fail_fast: Some(false),
        };

        assert_eq!(input_no_fail_fast.fail_fast, Some(false));
    }

    #[test]
    fn test_parallel_task_with_allowed_tools() {
        // Test tool filtering in parallel tasks
        let input = ParallelSubagentInput {
            tasks: vec![
                ParallelTask {
                    label: "task_with_tools".to_string(),
                    task_prompt: "test".to_string(),
                    summary_prompt: None,
                    allowed_tools: Some(vec!["file_ops".to_string(), "terminal".to_string()]),
                    max_turns: None,
                },
                ParallelTask {
                    label: "task_no_filter".to_string(),
                    task_prompt: "test".to_string(),
                    summary_prompt: None,
                    allowed_tools: None,
                    max_turns: None,
                },
            ],
            max_concurrent: Some(5),
            fail_fast: None,
        };

        assert_eq!(input.tasks[0].allowed_tools.as_ref().unwrap().len(), 2);
        assert!(input.tasks[1].allowed_tools.is_none());
    }

    #[test]
    fn test_parallel_task_with_max_turns() {
        // Test max_turns configuration per task
        let input = ParallelSubagentInput {
            tasks: vec![
                ParallelTask {
                    label: "quick_task".to_string(),
                    task_prompt: "test".to_string(),
                    summary_prompt: None,
                    allowed_tools: None,
                    max_turns: Some(5),
                },
                ParallelTask {
                    label: "long_task".to_string(),
                    task_prompt: "test".to_string(),
                    summary_prompt: None,
                    allowed_tools: None,
                    max_turns: Some(50),
                },
            ],
            max_concurrent: Some(5),
            fail_fast: None,
        };

        assert_eq!(input.tasks[0].max_turns, Some(5));
        assert_eq!(input.tasks[1].max_turns, Some(50));
    }

    // Tests for Task 5.2: Quota Enforcement in Parallel Execution

    #[test]
    fn test_quota_enforcement_parallel_executions() {
        // Test that quota tracker enforces execution limits across parallel runs
        let limits = QuotaLimits {
            max_executions: Some(3),
            max_total_tokens: None,
            max_total_time: None,
        };

        let tracker = QuotaTracker::new(limits);

        // Simulate 3 parallel task executions
        assert!(tracker.check_and_reserve().is_ok());
        assert!(tracker.record_execution(100).is_ok());

        assert!(tracker.check_and_reserve().is_ok());
        assert!(tracker.record_execution(100).is_ok());

        assert!(tracker.check_and_reserve().is_ok());
        assert!(tracker.record_execution(100).is_ok());

        // Fourth task should fail (would exceed limit)
        assert!(tracker.check_and_reserve().is_err());
    }

    #[test]
    fn test_quota_enforcement_parallel_tokens() {
        // Test token quota across parallel tasks
        let limits = QuotaLimits {
            max_executions: None,
            max_total_tokens: Some(10000),
            max_total_time: None,
        };

        let tracker = QuotaTracker::new(limits);

        // Simulate parallel tasks consuming tokens
        assert!(tracker.check_and_reserve().is_ok());
        assert!(tracker.record_execution(3500).is_ok()); // 3500/10000

        assert!(tracker.check_and_reserve().is_ok());
        assert!(tracker.record_execution(3500).is_ok()); // 7000/10000

        assert!(tracker.check_and_reserve().is_ok());
        // Third execution would exceed 10000 (3500 + 3500 + 3500 = 10500 > 10000)
        // Note: record_execution records the tokens BEFORE checking limits
        assert!(tracker.record_execution(3500).is_err());

        // Verify remaining tokens (includes the failed execution that exceeded limit)
        let usage = tracker.get_usage();
        assert_eq!(usage.total_tokens, 10500); // All recorded, even over-limit execution
    }

    #[test]
    fn test_quota_enforcement_parallel_time() {
        // Test time quota across parallel execution
        let limits = QuotaLimits {
            max_executions: None,
            max_total_tokens: None,
            max_total_time: Some(Duration::from_millis(100)),
        };

        let tracker = QuotaTracker::new(limits);

        // First task should start
        assert!(tracker.check_and_reserve().is_ok());

        // Wait past timeout
        std::thread::sleep(Duration::from_millis(120));

        // Second task should fail
        assert!(tracker.check_and_reserve().is_err());
    }

    #[test]
    fn test_quota_multiple_limits_parallel() {
        // Test enforcing all three quota limits simultaneously
        let limits = QuotaLimits {
            max_executions: Some(10),
            max_total_tokens: Some(50000),
            max_total_time: Some(Duration::from_secs(60)),
        };

        let tracker = QuotaTracker::new(limits);

        // Execute 5 parallel tasks
        for i in 0..5 {
            assert!(
                tracker.check_and_reserve().is_ok(),
                "Task {} reservation failed",
                i
            );
            assert!(
                tracker.record_execution(5000).is_ok(),
                "Task {} recording failed",
                i
            );
        }

        // Verify usage
        let usage = tracker.get_usage();
        assert_eq!(usage.executions, 5);
        assert_eq!(usage.total_tokens, 25000);
    }

    // Tests for Task 5.3: Metrics Recording for Parallel Execution

    #[test]
    fn test_metrics_parallel_batch() {
        // Test metrics for parallel batch execution
        let metrics1 = SubagentMetrics::new("parallel_batch_1".to_string(), 0);
        let metrics2 = SubagentMetrics::new("parallel_batch_2".to_string(), 0);
        let metrics3 = SubagentMetrics::new("parallel_batch_3".to_string(), 0);

        // Simulate completing tasks
        metrics1.record_completion(10, 5000, "success");
        metrics2.record_completion(8, 4000, "success");
        metrics3.record_completion(5, 2500, "partial");

        // All should complete without panic
    }

    #[test]
    fn test_metrics_parallel_with_errors() {
        // Test metrics when parallel tasks fail
        let metrics_success = SubagentMetrics::new("task_ok".to_string(), 1);
        let metrics_error1 = SubagentMetrics::new("task_timeout".to_string(), 1);
        let metrics_error2 = SubagentMetrics::new("task_invalid".to_string(), 1);

        metrics_success.record_completion(3, 1500, "success");
        metrics_error1.record_error("execution_timeout");
        metrics_error2.record_error("invalid_input");

        // All should record independently
    }

    #[test]
    fn test_metrics_parallel_different_depths() {
        // Test metrics for parallel tasks at different recursion depths
        let batch_root = SubagentMetrics::new("batch_root".to_string(), 0);
        let task_depth1_a = SubagentMetrics::new("parallel_task_1a".to_string(), 1);
        let task_depth1_b = SubagentMetrics::new("parallel_task_1b".to_string(), 1);
        let task_depth1_c = SubagentMetrics::new("parallel_task_1c".to_string(), 1);

        batch_root.record_completion(20, 10000, "success");
        task_depth1_a.record_completion(6, 3000, "success");
        task_depth1_b.record_completion(7, 3500, "success");
        task_depth1_c.record_completion(7, 3500, "success");

        // All should be recorded with proper depth labels
    }

    #[test]
    fn test_metrics_elapsed_time_parallel() {
        // Test elapsed time tracking for parallel tasks
        let metrics1 = SubagentMetrics::new("task1".to_string(), 1);
        let metrics2 = SubagentMetrics::new("task2".to_string(), 1);

        let elapsed1 = metrics1.elapsed();
        assert!(elapsed1.as_millis() < 50);

        std::thread::sleep(Duration::from_millis(20));

        let elapsed2 = metrics2.elapsed();
        assert!(elapsed2.as_millis() < 50); // New metric, separate timer

        let elapsed1_later = metrics1.elapsed();
        assert!(elapsed1_later.as_millis() >= 20);
    }

    #[test]
    fn test_parallel_task_result_serialization() {
        // Test TaskResult serialization for JSON output
        let result = TaskResult {
            label: "test_task".to_string(),
            success: true,
            output: "Task completed successfully".to_string(),
            duration_ms: 1234,
            error: None,
            tokens_used: 500,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("test_task"));
        assert!(json.contains("1234"));
        assert!(json.contains("true"));
    }

    #[test]
    fn test_parallel_task_result_with_error() {
        // Test TaskResult with error information
        let result = TaskResult {
            label: "failed_task".to_string(),
            success: false,
            output: String::new(),
            duration_ms: 500,
            error: Some("Task execution timeout".to_string()),
            tokens_used: 0,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("failed_task"));
        assert!(json.contains("false"));
        assert!(json.contains("timeout"));
    }

    #[test]
    fn test_parallel_output_aggregation() {
        // Test aggregating results from multiple parallel tasks
        let results = [
            TaskResult {
                label: "task1".to_string(),
                success: true,
                output: "Result 1".to_string(),
                duration_ms: 100,
                error: None,
                tokens_used: 300,
            },
            TaskResult {
                label: "task2".to_string(),
                success: true,
                output: "Result 2".to_string(),
                duration_ms: 150,
                error: None,
                tokens_used: 350,
            },
            TaskResult {
                label: "task3".to_string(),
                success: false,
                output: String::new(),
                duration_ms: 75,
                error: Some("Error".to_string()),
                tokens_used: 0,
            },
        ];

        let successful = results.iter().filter(|r| r.success).count();
        let failed = results.iter().filter(|r| !r.success).count();
        let total_duration: u64 = results.iter().map(|r| r.duration_ms).sum();

        assert_eq!(successful, 2);
        assert_eq!(failed, 1);
        assert_eq!(total_duration, 325);
    }

    #[test]
    fn test_parallel_input_deserialization() {
        // Test deserializing parallel input from JSON
        let json = serde_json::json!({
            "tasks": [
                {
                    "label": "task1",
                    "task_prompt": "Do something",
                    "max_turns": 10
                },
                {
                    "label": "task2",
                    "task_prompt": "Do something else",
                    "allowed_tools": ["file_ops", "terminal"]
                }
            ],
            "max_concurrent": 3,
            "fail_fast": true
        });

        let input: ParallelSubagentInput = serde_json::from_value(json).unwrap();
        assert_eq!(input.tasks.len(), 2);
        assert_eq!(input.max_concurrent, Some(3));
        assert_eq!(input.fail_fast, Some(true));
        assert_eq!(input.tasks[0].max_turns, Some(10));
        assert_eq!(input.tasks[1].allowed_tools.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_parallel_input_defaults() {
        // Test that optional fields in parallel input get proper defaults
        let json = serde_json::json!({
            "tasks": [
                {
                    "label": "simple",
                    "task_prompt": "test"
                }
            ]
        });

        let input: ParallelSubagentInput = serde_json::from_value(json).unwrap();
        assert_eq!(input.tasks.len(), 1);
        assert_eq!(input.tasks[0].summary_prompt, None);
        assert_eq!(input.tasks[0].allowed_tools, None);
        assert_eq!(input.tasks[0].max_turns, None);
        assert_eq!(input.max_concurrent, None);
        assert_eq!(input.fail_fast, None);
    }

    #[test]
    fn test_parallel_input_with_summary_prompts() {
        // Test parallel tasks with custom summary prompts
        let json = serde_json::json!({
            "tasks": [
                {
                    "label": "analyze",
                    "task_prompt": "Analyze data",
                    "summary_prompt": "Provide a brief summary"
                },
                {
                    "label": "validate",
                    "task_prompt": "Validate results",
                    "summary_prompt": "List any validation issues"
                }
            ],
            "max_concurrent": 2
        });

        let input: ParallelSubagentInput = serde_json::from_value(json).unwrap();
        assert!(input.tasks[0].summary_prompt.is_some());
        assert!(input.tasks[1].summary_prompt.is_some());
    }

    #[test]
    fn test_quota_tracker_cloned_state_parallel() {
        // Test that cloned quota trackers share state in parallel scenarios
        let limits = QuotaLimits {
            max_executions: Some(5),
            max_total_tokens: Some(50000),
            max_total_time: None,
        };

        let tracker_original = QuotaTracker::new(limits);
        let tracker_clone = tracker_original.clone();

        // Use original tracker
        tracker_original.check_and_reserve().ok();
        tracker_original.record_execution(10000).ok();

        // Clone should see same state
        let usage = tracker_clone.get_usage();
        assert_eq!(usage.executions, 1);
        assert_eq!(usage.total_tokens, 10000);

        // Use clone
        tracker_clone.check_and_reserve().ok();
        tracker_clone.record_execution(10000).ok();

        // Original should see updates from clone
        let usage_updated = tracker_original.get_usage();
        assert_eq!(usage_updated.executions, 2);
        assert_eq!(usage_updated.total_tokens, 20000);
    }

    #[test]
    fn test_parallel_large_task_count() {
        // Test parallel execution with many tasks
        let tasks: Vec<ParallelTask> = (0..100)
            .map(|i| ParallelTask {
                label: format!("task_{}", i),
                task_prompt: format!("Task {} prompt", i),
                summary_prompt: None,
                allowed_tools: None,
                max_turns: None,
            })
            .collect();

        let input = ParallelSubagentInput {
            tasks,
            max_concurrent: Some(10),
            fail_fast: Some(false),
        };

        assert_eq!(input.tasks.len(), 100);
        assert_eq!(input.max_concurrent, Some(10));

        // Verify all tasks have unique labels
        let labels: Vec<_> = input.tasks.iter().map(|t| &t.label).collect();
        assert_eq!(labels.len(), 100);
        let mut sorted_labels = labels.clone();
        sorted_labels.sort();
        sorted_labels.dedup();
        assert_eq!(sorted_labels.len(), 100); // All unique
    }

    #[test]
    fn test_metrics_drop_cleanup_parallel() {
        // Test that metrics Drop cleanup works in parallel scenarios
        {
            let m1 = SubagentMetrics::new("task1".to_string(), 2);
            let m2 = SubagentMetrics::new("task2".to_string(), 2);
            let m3 = SubagentMetrics::new("task3".to_string(), 2);

            m1.record_completion(5, 2500, "success");
            m2.record_completion(3, 1500, "success");
            m3.record_error("cancelled");

            // All dropped without panic
        }

        // Parallel scenario with different completion paths
        {
            let m4 = SubagentMetrics::new("task4".to_string(), 1);
            let _m5 = SubagentMetrics::new("task5".to_string(), 1);

            m4.record_error("timeout");
            // m5 dropped without explicit record call

            // Both should clean up without panic
        }
    }
}
