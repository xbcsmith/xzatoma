# Phase 3 Refactor Implementation

## Overview

Phase 3 reduces duplication in integration tests and narrows the `tools` public API surface.

## Components Delivered

- `tests/common/mod.rs` - Shared helpers for temporary SQLite storage and temporary config files.
- `tests/conversation_persistence_integration.rs` - Uses shared temp storage helper.
- `tests/integration_history_tool_integrity.rs` - Uses shared temp storage helper.
- `tests/integration_subagent.rs` - Uses shared temp config helper.
- `src/tools/mod.rs` - Changed convenience re-exports to `pub(crate)` for fetch/subagent/parallel/plan/registry builder symbols.
- `tests/integration_parallel.rs` - Switched imports to `xzatoma::tools::parallel_subagent::{...}`.

## Implementation Details

### Shared integration test helpers

Created a shared test module to avoid repeated setup logic for:

- temporary SQLite storage (`create_temp_storage()`)
- temporary `config.yaml` creation (`temp_config_file()`)

### Reduced public re-export surface

In `src/tools/mod.rs`, convenience re-exports that were not required as part of the external `xzatoma::tools` API were made crate-private using `pub(crate) use`:

- fetch symbols (`FetchTool`, `FetchedContent`, `RateLimiter`, `SsrfValidator`)
- subagent symbols (`SubagentTool`, `SubagentToolInput`)
- parallel subagent symbols (`ParallelSubagentInput`, `ParallelSubagentOutput`, `ParallelSubagentTool`, `ParallelTask`, `TaskResult`)
- plan and plan-format symbols (`load_plan`, `parse_plan`, `Plan`, `PlanParser`, `PlanStep`, `detect_plan_format`, `validate_plan`, `PlanFormat`, `ValidatedPlan`)
- `ToolRegistryBuilder`

Integration tests that relied on the old public re-export path were updated to import from the public module path directly.

## Testing

Commands executed:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

```text
test result: ok. 789 passed; 0 failed; 8 ignored; 0 measured; 0 filtered out
```

## Validation Results

- `cargo fmt --all` passed
- `cargo check --all-targets --all-features` passed
- `cargo clippy --all-targets --all-features -- -D warnings` passed
- `cargo test --all-features` passed
