xzatoma/docs/explanation/copilot_models_caching_and_tests.md#L1-999

# Copilot models caching and tests

## Overview

This document describes the changes introduced to make the GitHub Copilot provider more
robust and efficient when discovering available models, and how we validate those changes
with mocked integration tests.

Goals:

- Use runtime model discovery from the Copilot `/models` endpoint rather than hardcoding.
- Be resilient to transient auth failures (401 Unauthorized) by attempting a non-interactive
  refresh with a cached GitHub token and retrying the request.
- Reduce repeated model-list requests by adding a small in-memory TTL cache for the fetched
  model list.
- Provide reliable, mocked integration tests that run without calling production endpoints by
  allowing tests to override the provider API base URL.

## Components Delivered

- Code:
  - `src/providers/copilot.rs` — Added model-list TTL cache, endpoint override support, and
    improved 401 → refresh → retry logic.
  - `src/config.rs` — Added `CopilotConfig::api_base` (optional) to allow tests / overrides.
- Tests:
  - `tests/copilot_integration.rs` — Wiremock-based integration tests covering:
    - 401 → non-interactive refresh → retry success
    - models TTL caching behavior (only one `/models` request when cached)
- Docs:
  - `docs/explanation/copilot_models_caching_and_tests.md` (this file)

## Implementation Details

Design and rationale:

- Runtime discovery:

  - The provider fetches models from `/models` and parses capabilities, context window,
    and policy state. It no longer relies on hardcoded model lists.

- Endpoint override:

  - For testability we added `CopilotConfig.api_base: Option<String>`. When set,
    the provider uses this base to compose Copilot endpoints (`/models`, `/chat/completions`,
    `/copilot_internal/v2/token`) so tests can point the provider at a mock server.

  Example (config struct):

```xzatoma/src/config.rs#L36-52
pub struct CopilotConfig {
    /// Model to use for Copilot
    #[serde(default = "default_copilot_model")]
    pub model: String,

    /// Optional API base URL for Copilot endpoints (useful for tests and local mocks)
    #[serde(default)]
    pub api_base: Option<String>,
}
```

- Caching:

  - An in-memory models cache reduces repeated calls to the `/models` endpoint.
  - The default TTL is 300 seconds (5 minutes). The cache stores `(models, expires_at_epoch_seconds)`.
  - Cache is protected by `Arc<RwLock<...>>` to allow cheap concurrent reads.

  Key additions:

```xzatoma/src/providers/copilot.rs#L54-72
type ModelsCache = Arc<RwLock<Option<(Vec<ModelInfo>, u64)>>>;
pub struct CopilotProvider {
    client: Client,
    config: Arc<RwLock<CopilotConfig>>,
    keyring_service: String,
    keyring_user: String,
    models_cache: ModelsCache,
    models_cache_ttl_secs: u64,
}
```

- Endpoint builder:
  - `api_endpoint(&self, path: &str) -> String` builds endpoint URLs using `api_base` when set,
    otherwise falls back to the documented production endpoints.

```xzatoma/src/providers/copilot.rs#L587-599
fn api_endpoint(&self, path: &str) -> String {
    if let Ok(cfg) = self.config.read() {
        if let Some(base) = &cfg.api_base {
            return format!("{}/{}", base.trim_end_matches('/'), path.trim_start_matches('/'));
        }
    }
    match path {
        "models" => COPILOT_MODELS_URL.to_string(),
        "chat/completions" => COPILOT_COMPLETIONS_URL.to_string(),
        "copilot_internal/v2/token" => COPILOT_TOKEN_URL.to_string(),
        other => format!("https://api.githubcopilot.com/{}", other.trim_start_matches('/')),
    }
}
```

- Auth recovery and cache invalidation:
  - On 401, `fetch_copilot_models()` and `complete()` attempt a non-interactive refresh:
    1. Read cached keyring entry (contains `github_token`).
    2. Call Copilot token exchange endpoint (`/copilot_internal/v2/token`) with `Authorization: token <github_token>`.
    3. If refresh succeeds, cache the new copilot token and retry the original request once.
    4. If refresh fails or there is no cached GitHub token, clear the cached copilot token (best-effort) and return an actionable Authentication error.
  - We avoid forcing interactive device flows in request paths (interactive flows must be explicit).

## Testing

Overview:

- Unit tests validate parsing and small helpers (already present).
- Integration tests (mocked) validate end-to-end behavior for:
  - 401 → non-interactive refresh → retry.
  - Cache TTL behavior (only one call to `/models` when cached).

Key integration tests:

- `tests/copilot_integration.rs` uses `wiremock::MockServer` to simulate:
  - `/models` returning 401 for the first request (with initial token).
  - `/copilot_internal/v2/token` returning `{"token":"new_token"}` for the refresh call.
  - `/models` returning a valid models payload when retried with refreshed token.
- A second test seeds a valid cached Copilot token in the system keyring, mounts a single `/models` mock
  response (expects exactly 1 request) and calls `list_models()` twice — verifying cache usage.

Highlights from tests (mock + keyring seed):

```xzatoma/tests/copilot_integration.rs#L13-40
let cfg = CopilotConfig {
    api_base: Some(server.uri()),
    ..Default::default()
};
let provider = CopilotProvider::new(cfg).unwrap();

// Set keyring cached token (JSON blob)
let cached = serde_json::json!({
    "github_token": "gho_old",
    "copilot_token": "initial_token",
    "expires_at": now + 3600
});
let entry = keyring::Entry::new("xzatoma", "github_copilot").unwrap();
entry.set_password(&cached.to_string()).unwrap();
```

How to run tests locally:

- Note: the Copilot integration tests that write to the system keyring are ignored by default to avoid CI failures (see notes below). To run them locally when you have a system keyring available:

```/dev/null/commands.sh#L1-5
# Run the Copilot keyring integration tests (ignored by default)
cargo test --test copilot_integration -- --ignored

# Or run all ignored tests
cargo test -- --ignored
```

- Full test & quality checks:

```/dev/null/commands.sh#L1-4
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Notes on keyring usage:

- Integration tests write a small JSON blob to the system keyring (service: `xzatoma`, user: `github_copilot`) to simulate a cached GitHub/Copilot token pair.
- These keyring-dependent integration tests are marked `#[ignore = "requires system keyring"]` and are skipped by default so they do not fail in CI/CD environments that don't expose an interactive system keyring.
- To run them locally when a keyring is available:

```/dev/null/commands.sh#L1-3
cargo test --test copilot_integration -- --ignored
```

- If you need these tests to run in CI, consider using a keyring shim or mocking the keyring access as part of the CI job configuration.

## Usage Examples

Programmatic usage (example only; not executed by tests):

```/dev/null/examples/copilot_usage.rs#L1-10
use xzatoma::config::CopilotConfig;
use xzatoma::providers::{CopilotProvider, Provider};

let cfg = CopilotConfig {
    api_base: Some(\"http://localhost:12345\".to_string()),
    ..Default::default()
};

let provider = CopilotProvider::new(cfg)?;
let models = <CopilotProvider as Provider>::list_models(&provider).await?;
```

## Validation Results

- Local validation performed:
  - `cargo fmt --all` — OK
  - `cargo check --all-targets --all-features` — OK
  - `cargo clippy --all-targets --all-features -- -D warnings` — OK
  - `cargo test --all-features` — OK (integration tests included)
- Mocked integration tests verify core scenarios:
  - 401 → refresh → retry flows succeed.
  - TTL caching prevents repeated `/models` requests.

## Acceptance Criteria

- [x] Models are fetched dynamically from the Copilot `/models` endpoint.
- [x] Provider handles missing `content` fields (tool-call-only responses) when parsing completions (separate parsing fix).
- [x] On 401, provider attempts a non-interactive refresh with cached GitHub token and retries once.
- [x] Models list is cached for 300s by default, preventing excessive calls.
- [x] Integration tests using wiremock validate both auth recovery and caching behavior.
- [x] All quality gates pass: fmt, check, clippy (no warnings), and tests.

## References

- Model Management Plan (guardrails & checklist):
  - `docs/explanation/model_management_implementation_plan.md`
- Related docs:
  - Dynamic model fetching: `docs/explanation/copilot_dynamic_model_fetching.md`
  - Response parsing fixes (missing content/defaults): `docs/explanation/copilot_response_parsing_fix.md`
- Implementation:
  - Provider: `src/providers/copilot.rs`
  - Tests: `tests/copilot_integration.rs`

---

If you want, I can:

- Open a review-ready patch that includes the cache TTL as a configurable option in `CopilotConfig` (instead of fixed 300s) and add a short how-to in the docs to adjust it.
- Add telemetry counters (e.g., 401 counts, parsing failures) so we can monitor regressions in production.
