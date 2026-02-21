# Eval System Implementation Plan

## Overview

Implement a data-driven eval system for the `xzatoma run` command that validates
CLI argument parsing, plan parsing/validation, and the `--allow-dangerous` flag
path. Scenarios are defined in YAML, plan fixtures live alongside them, and a
Rust integration test drives execution against an intentionally invalid provider
so all scenarios remain deterministic and offline.

## Current State Analysis

### Existing Infrastructure

- `src/commands/mod.rs` — `run::run_plan_with_options(config, plan_path, prompt, allow_dangerous)` is the primary entry point for the `run` command.
- `src/tools/plan.rs` — `PlanParser` supports YAML, JSON, and Markdown plan files; `Plan::validate()` enforces name, steps, and action presence.
- `src/cli.rs` — `Commands::Run` defines `--plan`, `--prompt`, and `--allow-dangerous` flags.
- `src/test_utils.rs` — `temp_dir()`, `create_test_file()`, `test_config()` helpers available to tests.
- `tests/` — Integration tests use `tempfile`, `assert_cmd`, `predicates`, and `tokio::test`.
- `evals/run_command/README.md` — Describes the intended structure (`scenarios.yaml`, `plans/`) but neither file exists yet.

### Identified Issues

- `evals/run_command/` has no actual scenario definitions or plan fixtures.
- No integration test exercises the eval scenarios end-to-end.
- The README describes the intended structure but it has never been implemented.

## Implementation Phases

### Phase 1: Plan Fixtures

#### Task 1.1 Create `evals/run_command/plans/` directory with fixture files

Create the following YAML plan fixtures under `evals/run_command/plans/`:

| File | Purpose |
|---|---|
| `simple_plan.yaml` | Minimal valid plan (1 step) |
| `multi_step_plan.yaml` | Valid plan with 3 steps |
| `invalid_no_steps.yaml` | Plan with `steps: []` — triggers validation error |
| `invalid_no_name.yaml` | Plan with `name: ""` — triggers validation error |
| `invalid_step_no_action.yaml` | Plan with a step missing `action` — triggers step-level validation error |

Each fixture uses the `Plan` / `PlanStep` YAML schema from `src/tools/plan.rs`.

#### Task 1.2 Create `evals/run_command/scenarios.yaml`

Define 9 scenarios covering:

- No input (neither `--plan` nor `--prompt`) → error: `"Either --plan or --prompt must be provided"`
- Prompt-only input → reaches provider, fails with `"Unknown provider"`
- Valid single-step plan → reaches provider, fails with `"Unknown provider"`
- Valid multi-step plan → reaches provider, fails with `"Unknown provider"`
- Plan with no steps → `"Plan must have at least one step"`
- Plan with empty name → `"Plan name cannot be empty"`
- Plan with step missing action → `"has no action"`
- Non-existent plan file → `"Failed to read plan file"`
- `--allow-dangerous` with prompt → reaches provider, fails with `"Unknown provider"`

Each scenario entry has: `id`, `description`, `input` (`plan_file` or `prompt`, optional `allow_dangerous`), and `expect` (`outcome: error|ok`, optional `error_contains`).

#### Task 1.3 Deliverables

- `evals/run_command/plans/simple_plan.yaml`
- `evals/run_command/plans/multi_step_plan.yaml`
- `evals/run_command/plans/invalid_no_steps.yaml`
- `evals/run_command/plans/invalid_no_name.yaml`
- `evals/run_command/plans/invalid_step_no_action.yaml`
- `evals/run_command/scenarios.yaml`

#### Task 1.4 Success Criteria

- All fixture files parse without error via `PlanParser::from_file`.
- `scenarios.yaml` deserializes cleanly into the `Scenario` struct used by the test.

---

### Phase 2: Integration Test

#### Task 2.1 Create `tests/eval_run_command.rs`

Add a new integration test file that:

1. Defines `Scenario`, `ScenarioInput`, and `ScenarioExpect` structs with `serde::Deserialize`.
2. Loads `evals/run_command/scenarios.yaml` using `env!("CARGO_MANIFEST_DIR")` for a stable path.
3. Iterates over all scenarios in a single `#[tokio::test]` (named `eval_run_command_scenarios`).
4. For each scenario:
   - Builds a `Config` with `provider.provider_type = "invalid_provider"` (offline, deterministic).
   - Resolves `plan_file` paths relative to `evals/run_command/plans/`.
   - Calls `xzatoma::commands::run::run_plan_with_options(config, plan_path, prompt, allow_dangerous).await`.
   - Asserts `result.is_err()` when `outcome == "error"`, or `result.is_ok()` when `outcome == "ok"`.
   - Asserts `error_contains` substring is present in the error message when specified.
   - Prints a clear per-scenario failure message on assertion failure.

#### Task 2.2 Integrate with Cargo test suite

No `Cargo.toml` changes are needed — `serde_yaml`, `tokio`, and `tempfile` are already in `[dev-dependencies]`. The new file in `tests/` is automatically picked up by Cargo.

#### Task 2.3 Testing Requirements

- Run `cargo test --test eval_run_command -- --nocapture` to execute only the eval tests.
- Run `cargo test` to confirm no regressions across the full suite.
- All 9 scenarios must pass.

#### Task 2.4 Deliverables

- `tests/eval_run_command.rs`

#### Task 2.5 Success Criteria

- `cargo test --test eval_run_command` exits 0 with all 9 scenarios passing.
- No existing tests are broken (`cargo test` exits 0).
- No network calls are made during the test run.

---

### Phase 3: Documentation Update

#### Task 3.1 Update `evals/run_command/README.md`

Expand the README to document:

- How to add a new scenario (edit `scenarios.yaml`, optionally add a fixture to `plans/`).
- How to run only the eval tests (`cargo test --test eval_run_command`).
- The offline/deterministic design rationale (invalid provider).
- The scenario YAML schema (`id`, `description`, `input`, `expect`).

#### Task 3.2 Deliverables

- Updated `evals/run_command/README.md`

#### Task 3.3 Success Criteria

- README accurately reflects the implemented structure and is sufficient for a new contributor to add a scenario without reading the test source.
