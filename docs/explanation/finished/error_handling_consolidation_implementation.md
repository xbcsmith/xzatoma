# Error Handling Consolidation Implementation

## Overview

This document summarizes the Phase 2 error handling consolidation work from
`docs/explanation/codebase_cleanup_plan.md`.

The goal of this effort was to move the codebase toward a single typed result
strategy based on `crate::error::Result<T>` and `XzatomaError`, reduce silent
error loss, remove overlapping ACP error handling paths, and standardize tool
argument parsing.

## Implemented Changes

### 1. Consolidated crate-wide result handling

The crate-level alias in `src/error.rs` uses:

- `pub type Result<T> = std::result::Result<T, XzatomaError>`

This makes typed error matching the default behavior at crate boundaries and
avoids relying on implicit dynamic wrapping for the primary project result
alias.

### 2. Added centralized tool argument parsing

A shared helper was added to `src/error.rs` and re-exported from
`src/tools/mod.rs`:

- `parse_tool_args<T>(args: serde_json::Value) -> Result<T>`

This standardizes invalid tool input handling through
`XzatomaError::Tool("Invalid tool parameters: ...")`.

The helper was adopted across tool implementations, including:

- `src/tools/activate_skill.rs`
- `src/tools/copy_path.rs`
- `src/tools/create_directory.rs`
- `src/tools/delete_path.rs`
- `src/tools/edit_file.rs`
- `src/tools/find_path.rs`
- `src/tools/list_directory.rs`
- `src/tools/move_path.rs`
- `src/tools/parallel_subagent.rs`
- `src/tools/read_file.rs`
- `src/tools/subagent.rs`
- `src/tools/write_file.rs`

### 3. Collapsed overlapping ACP error conversion paths

`XzatomaError` now includes a consolidated ACP variant:

- `Acp(#[from] crate::acp::error::AcpError)`

This replaces the older split ACP mapping model and allows ACP domain errors to
flow through the crate-wide error type via `From` conversions.

In addition:

- `impl From<AcpValidationError> for XzatomaError` was added
- ACP validation helpers in `src/acp/error.rs` now return
  `crate::error::Result<()>`
- the ACP-local `Result<T>` alias was removed

This improves consistency between ACP-local validation and crate-level error
propagation.

### 4. Added missing `From` conversions into `XzatomaError`

`src/error.rs` now centralizes conversions from module-local error types into
appropriate `XzatomaError` variants, including mappings for:

- `AcpValidationError`
- `FileMetadataError`
- `FileUtilsError`
- `CommandError`
- `rustyline::error::ReadlineError`
- `GenericWatcherError`
- `WatcherError`
- `ConsumerError`
- `ClientError`
- watcher `ConfigError`

These conversions preserve typed propagation with `?` at module boundaries.

### 5. Replaced `anyhow::anyhow!(...)` usage in key production paths

Several production call sites were updated to construct typed errors directly
instead of wrapping with ad hoc dynamic error creation.

Examples include:

- `src/agent/quota.rs`
- `src/mention_parser.rs`
- `src/mcp/transport/http.rs`
- `src/mcp/transport/fake.rs`
- `src/mcp/transport/stdio.rs`

This keeps error provenance explicit and makes matching on `XzatomaError`
variants straightforward.

### 6. Standardized watcher result imports

Watcher modules using direct `anyhow::Result` imports were updated to use the
crate result alias instead.

This included:

- `src/watcher/generic/matcher.rs`
- `src/watcher/generic/watcher.rs`
- `src/watcher/xzepr/filter.rs`

This change aligns watcher code with the crate-wide error strategy.

### 7. Audited silent `let _ =` error discards in production code

A focused audit was applied to production sites where results were discarded.

#### `src/acp/executor.rs`

Discarded runtime calls were replaced with one of:

- direct propagation using `?`
- warning logs when background cleanup/reporting fails

This affected:

- `get_run`
- `mark_queued`
- `mark_running`
- `append_output_message`
- `complete_run`
- background failure reporting
- terminal failure recording

#### `src/acp/runtime.rs`

Silent restoration and broadcast failures were addressed by:

- logging failed startup restoration in `new`
- logging failed event broadcast in `create_run`
- removing unnecessary discarded success values in restore flow

#### `src/agent/metrics.rs`

Prometheus exporter installation now logs failures explicitly instead of
discarding the result.

#### `src/mcp/auth/flow.rs`

Browser launch failures and callback response flush failures are now logged
instead of being silently ignored.

#### `src/mcp/client.rs`

Closed channel send failures are now handled with debug logging in places where
the receiver may legitimately already be gone.

#### `src/commands/mod.rs`

Best-effort operations now include explicit intent:

- readline history insertion failures are intentionally treated as non-fatal and
  logged at debug level
- watcher shutdown signal send failures are logged when the receiver is already
  dropped

### 8. Reduced double error reporting

In `src/providers/ollama.rs`, error paths that both logged and returned the same
error were simplified so the provider returns typed errors without duplicate
error-level logging at the same boundary.

This follows the cleanup plan guidance that callers should control final
reporting behavior.

### 9. Simplified MCP auth error detection

`src/mcp/manager.rs` was updated so MCP auth detection works directly on
`&XzatomaError` instead of inspecting a dynamic error chain.

This is an important effect of the typed result migration and removes the need
for downcasting in this path.

## Testing and Validation Additions

Additional tests were added in `src/error.rs` to validate:

- successful tool argument parsing
- invalid tool argument mapping to `XzatomaError::Tool`
- conversion from `AcpValidationError` to `XzatomaError::Acp`

These tests exercise the new shared parsing path and one of the new typed error
conversion chains.

## Remaining Work

This implementation moved the project substantially toward the Phase 2 target,
but some planned cleanup may still remain elsewhere in the codebase, especially
in:

- documentation examples that still reference `anyhow::Result`
- any remaining dynamic error utility usage outside the edited production paths
- broader `let _ =` cleanup in tests and doc examples
- possible dependency cleanup in `Cargo.toml` if `anyhow` becomes entirely
  unused

## Deliverables Covered

This implementation addresses the Phase 2 deliverables by providing:

- a single crate-wide typed `Result` alias
- additional `From` conversions into `XzatomaError`
- audited and resolved production `let _ =` sites in key modules
- a shared `parse_tool_args` helper used across tools
- consistent watcher result imports through `crate::error::Result`

## Notes

This summary reflects the implementation work completed for the Phase 2 error
handling consolidation effort and is intended to accompany the corresponding
source changes as the required explanation document for the task.
