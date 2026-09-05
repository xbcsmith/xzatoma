//! Circuit breaker for watcher event loops.
//!
//! The [`CircuitBreaker`] prevents cascading failures by tracking consecutive
//! errors and temporarily blocking new attempts after a threshold is reached.
//!
//! States:
//! - [`CircuitState::Closed`] — normal operation; failures are counted.
//! - [`CircuitState::Open`] — circuit is tripped; calls are rejected immediately.
//! - [`CircuitState::HalfOpen`] — probe state; one attempt is allowed.

use std::time::{Duration, Instant};

/// Current state of the circuit breaker.
///
/// # Examples
///
/// ```
/// use xzatoma::watcher::CircuitState;
///
/// let state = CircuitState::Closed;
/// assert_eq!(state, CircuitState::Closed);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation; errors are tracked.
    Closed,
    /// Circuit is tripped; new attempts are rejected without execution.
    Open,
    /// Probe state; one attempt is allowed to test recovery.
    HalfOpen,
}

/// Configuration for [`CircuitBreaker`].
///
/// # Examples
///
/// ```
/// use xzatoma::watcher::CircuitBreakerConfig;
///
/// let config = CircuitBreakerConfig::default();
/// assert_eq!(config.failure_threshold, 5);
/// assert_eq!(config.reset_timeout_secs, 60);
/// ```
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures required to open the circuit.
    pub failure_threshold: u32,
    /// Seconds to wait in the `Open` state before transitioning to `HalfOpen`.
    pub reset_timeout_secs: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            reset_timeout_secs: 60,
        }
    }
}

/// Three-state circuit breaker for protecting watcher event loops.
///
/// # State transitions
///
/// ```text
/// Closed --[threshold failures]--> Open --[reset_timeout]--> HalfOpen
/// HalfOpen --[success]--> Closed
/// HalfOpen --[failure]--> Open
/// ```
///
/// # Examples
///
/// ```
/// use xzatoma::watcher::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
///
/// let mut cb = CircuitBreaker::new(CircuitBreakerConfig::default());
/// assert_eq!(cb.state(), CircuitState::Closed);
///
/// // Record threshold failures to open the circuit.
/// for _ in 0..5 {
///     cb.on_failure();
/// }
/// assert_eq!(cb.state(), CircuitState::Open);
/// ```
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: CircuitState,
    consecutive_failures: u32,
    opened_at: Option<Instant>,
}

impl CircuitBreaker {
    /// Creates a new circuit breaker in the `Closed` state.
    ///
    /// # Arguments
    ///
    /// * `config` - Thresholds and timeouts.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
    ///
    /// let cb = CircuitBreaker::new(CircuitBreakerConfig::default());
    /// assert_eq!(cb.state(), CircuitState::Closed);
    /// ```
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: CircuitState::Closed,
            consecutive_failures: 0,
            opened_at: None,
        }
    }

    /// Returns the current circuit state.
    pub fn state(&self) -> CircuitState {
        self.state
    }

    /// Returns `true` when the circuit is open and calls must be rejected.
    ///
    /// Also checks whether the reset timeout has elapsed and transitions
    /// to `HalfOpen` if so.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
    ///
    /// let mut cb = CircuitBreaker::new(CircuitBreakerConfig::default());
    /// assert!(!cb.is_open());
    /// ```
    pub fn is_open(&mut self) -> bool {
        if self.state == CircuitState::Open {
            if let Some(opened_at) = self.opened_at {
                let elapsed = opened_at.elapsed();
                let reset_duration = Duration::from_secs(self.config.reset_timeout_secs);
                if elapsed >= reset_duration {
                    self.state = CircuitState::HalfOpen;
                    tracing::info!(
                        elapsed_secs = elapsed.as_secs(),
                        "Circuit breaker transitioning from Open to HalfOpen"
                    );
                    return false;
                }
            }
            return true;
        }
        false
    }

    /// Records a successful operation.
    ///
    /// - `Closed` → resets the consecutive failure counter.
    /// - `HalfOpen` → transitions back to `Closed`.
    /// - `Open` → no-op (use [`is_open`][Self::is_open] to check probe eligibility).
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
    ///
    /// let mut cb = CircuitBreaker::new(CircuitBreakerConfig::default());
    /// cb.on_success();
    /// assert_eq!(cb.state(), CircuitState::Closed);
    /// ```
    pub fn on_success(&mut self) {
        match self.state {
            CircuitState::HalfOpen => {
                self.state = CircuitState::Closed;
                self.consecutive_failures = 0;
                self.opened_at = None;
                tracing::info!("Circuit breaker closed after successful probe");
            }
            CircuitState::Closed => {
                self.consecutive_failures = 0;
            }
            CircuitState::Open => {}
        }
    }

    /// Records a failed operation.
    ///
    /// - `Closed` → increments counter; opens circuit when threshold is reached.
    /// - `HalfOpen` → transitions back to `Open`.
    /// - `Open` → no-op.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
    ///
    /// let config = CircuitBreakerConfig { failure_threshold: 2, reset_timeout_secs: 60 };
    /// let mut cb = CircuitBreaker::new(config);
    ///
    /// cb.on_failure();
    /// assert_eq!(cb.state(), CircuitState::Closed);
    /// cb.on_failure();
    /// assert_eq!(cb.state(), CircuitState::Open);
    /// ```
    pub fn on_failure(&mut self) {
        match self.state {
            CircuitState::Closed => {
                self.consecutive_failures += 1;
                if self.consecutive_failures >= self.config.failure_threshold {
                    self.state = CircuitState::Open;
                    self.opened_at = Some(Instant::now());
                    tracing::warn!(
                        failures = self.consecutive_failures,
                        threshold = self.config.failure_threshold,
                        "Circuit breaker opened after consecutive failures"
                    );
                }
            }
            CircuitState::HalfOpen => {
                self.state = CircuitState::Open;
                self.opened_at = Some(Instant::now());
                tracing::warn!("Circuit breaker reopened after probe failure");
            }
            CircuitState::Open => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_starts_closed() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::default());
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_opens_after_threshold_failures() {
        let mut cb = CircuitBreaker::new(CircuitBreakerConfig::default());
        for _ in 0..5 {
            cb.on_failure();
        }
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[test]
    fn test_circuit_breaker_does_not_open_before_threshold() {
        let mut cb = CircuitBreaker::new(CircuitBreakerConfig::default());
        for _ in 0..4 {
            cb.on_failure();
        }
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_is_open_returns_true_when_open() {
        let mut cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            reset_timeout_secs: 9999,
        });
        cb.on_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(cb.is_open());
    }

    #[test]
    fn test_circuit_breaker_transitions_to_half_open_after_timeout() {
        let mut cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            reset_timeout_secs: 0,
        });
        cb.on_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        // With reset_timeout_secs=0, is_open() should immediately see elapsed >= reset.
        assert!(!cb.is_open());
        assert_eq!(cb.state(), CircuitState::HalfOpen);
    }

    #[test]
    fn test_circuit_breaker_half_open_success_closes_circuit() {
        let mut cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            reset_timeout_secs: 0,
        });
        cb.on_failure();
        cb.is_open(); // triggers HalfOpen transition
        cb.on_success();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_half_open_failure_reopens() {
        let mut cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            reset_timeout_secs: 0,
        });
        cb.on_failure();
        cb.is_open(); // triggers HalfOpen transition
        cb.on_failure();
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[test]
    fn test_circuit_breaker_on_success_resets_failure_counter_when_closed() {
        let mut cb = CircuitBreaker::new(CircuitBreakerConfig::default());
        for _ in 0..4 {
            cb.on_failure();
        }
        cb.on_success();
        // After success, threshold should be reset, so 4 more failures don't open yet.
        for _ in 0..4 {
            cb.on_failure();
        }
        assert_eq!(cb.state(), CircuitState::Closed);
    }
}
