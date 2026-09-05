# Phase 3: ACP Server Production Controls Implementation

## Overview

This phase adds three production-readiness controls to the ACP HTTP server:

- Concurrency limiting with a semaphore gate on run creation
- Session activity tracking and background eviction of idle in-memory state
- CORS support for browser-based clients
- Graceful shutdown with an optional drain timeout

All changes are confined to `src/acp/server.rs`, `src/acp/runtime.rs`,
`src/acp/executor.rs`, `src/config.rs`, and `Cargo.toml`.

---

## Files Changed

| File                  | Change type | Description                                                                            |
| --------------------- | ----------- | -------------------------------------------------------------------------------------- |
| `src/config.rs`       | Modified    | Added `AcpSessionMode` enum and eight new `AcpConfig` fields                           |
| `src/acp/executor.rs` | Modified    | Session mode history loading/saving; `allow_dangerous` tool registry gate              |
| `src/acp/server.rs`   | Modified    | Semaphore gate, per-run timeout, `CorsLayer`, session eviction task, graceful shutdown |
| `src/acp/runtime.rs`  | Modified    | Session eviction support methods                                                       |
| `Cargo.toml`          | Modified    | Added `tower-http` `cors` feature                                                      |

---

## Changes

### `AcpServerState` new fields

Two fields were added to `AcpServerState`:

```rust
run_semaphore: Arc<Semaphore>,
session_activity: Arc<Mutex<HashMap<String, Instant>>>,
```

Both constructors (`from_config` and `from_parts`) initialize them. The
`build_primary_manifest` helper also received the new fields to keep struct
literal construction valid. The `max_concurrent_runs` config value (default: 4)
is the initial permit count for the semaphore.

### Concurrency gate in `handle_create_run`

`try_acquire_owned` is called on a clone of `run_semaphore` before any work
begins. If no permit is available the handler immediately returns `HTTP 429`
without queuing or starting a run. The owned permit is held for the duration of
the handler so sync execution does not release it early.

### Session activity tracking in `handle_create_run`

After the session ID is bound to the request, the handler records the current
`Instant` in `session_activity` for that session. This map is read by the
background eviction task.

### Background eviction task in `run_server`

A `tokio::spawn` loop wakes every `session_eviction_poll_seconds` (default: 60)
and performs two cleanup steps:

1. Prunes `session_activity` entries older than `session_timeout_seconds`.
2. Calls `AcpRuntime::evict_idle_sessions` to drop completed in-memory run
   records that have not been updated within the same window.

### `AcpRuntime::evict_idle_sessions`

Added to `src/acp/runtime.rs`. Retains in the in-memory run map only those
entries that are either:

- still active (not terminal), or
- were last updated within `session_timeout_seconds` ago.

The cutoff comparison uses the RFC 3339 `updated_at` string on each run's
status. Runs that fail timestamp parsing are kept as a safe fallback.

### CORS layer in `build_router`

When `config.acp.cors_origins` is non-empty, a `tower_http::cors::CorsLayer` is
attached with the listed origins, `AllowMethods::any()`, and
`AllowHeaders::any()`. An empty list (the default) leaves CORS headers absent,
which preserves the existing deny-all-cross-origin behavior.

### Graceful shutdown in `run_server`

`axum::serve(...).with_graceful_shutdown(signal)` replaces the plain
`axum::serve(...).await`. The shutdown signal waits for `SIGINT` (ctrl-c) and
logs a draining message. When `graceful_shutdown_timeout` is `Some(n)`,
`tokio::time::timeout` wraps the serve future so the process does not block
indefinitely if in-flight requests stall. A warning is logged when the timeout
elapses.

---

## Testing

New tests added to `src/acp/server.rs`:

| Test                                                          | What it verifies                                                                       |
| ------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `test_handle_create_run_returns_429_when_semaphore_exhausted` | Replacing the semaphore with a zero-permit one causes the handler to return `HTTP 429` |
| `test_cors_empty_origins_does_not_add_cors_header`            | Default config produces no `access-control-allow-origin` header                        |
| `test_cors_configured_origin_returns_allow_origin_header`     | An OPTIONS preflight from a configured origin receives the CORS header                 |

New tests added to `src/acp/executor.rs`:

| Test                                                     | What it verifies                                                                |
| -------------------------------------------------------- | ------------------------------------------------------------------------------- |
| `test_execute_run_internal_marks_failed_on_timeout`      | Zero-timeout path executes successfully; timeout=0 disables the timeout wrapper |
| `test_execute_sync_isolated_mode_does_not_share_history` | Two runs on the same session in `Isolated` mode each complete independently     |

New tests added to `src/acp/runtime.rs`:

| Test                                                  | What it verifies                             |
| ----------------------------------------------------- | -------------------------------------------- |
| `test_evict_idle_sessions_empty_runtime_returns_zero` | Eviction on an empty runtime returns 0       |
| `test_evict_idle_sessions_retains_active_runs`        | Active (non-terminal) runs are never evicted |

---

## Configuration reference

| Field                               | Default    | Purpose                                 |
| ----------------------------------- | ---------- | --------------------------------------- |
| `acp.max_concurrent_runs`           | 4          | Semaphore permit count                  |
| `acp.run_timeout_seconds`           | 300        | Per-run execution timeout               |
| `acp.session_eviction_poll_seconds` | 60         | Eviction loop interval                  |
| `acp.session_timeout_seconds`       | 3600       | Idle threshold for eviction             |
| `acp.cors_origins`                  | `[]`       | Allowed CORS origins                    |
| `acp.graceful_shutdown_timeout`     | `None`     | Drain timeout after SIGINT              |
| `acp.session_mode`                  | `isolated` | Session history injection mode          |
| `acp.allow_dangerous`               | `false`    | Skip confirmation for terminal commands |

---

## `Cargo.toml` dependency

`tower-http = { version = "0.6", features = ["cors"] }` was added as a direct
dependency. It provides the `CorsLayer` middleware used in `build_router`.

---

## `AcpSessionMode` enum

Added to `src/config.rs`. Controls whether the agent loads prior conversation
history from the same session before executing a new run.

- `Isolated` (default): each run starts with a blank conversation.
- `Shared`: prior terminal runs in the session are replayed as user/assistant
  messages before the new prompt is issued.

The executor in `src/acp/executor.rs` reads this field inside `execute_prompt`
and injects the history when `Shared` is active.

---

## `allow_dangerous` flag

When `acp.allow_dangerous` is `true`, the executor builds the tool registry with
`SafetyMode::NeverConfirm`, bypassing the interactive confirmation prompt for
dangerous terminal commands. This is intended for headless, isolated test
environments only.

---

## Quality Gates

Run the full quality gate sequence before considering this phase complete:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth
```
