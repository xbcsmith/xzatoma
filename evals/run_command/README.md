# Run Command Eval Scenarios

This directory contains evaluation scenarios for the `xzatoma run` command.

## Contents

- `scenarios.yaml` - Scenario definitions and expected outcomes
- `plans/` - Plan fixtures used by the scenarios
- `README.md` - This file

## Scope

These scenarios validate:

- CLI argument parsing for `run` (e.g., detecting missing input)
- Plan parsing and validation (independent of model/provider)
- Deterministic behavior of the `--allow-dangerous` option path
- Error handling for missing files or invalid plan structures

## Running Evals

Evals are integrated into the main Rust test suite as an integration test. Run them with:

```bash
cargo test --test eval_run_command -- --nocapture
```

The `--nocapture` flag allows you to see the per-scenario pass/fail output.

## Adding Scenarios

To add a new scenario:

1. **Define the Input**: If your scenario requires a plan file, create a new `.yaml` file in the `plans/` directory.
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

3. **Run Evals**: Execute the test command above to verify your new scenario passes.

## Architecture

The eval system is designed to be **offline and deterministic**:

- Scenarios requiring full execution use a configuration with an `invalid_provider`. This ensures the command reaches the point of initializing the agent but fails at provider instantiation, validating the path through the logic without making real network calls.
- Plan validation scenarios use `test_mode: parse_only` to call the `PlanParser` directly, bypassing provider setup entirely.
