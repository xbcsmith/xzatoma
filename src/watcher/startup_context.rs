//! Startup stabilization context for rdkafka consumers.
//!
//! During the first few seconds after a Kafka consumer starts, rdkafka probes
//! its configured brokers and emits connectivity errors while retrying.
//! [`QuietStartupContext`] suppresses these messages to `DEBUG` level during a
//! configurable startup window, restoring normal log severity afterwards.

use std::time::{Duration, Instant};

use rdkafka::client::ClientContext;
use rdkafka::config::RDKafkaLogLevel;
use rdkafka::consumer::ConsumerContext;
use rdkafka::error::KafkaError;

/// rdkafka [`ClientContext`] that suppresses broker-probe noise during startup.
///
/// Construct with [`QuietStartupContext::new`], passing the stabilization
/// window duration. All rdkafka log callbacks emitted during the window are
/// downgraded to `DEBUG` regardless of their original severity. After the
/// window elapses, callbacks are forwarded at their original severity.
///
/// Pass `Duration::ZERO` when startup suppression is not required; the window
/// expires immediately and all callbacks pass through at normal severity.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use xzatoma::watcher::startup_context::QuietStartupContext;
///
/// // Suppress noise for 10 seconds after consumer creation.
/// let ctx = QuietStartupContext::new(Duration::from_secs(10));
/// assert!(ctx.is_in_startup_window()); // still in window at creation time
///
/// // No suppression when duration is zero.
/// let passthrough = QuietStartupContext::new(Duration::ZERO);
/// assert!(!passthrough.is_in_startup_window());
/// ```
pub struct QuietStartupContext {
    /// Absolute point in time after which normal log severity resumes.
    startup_deadline: Instant,
}

impl QuietStartupContext {
    /// Create a new context with the given stabilization window.
    ///
    /// # Arguments
    ///
    /// * `duration` - How long to suppress error-level log callbacks.
    ///   Pass [`Duration::ZERO`] to disable suppression entirely.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use xzatoma::watcher::startup_context::QuietStartupContext;
    ///
    /// let ctx = QuietStartupContext::new(Duration::from_secs(5));
    /// assert!(ctx.is_in_startup_window());
    /// ```
    pub fn new(duration: Duration) -> Self {
        Self {
            startup_deadline: Instant::now() + duration,
        }
    }

    /// Returns `true` if the startup stabilization window is still active.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use xzatoma::watcher::startup_context::QuietStartupContext;
    ///
    /// let expired = QuietStartupContext::new(Duration::ZERO);
    /// assert!(!expired.is_in_startup_window());
    /// ```
    pub fn is_in_startup_window(&self) -> bool {
        Instant::now() < self.startup_deadline
    }
}

impl ClientContext for QuietStartupContext {
    fn log(&self, level: RDKafkaLogLevel, fac: &str, log_message: &str) {
        if self.is_in_startup_window() {
            tracing::debug!(facility = fac, level = ?level, "rdkafka (startup suppressed): {}", log_message);
            return;
        }
        match level {
            RDKafkaLogLevel::Emerg
            | RDKafkaLogLevel::Alert
            | RDKafkaLogLevel::Critical
            | RDKafkaLogLevel::Error => {
                tracing::error!(facility = fac, "{}", log_message);
            }
            RDKafkaLogLevel::Warning => {
                tracing::warn!(facility = fac, "{}", log_message);
            }
            RDKafkaLogLevel::Notice | RDKafkaLogLevel::Info => {
                tracing::info!(facility = fac, "{}", log_message);
            }
            RDKafkaLogLevel::Debug => {
                tracing::debug!(facility = fac, "{}", log_message);
            }
        }
    }

    fn error(&self, error: KafkaError, reason: &str) {
        if self.is_in_startup_window() {
            tracing::debug!(error = %error, "rdkafka error (startup suppressed): {}", reason);
            return;
        }
        tracing::error!(error = %error, "{}", reason);
    }
}

/// `ConsumerContext` uses all default implementations.
impl ConsumerContext for QuietStartupContext {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quiet_startup_context_new_with_nonzero_duration_is_in_window() {
        let ctx = QuietStartupContext::new(Duration::from_secs(60));
        assert!(ctx.is_in_startup_window());
    }

    #[test]
    fn test_quiet_startup_context_new_with_zero_duration_is_not_in_window() {
        let ctx = QuietStartupContext::new(Duration::ZERO);
        assert!(!ctx.is_in_startup_window());
    }

    #[test]
    fn test_quiet_startup_context_expired_after_deadline() {
        use std::time::Duration;
        // Create a context with a 1ns window — it will be expired immediately.
        let ctx = QuietStartupContext::new(Duration::from_nanos(1));
        std::thread::sleep(Duration::from_millis(1));
        assert!(!ctx.is_in_startup_window());
    }
}
