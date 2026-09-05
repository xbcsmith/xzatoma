# Phase 2 Error-Handling Standardization Implementation

## Overview

Phase 2 of the codebase cleanup plan standardizes error handling across XZatoma
so that error context is preserved and production code no longer panics on
fallible operations. It migrates the storage layer to structured error helpers,
converts fallible constructors and panics into `Result` values, compiles
one-shot regexes once, justifies every remaining production `unwrap`/`expect`,
and enforces that policy with crate-wide clippy lints.

All work in this document is complete. Every quality gate in `AGENTS.md` passes:
`cargo fmt --all`, `cargo check --all-targets --all-features`,
`cargo clippy --all-targets --all-features -- -D warnings` (now including
`clippy::unwrap_used` and `clippy::expect_used`), and
`cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth`
(2342 tests pass).

## Current State Analysis

### Existing Infrastructure

- `src/error.rs` already defined structured storage variants
  (`StorageDatabaseOpen`, `StorageMigration`, `StorageQuery`,
  `StorageRowDecode`, `StorageSerialization`, `StoragePersistencePath`) with a
  `#[source]` chain.
- `src/storage/mod.rs` already defined the matching helper constructors
  (`storage_query_error`, `storage_row_decode_error`, etc.) and used them in the
  newer code paths.
- Newer ACP code paths preserved error sources; older storage methods discarded
  them via `map_err(|e| XzatomaError::Storage(e.to_string()))`.

### Identified Issues

- 44 storage sites collapsed rich errors into a flat string, losing the source
  chain.
- `HttpTransport::new` used `.expect("failed to build reqwest client")` on a
  fallible constructor.
- `parallel_subagent.rs` used `panic!("Semaphore closed")` on a recoverable
  condition.
- `fetch.rs::html_to_markdown` recompiled 9 regexes with
  `Regex::new(...).unwrap()` on every call.
- Several production `Mutex::lock().unwrap()` and constant-regex `expect` sites
  carried no justification.
- No lint enforced the "no unjustified unwrap/expect" policy, so regressions
  could slip in.

## Implementation Phases

### Phase 2: Error-Handling Standardization

#### Task 2.1 Feature Work

- Storage: all 44 `map_err(|e| XzatomaError::Storage(e.to_string()))` sites in
  `src/storage/mod.rs` now call the appropriate structured helper
  (`storage_database_open_error`, `storage_query_error`,
  `storage_serialization_error`, etc.), removing the paired `.context(...)` and
  turning the context string into the helper's operation phrase. The now-unused
  `anyhow::Context` import was removed.
- `src/mcp/transport/http.rs`: `HttpTransport::new` now returns `Result<Self>`
  and maps the reqwest build failure to `XzatomaError::McpTransport`. Both
  callers in `src/mcp/manager.rs` `build_transport` propagate with `?`.
- `src/tools/parallel_subagent.rs`: the `panic!("Semaphore closed")` was
  replaced by returning a failed `TaskResult` (label preserved,
  `success: false`, populated `error`) so a closed semaphore is handled, not
  fatal.
- `src/tools/fetch.rs`: the 9 inline regexes in `html_to_markdown` are now
  module-level `LazyLock<regex::Regex>` statics (`SCRIPT_TAG_RE`,
  `STYLE_TAG_RE`, `PARAGRAPH_RE`, `ANCHOR_RE`, `BOLD_RE`, `ITALIC_RE`,
  `LINE_BREAK_RE`, `HTML_TAG_RE`, `WHITESPACE_RE`) compiled once.

#### Task 2.2 Integrate Feature

- Justified the remaining production locks/expects with `// SAFETY:` comments
  plus `#[allow(clippy::unwrap_used)]` / `#[allow(clippy::expect_used)]`:
  `agent/core.rs` accumulation locks (~778, ~1239, ~1707) and streaming-callback
  locks (function-scoped allow on `execute_with_observer` and
  `execute_provider_messages_with_observer`), `commands/mod.rs` skill-registry
  lock (~854) and the `prompt` invariant expect (~2319), `tools/terminal.rs`
  constant denylist-regex expect (~177), `mcp/server.rs` constant id-regex
  expect, and the `security.rs` / `fetch.rs` constant-regex statics.
- Converted the five `SystemTime::now().duration_since(UNIX_EPOCH).unwrap()`
  sites in `providers/copilot.rs` to `.unwrap_or_default()`, removing the panic
  entirely (a pre-epoch clock simply yields a zero duration, treating tokens as
  expired).
- Made fire-and-forget discards explicit with intent comments in
  `mcp/transport/http.rs` (SSE forwarding and `Drop` session teardown) and
  clarified the in-memory `table.print` discard in `commands/models.rs`.

#### Task 2.3 Configuration Updates

- Added `#![warn(clippy::unwrap_used, clippy::expect_used)]` to `src/lib.rs` and
  `src/main.rs`, promoted to errors by the existing `-D warnings` policy.
- Added `clippy.toml` with `allow-unwrap-in-tests = true` and
  `allow-expect-in-tests = true` so the restriction lints target production code
  only; doc-test code is unaffected because doc examples do not inherit the
  crate-level attribute.

#### Task 2.4 Testing Requirements

- `storage/mod.rs`: `test_list_sessions_missing_db_preserves_error_source`,
  `test_list_sessions_corrupt_db_preserves_error_source`, and
  `test_new_with_path_uncreatable_parent_preserves_error_source` assert the
  returned error is the expected structured variant and that
  `std::error::Error::source()` is `Some`.
- `parallel_subagent.rs`:
  `test_parallel_execute_closed_semaphore_yields_failed_result` validates the
  failed-`TaskResult` shape produced when the semaphore is closed.
- `mcp/transport/http.rs`:
  `test_http_transport_new_returns_ok_for_valid_endpoint` validates the fallible
  constructor's `Ok` path.

#### Task 2.5 Deliverables

- [x] `storage/mod.rs` fully migrated to structured error helpers (0 remaining
      `Storage(e.to_string())`).
- [x] No production `panic!`/`.expect()` on genuinely fallible operations.
- [x] Static regexes in `fetch.rs`; all production `unwrap`/`expect` either
      removed or justified.
- [x] Clippy `unwrap_used`/`expect_used` enforced for production builds.

#### Task 2.6 Success Criteria

- Production `unwrap`/`expect` are either removed or carry an explicit
  `#[allow(...)]` plus justification;
  `cargo clippy --all-targets --all-features -- -D warnings` passes with the new
  lints enabled; storage error chains are observable in tests via `source()`.

## Notes

- The five copilot `SystemTime` sites and the streaming-callback locks were not
  in the plan's explicit line list but were surfaced by enabling the new lints
  crate-wide; they were resolved as part of Task 2.3's "verify the lint does not
  fire" requirement.
- `agent/core.rs::execute_with_observer` and
  `execute_provider_messages_with_observer` contain byte-identical streaming
  closure blocks; this duplication is a candidate for the Phase 5 DRY work.
- The remaining `panic!("Semaphore closed")` string in `parallel_subagent.rs`
  appears only inside a test comment documenting the replaced behavior.
