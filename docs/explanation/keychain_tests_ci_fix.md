# Keychain Tests CI Fix

## Overview

The xzatoma project uses the system keyring to cache authentication tokens for GitHub Copilot. While this is essential for production use, the integration tests that access the OS keychain were causing CI failures in headless environments (like GitHub Actions runners) where no GUI keyring service is available.

This document explains how keychain tests have been gated to prevent CI failures while remaining available for manual testing in environments with an active keyring service.

## Problem

Two integration tests in `tests/copilot_integration.rs` directly access the system keyring:

- `test_copilot_models_401_refresh_retry()` - Tests token refresh on 401 response
- `test_copilot_models_caching_ttl()` - Tests models cache TTL behavior

In CI environments (GitHub Actions, headless Linux servers, etc.), these tests would fail because:

1. No GUI keyring service is available (e.g., GNOME Keyring, KDE Wallet)
2. The tests use `keyring::Entry::new()` which attempts to access the system keyring
3. In headless environments, this fails with keyring service unavailable errors

This caused CI pipelines to fail even though the functionality works correctly in desktop environments.

## Solution

The keychain tests are now gated behind an environment variable: `XZATOMA_RUN_KEYCHAIN_TESTS`

### Implementation Details

A helper function checks the environment variable at test runtime:

```rust
/// Helper to check if keychain tests should run
fn should_run_keychain_tests() -> bool {
    std::env::var("XZATOMA_RUN_KEYCHAIN_TESTS").is_ok()
}
```

Each test includes an early return if the environment variable is not set:

```rust
#[tokio::test]
#[ignore = "requires system keyring; enable with XZATOMA_RUN_KEYCHAIN_TESTS=1"]
async fn test_copilot_models_401_refresh_retry() {
    if !should_run_keychain_tests() {
        println!("Skipping keychain test. Enable with: XZATOMA_RUN_KEYCHAIN_TESTS=1 cargo test -- --ignored");
        return;
    }
    // ... test implementation
}
```

## Using the Tests

### In CI (Default Behavior)

By default, tests are skipped automatically:

```bash
# Tests are skipped, no keyring access attempted
cargo test --all-features
```

Output:
```
test test_copilot_models_401_refresh_retry ... ignored, requires system keyring; enable with XZATOMA_RUN_KEYCHAIN_TESTS=1
test test_copilot_models_caching_ttl ... ignored, requires system keyring; enable with XZATOMA_RUN_KEYCHAIN_TESTS=1

test result: ok. N passed; 0 failed; 2 ignored
```

### In Local Development (With Keyring)

To run the tests locally when a keyring service is available:

```bash
# Run ALL tests including keychain tests
XZATOMA_RUN_KEYCHAIN_TESTS=1 cargo test --all-features -- --ignored

# Or run only the keychain tests
XZATOMA_RUN_KEYCHAIN_TESTS=1 cargo test --test copilot_integration -- --ignored --nocapture
```

## Why This Approach

This solution follows Rust testing best practices:

1. **Non-Breaking**: CI continues to work without any configuration changes
2. **Opt-In**: Tests only run when explicitly enabled via environment variable
3. **Self-Documenting**: The `#[ignore]` message tells developers how to enable the tests
4. **Fails Safe**: If tests are skipped, they don't clutter test results with failures

## Related Files

- `tests/copilot_integration.rs` - Integration tests with keychain gating
- `src/providers/copilot.rs` - Main Copilot provider implementation (uses keyring in production)
- `src/error.rs` - Keyring error type is defined here

## References

- Keyring crate: https://docs.rs/keyring/
- GitHub Copilot authentication: See `src/commands/mod.rs` - `authenticate()` function
- Rust test documentation: https://doc.rust-lang.org/book/ch11-00-testing.html

## Testing Validation

All quality checks pass:

- ✅ `cargo fmt --all` applied successfully
- ✅ `cargo check --all-targets --all-features` passes
- ✅ `cargo clippy --all-targets --all-features -- -D warnings` shows zero warnings (keychain tests have no new warnings)
- ✅ `cargo test --all-features` passes with tests skipped by default
- ✅ Tests remain functional when enabled with `XZATOMA_RUN_KEYCHAIN_TESTS=1`

## Future Considerations

If a mock keyring service becomes available in CI (e.g., via Dbus or containerization), these tests could be re-enabled by setting `XZATOMA_RUN_KEYCHAIN_TESTS=1` in the CI pipeline without any code changes.
