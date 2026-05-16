//! Shared model cache types and helpers for AI providers.
//!
//! This module provides a single canonical `ModelCache` type alias and
//! TTL-checking helper that replace identical definitions in the Copilot,
//! OpenAI, and Ollama provider modules.
//!
//! All provider model caches use a 300-second (five-minute) TTL.

use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use crate::providers::ModelInfo;

/// Shared in-memory model cache type for AI providers.
///
/// Wraps an optional tuple of `(models, fetch_time)` behind an
/// `Arc<RwLock<_>>` so the cache can be shared safely across async tasks.
/// The `Instant` records when the cache was last populated; callers use
/// [`is_cache_valid`] to determine whether the cached value is still fresh.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::cache::{new_model_cache, is_cache_valid};
///
/// let cache = new_model_cache();
/// // Cache starts empty
/// assert!(cache.read().unwrap().is_none());
/// ```
pub type ModelCache = Arc<RwLock<Option<(Vec<ModelInfo>, Instant)>>>;

/// Time-to-live for provider model caches in seconds.
///
/// All provider model lists are considered stale after this duration elapses
/// since the last successful fetch. The value is 300 seconds (five minutes).
pub const MODEL_CACHE_TTL_SECS: u64 = 300;

/// Create a new, empty model cache.
///
/// Returns an `Arc<RwLock<None>>` ready to be stored in a provider struct.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::cache::new_model_cache;
///
/// let cache = new_model_cache();
/// assert!(cache.read().unwrap().is_none());
/// ```
pub fn new_model_cache() -> ModelCache {
    Arc::new(RwLock::new(None))
}

/// Return `true` if a cache entry is still within its TTL window.
///
/// Compares the elapsed time since `cached_at` against
/// [`MODEL_CACHE_TTL_SECS`]. Returns `false` as soon as the cache is
/// expired so callers know to re-fetch.
///
/// # Arguments
///
/// * `cached_at` - The `Instant` when the cache was last populated
///
/// # Returns
///
/// `true` when fewer than [`MODEL_CACHE_TTL_SECS`] seconds have elapsed
///
/// # Examples
///
/// ```
/// use xzatoma::providers::cache::is_cache_valid;
/// use std::time::Instant;
///
/// let now = Instant::now();
/// assert!(is_cache_valid(now)); // freshly created, definitely valid
/// ```
pub fn is_cache_valid(cached_at: Instant) -> bool {
    cached_at.elapsed() < Duration::from_secs(MODEL_CACHE_TTL_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_model_cache_starts_empty() {
        let cache = new_model_cache();
        assert!(cache.read().unwrap().is_none());
    }

    #[test]
    fn test_is_cache_valid_returns_true_for_fresh_instant() {
        let now = Instant::now();
        assert!(is_cache_valid(now));
    }

    #[test]
    fn test_is_cache_valid_returns_false_for_expired_instant() {
        // Simulate an instant 301 seconds in the past
        let old = Instant::now()
            .checked_sub(Duration::from_secs(MODEL_CACHE_TTL_SECS + 1))
            .unwrap_or_else(Instant::now);
        // On most platforms, subtracting more than uptime saturates at startup.
        // We only assert false when the subtraction actually produced an old instant.
        if old.elapsed() > Duration::from_secs(MODEL_CACHE_TTL_SECS) {
            assert!(!is_cache_valid(old));
        }
    }

    #[test]
    fn test_model_cache_ttl_secs_is_300() {
        assert_eq!(MODEL_CACHE_TTL_SECS, 300);
    }

    #[test]
    fn test_cache_can_be_cloned_and_shared() {
        let cache = new_model_cache();
        let cache2 = Arc::clone(&cache);
        assert!(Arc::ptr_eq(&cache, &cache2));
    }
}
