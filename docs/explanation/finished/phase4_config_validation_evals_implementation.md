# Phase 4: Configuration Validation Evals -- Implementation Summary

## Overview

Phase 4 introduces a data-driven eval suite for `Config::validate()`, covering
every validation branch across all configuration sections. With 28 fixture files
and 28 matching scenarios, this suite provides deterministic, offline coverage
of the entire config validation surface area.

## Deliverables

| File                                     | Purpose                                            |
| ---------------------------------------- | -------------------------------------------------- |
| `evals/config_validation/configs/`       | 28 YAML config fixture files (5 valid, 23 invalid) |
| `evals/config_validation/scenarios.yaml` | 28 scenario definitions with expected outcomes     |
| `evals/config_validation/README.md`      | Usage instructions and schema reference            |
| `tests/eval_config_validation.rs`        | Integration test harness that loads scenarios      |

## Fixture Design

### Valid Fixtures (5)

| File                         | Purpose                                          |
| ---------------------------- | ------------------------------------------------ |
| `valid_copilot.yaml`         | Minimal valid config with copilot provider       |
| `valid_ollama.yaml`          | Minimal valid config with ollama provider        |
| `valid_full.yaml`            | Complete config with all sections populated      |
| `valid_generic_watcher.yaml` | Valid generic watcher with kafka and regex rules |
| `valid_skills_disabled.yaml` | Skills disabled still passes numeric validation  |

### Invalid Fixtures (23)

Organized by validation category:

- **Provider** (2): empty type, unknown type
- **Agent** (3): zero max_turns, excessive max_turns, zero timeout
- **Conversation** (4): zero max_tokens, out-of-range prune_threshold,
  out-of-range warning_threshold, threshold ordering violation
- **Subagent** (6): zero max_depth, excessive max_depth, zero default_max_turns,
  excessive default_max_turns, below-minimum output_max_size, invalid provider
  override
- **Watcher/Kafka** (4): empty brokers, empty topic, generic without kafka,
  invalid regex in generic_match
- **ACP** (2): empty host, zero port
- **MCP** (1): duplicate server id
- **Skills** (1): zero max_discovered_skills

### Key Design Decision: Required `agent` Section

The `Config` struct does not mark its `agent` field with `#[serde(default)]`, so
every fixture must include at least `agent: {}` to pass deserialization.
Fixtures that test non-agent validation branches include this minimal agent
section to ensure deserialization succeeds and the intended validation rule is
reached.

## Scenario Schema

Each scenario in `scenarios.yaml` has these fields:

- `id` -- unique identifier for test output
- `description` -- human-readable explanation
- `config_file` -- path to fixture, relative to `configs/`
- `expect.outcome` -- `"valid"` or `"invalid"`
- `expect.error_contains` -- optional substring for invalid outcomes

## Test Harness Architecture

The integration test (`tests/eval_config_validation.rs`) follows the same
pattern established in Phase 2:

1. **Load scenarios** from `evals/config_validation/scenarios.yaml` using
   `env!("CARGO_MANIFEST_DIR")`.
2. **For each scenario**, read the fixture YAML, deserialize it into `Config`
   via `serde_yaml::from_str`, and call `config.validate()`.
3. **Assert outcome** matches expectations. Deserialization failures are treated
   as invalid outcomes, allowing fixtures to test both parsing and validation
   errors through the same pipeline.
4. **Report** per-scenario pass/fail with a final summary.

### Two-Stage Error Handling

The harness distinguishes between deserialization errors and validation errors:

- If `serde_yaml::from_str::<Config>()` fails, the error is checked against
  `error_contains` (if the expected outcome is `"invalid"`).
- If deserialization succeeds, `config.validate()` is called and its result is
  checked.

This approach means a fixture can trigger either a parse-time or validation-time
error, and the harness handles both uniformly.

## Validation Coverage

The 28 scenarios cover every branch of `Config::validate()`, including:

| Category     | Branches Covered                                            |
| ------------ | ----------------------------------------------------------- |
| Provider     | Empty type, unknown type                                    |
| Agent        | Zero max_turns, max_turns > 1000, zero timeout_seconds      |
| Conversation | Zero max_tokens, prune_threshold out of range,              |
|              | warning_threshold out of range, auto_summary < warning      |
| Subagent     | Zero/excessive max_depth, zero/excessive default_max_turns, |
|              | output_max_size < 1024, invalid provider override           |
| Watcher      | Empty brokers, empty topic, generic without kafka,          |
|              | invalid regex in generic_match.action                       |
| ACP          | Empty host, zero port                                       |
| MCP          | Duplicate server id                                         |
| Skills       | Zero max_discovered_skills                                  |

## Success Criteria Verification

- `cargo test --test eval_config_validation` exits 0 with all 28 scenarios
  passing.
- Every `Config::validate()` error branch is covered by at least one scenario.
- No existing library tests broken (1627 passed, 0 failed, 28 ignored).
- All Rust quality gates pass: `cargo fmt`, `cargo check`, `cargo clippy`.
- README and implementation doc pass `markdownlint` and `prettier`.

## Running the Eval

```bash
cargo test --test eval_config_validation -- --nocapture
```

The `--nocapture` flag prints per-scenario pass/fail output for visibility.

## Relationship to Other Phases

- **Phases 1-3** (Run Command) established the eval pattern: YAML fixtures,
  scenario definitions, and a data-driven integration test harness.
- **Phase 4** applies the same pattern to configuration validation, which is the
  highest-leverage target due to its 36+ pure-logic validation rules.
- Later phases (5 through 8) extend coverage to plan parsing formats, skills
  commands, mention parsing, and CLI commands.
