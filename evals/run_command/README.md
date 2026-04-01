# Run Command Eval Scenarios

This directory contains evaluation scenarios for the `xzatoma run` command.

## Contents

- `scenarios.yaml` -- Scenario definitions and expected outcomes
- `plans/` -- Plan fixture files used by the scenarios (YAML, JSON, Markdown)
- `README.md` -- This file

## Plan Fixtures

### YAML Fixtures

| File                          | Valid | Purpose                                            |
| ----------------------------- | ----- | -------------------------------------------------- |
| `simple_plan.yaml`            | Yes   | Minimal valid plan with a single step              |
| `multi_step_plan.yaml`        | Yes   | Valid plan with three sequential steps             |
| `invalid_no_steps.yaml`       | No    | Plan with `steps: []` -- triggers validation error |
| `invalid_no_name.yaml`        | No    | Plan with `name: ""` -- triggers validation error  |
| `invalid_step_no_action.yaml` | No    | Step missing `action` -- triggers step validation  |
| `empty_file.yaml`             | No    | Completely empty file -- triggers parse error      |
| `malformed_yaml.yaml`         | No    | Invalid YAML syntax -- triggers deserialize error  |

### JSON Fixtures

| File                    | Valid | Purpose                                              |
| ----------------------- | ----- | ---------------------------------------------------- |
| `simple_plan.json`      | Yes   | Minimal valid plan in JSON format                    |
| `multi_step_plan.json`  | Yes   | Multi-step plan in JSON format                       |
| `invalid_no_steps.json` | No    | JSON plan with empty steps array -- fails validation |
| `malformed_json.json`   | No    | Invalid JSON syntax -- triggers parse error          |

### Markdown Fixtures

| File                  | Valid | Purpose                                                    |
| --------------------- | ----- | ---------------------------------------------------------- |
| `simple_plan.md`      | Yes   | Minimal valid plan in Markdown (H1 = name, H2 = steps)     |
| `multi_step_plan.md`  | Yes   | Multi-step plan in Markdown format                         |
| `invalid_no_steps.md` | No    | Markdown plan with no H2 step headings -- fails validation |

### Other Fixtures

| File                    | Valid | Purpose                                                        |
| ----------------------- | ----- | -------------------------------------------------------------- |
| `unknown_extension.txt` | No    | Valid YAML content with `.txt` extension -- unsupported format |

## Scenario YAML Schema

Each entry in the `scenarios` list has the following fields:

| Field                   | Type   | Required | Description                                         |
| ----------------------- | ------ | -------- | --------------------------------------------------- |
| `id`                    | string | Yes      | Unique identifier shown in test output              |
| `description`           | string | Yes      | Human-readable explanation of what is being tested  |
| `test_mode`             | string | No       | Set to `parse_only` to call `PlanParser` directly   |
| `input.plan_file`       | string | No       | Path to a plan file, relative to `plans/`           |
| `input.prompt`          | string | No       | Direct prompt text passed as `--prompt`             |
| `input.allow_dangerous` | bool   | No       | Whether to pass `--allow-dangerous` (default false) |
| `expect.outcome`        | string | Yes      | Expected result: `error` or `ok`                    |
| `expect.error_contains` | string | No       | Substring that must appear in the error message     |

At least one of `input.plan_file` or `input.prompt` should be provided unless
the scenario specifically tests the missing-input error path.

## Scenarios

### Core Scenarios (Phase 1)

| ID                            | Mode       | Tests                                        |
| ----------------------------- | ---------- | -------------------------------------------- |
| `no_input`                    | full       | Missing both `--plan` and `--prompt`         |
| `prompt_only`                 | full       | Prompt reaches provider boundary             |
| `simple_plan`                 | full       | Single-step plan parses and reaches provider |
| `multi_step_plan`             | full       | Multi-step plan parses and reaches provider  |
| `plan_invalid_no_steps`       | parse_only | Empty steps list rejected                    |
| `plan_invalid_no_name`        | parse_only | Empty plan name rejected                     |
| `plan_invalid_step_no_action` | parse_only | Step with missing action rejected            |
| `plan_file_not_found`         | parse_only | Non-existent file produces read error        |
| `allow_dangerous_with_prompt` | full       | Dangerous flag with prompt reaches provider  |

### Multi-Format Scenarios (Phase 5)

| ID                      | Mode       | Tests                                          |
| ----------------------- | ---------- | ---------------------------------------------- |
| `json_simple_plan`      | parse_only | Valid single-step JSON plan parses             |
| `json_multi_step_plan`  | parse_only | Valid multi-step JSON plan parses              |
| `json_invalid_no_steps` | parse_only | JSON plan with empty steps fails validation    |
| `md_simple_plan`        | parse_only | Valid single-step Markdown plan parses         |
| `md_multi_step_plan`    | parse_only | Valid multi-step Markdown plan parses          |
| `md_invalid_no_steps`   | parse_only | Markdown plan with no steps fails validation   |
| `empty_yaml_file`       | parse_only | Empty YAML file triggers deserialization error |
| `malformed_yaml`        | parse_only | Broken YAML syntax triggers parse error        |
| `malformed_json`        | parse_only | Broken JSON syntax triggers parse error        |
| `unsupported_extension` | parse_only | `.txt` extension triggers unsupported format   |

## Scope

These scenarios validate:

- CLI argument parsing for `run` (detecting missing input)
- Plan parsing and validation (independent of model/provider)
- Multi-format plan parsing: YAML, JSON, and Markdown
- Error handling for malformed content and unsupported file extensions
- Deterministic behavior of the `--allow-dangerous` option path
- Error handling for missing files or invalid plan structures

## Supported Plan Formats

The `PlanParser` supports three plan file formats, selected by file extension:

- **YAML** (`.yaml`, `.yml`) -- Structured YAML with `name`, `description`, and
  `steps` fields
- **JSON** (`.json`) -- Equivalent structure to YAML, serialized as JSON
- **Markdown** (`.md`) -- H1 heading becomes the plan name, H2 headings become
  steps, first non-empty line under a step is the action, and fenced code blocks
  become step context

Any other extension produces an `Unsupported plan format` error.

## Running Evals

Evals are integrated into the Rust test suite as an integration test. Run them
with:

```bash
cargo test --test eval_run_command -- --nocapture
```

The `--nocapture` flag allows you to see the per-scenario pass/fail output.

## Adding Scenarios

To add a new scenario:

1. **Define the Input**: If your scenario requires a plan file, create a new
   file in the `plans/` directory. Use `.yaml`, `.json`, or `.md` as the
   extension to match the format you want to test.
2. **Add to `scenarios.yaml`**: Add a new entry to the `scenarios` list:

   ```yaml
   - id: my_new_scenario
     description: "description of what it tests"
     test_mode: parse_only # optional; use if testing PlanParser directly
     input:
       plan_file: "my_plan.yaml" # or prompt: "..."
     expect:
       outcome: error # or ok
       error_contains: "expected error substring"
   ```

3. **Run Evals**: Execute the test command above to verify your new scenario
   passes.

## Architecture

The eval system is designed to be **offline and deterministic**:

- **Full execution scenarios** (default) use a configuration with an
  `invalid_provider`. This ensures the command reaches the point of initializing
  the agent but fails at provider instantiation, validating the entire path
  through the logic without making real network calls.
- **Parse-only scenarios** (`test_mode: parse_only`) call the `PlanParser`
  directly, bypassing provider setup entirely. These run faster and produce more
  targeted error messages for plan validation testing.

No API keys, external services, or network access are required to run any
scenario in this suite.
