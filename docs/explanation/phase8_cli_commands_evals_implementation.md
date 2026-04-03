# Phase 8: CLI Commands Evals Implementation

## Overview

Phase 8 adds a data-driven eval suite that exercises CLI argument parsing and
basic command dispatch for all ten `xzatoma` subcommands using `assert_cmd`
against the compiled binary. The suite validates argument acceptance, help
output, and early error paths without requiring live network access or external
services.

## Deliverables

| File                                                           | Purpose                                          |
| -------------------------------------------------------------- | ------------------------------------------------ |
| `evals/cli_commands/scenarios.yaml`                            | 31 scenario definitions covering all subcommands |
| `tests/eval_cli_commands.rs`                                   | Data-driven test harness using `assert_cmd`      |
| `evals/cli_commands/README.md`                                 | Usage guide and schema reference                 |
| `docs/explanation/phase8_cli_commands_evals_implementation.md` | This document                                    |

## Design Decisions

### Two Classes of Scenario

Scenarios are divided into two classes based on whether they reach config
loading.

**Help / clap-only** (`needs_config: false`): clap handles `--help`,
`--version`, and missing-required-subcommand errors before `Config::load` is
called. These scenarios receive no `--config` argument and complete in
milliseconds. They are safe to run in any environment.

**Execution** (`needs_config: true`): commands that must pass `Config::load` and
`Config::validate` before dispatch. The harness writes a minimal config file to
a per-scenario temp directory and injects `--config <path>` before the
subcommand name. `XZATOMA_HISTORY_DB` and `XZATOMA_SKILLS_TRUST_STORE_PATH` are
also redirected to temp paths to keep each scenario hermetic.

### Minimal Config File

All execution scenarios share a single minimal config written by the harness at
runtime:

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
scanning the real project directory or the developer's home directory during
test execution, keeping results deterministic regardless of the host filesystem.

### TEMP_DIR Token Substitution

Some subcommands accept explicit database or file path arguments (for example,
`replay --db-path`). Rather than adding per-scenario schema fields, the harness
replaces the token `__TEMP_DIR__` anywhere in an argument string with the
absolute path of the per-scenario temporary directory. This keeps the YAML
schema simple while supporting path injection.

Example scenario using the token:

```yaml
- id: replay_no_flags
  args: ["replay", "--db-path", "__TEMP_DIR__/replay.db"]
  needs_config: true
  expect:
    success: false
    stderr_contains: "Must specify --list or --id"
```

### Hermetic Storage

When `needs_config: true`, the harness always sets two environment variables:

- `XZATOMA_HISTORY_DB=<temp>/history.db` -- redirects `SqliteStorage::new()`
  away from the real application data directory.
- `XZATOMA_SKILLS_TRUST_STORE_PATH=<temp>/skills_trust.yaml` -- redirects the
  skills trust store away from `~/.xzatoma/`.

Both environment variables are checked by the application at startup and take
priority over filesystem defaults, so no changes to application code were
required.

### Exit Code Semantics

The harness distinguishes only zero vs non-zero exit codes (`success: true` /
`success: false`). Exact non-zero codes are not asserted because:

- Clap errors exit with code 2.
- Runtime errors propagated through `main` exit with code 1.
- `std::process::exit(1)` used in the `replay` command also exits with code 1.

Treating all non-zero exits uniformly avoids brittle assertions that would break
if exit code conventions change internally.

## Scenario Coverage

The suite contains 31 scenarios across all subcommands.

### Global Options (3 scenarios)

| Scenario id            | Args        | Expected                               |
| ---------------------- | ----------- | -------------------------------------- |
| `global_help`          | `--help`    | exit 0, stdout contains "xzatoma"      |
| `global_version`       | `--version` | exit 0, stdout contains "xzatoma"      |
| `global_no_subcommand` | (none)      | exit non-zero, stderr contains "error" |

### run Command (3 scenarios)

| Scenario id             | Notes                                      |
| ----------------------- | ------------------------------------------ |
| `run_help`              | clap help, no config needed                |
| `run_no_plan_or_prompt` | triggers "Either --plan or --prompt" error |
| `run_missing_plan_file` | triggers plan file not found error         |

### chat Command (2 scenarios)

`chat_help` verifies clap help output. `chat_mode_flag_accepted` confirms that
`--mode write` is accepted by clap and the process fails only at provider
connection time, not at argument parsing.

### watch Command (2 scenarios)

`watch_help` verifies clap help output. `watch_generic_offline` passes
`--watcher-type generic --topic test-topic` with a valid config and expects
failure at the Kafka bootstrap phase since no broker is running.

### auth Command (2 scenarios)

`auth_help` verifies clap help output. `auth_copilot_offline` passes
`--provider copilot` and expects failure at the GitHub OAuth flow since no
network credentials are present.

### models Command (4 scenarios)

Three `--help` scenarios (one per subcommand) plus `models_no_subcommand` which
exercises clap's missing-required-subcommand error path.

### history Command (4 scenarios)

Three `--help` scenarios plus `history_list_empty` which runs `history list`
against an isolated empty database and expects exit 0.

### mcp Command (2 scenarios)

`mcp_list_help` plus `mcp_list_no_servers` which runs `mcp list` with the
minimal config (no servers defined) and expects exit 0 with the output "No MCP
servers configured".

### acp Command (3 scenarios)

`acp_config` runs `acp config` and expects exit 0 with JSON output containing
"host". `acp_validate_help` and `acp_serve_help` verify clap help for the two
other ACP subcommands.

### skills Command (4 scenarios)

All four scenarios use `needs_config: true`. Discovery is disabled in the
minimal config so the binary does not scan the project tree. All four
(`skills_list_empty`, `skills_paths`, `skills_trust_show`, `skills_validate`)
are expected to exit 0.

### replay Command (3 scenarios)

`replay_help` verifies clap help output. `replay_list_empty` runs
`replay --list --db-path __TEMP_DIR__/replay.db` against an empty temp database
and expects exit 0. `replay_no_flags` runs with only `--db-path` set and expects
exit 1 with the stderr message "Must specify --list or --id".

## Test Harness Architecture

```text
eval_cli_commands_scenarios()
  |
  +-- loads evals/cli_commands/scenarios.yaml
  |
  +-- for each scenario:
        |
        +-- run_scenario(scenario)
              |
              +-- TempDir::new()           per-scenario isolation
              |
              +-- if needs_config:
              |     write minimal config.yaml to temp dir
              |     prepend --config <path> to args
              |     set XZATOMA_HISTORY_DB env
              |     set XZATOMA_SKILLS_TRUST_STORE_PATH env
              |
              +-- substitute __TEMP_DIR__ token in args
              |
              +-- Command::cargo_bin("xzatoma")
              |     .args(final_args)
              |     .env(scenario env + harness env)
              |     .output()
              |
              +-- check_output(status, stdout, stderr, expect)
                    exit code check (success vs non-zero)
                    stdout_contains check (if set)
                    stderr_contains check (if set)
```

## Quality Gate Results

All quality gates were run in the following order:

```text
cargo fmt --all                               -- no changes
cargo check --all-targets --all-features      -- ok
cargo clippy --all-targets --all-features -- -D warnings  -- ok
cargo test --all-features                     -- all tests pass
```

Markdown files were also linted and formatted:

```text
markdownlint --fix --config .markdownlint.json evals/cli_commands/README.md
prettier --write --parser markdown --prose-wrap always evals/cli_commands/README.md
markdownlint --fix --config .markdownlint.json docs/explanation/phase8_cli_commands_evals_implementation.md
prettier --write --parser markdown --prose-wrap always docs/explanation/phase8_cli_commands_evals_implementation.md
```

## Relationship to Other Eval Phases

Phase 8 completes the eval system by covering the binary's external interface.
Previous phases covered internal library functions:

| Phase | Scope                             | Approach               |
| ----- | --------------------------------- | ---------------------- |
| 1-3   | `run` command plan fixtures       | library function calls |
| 4     | config validation                 | library function calls |
| 5     | plan parsing formats              | library function calls |
| 6     | skills command                    | library function calls |
| 7     | mention parser                    | library function calls |
| 8     | CLI command dispatch (this phase) | binary subprocess      |

The binary-level approach used in Phase 8 complements the library-level approach
of earlier phases: library tests verify logic in isolation while binary tests
verify that the assembled binary correctly routes arguments to the right
handlers and produces the expected exit codes and output.
