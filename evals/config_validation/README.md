# Configuration Validation Eval Scenarios

This directory contains evaluation scenarios for `Config::validate()`, covering
every validation branch across all configuration sections.

## Contents

- `scenarios.yaml` -- Scenario definitions and expected outcomes
- `configs/` -- YAML config fixture files used by the scenarios
- `README.md` -- This file

## Config Fixtures

| File                                 | Valid | Purpose                                          |
| ------------------------------------ | ----- | ------------------------------------------------ |
| `valid_copilot.yaml`                 | Yes   | Minimal valid config with copilot provider       |
| `valid_ollama.yaml`                  | Yes   | Minimal valid config with ollama provider        |
| `valid_full.yaml`                    | Yes   | Complete config with all sections populated      |
| `valid_generic_watcher.yaml`         | Yes   | Valid generic watcher with kafka and regex rules |
| `valid_skills_disabled.yaml`         | Yes   | Skills disabled still passes validation          |
| `invalid_empty_provider.yaml`        | No    | Empty provider type                              |
| `invalid_unknown_provider.yaml`      | No    | Unsupported provider type                        |
| `invalid_max_turns_zero.yaml`        | No    | `max_turns: 0` below minimum                     |
| `invalid_max_turns_over.yaml`        | No    | `max_turns: 5000` above maximum                  |
| `invalid_timeout_zero.yaml`          | No    | `timeout_seconds: 0` below minimum               |
| `invalid_max_tokens_zero.yaml`       | No    | `max_tokens: 0` below minimum                    |
| `invalid_prune_threshold.yaml`       | No    | `prune_threshold: 1.5` out of range              |
| `invalid_warning_threshold.yaml`     | No    | `warning_threshold: 0.0` out of range            |
| `invalid_summary_below_warning.yaml` | No    | `auto_summary_threshold` < `warning_threshold`   |
| `invalid_subagent_depth_zero.yaml`   | No    | `max_depth: 0` below minimum                     |
| `invalid_subagent_depth_over.yaml`   | No    | `max_depth: 20` above maximum                    |
| `invalid_subagent_turns_zero.yaml`   | No    | `default_max_turns: 0` below minimum             |
| `invalid_subagent_turns_over.yaml`   | No    | `default_max_turns: 200` above maximum           |
| `invalid_subagent_output_small.yaml` | No    | `output_max_size: 512` below 1024 minimum        |
| `invalid_subagent_provider.yaml`     | No    | `provider: openai` not in valid provider list    |
| `invalid_kafka_empty_brokers.yaml`   | No    | Empty `brokers` string                           |
| `invalid_kafka_empty_topic.yaml`     | No    | Empty `topic` string                             |
| `invalid_generic_no_kafka.yaml`      | No    | Generic watcher without required kafka section   |
| `invalid_generic_bad_regex.yaml`     | No    | Invalid regex in `generic_match.action`          |
| `invalid_acp_empty_host.yaml`        | No    | Empty ACP host string                            |
| `invalid_acp_port_zero.yaml`         | No    | ACP port set to zero                             |
| `invalid_mcp_duplicate_id.yaml`      | No    | Two MCP servers with the same id                 |
| `invalid_skills_max_zero.yaml`       | No    | `max_discovered_skills: 0` below minimum         |

## Scenario YAML Schema

Each entry in the `scenarios` list has the following fields:

| Field                   | Type   | Required | Description                                           |
| ----------------------- | ------ | -------- | ----------------------------------------------------- |
| `id`                    | string | Yes      | Unique identifier shown in test output                |
| `description`           | string | Yes      | Human-readable explanation of what is being tested    |
| `config_file`           | string | Yes      | Path to a config fixture file, relative to `configs/` |
| `expect.outcome`        | string | Yes      | Expected result: `valid` or `invalid`                 |
| `expect.error_contains` | string | No       | Substring that must appear in the error message       |

## Scenarios

The following scenarios are defined in `scenarios.yaml`:

| ID                              | Outcome | Category     | Tests                                    |
| ------------------------------- | ------- | ------------ | ---------------------------------------- |
| `valid_copilot`                 | valid   | provider     | Minimal copilot config passes            |
| `valid_ollama`                  | valid   | provider     | Minimal ollama config passes             |
| `valid_full`                    | valid   | all          | Complete config with all sections passes |
| `valid_generic_watcher`         | valid   | watcher      | Generic watcher with valid regex passes  |
| `valid_skills_disabled`         | valid   | skills       | Disabled skills still passes validation  |
| `invalid_empty_provider`        | invalid | provider     | Empty provider type rejected             |
| `invalid_unknown_provider`      | invalid | provider     | Unsupported provider type rejected       |
| `invalid_max_turns_zero`        | invalid | agent        | Zero max_turns rejected                  |
| `invalid_max_turns_over`        | invalid | agent        | Excessive max_turns rejected             |
| `invalid_timeout_zero`          | invalid | agent        | Zero timeout rejected                    |
| `invalid_max_tokens_zero`       | invalid | conversation | Zero max_tokens rejected                 |
| `invalid_prune_threshold`       | invalid | conversation | Out-of-range prune_threshold rejected    |
| `invalid_warning_threshold`     | invalid | conversation | Out-of-range warning_threshold rejected  |
| `invalid_summary_below_warning` | invalid | conversation | Threshold ordering violation rejected    |
| `invalid_subagent_depth_zero`   | invalid | subagent     | Zero max_depth rejected                  |
| `invalid_subagent_depth_over`   | invalid | subagent     | Excessive max_depth rejected             |
| `invalid_subagent_turns_zero`   | invalid | subagent     | Zero default_max_turns rejected          |
| `invalid_subagent_turns_over`   | invalid | subagent     | Excessive default_max_turns rejected     |
| `invalid_subagent_output_small` | invalid | subagent     | Below-minimum output_max_size rejected   |
| `invalid_subagent_provider`     | invalid | subagent     | Invalid provider override rejected       |
| `invalid_kafka_empty_brokers`   | invalid | watcher      | Empty kafka brokers rejected             |
| `invalid_kafka_empty_topic`     | invalid | watcher      | Empty kafka topic rejected               |
| `invalid_generic_no_kafka`      | invalid | watcher      | Generic watcher without kafka rejected   |
| `invalid_generic_bad_regex`     | invalid | watcher      | Invalid regex in generic_match rejected  |
| `invalid_acp_empty_host`        | invalid | acp          | Empty ACP host rejected                  |
| `invalid_acp_port_zero`         | invalid | acp          | Zero ACP port rejected                   |
| `invalid_mcp_duplicate_id`      | invalid | mcp          | Duplicate MCP server id rejected         |
| `invalid_skills_max_zero`       | invalid | skills       | Zero max_discovered_skills rejected      |

## Scope

These scenarios validate every branch of `Config::validate()`, including:

- Provider type validation (empty, unknown)
- Agent limits (max_turns, timeout_seconds)
- Conversation thresholds (max_tokens, prune, warning, auto_summary ordering)
- Subagent limits (max_depth, default_max_turns, output_max_size, provider)
- Watcher/Kafka validation (empty fields, missing kafka for generic, bad regex)
- ACP validation (empty host, zero port)
- MCP validation (duplicate server ids)
- Skills validation (zero max_discovered_skills)

## Running Evals

Evals are integrated into the Rust test suite as an integration test. Run them
with:

```bash
cargo test --test eval_config_validation -- --nocapture
```

The `--nocapture` flag allows you to see the per-scenario pass/fail output.

## Adding Scenarios

To add a new scenario:

1. **Create a Fixture**: Add a new `.yaml` config file in the `configs/`
   directory. Use the naming convention `valid_<description>.yaml` or
   `invalid_<description>.yaml`.
2. **Add to `scenarios.yaml`**: Add a new entry to the `scenarios` list:

   ```yaml
   - id: my_new_scenario
     description: "description of what it tests"
     config_file: "my_config.yaml"
     expect:
       outcome: invalid # or valid
       error_contains: "expected error substring"
   ```

3. **Run Evals**: Execute the test command above to verify your new scenario
   passes.

## Architecture

The eval system is designed to be **offline and deterministic**:

- Each scenario reads a fixture YAML file, deserializes it into a `Config`
  struct using `serde_yaml::from_str`, and then calls `Config::validate()`.
- No provider instantiation, network calls, or external services are involved.
- Deserialization failures (e.g., type mismatches) are treated as invalid
  outcomes, allowing fixtures to test both parsing and validation errors.
- All validation logic is pure and side-effect-free, making results fully
  reproducible across environments.
