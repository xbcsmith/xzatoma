# CLI Commands Evals

Data-driven evaluation suite that exercises CLI argument parsing and basic
command dispatch for every `xzatoma` subcommand using `assert_cmd` against the
compiled binary.

## Purpose

These evals verify that:

- Every subcommand accepts `--help` and exits 0 with relevant usage text.
- Global flags (`--help`, `--version`) work correctly.
- Missing required subcommands are rejected by clap with a non-zero exit.
- Execution-path scenarios reach the expected early failure point without
  requiring live network access or external services (Kafka, AI providers).

## Directory Structure

```text
evals/cli_commands/
  scenarios.yaml    scenario definitions
  README.md         this file
```

## Scenario Schema

Each entry in `scenarios.yaml` has the following fields:

| Field                    | Type              | Required | Description                                                                                       |
| ------------------------ | ----------------- | -------- | ------------------------------------------------------------------------------------------------- |
| `id`                     | string            | yes      | Unique identifier shown in test output                                                            |
| `description`            | string            | yes      | Human-readable purpose of the scenario                                                            |
| `args`                   | list of strings   | yes      | CLI arguments passed to the binary                                                                |
| `env`                    | map string:string | no       | Extra environment variables set on the subprocess                                                 |
| `needs_config`           | bool              | no       | When true the harness provides `--config` pointing at a minimal temp config file (default: false) |
| `expect.success`         | bool              | yes      | `true` if exit code must be 0, `false` if non-zero                                                |
| `expect.stdout_contains` | string            | no       | Substring that must appear in stdout                                                              |
| `expect.stderr_contains` | string            | no       | Substring that must appear in stderr                                                              |

The token `__TEMP_DIR__` anywhere in `args` is replaced at runtime with the
absolute path of the per-scenario temporary directory. Use it for subcommands
that accept an explicit database or file path argument, for example:

```yaml
args: ["replay", "--db-path", "__TEMP_DIR__/replay.db"]
```

## Two Classes of Scenario

### Help / clap-only (`needs_config: false`)

These scenarios invoke `--help`, `--version`, or omit a required subcommand.
Clap handles them before `Config::load` is called. No config file is written and
no environment overrides are injected.

Examples: `global_help`, `run_help`, `models_list_help`, `replay_help`.

### Execution (`needs_config: true`)

These scenarios must pass `Config::load` and `Config::validate` before command
dispatch. The test harness:

1. Writes a minimal `config.yaml` to a per-scenario temp directory.
2. Prepends `["--config", "<temp>/config.yaml"]` to the argument list.
3. Sets `XZATOMA_HISTORY_DB=<temp>/history.db` so history commands use an
   isolated database and never touch the developer's real storage.
4. Sets `XZATOMA_SKILLS_TRUST_STORE_PATH=<temp>/skills_trust.yaml` so skills
   trust commands use an isolated store.

Each execution scenario is hermetic: temp directories are created fresh and
cleaned up automatically after each scenario.

## Minimal Config File

The harness provides this config for all `needs_config: true` scenarios:

```yaml
provider:
  type: ollama
  ollama:
    host: "http://localhost:11434"
    model: "llama3.2:latest"
agent:
  max_turns: 50
  timeout_seconds: 300
skills:
  enabled: true
  project_enabled: false
  user_enabled: false
  additional_paths: []
```

`project_enabled: false` and `user_enabled: false` prevent the binary from
scanning the real project tree or home directory during test execution.

## Running the Evals

```bash
cargo test --test eval_cli_commands
```

To see per-scenario pass/fail output:

```bash
cargo test --test eval_cli_commands -- --nocapture
```

## Adding New Scenarios

1. Determine whether the new scenario needs config loading
   (`needs_config: true`) or is handled entirely by clap
   (`needs_config: false`).
2. Add an entry to `scenarios.yaml` following the schema above.
3. Run `cargo test --test eval_cli_commands -- --nocapture` and confirm the new
   scenario passes.
4. No Rust code changes are required for purely argument-level variations.

## Coverage

The suite covers all 10 subcommands:

| Subcommand | Help scenario | Execution scenario(s)                   |
| ---------- | ------------- | --------------------------------------- |
| `run`      | yes           | no-plan/prompt error, missing plan file |
| `chat`     | yes           | mode flag accepted (fails at provider)  |
| `watch`    | yes           | generic watcher offline failure         |
| `auth`     | yes           | copilot offline failure                 |
| `models`   | yes (3)       | missing subcommand error                |
| `history`  | yes (3)       | list with empty isolated storage        |
| `mcp`      | yes           | list with no servers configured         |
| `acp`      | yes (2)       | config output, validate                 |
| `skills`   | no            | list, paths, trust show, validate       |
| `replay`   | yes           | list empty db, no-flags error           |

Global options covered: `--help`, `--version`, no-subcommand error.

## Notes on Offline Scenarios

The `watch_generic_offline` and `auth_copilot_offline` scenarios intentionally
trigger early failure paths that require an unavailable external service (Kafka
and GitHub OAuth respectively). In environments where connection attempts to
closed ports succeed quickly (connection refused), these scenarios complete in
under one second. In environments with aggressive firewall rules that drop
packets silently, they may take several seconds to time out. Both are expected
to fail non-zero; the exact error message is not asserted.
