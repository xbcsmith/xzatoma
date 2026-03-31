# Phase 3: Run Command Documentation Update -- Implementation Summary

## Overview

Phase 3 enhances the `evals/run_command/README.md` to serve as a complete
reference for the run command eval suite. The update adds an explicit scenario
YAML schema reference, a plan fixtures inventory, a scenario summary table, and
an expanded architecture section describing both execution modes.

## Deliverables

| File                          | Purpose                                           |
| ----------------------------- | ------------------------------------------------- |
| `evals/run_command/README.md` | Complete reference for the run command eval suite |

## Changes Made

### Plan Fixtures Inventory

Added a table listing every fixture file in the `plans/` directory with its
validity status and purpose. This gives contributors a quick overview of
available fixtures without having to read each file individually.

### Scenario YAML Schema

Added a formal schema reference table documenting every field in the scenario
YAML format:

- `id` -- unique identifier shown in test output (required)
- `description` -- human-readable explanation (required)
- `test_mode` -- optional; set to `parse_only` to call `PlanParser` directly
- `input.plan_file` -- path to a plan file, relative to `plans/`
- `input.prompt` -- direct prompt text passed as `--prompt`
- `input.allow_dangerous` -- whether to pass `--allow-dangerous` (default false)
- `expect.outcome` -- expected result: `error` or `ok` (required)
- `expect.error_contains` -- substring that must appear in the error message

This schema section eliminates ambiguity for contributors adding new scenarios.

### Scenario Summary Table

Added a table listing all nine scenarios with their IDs, test modes, and
descriptions. This provides a quick reference of what is already covered without
having to read the full `scenarios.yaml` file.

### Expanded Architecture Section

Expanded the architecture section to clearly distinguish the two execution
modes:

- **Full execution scenarios** (default) -- use an `invalid_provider`
  configuration to exercise the complete command path up to provider
  instantiation without network access.
- **Parse-only scenarios** (`test_mode: parse_only`) -- call `PlanParser`
  directly, bypassing provider setup for faster and more targeted plan
  validation testing.

Added an explicit note that no API keys, external services, or network access
are required for any scenario.

## README Structure

The updated README is organized into the following sections:

1. **Contents** -- file listing for the directory
2. **Plan Fixtures** -- inventory table of all fixture files
3. **Scenario YAML Schema** -- field reference table
4. **Scenarios** -- summary table of all defined scenarios
5. **Scope** -- what the scenarios validate
6. **Running Evals** -- command to execute the test
7. **Adding Scenarios** -- step-by-step guide with YAML example
8. **Architecture** -- offline/deterministic design rationale

## Success Criteria Verification

- README accurately reflects the implemented structure, including all fixture
  files, all nine scenarios, and both test execution modes.
- README passes `markdownlint` and `prettier` checks.
- All Rust quality gates continue to pass (`cargo fmt`, `cargo check`,
  `cargo clippy`, `cargo test`).

## Relationship to Other Phases

- **Phase 1** (Run Command Plan Fixtures) created the fixture files and
  `scenarios.yaml` that the README documents.
- **Phase 2** (Run Command Integration Test) created the test harness that the
  README instructs contributors how to run.
- Later phases follow the same documentation pattern, with each eval suite
  directory containing its own `README.md` with schema reference, fixture
  inventory, and architecture notes.
