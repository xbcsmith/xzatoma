//! Shared internal utilities for the provider implementations.
//!
//! This module centralizes small helpers that would otherwise be duplicated
//! verbatim across the Copilot, Ollama, and OpenAI providers.

use crate::error::{Result, XzatomaError};
use std::sync::{RwLock, RwLockReadGuard};

/// Acquire a read lock on a provider's configuration.
///
/// Every provider stores its configuration as an `Arc<RwLock<Config>>` and
/// needs a read guard in several hot paths. Each site previously repeated the
/// same `map_err` closure that converts a poisoned lock into
/// `XzatomaError::Provider`. This helper centralizes that mapping so the error
/// message stays identical everywhere.
///
/// # Arguments
///
/// * `lock` - The `RwLock` guarding the provider configuration.
///
/// # Errors
///
/// Returns `XzatomaError::Provider` with the message
/// `"Failed to acquire read lock on config"` when the lock is poisoned.
///
/// # Examples
///
/// ```
/// use std::sync::RwLock;
/// use xzatoma::providers::util::read_config_lock;
///
/// let lock = RwLock::new(7u32);
/// let guard = read_config_lock(&lock).unwrap();
/// assert_eq!(*guard, 7);
/// ```
pub fn read_config_lock<T>(lock: &RwLock<T>) -> Result<RwLockReadGuard<'_, T>> {
    lock.read()
        .map_err(|_| XzatomaError::Provider("Failed to acquire read lock on config".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_config_lock_returns_guard_on_healthy_lock() {
        let lock = RwLock::new(String::from("gpt-4o-mini"));
        let guard = read_config_lock(&lock).expect("healthy lock should yield a guard");
        assert_eq!(*guard, "gpt-4o-mini");
    }

    #[test]
    fn test_read_config_lock_reports_provider_error_on_poison() {
        let lock = RwLock::new(0u8);
        // Poison the lock by panicking while a write guard is held.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = lock.write().unwrap();
            panic!("poison the lock");
        }));

        let err = read_config_lock(&lock).expect_err("poisoned lock must error");
        assert_eq!(
            err.to_string(),
            XzatomaError::Provider("Failed to acquire read lock on config".to_string()).to_string()
        );
    }
}
