# Eval System Implementation Plan

## Overview

Implement a data-driven eval system for XZatoma that validates CLI argument
parsing, plan parsing/validation, configuration validation, skills management,
mention parsing, and command-level behavior. Scenarios are defined in YAML,
fixtures live alongside them, and Rust integration tests drive execution against
deterministic, offline configurations so all scenarios remain reproducible
without network access.

The original plan covered only the `xzatoma run` command. Since then, the
project has added MCP client management, ACP server support, a skills subsystem,
context mention parsing (`@file:`, `@search:`, `@grep:`, `@url:`), Kafka-backed
watchers (xzepr and generic), conversation history/replay, model management, and
chat modes. This revision marks the original three phases as completed and adds
five new phases to cover the expanded surface area.

## Current State Analysis

### Existing Infrastructure

- `src/commands/mod.rs` --
  `run::run_plan_with_options(config, plan_path, prompt, allow_dangerous)` is
  the primary entry point for the `run` command.
- `src/tools/plan.rs` -- `PlanParser` supports YAML, JSON, and Markdown plan
  files; `Plan::validate()` enforces name, steps, and action presence.
- `src/cli.rs` -- `Commands` enum defines 10 subcommands: `Chat`, `Run`,
  `Watch`, `Auth`, `Models`, `History`, `Replay`, `Mcp`, `Acp`, `Skills`.
- `src/config.rs` -- `Config::validate()` performs 36+ validation checks across
  provider, agent, conversation, tools, subagent, watcher/kafka, ACP, MCP, and
  skills configuration sections.
- `src/mention_parser.rs` -- `parse_mentions()`, `resolve_mention_path()`,
  `augment_prompt_with_mentions()`, `expand_common_abbreviations()` with 57 unit
  tests.
- `src/skills/` -- Discovery, parsing, catalog, activation, trust, validation,
  and disclosure modules.
- `src/mcp/` -- Client, server config, transport (stdio/http), auth (OAuth 2.1),
  tool bridge, sampling, elicitation, protocol, task manager.
- `src/acp/` -- HTTP server, runtime, handlers, routes, events, session,
  streaming, manifest, types.
- `src/commands/skills.rs` -- `list_skills`, `validate_skills`, `show_skill`,
  `show_paths`, `handle_trust` entry points.
- `src/commands/mcp.rs` -- `handle_mcp(McpCommands, Config)` with `List`
  subcommand.
- `src/commands/acp.rs` -- `handle_acp(AcpCommand, Config)` with `Serve`,
  `Config`, `Runs`, `Validate` subcommands.
- `src/commands/history.rs` -- `handle_history(HistoryCommand)` with `List`,
  `Show`, `Delete` subcommands using `SqliteStorage`.
- `src/commands/replay.rs` -- `run_replay(ReplayArgs)` for conversation replay
  with `--list`, `--id`, `--tree`.
- `src/commands/models.rs` -- `list_models`, `show_model_info`,
  `show_current_model` plus offline formatting helpers.
- `src/test_utils.rs` -- `temp_dir()`, `create_test_file()`,
  `assert_error_contains()`, `test_config()`, `test_config_yaml()` helpers.
- `tests/common/mod.rs` -- `create_temp_storage()`, `temp_config_file()` shared
  helpers.
- `evals/run_command/` -- Fully implemented: `scenarios.yaml` (9 scenarios),
  `plans/` (5 fixtures), `README.md`.
- `tests/eval_run_command.rs` -- Working integration test driver with
  `parse_only` and full-path modes.
- Dev-dependencies: `mockall`, `tempfile`, `tokio-test`, `wiremock`,
  `assert_cmd`, `predicates`, `serial_test`.

### Existing Integration Test Coverage

| Feature Area      | Test Files                              | Coverage Level           |
| ----------------- | --------------------------------------- | ------------------------ |
| Skills            | 10 files (`skills_*.rs`)                | High                     |
| MCP               | 12 files (`mcp_*_test.rs`)              | High                     |
| ACP               | 5 files (`acp_*.rs`)                    | Medium                   |
| Models            | `models_json_output.rs`                 | Medium (formatting only) |
| Run command       | `eval_run_command.rs`                   | High (data-driven)       |
| CLI parsing       | `src/cli.rs` unit tests                 | High (151 tests)         |
| Config validation | `src/config.rs` unit tests              | High (100+ tests)        |
| Mention parser    | `src/mention_parser.rs` unit tests      | High (57 tests)          |
| History           | `integration_history_tool_integrity.rs` | Low                      |
| Watcher/Kafka     | None                                    | None                     |
| Chat mode         | None (unit tests only)                  | Low                      |

### Identified Gaps

- No data-driven eval suite exists for configuration validation despite 36+
  rules.
- Plan parsing evals cover only YAML format; JSON and Markdown fixtures are
  missing.
- Skills command-level behavior has no data-driven eval (existing tests are
  programmatic).
- Mention parser has strong unit tests but no data-driven eval with fixture
  files.
- CLI argument parsing for `watch`, `mcp`, `acp`, `skills`, `replay` has unit
  tests but no eval-style validation.
- History and replay commands have minimal integration coverage.
- Watcher subsystem has zero integration tests (requires Kafka; not suitable for
  offline evals but noted for completeness).

## Implementation Phases

### Phase 1: Run Command Plan Fixtures (COMPLETED)

#### Task 1.1 Create `evals/run_command/plans/` directory with fixture files

Created the following YAML plan fixtures under `evals/run_command/plans/`:

| File                          | Purpose                                                                   |
| ----------------------------- | ------------------------------------------------------------------------- |
| `simple_plan.yaml`            | Minimal valid plan (1 step)                                               |
| `multi_step_plan.yaml`        | Valid plan with 3 steps                                                   |
| `invalid_no_steps.yaml`       | Plan with `steps: []` -- triggers validation error                        |
| `invalid_no_name.yaml`        | Plan with `name: ""` -- triggers validation error                         |
| `invalid_step_no_action.yaml` | Plan with a step missing `action` -- triggers step-level validation error |

#### Task 1.2 Create `evals/run_command/scenarios.yaml`

Defined 9 scenarios covering: no input, prompt-only, valid plans, invalid plans,
non-existent file, and `--allow-dangerous` flag path. Scenarios with
`test_mode: parse_only` exercise `PlanParser` directly; default scenarios call
`run_plan_with_options` with an invalid provider for deterministic offline
execution.

#### Task 1.3 Deliverables (COMPLETED)

- `evals/run_command/plans/simple_plan.yaml`
- `evals/run_command/plans/multi_step_plan.yaml`
- `evals/run_command/plans/invalid_no_steps.yaml`
- `evals/run_command/plans/invalid_no_name.yaml`
- `evals/run_command/plans/invalid_step_no_action.yaml`
- `evals/run_command/scenarios.yaml`

#### Task 1.4 Success Criteria (MET)

- All fixture files parse without error via `PlanParser::from_file`.
- `scenarios.yaml` deserializes cleanly into the `Scenario` struct used by the
  test.

---

### Phase 2: Run Command Integration Test (COMPLETED)

#### Task 2.1 Create `tests/eval_run_command.rs`

Created integration test file that:

1. Defines `Scenario`, `ScenarioInput`, and `ScenarioExpect` structs with
   `serde::Deserialize`.
2. Loads `evals/run_command/scenarios.yaml` using `env!("CARGO_MANIFEST_DIR")`.
3. Iterates over all scenarios in a single `#[tokio::test]`.
4. Dispatches `parse_only` scenarios to `PlanParser::from_file` and default
   scenarios to `run_plan_with_options` with an invalid provider.
5. Asserts outcome and optional `error_contains` substring.

#### Task 2.2 Deliverables (COMPLETED)

- `tests/eval_run_command.rs`

#### Task 2.3 Success Criteria (MET)

- `cargo test --test eval_run_command` exits 0 with all 9 scenarios passing.
- No existing tests broken.
- No network calls made.

---

### Phase 3: Run Command Documentation Update (COMPLETED)

#### Task 3.1 Update `evals/run_command/README.md`

Expanded README to document how to add scenarios, run eval tests, the
offline/deterministic design rationale, and the scenario YAML schema.

#### Task 3.2 Deliverables (COMPLETED)

- Updated `evals/run_command/README.md`

#### Task 3.3 Success Criteria (MET)

- README accurately reflects the implemented structure.

---

### Phase 4: Configuration Validation Evals (COMPLETED)

Configuration validation is the highest-leverage target for data-driven evals.
`Config::validate()` enforces 36+ rules across 8 configuration sections, all of
which are pure logic with no I/O. A YAML-driven eval suite can cover every
validation branch deterministically.

#### Task 4.1 Create `evals/config_validation/configs/` directory with fixture files

Created YAML configuration fixtures under `evals/config_validation/configs/`:

| File                                 | Purpose                                                   |
| ------------------------------------ | --------------------------------------------------------- |
| `valid_copilot.yaml`                 | Minimal valid config with copilot provider                |
| `valid_ollama.yaml`                  | Minimal valid config with ollama provider                 |
| `valid_full.yaml`                    | Complete config with all sections populated               |
| `invalid_empty_provider.yaml`        | `provider.type: ""` -- triggers empty provider error      |
| `invalid_unknown_provider.yaml`      | `provider.type: "gpt"` -- triggers invalid provider error |
| `invalid_max_turns_zero.yaml`        | `agent.max_turns: 0` -- triggers range error              |
| `invalid_max_turns_over.yaml`        | `agent.max_turns: 5000` -- triggers upper bound error     |
| `invalid_timeout_zero.yaml`          | `agent.timeout_seconds: 0` -- triggers positive check     |
| `invalid_max_tokens_zero.yaml`       | `conversation.max_tokens: 0` -- triggers positive check   |
| `invalid_prune_threshold.yaml`       | `prune_threshold: 1.5` -- triggers range error            |
| `invalid_warning_threshold.yaml`     | `warning_threshold: 0.0` -- triggers range error          |
| `invalid_summary_below_warning.yaml` | `auto_summary_threshold` < `warning_threshold`            |
| `invalid_subagent_depth_zero.yaml`   | `subagent.max_depth: 0`                                   |
| `invalid_subagent_depth_over.yaml`   | `subagent.max_depth: 20`                                  |
| `invalid_subagent_turns_zero.yaml`   | `subagent.default_max_turns: 0`                           |
| `invalid_subagent_turns_over.yaml`   | `subagent.default_max_turns: 200`                         |
| `invalid_subagent_output_small.yaml` | `subagent.output_max_size: 512` -- below 1024 minimum     |
| `invalid_subagent_provider.yaml`     | `subagent.provider: "openai"` -- not in valid list        |
| `invalid_kafka_empty_brokers.yaml`   | `watcher.kafka.brokers: ""`                               |
| `invalid_kafka_empty_topic.yaml`     | `watcher.kafka.topic: ""`                                 |
| `invalid_generic_no_kafka.yaml`      | `watcher_type: generic` without `kafka` section           |
| `invalid_generic_bad_regex.yaml`     | `generic_match.action: "["` -- invalid regex              |
| `invalid_acp_empty_host.yaml`        | `acp.host: ""`                                            |
| `invalid_acp_port_zero.yaml`         | `acp.port: 0`                                             |
| `invalid_mcp_duplicate_id.yaml`      | Two MCP servers with the same `id`                        |
| `invalid_skills_max_zero.yaml`       | `skills.max_discovered_skills: 0`                         |
| `valid_skills_disabled.yaml`         | `skills.enabled: false` -- validation should pass         |
| `valid_generic_watcher.yaml`         | Complete generic watcher with kafka and match rules       |

#### Task 4.2 Create `evals/config_validation/scenarios.yaml`

Defined 28 scenarios covering every validation branch. Each scenario entry has:

- `id` -- unique identifier
- `description` -- human-readable purpose
- `config_file` -- path relative to `configs/`
- `expect.outcome` -- `"valid"` or `"invalid"`
- `expect.error_contains` -- optional substring for invalid outcomes

Target: 28+ scenarios covering all validation categories (provider, agent,
conversation, tools, subagent, watcher/kafka, ACP, MCP, skills).

#### Task 4.3 Create `tests/eval_config_validation.rs`

Created integration test that:

1. Defines `ConfigScenario` and related deserialization structs.
2. Loads `evals/config_validation/scenarios.yaml`.
3. For each scenario, reads the config fixture, calls
   `serde_yaml::from_str::<Config>()` then `config.validate()`.
4. Asserts outcome matches and error substring is present when specified.
5. Reports per-scenario pass/fail with clear messages.

#### Task 4.4 Create `evals/config_validation/README.md`

Documented how to add config validation scenarios, the fixture naming
convention, the scenario YAML schema, and how to run the eval tests.

#### Task 4.5 Testing Requirements (MET)

- Run `cargo test --test eval_config_validation -- --nocapture`.
- All 28 scenarios pass.
- No network calls.

#### Task 4.6 Deliverables (COMPLETED)

- `evals/config_validation/configs/` (28 fixture files: 5 valid, 23 invalid)
- `evals/config_validation/scenarios.yaml`
- `evals/config_validation/README.md`
- `tests/eval_config_validation.rs`

#### Task 4.7 Success Criteria (MET)

- `cargo test --test eval_config_validation` exits 0 with all 28 scenarios
  passing.
- Every `Config::validate()` error branch is covered by at least one scenario
  across 8 categories: provider, agent, conversation, subagent, watcher/kafka,
  ACP, MCP, and skills.
- No existing tests break (1627 library tests pass).

---

### Phase 5: Plan Parsing Format Evals (COMPLETED)

The existing eval covers YAML plan fixtures only. `PlanParser` also supports
JSON and Markdown formats. This phase extends plan parsing coverage to all three
formats and adds edge-case fixtures.

#### Task 5.1 Add JSON and Markdown plan fixtures to `evals/run_command/plans/`

| File                    | Purpose                                                                 |
| ----------------------- | ----------------------------------------------------------------------- |
| `simple_plan.json`      | Minimal valid plan in JSON format                                       |
| `multi_step_plan.json`  | Valid multi-step plan in JSON format                                    |
| `invalid_no_steps.json` | JSON plan with empty steps array                                        |
| `simple_plan.md`        | Minimal valid plan in Markdown format (H1 = name, H2 = steps)           |
| `multi_step_plan.md`    | Multi-step plan in Markdown format                                      |
| `invalid_no_steps.md`   | Markdown plan with no step headings                                     |
| `empty_file.yaml`       | Completely empty file -- triggers parse error                           |
| `malformed_yaml.yaml`   | Invalid YAML syntax -- triggers deserialize error                       |
| `malformed_json.json`   | Invalid JSON syntax -- triggers parse error                             |
| `unknown_extension.txt` | Valid content but `.txt` extension -- triggers unsupported format error |

#### Task 5.2 Add scenarios to `evals/run_command/scenarios.yaml`

Add 10+ scenarios for the new fixtures, all using `test_mode: parse_only`:

- Valid JSON single-step plan parses successfully
- Valid JSON multi-step plan parses successfully
- JSON plan with no steps fails validation
- Valid Markdown plan parses successfully
- Multi-step Markdown plan parses successfully
- Markdown plan with no steps fails validation
- Empty YAML file fails parsing
- Malformed YAML fails parsing
- Malformed JSON fails parsing
- Unsupported file extension fails with format error

#### Task 5.3 Testing Requirements (MET)

- Run `cargo test --test eval_run_command -- --nocapture`.
- All original + new scenarios pass (target: 19+ total).
- Result: 19 passed, 0 failed out of 19 scenarios.

#### Task 5.4 Deliverables (COMPLETED)

- 10 new fixture files in `evals/run_command/plans/`
- Updated `evals/run_command/scenarios.yaml` with 10+ new scenarios
- Updated `evals/run_command/README.md` noting multi-format support
- Created `docs/explanation/phase5_plan_parsing_format_evals_implementation.md`

#### Task 5.5 Success Criteria (MET)

- `cargo test --test eval_run_command` exits 0 with 19 scenarios passing.
- JSON, Markdown, and error-path fixtures are all exercised.
- All quality gates passed: `cargo fmt`, `cargo check`, `cargo clippy`,
  `cargo test`, `markdownlint`, and `prettier`.

---

### Phase 6: Skills Command Evals (COMPLETED)

The skills subsystem is fully testable offline using temporary directories and
skill file fixtures. This phase creates a data-driven eval suite for skills
discovery, validation, trust management, and catalog visibility.

#### Task 6.1 Create `evals/skills_command/skills/` directory with skill fixtures

Create skill file fixtures under `evals/skills_command/skills/`:

| File                        | Purpose                                                   |
| --------------------------- | --------------------------------------------------------- |
| `valid_simple.md`           | Minimal valid skill (name, description, instruction body) |
| `valid_with_tools.md`       | Valid skill declaring allowed tools                       |
| `valid_with_inputs.md`      | Valid skill with input parameters                         |
| `invalid_no_name.md`        | Missing `name` in frontmatter                             |
| `invalid_no_description.md` | Missing `description` in frontmatter                      |
| `invalid_empty_body.md`     | Valid frontmatter but empty instruction body              |
| `invalid_bad_yaml.md`       | Malformed YAML frontmatter                                |
| `not_a_skill.txt`           | Wrong file extension -- should be ignored by discovery    |

Each skill fixture uses the YAML frontmatter + Markdown body format expected by
`src/skills/parser.rs`.

#### Task 6.2 Create `evals/skills_command/scenarios.yaml`

Define scenarios covering:

- Discovery with no skill directories -- returns empty catalog
- Discovery with valid skills -- returns populated catalog
- Discovery with skills disabled -- returns empty catalog immediately
- Validation of valid skill -- passes
- Validation of skill with missing name -- fails with error
- Validation of skill with missing description -- fails with error
- Validation of skill with empty body -- fails with error
- Validation of malformed frontmatter -- fails with parse error
- Show skill that exists -- succeeds
- Show skill that does not exist -- fails with not-found error
- Trust add/remove round-trip -- succeeds
- Trust show on empty store -- returns empty list
- Paths listing with project and user discovery -- shows expected roots

Each scenario has: `id`, `description`, `command` (one of `list`, `validate`,
`show`, `paths`, `trust_show`, `trust_add`, `trust_remove`), `setup` (optional
directory/file creation instructions), and `expect` (`outcome`, optional
`error_contains`, optional `output_contains`).

#### Task 6.3 Create `tests/eval_skills_command.rs`

Integration test that:

1. Loads `evals/skills_command/scenarios.yaml`.
2. For each scenario, creates a temporary directory structure using the `setup`
   instructions.
3. Copies skill fixture files from `evals/skills_command/skills/` into the temp
   structure.
4. Builds a `Config` with `skills.enabled` and discovery paths pointing at the
   temp dir.
5. Calls the appropriate skills command function (`list_skills`,
   `validate_skills`, `show_skill`, `show_paths`, or `handle_trust`).
6. Asserts outcome, error substring, and optional output substring.

#### Task 6.4 Create `evals/skills_command/README.md`

Document the skills eval structure, fixture format, and how to add scenarios.

#### Task 6.5 Testing Requirements

- Run `cargo test --test eval_skills_command -- --nocapture`.
- All scenarios pass.
- No network calls.
- Temporary directories are cleaned up after each scenario.

#### Task 6.6 Deliverables (COMPLETED)

- `evals/skills_command/skills/` (8 fixture files)
- `evals/skills_command/scenarios.yaml` (16 scenarios)
- `evals/skills_command/README.md`
- `tests/eval_skills_command.rs`
- `docs/explanation/phase6_skills_command_evals_implementation.md`

#### Task 6.7 Success Criteria (MET)

- `cargo test --test eval_skills_command` exits 0 with 16 scenarios passing.
- Discovery, validation, trust, and visibility branches are all exercised.
- No existing tests break.
- All quality gates passed: `cargo fmt`, `cargo check`, `cargo clippy`,
  `cargo test`, `markdownlint`, and `prettier`.

---

### Phase 7: Mention Parser Evals

The mention parser has 57 unit tests but no data-driven eval suite with fixture
files. This phase adds YAML-driven scenarios that test mention parsing, path
resolution, and prompt augmentation using temporary file fixtures.

#### Task 7.1 Create `evals/mention_parser/files/` directory with content fixtures

Create content files under `evals/mention_parser/files/`:

| File               | Purpose                                                    |
| ------------------ | ---------------------------------------------------------- |
| `sample.rs`        | Small Rust source file (20 lines) for file mention testing |
| `config.yaml`      | Small YAML config file for file mention testing            |
| `large_binary.bin` | Small binary file (non-UTF-8) for binary rejection testing |
| `README.md`        | Markdown file for abbreviation expansion testing           |

#### Task 7.2 Create `evals/mention_parser/scenarios.yaml`

Define scenarios in two categories:

**Parse-only scenarios** (`test_mode: parse_only`) -- test `parse_mentions()` in
isolation:

- Simple file mention `@path/to/file.rs` -- parses as `File` mention
- File mention with line range `@file.rs#L10-20` -- parses with range
- Search mention `@search:"pattern"` -- parses as `Search` mention
- Grep mention `@grep:"^pub fn"` -- parses as `Grep` mention
- URL mention `@url:https://example.com` -- parses as `Url` mention
- Multiple mentions in one prompt -- parses all correctly
- Escaped `@@` -- not parsed as a mention
- No mentions in plain text -- returns empty list
- Absolute path rejected `@/etc/passwd` -- returns error
- Path traversal rejected `@../../etc/passwd` -- returns error

**Augmentation scenarios** (`test_mode: augment`) -- test
`augment_prompt_with_mentions()` with fixture files:

- File mention resolves to content -- augmented prompt contains file content
- File mention with line range -- augmented prompt contains only specified lines
- Abbreviation `@readme` expands to `README.md` content
- Missing file -- augmented prompt contains error placeholder
- Binary file -- augmented prompt contains skip message
- Search mention injects results -- augmented prompt contains match context
- Grep mention injects results -- augmented prompt contains match context

Each scenario has: `id`, `description`, `test_mode`, `input` (`prompt`, optional
`working_dir_files` mapping filenames to fixture sources), and `expect`
(`outcome`, optional `mention_count`, optional `mention_types`, optional
`output_contains`, optional `error_contains`).

#### Task 7.3 Create `tests/eval_mention_parser.rs`

Integration test that:

1. Loads `evals/mention_parser/scenarios.yaml`.
2. For `parse_only` scenarios, calls `parse_mentions(prompt)` and asserts
   mention count, types, and error conditions.
3. For `augment` scenarios, creates a temp directory, copies fixture files from
   `evals/mention_parser/files/`, calls `augment_prompt_with_mentions()`, and
   asserts output content.
4. Reports per-scenario pass/fail.

#### Task 7.4 Create `evals/mention_parser/README.md`

Document the mention parser eval structure and how to add scenarios.

#### Task 7.5 Testing Requirements

- Run `cargo test --test eval_mention_parser -- --nocapture`.
- All scenarios pass.
- No network calls (URL mentions that would require fetching should use
  `expect.outcome: error` or be omitted).

#### Task 7.6 Deliverables

- `evals/mention_parser/files/` (4 fixture files)
- `evals/mention_parser/scenarios.yaml`
- `evals/mention_parser/README.md`
- `tests/eval_mention_parser.rs`

#### Task 7.7 Success Criteria

- `cargo test --test eval_mention_parser` exits 0 with 17+ scenarios passing.
- Parse and augmentation paths are both covered.
- No existing tests break.

---

### Phase 8: CLI Command Evals

This phase creates a data-driven eval suite that exercises CLI argument parsing
and basic command dispatch for all 10 subcommands using `assert_cmd` against the
compiled binary. Scenarios validate argument acceptance, rejection, help output,
and early error paths without requiring network or external services.

#### Task 8.1 Create `evals/cli_commands/scenarios.yaml`

Define scenarios covering argument parsing and early error paths for each
command. Each scenario entry has:

- `id` -- unique identifier
- `description` -- human-readable purpose
- `args` -- list of CLI arguments to pass to the binary
- `env` -- optional map of environment variables to set
- `expect.exit_code` -- expected exit code (0 or non-zero)
- `expect.stdout_contains` -- optional substring in stdout
- `expect.stderr_contains` -- optional substring in stderr

Target scenarios (25+):

**Global options:**

- `--help` shows usage
- `--version` shows version
- No subcommand shows error

**run command:**

- `run` with no `--plan` or `--prompt` -- error
- `run --plan nonexistent.yaml` -- error about file
- `run --help` -- shows run help

**chat command:**

- `chat --help` -- shows chat help
- `chat --mode invalid_mode` -- still accepted (string field, validated later)

**watch command:**

- `watch --help` -- shows watch help
- `watch --watcher-type generic --topic test` -- reaches Kafka error (offline)
- `watch --watcher-type invalid` -- accepted by clap (validated later)

**auth command:**

- `auth --help` -- shows auth help
- `auth --provider copilot` -- reaches auth flow (offline error)

**models command:**

- `models list --help` -- shows help
- `models info --help` -- shows help
- `models current --help` -- shows help
- `models` with no subcommand -- error

**history command:**

- `history list --help` -- shows help
- `history show --help` -- shows help
- `history delete --help` -- shows help

**mcp command:**

- `mcp list --help` -- shows help
- `mcp list` -- succeeds (empty server list with default config)

**acp command:**

- `acp config` -- succeeds (prints JSON config)
- `acp validate --help` -- shows help
- `acp serve --help` -- shows help

**skills command:**

- `skills list` -- succeeds (possibly empty)
- `skills paths` -- succeeds
- `skills trust show` -- succeeds
- `skills validate` -- succeeds

**replay command:**

- `replay --help` -- shows help
- `replay` with no flags -- error

#### Task 8.2 Create `tests/eval_cli_commands.rs`

Integration test that:

1. Loads `evals/cli_commands/scenarios.yaml`.
2. For each scenario, uses `assert_cmd::Command::cargo_bin("xzatoma")` to invoke
   the binary with the specified args and env.
3. Asserts exit code, stdout substring, and stderr substring.
4. Provides a temporary config file via `--config` for scenarios that need valid
   config to pass parsing.
5. Reports per-scenario pass/fail.

Some scenarios need a valid config file to get past config loading. The test
harness writes a minimal `config.yaml` to a temp directory and passes
`--config <temp>/config.yaml` for those scenarios. Scenarios that test help
output (`--help`) do not need a config file since clap handles them before
config loading.

#### Task 8.3 Create `evals/cli_commands/README.md`

Document the CLI eval structure, how to add scenarios, and the distinction
between help scenarios (no config needed) and execution scenarios (config
provided).

#### Task 8.4 Testing Requirements

- Run `cargo test --test eval_cli_commands -- --nocapture`.
- All scenarios pass.
- Binary must be built before running (`cargo test` handles this automatically).
- No network calls.

#### Task 8.5 Deliverables

- `evals/cli_commands/scenarios.yaml`
- `evals/cli_commands/README.md`
- `tests/eval_cli_commands.rs`

#### Task 8.6 Success Criteria

- `cargo test --test eval_cli_commands` exits 0 with 25+ scenarios passing.
- Every subcommand has at least one `--help` scenario and one execution
  scenario.
- No existing tests break.

---

## Eval System Architecture

### Directory Structure

```text
evals/
├── run_command/                    # Phase 1-3 (COMPLETED)
│   ├── README.md
│   ├── scenarios.yaml
│   └── plans/
│       ├── simple_plan.yaml
│       ├── multi_step_plan.yaml
│       ├── simple_plan.json       # Phase 5
│       ├── multi_step_plan.json   # Phase 5
│       ├── simple_plan.md         # Phase 5
│       ├── multi_step_plan.md     # Phase 5
│       ├── invalid_no_steps.yaml
│       ├── invalid_no_steps.json  # Phase 5
│       ├── invalid_no_steps.md    # Phase 5
│       ├── invalid_no_name.yaml
│       ├── invalid_step_no_action.yaml
│       ├── empty_file.yaml        # Phase 5
│       ├── malformed_yaml.yaml    # Phase 5
│       ├── malformed_json.json    # Phase 5
│       └── unknown_extension.txt  # Phase 5
├── config_validation/              # Phase 4
│   ├── README.md
│   ├── scenarios.yaml
│   └── configs/
│       ├── valid_copilot.yaml
│       ├── valid_ollama.yaml
│       ├── valid_full.yaml
│       ├── invalid_empty_provider.yaml
│       └── ... (28+ files)
├── skills_command/                 # Phase 6
│   ├── README.md
│   ├── scenarios.yaml
│   └── skills/
│       ├── valid_simple.md
│       ├── valid_with_tools.md
│       └── ... (8 files)
├── mention_parser/                 # Phase 7
│   ├── README.md
│   ├── scenarios.yaml
│   └── files/
│       ├── sample.rs
│       ├── config.yaml
│       ├── large_binary.bin
│       └── README.md
└── cli_commands/                   # Phase 8
    ├── README.md
    └── scenarios.yaml

tests/
├── eval_run_command.rs             # Phase 2 (COMPLETED)
├── eval_config_validation.rs       # Phase 4
├── eval_skills_command.rs          # Phase 6
├── eval_mention_parser.rs          # Phase 7
└── eval_cli_commands.rs            # Phase 8
```

### Design Principles

1. **Offline and deterministic** -- No eval scenario requires network access.
   Provider-dependent paths use an invalid provider type. URL mention scenarios
   expect errors.

2. **Data-driven** -- Scenarios live in YAML files, not Rust code. Adding a
   scenario means editing a YAML file and optionally adding a fixture, not
   writing Rust.

3. **Fixture co-location** -- Each eval suite keeps its fixtures in a
   subdirectory alongside `scenarios.yaml`. Paths in scenarios are relative to
   the suite directory.

4. **Consistent schema** -- All scenario files follow the same pattern: `id`,
   `description`, `input`/`args`, `expect` with `outcome`/`exit_code` and
   optional substring matchers.

5. **Independent execution** -- Each eval suite has its own integration test
   file. Suites can be run individually or together.

### Running All Evals

```bash
# Run all eval suites
cargo test --test 'eval_*' -- --nocapture

# Run a specific suite
cargo test --test eval_run_command -- --nocapture
cargo test --test eval_config_validation -- --nocapture
cargo test --test eval_skills_command -- --nocapture
cargo test --test eval_mention_parser -- --nocapture
cargo test --test eval_cli_commands -- --nocapture
```

## Recommended Implementation Order

1. **Phase 4: Config Validation Evals** -- Highest leverage. Config validation
   is pure logic with 36+ branches, all testable with simple YAML fixtures.
2. **Phase 5: Plan Parsing Format Evals** -- Low effort, extends existing
   infrastructure. Only requires new fixture files and scenario entries.
3. **Phase 8: CLI Command Evals** -- Broad coverage of the user-facing surface.
   Uses `assert_cmd` which is already a dev-dependency.
4. **Phase 6: Skills Command Evals** -- Medium complexity due to temp directory
   setup, but skills are a major feature area.
5. **Phase 7: Mention Parser Evals** -- Medium complexity. The mention parser
   already has 57 unit tests, so data-driven evals add incremental value for
   augmentation paths that involve file I/O.

## Out of Scope

The following areas are noted but intentionally excluded from this eval plan:

- **Watcher/Kafka integration** -- Requires a running Kafka or Redpanda cluster.
  Consider `rdkafka::mocking::MockCluster` for a separate integration test plan.
- **Provider API calls** -- Copilot and Ollama require external services. Use
  `wiremock` for mock HTTP server tests in a separate effort.
- **ACP server lifecycle** -- `acp serve` starts an HTTP server, which is better
  tested through dedicated HTTP integration tests using `wiremock` or
  `axum::test`.
- **Interactive chat mode** -- Requires stdin/stdout interaction. Consider
  `rexpect` or `pty`-based testing in a separate plan.

## Resolved Decisions

- Eval scenarios use YAML (not JSON or TOML) to match the project convention.
- Fixture files use `.yaml` extension per AGENTS.md Rule 1.
- The `test_mode` field allows scenarios to target different layers (parser,
  command, binary) within the same suite.
- CLI command evals use `assert_cmd` for binary-level testing, while other
  suites call library functions directly for faster execution and better error
  messages.
- Config validation evals deserialize fixtures via
  `serde_yaml::from_str::<Config>()` rather than `Config::load()` to isolate
  validation from file I/O and env var resolution.
