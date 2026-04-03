# Phase 5: Plan Parsing Format Evals Implementation

## Overview

Phase 5 extends the run command eval suite to cover all three plan formats
supported by `PlanParser`: YAML, JSON, and Markdown. It also adds edge-case
fixtures for empty files, malformed content, and unsupported file extensions.

Before this phase, the eval suite only exercised YAML plan fixtures (9
scenarios). After this phase, the suite covers 19 scenarios across all supported
formats and error paths.

## What Was Implemented

### New Fixture Files (10 files in `evals/run_command/plans/`)

| File                    | Format   | Valid | Purpose                                        |
| ----------------------- | -------- | ----- | ---------------------------------------------- |
| `simple_plan.json`      | JSON     | Yes   | Minimal valid single-step plan                 |
| `multi_step_plan.json`  | JSON     | Yes   | Valid plan with three steps                    |
| `invalid_no_steps.json` | JSON     | No    | Empty steps array triggers validation error    |
| `simple_plan.md`        | Markdown | Yes   | Single-step plan using H1/H2 heading structure |
| `multi_step_plan.md`    | Markdown | Yes   | Three-step plan with code fence contexts       |
| `invalid_no_steps.md`   | Markdown | No    | Plan with no H2 step headings                  |
| `empty_file.yaml`       | YAML     | No    | Zero-byte file triggers deserialization error  |
| `malformed_yaml.yaml`   | YAML     | No    | Broken YAML syntax triggers parse error        |
| `malformed_json.json`   | JSON     | No    | Invalid JSON syntax triggers parse error       |
| `unknown_extension.txt` | Text     | No    | Valid content but unsupported file extension   |

### New Scenarios (10 entries added to `evals/run_command/scenarios.yaml`)

All new scenarios use `test_mode: parse_only`, which calls
`PlanParser::from_file` directly without provider instantiation:

| Scenario ID             | Outcome | Error Assertion                    |
| ----------------------- | ------- | ---------------------------------- |
| `json_simple_plan`      | ok      | --                                 |
| `json_multi_step_plan`  | ok      | --                                 |
| `json_invalid_no_steps` | error   | "Plan must have at least one step" |
| `md_simple_plan`        | ok      | --                                 |
| `md_multi_step_plan`    | ok      | --                                 |
| `md_invalid_no_steps`   | error   | "Plan must have at least one step" |
| `empty_yaml_file`       | error   | "missing field `name`"             |
| `malformed_yaml`        | error   | "invalid type: sequence"           |
| `malformed_json`        | error   | "key must be a string"             |
| `unsupported_extension` | error   | "Unsupported plan format"          |

### Updated Documentation

- `evals/run_command/README.md` updated with:
  - Fixture tables organized by format (YAML, JSON, Markdown, Other)
  - Scenario tables split into Core (Phase 1) and Multi-Format (Phase 5) groups
  - New "Supported Plan Formats" section explaining the three formats
  - Updated "Adding Scenarios" instructions mentioning all three extensions

## Design Decisions

### All New Scenarios Use `parse_only` Mode

The Phase 5 fixtures test format parsing, not provider interaction. Using
`parse_only` mode calls `PlanParser::from_file` directly, which is faster and
produces more targeted error messages. Full execution mode would only add
unnecessary provider-boundary noise.

### Precise `error_contains` Values

Each error scenario includes an `error_contains` substring that matches the
actual error message produced by the parser. These were verified empirically
against the current `serde_yaml`, `serde_json`, and `PlanParser::validate`
implementations:

- **Empty YAML**: `serde_yaml` reports `missing field 'name'` because it parses
  the empty input as a unit value that lacks the required `name` field.
- **Malformed YAML**: `serde_yaml` reports `invalid type: sequence` because the
  unclosed bracket syntax produces a sequence where a string was expected.
- **Malformed JSON**: `serde_json` reports `key must be a string` because the
  trailing comma before `]` creates an invalid key position.
- **Unsupported extension**: `PlanParser::from_file` produces
  `Unsupported plan format: txt` via the extension match fallback.

### Markdown Plan Structure Convention

The Markdown fixtures follow the parsing rules defined in
`PlanParser::from_markdown`:

- `# H1` heading becomes the plan name
- First non-empty line after H1 (outside any step) becomes the description
- `## H2` headings become step names
- First non-empty line under a step becomes the action
- Fenced code blocks become step context

The `invalid_no_steps.md` fixture has an H1 heading and a description paragraph
but no H2 headings, which means `steps` remains empty and validation rejects the
plan.

## Test Results

```text
Eval results: 19 passed, 0 failed out of 19 scenarios

Breakdown:
  - 9 original scenarios (Phase 1): all passing
  - 10 new scenarios (Phase 5): all passing
```

## Quality Gates

All mandatory quality gates passed:

- `cargo fmt --all` -- no formatting changes needed
- `cargo check --all-targets --all-features` -- clean
- `cargo clippy --all-targets --all-features -- -D warnings` -- no warnings
- `cargo test --test eval_run_command` -- 19/19 scenarios passing
- `markdownlint` and `prettier` -- applied to all changed Markdown files

## Files Changed

| File                                                                  | Action  |
| --------------------------------------------------------------------- | ------- |
| `evals/run_command/plans/simple_plan.json`                            | Created |
| `evals/run_command/plans/multi_step_plan.json`                        | Created |
| `evals/run_command/plans/invalid_no_steps.json`                       | Created |
| `evals/run_command/plans/simple_plan.md`                              | Created |
| `evals/run_command/plans/multi_step_plan.md`                          | Created |
| `evals/run_command/plans/invalid_no_steps.md`                         | Created |
| `evals/run_command/plans/empty_file.yaml`                             | Created |
| `evals/run_command/plans/malformed_yaml.yaml`                         | Created |
| `evals/run_command/plans/malformed_json.json`                         | Created |
| `evals/run_command/plans/unknown_extension.txt`                       | Created |
| `evals/run_command/scenarios.yaml`                                    | Updated |
| `evals/run_command/README.md`                                         | Updated |
| `docs/explanation/phase5_plan_parsing_format_evals_implementation.md` | Created |
| `docs/explanation/eval_system_implementation_plan.md`                 | Updated |
