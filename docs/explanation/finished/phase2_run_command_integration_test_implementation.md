# Phase 2: Run Command Integration Test -- Implementation Summary

## Overview

Phase 2 delivers the integration test harness that loads scenario definitions
from `evals/run_command/scenarios.yaml` and executes each one against the
appropriate layer of the `xzatoma run` command. The harness is data-driven:
adding a new test case requires only a YAML entry (and optionally a new fixture
file), with no Rust code changes.

## Deliverables

| File                        | Purpose                                                |
| --------------------------- | ------------------------------------------------------ |
| `tests/eval_run_command.rs` | Integration test that loads and executes all scenarios |

## Test Harness Design

### Scenario Schema Structs

The test defines three `serde::Deserialize` structs that mirror the YAML schema
established in Phase 1:

- **`ScenarioFile`** -- top-level wrapper containing a `Vec<Scenario>`.
- **`Scenario`** -- individual test case with `id`, `description`, optional
  `test_mode`, `input`, and `expect`.
- **`ScenarioInput`** -- holds `plan_file` (relative path), `prompt` (direct
  text), and `allow_dangerous` (boolean flag).
- **`ScenarioExpect`** -- holds `outcome` (`"error"` or `"ok"`) and optional
  `error_contains` substring.

### Scenario Loading

The harness locates fixtures at runtime using `env!("CARGO_MANIFEST_DIR")` to
build an absolute path to `evals/run_command/scenarios.yaml`. This ensures the
test works regardless of the working directory from which `cargo test` is
invoked.

### Dispatch Logic

A single `#[tokio::test]` function (`eval_run_command_scenarios`) iterates over
every scenario and dispatches based on the `test_mode` field:

- **`parse_only`** -- calls `PlanParser::from_file` directly, testing plan
  parsing and validation in isolation from the command pipeline.
- **Default (no `test_mode`)** -- calls `run_plan_with_options` with an
  `offline_config()` that sets `provider_type` to `"invalid_provider"`. The
  command proceeds through argument validation, plan parsing, and provider
  instantiation, then fails deterministically at the provider boundary.

### Result Checking

The `check_result` helper normalizes both execution paths into
`anyhow::Result<()>` and asserts:

1. The outcome matches `"error"` or `"ok"`.
2. When `error_contains` is specified, the error message includes the expected
   substring.

### Reporting

The harness collects per-scenario pass/fail results and prints a summary line
for each. If any scenario fails, it panics with a consolidated list of all
failures, making it straightforward to identify which scenarios need attention.

## Offline and Deterministic Execution

All scenarios run without network access. The `offline_config()` helper builds a
`Config` with `provider_type` set to `"invalid_provider"`, causing provider
instantiation to fail with `"Unknown provider"`. This guarantees:

- Reproducible results across environments
- No API keys or external services required
- Deterministic, inspectable failure modes

## Key Implementation Details

### Helper Functions

| Function           | Purpose                                                    |
| ------------------ | ---------------------------------------------------------- |
| `evals_dir()`      | Returns absolute path to `evals/run_command/`              |
| `plans_dir()`      | Returns absolute path to `evals/run_command/plans/`        |
| `offline_config()` | Builds a `Config` with an invalid provider for offline use |
| `run_scenario()`   | Dispatches a scenario to the correct execution path        |
| `check_result()`   | Asserts outcome and error substring match expectations     |

### Error Handling in Tests

The harness uses `unwrap_or_else` with descriptive panic messages when loading
the scenarios file or parsing YAML. This provides clear diagnostics if the
fixture infrastructure is missing or malformed, rather than opaque
deserialization errors.

## Success Criteria Verification

- `cargo test --test eval_run_command` exits 0 with all 9 scenarios passing.
- No existing library tests broken (1627 passed, 0 failed, 28 ignored).
- No network calls made during test execution.
- `cargo fmt`, `cargo check`, and `cargo clippy` all pass cleanly.

## Running the Test

```bash
cargo test --test eval_run_command -- --nocapture
```

The `--nocapture` flag prints per-scenario pass/fail output for visibility.

## Relationship to Other Phases

- **Phase 1** (Run Command Plan Fixtures) provides the fixture files and
  `scenarios.yaml` that this test consumes.
- **Phase 3** (Run Command Documentation Update) documents the eval structure
  and how to extend it in the `evals/run_command/README.md`.
- Later phases (4 through 8) follow the same pattern: YAML scenarios loaded by
  an integration test, with fixtures on disk and offline execution.
