# Phase 1: Run Command Plan Fixtures -- Implementation Summary

## Overview

Phase 1 establishes the foundational fixture and scenario infrastructure for
evaluating the `xzatoma run` command. It provides YAML plan fixtures that
exercise both valid and invalid plan structures, a `scenarios.yaml` file that
drives data-driven integration tests, and a corresponding integration test
harness in `tests/eval_run_command.rs`.

## Deliverables

| File                                                  | Purpose                                          |
| ----------------------------------------------------- | ------------------------------------------------ |
| `evals/run_command/plans/simple_plan.yaml`            | Minimal valid plan with a single step            |
| `evals/run_command/plans/multi_step_plan.yaml`        | Valid plan with three steps                      |
| `evals/run_command/plans/invalid_no_steps.yaml`       | Plan with `steps: []` to trigger validation      |
| `evals/run_command/plans/invalid_no_name.yaml`        | Plan with `name: ""` to trigger validation       |
| `evals/run_command/plans/invalid_step_no_action.yaml` | Step missing `action` to trigger validation      |
| `evals/run_command/scenarios.yaml`                    | Nine scenario definitions with expected outcomes |
| `evals/run_command/README.md`                         | Usage instructions and architecture notes        |
| `tests/eval_run_command.rs`                           | Integration test harness that loads scenarios    |

## Architecture

### Fixture Design

Plan fixtures are plain YAML files that conform to the `Plan` struct expected by
`tools::plan::PlanParser`. Each fixture targets a specific parsing or validation
branch:

- **Valid fixtures** (`simple_plan.yaml`, `multi_step_plan.yaml`) confirm that
  well-formed plans parse successfully and reach the provider instantiation
  boundary.
- **Invalid fixtures** (`invalid_no_steps.yaml`, `invalid_no_name.yaml`,
  `invalid_step_no_action.yaml`) confirm that the `PlanParser` validation layer
  rejects malformed plans with the correct error messages before any provider
  interaction occurs.

### Scenario Schema

The `scenarios.yaml` file defines a list of scenario objects with the following
fields:

- `id` -- unique identifier for test output
- `description` -- human-readable explanation
- `test_mode` -- optional; `parse_only` bypasses provider setup
- `input` -- contains `plan_file`, `prompt`, and `allow_dangerous`
- `expect` -- contains `outcome` (`error` or `ok`) and optional `error_contains`
  substring

### Test Modes

The harness supports two execution paths:

1. **Full execution** (default) -- calls `run_plan_with_options` with an
   `invalid_provider` configuration. The command proceeds through CLI argument
   validation, plan parsing, and provider instantiation, then fails
   deterministically at the provider boundary. This validates the entire path up
   to provider creation without network access.

2. **Parse-only** (`test_mode: parse_only`) -- calls `PlanParser::from_file`
   directly. This isolates plan parsing and validation logic from the rest of
   the command pipeline, providing faster and more targeted feedback on fixture
   correctness.

### Offline and Deterministic Execution

All scenarios run without network access. The `offline_config()` helper builds a
`Config` with `provider_type` set to `"invalid_provider"`, which causes provider
instantiation to fail with `"Unknown provider"`. This design ensures:

- Tests are reproducible across environments
- No API keys or external services are required
- Failures are deterministic and inspectable

## Scenarios Covered

| ID                            | Mode       | Tests                                        |
| ----------------------------- | ---------- | -------------------------------------------- |
| `no_input`                    | full       | Missing both `--plan` and `--prompt`         |
| `prompt_only`                 | full       | Prompt reaches provider boundary             |
| `simple_plan`                 | full       | Single-step plan parses and reaches provider |
| `multi_step_plan`             | full       | Multi-step plan parses and reaches provider  |
| `plan_invalid_no_steps`       | parse_only | Empty steps list rejected                    |
| `plan_invalid_no_name`        | parse_only | Empty plan name rejected                     |
| `plan_invalid_step_no_action` | parse_only | Step with no action rejected                 |
| `plan_file_not_found`         | parse_only | Non-existent file produces read error        |
| `allow_dangerous_with_prompt` | full       | Dangerous flag with prompt reaches provider  |

## Success Criteria Verification

- All five fixture files parse without error via `PlanParser::from_file` (valid
  fixtures) or produce the expected validation error (invalid fixtures).
- `scenarios.yaml` deserializes cleanly into the `ScenarioFile` struct used by
  the integration test.
- All nine scenarios pass: `cargo test --test eval_run_command -- --nocapture`
  reports 9 passed, 0 failed.

## Running the Eval

```bash
cargo test --test eval_run_command -- --nocapture
```

The `--nocapture` flag prints per-scenario pass/fail lines for visibility.

## Design Decisions

1. **YAML fixtures over inline strings** -- Fixtures live on disk so they can be
   inspected, reused, and extended without modifying Rust code.
2. **Scenario-driven testing** -- Adding a new test case requires only a YAML
   entry and optionally a new fixture file, with no Rust changes.
3. **Invalid provider for offline execution** -- Using an impossible provider
   type guarantees deterministic failure at a well-defined boundary.
4. **Separate parse-only mode** -- Validation tests that do not need the full
   command pipeline run faster and produce clearer error messages.
