//! Resource quota tracking and enforcement
//!
//! This module provides quota management for subagent execution,
//! including limits on executions, tokens, and wall-clock time.

use crate::error::{Result, XzatomaError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Resource quota limits for subagent execution
///
/// Specifies the maximum resources that can be consumed by subagents
/// during a session or execution context.
///
/// # Fields
///
/// * `max_executions` - Maximum number of subagent executions allowed
/// * `max_total_tokens` - Maximum total tokens across all subagents
/// * `max_total_time` - Maximum wall-clock time for all subagents
///
/// # Examples
///
/// ```ignore
/// use xzatoma::agent::quota::QuotaLimits;
/// use std::time::Duration;
///
/// let limits = QuotaLimits {
///     max_executions: Some(10),
///     max_total_tokens: Some(50000),
///     max_total_time: Some(Duration::from_secs(300)),
/// };
/// ```
#[derive(Debug, Clone)]
pub struct QuotaLimits {
    /// Maximum number of subagent executions (None = unlimited)
    pub max_executions: Option<usize>,

    /// Maximum total tokens consumable (None = unlimited)
    pub max_total_tokens: Option<usize>,

    /// Maximum wall-clock time for all subagents (None = unlimited)
    pub max_total_time: Option<Duration>,
}

/// Current quota usage tracking
///
/// Maintains the current state of resource consumption relative to limits.
///
/// # Fields
///
/// * `executions` - Number of subagents executed so far
/// * `total_tokens` - Total tokens consumed so far
/// * `start_time` - When quota tracking started
#[derive(Debug, Clone)]
pub struct QuotaUsage {
    /// Count of completed subagent executions
    pub executions: usize,

    /// Total tokens consumed by all subagents
    pub total_tokens: usize,

    /// When quota tracking started
    pub start_time: Instant,
}

/// Thread-safe quota tracker
///
/// Manages resource quotas and tracks usage with thread-safe operations.
/// Allows checking available quota before execution and recording usage after.
pub struct QuotaTracker {
    /// The configured limits
    limits: QuotaLimits,

    /// Current usage (thread-safe)
    usage: Arc<Mutex<QuotaUsage>>,
}

impl QuotaTracker {
    /// Creates a new quota tracker with specified limits
    ///
    /// # Arguments
    ///
    /// * `limits` - The quota limits to enforce
    ///
    /// # Returns
    ///
    /// A new `QuotaTracker` instance
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use xzatoma::agent::quota::{QuotaTracker, QuotaLimits};
    /// use std::time::Duration;
    ///
    /// let limits = QuotaLimits {
    ///     max_executions: Some(10),
    ///     max_total_tokens: Some(50000),
    ///     max_total_time: Some(Duration::from_secs(300)),
    /// };
    /// let tracker = QuotaTracker::new(limits);
    /// ```
    pub fn new(limits: QuotaLimits) -> Self {
        Self {
            limits,
            usage: Arc::new(Mutex::new(QuotaUsage {
                executions: 0,
                total_tokens: 0,
                start_time: Instant::now(),
            })),
        }
    }

    /// Checks if quota is available and reserves one execution slot
    ///
    /// This should be called before executing a subagent to verify
    /// that quota is available. If any limit is exceeded, returns an error.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if quota is available, or `Err` if a limit is exceeded
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::QuotaExceeded` if:
    /// - Execution limit has been reached
    /// - Time limit has been exceeded
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use xzatoma::agent::quota::{QuotaTracker, QuotaLimits};
    /// use std::time::Duration;
    ///
    /// let limits = QuotaLimits {
    ///     max_executions: Some(5),
    ///     max_total_tokens: None,
    ///     max_total_time: None,
    /// };
    /// let tracker = QuotaTracker::new(limits);
    ///
    /// // First 5 executions should succeed
    /// for _ in 0..5 {
    ///     tracker.check_and_reserve()?;
    /// }
    ///
    /// // 6th execution should fail
    /// assert!(tracker.check_and_reserve().is_err());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn check_and_reserve(&self) -> Result<()> {
        let usage = self.usage.lock().unwrap();

        // Check execution limit
        if let Some(max) = self.limits.max_executions {
            if usage.executions >= max {
                return Err(anyhow::anyhow!(XzatomaError::QuotaExceeded(format!(
                    "Execution limit reached: {}/{}",
                    usage.executions, max
                ))));
            }
        }

        // Check time limit
        if let Some(max_time) = self.limits.max_total_time {
            if usage.start_time.elapsed() >= max_time {
                return Err(anyhow::anyhow!(XzatomaError::QuotaExceeded(format!(
                    "Time limit exceeded: {:?} >= {:?}",
                    usage.start_time.elapsed(),
                    max_time
                ))));
            }
        }

        Ok(())
    }

    /// Records resource consumption after subagent execution
    ///
    /// This should be called after a subagent completes to update the
    /// usage counters. Verifies that the new usage doesn't exceed limits.
    ///
    /// # Arguments
    ///
    /// * `tokens` - Tokens consumed by this execution
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or `Err` if token limit is exceeded
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::QuotaExceeded` if:
    /// - Token limit is exceeded after this execution
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use xzatoma::agent::quota::{QuotaTracker, QuotaLimits};
    ///
    /// let limits = QuotaLimits {
    ///     max_executions: None,
    ///     max_total_tokens: Some(10000),
    ///     max_total_time: None,
    /// };
    /// let tracker = QuotaTracker::new(limits);
    ///
    /// tracker.check_and_reserve()?;
    /// tracker.record_execution(5000)?;
    ///
    /// // This should fail (5000 + 6000 > 10000)
    /// tracker.check_and_reserve()?;
    /// assert!(tracker.record_execution(6000).is_err());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn record_execution(&self, tokens: usize) -> Result<()> {
        let mut usage = self.usage.lock().unwrap();
        usage.executions += 1;
        usage.total_tokens += tokens;

        // Check token limit
        if let Some(max) = self.limits.max_total_tokens {
            if usage.total_tokens > max {
                return Err(anyhow::anyhow!(XzatomaError::QuotaExceeded(format!(
                    "Token limit exceeded: {}/{}",
                    usage.total_tokens, max
                ))));
            }
        }

        Ok(())
    }

    /// Returns current quota usage snapshot
    ///
    /// # Returns
    ///
    /// A snapshot of the current usage state
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use xzatoma::agent::quota::{QuotaTracker, QuotaLimits};
    ///
    /// let tracker = QuotaTracker::new(QuotaLimits {
    ///     max_executions: None,
    ///     max_total_tokens: None,
    ///     max_total_time: None,
    /// });
    ///
    /// let usage = tracker.get_usage();
    /// assert_eq!(usage.executions, 0);
    /// assert_eq!(usage.total_tokens, 0);
    /// ```
    pub fn get_usage(&self) -> QuotaUsage {
        self.usage.lock().unwrap().clone()
    }

    /// Calculates remaining execution budget
    ///
    /// # Returns
    ///
    /// The number of executions remaining before hitting the limit,
    /// or None if execution quota is unlimited
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use xzatoma::agent::quota::{QuotaTracker, QuotaLimits};
    ///
    /// let limits = QuotaLimits {
    ///     max_executions: Some(10),
    ///     max_total_tokens: None,
    ///     max_total_time: None,
    /// };
    /// let tracker = QuotaTracker::new(limits);
    ///
    /// assert_eq!(tracker.remaining_executions(), Some(10));
    /// ```
    pub fn remaining_executions(&self) -> Option<usize> {
        let usage = self.usage.lock().unwrap();
        self.limits
            .max_executions
            .map(|max| max.saturating_sub(usage.executions))
    }

    /// Calculates remaining token budget
    ///
    /// # Returns
    ///
    /// The number of tokens remaining before hitting the limit,
    /// or None if token quota is unlimited
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use xzatoma::agent::quota::{QuotaTracker, QuotaLimits};
    ///
    /// let limits = QuotaLimits {
    ///     max_executions: None,
    ///     max_total_tokens: Some(100000),
    ///     max_total_time: None,
    /// };
    /// let tracker = QuotaTracker::new(limits);
    ///
    /// assert_eq!(tracker.remaining_tokens(), Some(100000));
    /// ```
    pub fn remaining_tokens(&self) -> Option<usize> {
        let usage = self.usage.lock().unwrap();
        self.limits
            .max_total_tokens
            .map(|max| max.saturating_sub(usage.total_tokens))
    }

    /// Calculates remaining time budget
    ///
    /// # Returns
    ///
    /// The remaining duration before hitting the time limit,
    /// or None if time quota is unlimited
    pub fn remaining_time(&self) -> Option<Duration> {
        let usage = self.usage.lock().unwrap();
        self.limits.max_total_time.map(|max| {
            let elapsed = usage.start_time.elapsed();
            if elapsed >= max {
                Duration::ZERO
            } else {
                max - elapsed
            }
        })
    }
}

impl Clone for QuotaTracker {
    /// Clones the quota tracker
    ///
    /// Creates a new tracker that shares the same usage state,
    /// allowing multiple handles to track against the same quotas.
    fn clone(&self) -> Self {
        Self {
            limits: self.limits.clone(),
            usage: Arc::clone(&self.usage),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quota_limits_creation() {
        let limits = QuotaLimits {
            max_executions: Some(10),
            max_total_tokens: Some(50000),
            max_total_time: Some(Duration::from_secs(300)),
        };

        assert_eq!(limits.max_executions, Some(10));
        assert_eq!(limits.max_total_tokens, Some(50000));
        assert_eq!(limits.max_total_time, Some(Duration::from_secs(300)));
    }

    #[test]
    fn test_quota_usage_tracking() {
        let usage = QuotaUsage {
            executions: 5,
            total_tokens: 1000,
            start_time: Instant::now(),
        };

        assert_eq!(usage.executions, 5);
        assert_eq!(usage.total_tokens, 1000);
    }

    #[test]
    fn test_quota_tracker_new() {
        let limits = QuotaLimits {
            max_executions: Some(10),
            max_total_tokens: None,
            max_total_time: None,
        };
        let tracker = QuotaTracker::new(limits);
        let usage = tracker.get_usage();

        assert_eq!(usage.executions, 0);
        assert_eq!(usage.total_tokens, 0);
    }

    #[test]
    fn test_quota_check_and_reserve_success() {
        let limits = QuotaLimits {
            max_executions: Some(10),
            max_total_tokens: None,
            max_total_time: None,
        };
        let tracker = QuotaTracker::new(limits);

        assert!(tracker.check_and_reserve().is_ok());
    }

    #[test]
    fn test_quota_execution_limit_exceeded() {
        let limits = QuotaLimits {
            max_executions: Some(2),
            max_total_tokens: None,
            max_total_time: None,
        };
        let tracker = QuotaTracker::new(limits);

        assert!(tracker.check_and_reserve().is_ok());
        let _ = tracker.record_execution(100);
        assert!(tracker.check_and_reserve().is_ok());
        let _ = tracker.record_execution(100);

        // Third execution should fail
        assert!(tracker.check_and_reserve().is_err());
    }

    #[test]
    fn test_quota_token_limit_exceeded() {
        let limits = QuotaLimits {
            max_executions: None,
            max_total_tokens: Some(1000),
            max_total_time: None,
        };
        let tracker = QuotaTracker::new(limits);

        assert!(tracker.check_and_reserve().is_ok());
        assert!(tracker.record_execution(600).is_ok());

        assert!(tracker.check_and_reserve().is_ok());
        assert!(tracker.record_execution(500).is_err());
    }

    #[test]
    fn test_quota_no_limits() {
        let limits = QuotaLimits {
            max_executions: None,
            max_total_tokens: None,
            max_total_time: None,
        };
        let tracker = QuotaTracker::new(limits);

        for _ in 0..100 {
            assert!(tracker.check_and_reserve().is_ok());
            assert!(tracker.record_execution(1000).is_ok());
        }
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

        let _ = tracker.check_and_reserve();
        let _ = tracker.record_execution(100);
        assert_eq!(tracker.remaining_executions(), Some(9));
    }

    #[test]
    fn test_quota_remaining_tokens() {
        let limits = QuotaLimits {
            max_executions: None,
            max_total_tokens: Some(1000),
            max_total_time: None,
        };
        let tracker = QuotaTracker::new(limits);

        assert_eq!(tracker.remaining_tokens(), Some(1000));

        let _ = tracker.check_and_reserve();
        let _ = tracker.record_execution(300);
        assert_eq!(tracker.remaining_tokens(), Some(700));
    }

    #[test]
    fn test_quota_remaining_time() {
        let limits = QuotaLimits {
            max_executions: None,
            max_total_tokens: None,
            max_total_time: Some(Duration::from_secs(10)),
        };
        let tracker = QuotaTracker::new(limits);

        let remaining = tracker.remaining_time();
        assert!(remaining.is_some());
        assert!(remaining.unwrap() <= Duration::from_secs(10));
    }

    #[test]
    fn test_quota_tracker_clone() {
        let limits = QuotaLimits {
            max_executions: Some(10),
            max_total_tokens: None,
            max_total_time: None,
        };
        let tracker1 = QuotaTracker::new(limits);
        let tracker2 = tracker1.clone();

        let _ = tracker1.check_and_reserve();
        let _ = tracker1.record_execution(100);

        // tracker2 should see the same usage
        let usage = tracker2.get_usage();
        assert_eq!(usage.executions, 1);
        assert_eq!(usage.total_tokens, 100);
    }

    #[test]
    fn test_quota_zero_max_concurrent() {
        let limits = QuotaLimits {
            max_executions: Some(0),
            max_total_tokens: None,
            max_total_time: None,
        };
        let tracker = QuotaTracker::new(limits);

        assert!(tracker.check_and_reserve().is_err());
    }

    #[test]
    fn test_quota_usage_clone() {
        let usage = QuotaUsage {
            executions: 5,
            total_tokens: 1000,
            start_time: Instant::now(),
        };
        let cloned = usage.clone();

        assert_eq!(cloned.executions, usage.executions);
        assert_eq!(cloned.total_tokens, usage.total_tokens);
    }
}
