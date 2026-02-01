# CLI Reference

## Overview

This document describes the `xzatoma` command-line interface: global options, subcommands, flags, and common usage patterns. XZatoma is an autonomous AI agent CLI that runs plans (workflows), provides interactive chat, handles provider authentication, and can manage and inspect AI models.

Primary commands:

- `chat` — start interactive agent chat
- `run` — execute a plan file or a single prompt
- `auth` — perform provider authentication flows
- `models` — inspect and manage provider models

Default config file: `config/config.yaml` (the CLI's `--config`/`-c` option defaults to this path).

## Usage

```
xzatoma [OPTIONS] <SUBCOMMAND>
```

Global options:

- `-c, --config <PATH>` — path to configuration file (default: `config/config.yaml`)
- `-v, --verbose` — enable verbose logging (enables more debug output)
- `-h, --help` — show help and exit
- `--version` — print version information and exit

Note: When running via `cargo run` forward arguments to the binary with `--`:

```bash
cargo run -- --config custom.yaml run --plan examples/quickstart_plan.yaml
```

## Subcommands

### chat

Start an interactive chat session with the agent.

Synopsis:

```
xzatoma chat [--provider <name>] [--mode <planning|write>] [-s|--safe]
```

Options:

- `-p, --provider <name>` — temporarily override the configured provider (e.g., `copilot`, `ollama`)
- `-m, --mode <planning|write>` — chat mode; defaults to `planning`. Modes control whether the agent operates in read-only planning mode or in write mode (which may propose changes).
- `-s, --safe` — enable safety mode; the agent will confirm potentially dangerous operations

Examples:

```bash
# Start chat in planning mode (default)
xzatoma chat

# Start write-mode chat using ollama
xzatoma chat --provider ollama --mode write

# Start chat with safety confirmation
xzatoma chat --safe
```

### run

Execute a plan file or run a single prompt. The `run` command constructs a task from the plan (or prompt) and sends it to the configured provider; the agent may execute tools available in the environment (file ops, terminal, etc.).

Synopsis:

```
xzatoma run [--plan <PATH>] [--prompt <TEXT>] [--allow-dangerous]
```

Options:

- `-p, --plan <PATH>` — path to a plan file (YAML, JSON or Markdown). The plan is validated before execution.
- `--prompt <TEXT>` — direct prompt to execute; mutually exclusive with `--plan` (one of them must be provided).
- `--allow-dangerous` — escalate execution mode to `FullAutonomous` (use with caution).

Notes:

- The `run` subcommand does not include a `--provider` flag. To override the provider for a single execution, set the `XZATOMA_PROVIDER` environment variable:

```bash
XZATOMA_PROVIDER=ollama xzatoma run --plan plans/generate_docs.yaml
```

Examples:

```bash
# Run a plan from a YAML file
xzatoma run --plan examples/quickstart_plan.yaml

# Run a one-line prompt instead of a plan
xzatoma run --prompt "Analyze the repository and propose a documentation plan."

# Allow escalated execution (dangerous): use only when you understand the implications
xzatoma run --plan plans/dangerous_plan.yaml --allow-dangerous
```

### auth

Trigger provider-specific authentication flows.

Synopsis:

```
xzatoma auth [--provider <name>]
```

Options:

- `-p, --provider <name>` — provider to authenticate with (e.g., `copilot`, `ollama`)

Behavior:

- `copilot`: initiates a GitHub OAuth device flow. A verification URL and code will be printed. After you complete the device flow, tokens are cached (keyring) for subsequent runs.
- `ollama`: typically requires no cloud authentication; ensure your local or remote Ollama instance is reachable and configured.

Example:

```bash
# Start Copilot device flow:
xzatoma auth --provider copilot
```

### models

Commands for inspecting and managing provider models.

Subcommands:

- `xzatoma models list [--provider <name>] [--json] [--summary]` — list available models, optionally filtered by provider. Use `--json` to produce pretty-printed JSON suitable for scripting/export. Use `--summary` to produce a compact, human-readable summary table (columns: Model Name, Display Name, Context Window, State, Tool Calls, Vision). Combine both (`--json --summary`) to serialize the summary format as JSON.
- `xzatoma models info --model <id> [--provider <name>] [--json] [--summary]` — show detailed information about a specific model. Use `--json` for pretty JSON output; `--summary` shows a compact summary view. Combine both to get summary JSON output.
- `xzatoma models current [--provider <name>]` — show currently active model for a provider

Examples:

```bash
# List all models (table)
xzatoma models list

# List Ollama models
xzatoma models list --provider ollama

# Export all models as pretty JSON
xzatoma models list --json > all_models.json

# Show a compact human-readable summary
xzatoma models list --summary

# Export summary-enriched models as JSON
xzatoma models list --json --summary > all_models_with_summary.json

# Get info about a model (table)
xzatoma models info --model gpt-4

# Get summary info for a model
xzatoma models info --model gpt-4 --summary

# Export model info as JSON
xzatoma models info --model gpt-4 --json > gpt4_info.json

# Show current model for Copilot
xzatoma models current --provider copilot
```

## Environment variables and configuration precedence

Configuration is loaded from the file specified by `--config` (default `config/config.yaml`), then environment variables are applied, and finally CLI overrides (where implemented) are applied.

Common environment variables:

- `XZATOMA_PROVIDER` — override provider (e.g., `copilot` or `ollama`)
- `XZATOMA_COPILOT_MODEL` — default model name for Copilot
- `XZATOMA_OLLAMA_HOST` — host (and port) for Ollama (e.g., `localhost:11434`)
- `XZATOMA_OLLAMA_MODEL` — default model for Ollama
- `XZATOMA_MAX_TURNS` — agent max turns
- `XZATOMA_TIMEOUT_SECONDS` — agent timeout in seconds
- `XZATOMA_EXECUTION_MODE` — execution mode (`interactive`, `restricted_autonomous`, or `full_autonomous`)

Use environment variables for short-lived overrides or CI-based configuration. Use the configuration file for persistent, repo-specific settings.

See `docs/reference/configuration.md` for the full configuration schema.

## Execution modes

The agent supports multiple execution modes that influence safety and validation:

- `Interactive` — human-in-the-loop
- `RestrictedAutonomous` — limited autonomous operations (safer defaults)
- `FullAutonomous` — full autonomy (dangerous; requires explicit consent via `--allow-dangerous`)

The mode affects validators used by tools such as the terminal to prevent accidental dangerous commands.

## Help and discovery

- Show global help:

```bash
xzatoma --help
```

- Show help for a subcommand:

```bash
xzatoma run --help
xzatoma models info --help
```

## Exit codes

- `0` — success
- Non-zero — an error occurred (validation error, provider error, tool error, etc.)
- When running under `cargo run`, the binary exit code is propagated to the shell.

## Examples

Run the quickstart plan:

```bash
# Run using the installed binary
xzatoma run --plan examples/quickstart_plan.yaml

# Run via cargo (forward args with --)
cargo run -- run --plan examples/quickstart_plan.yaml
```

Authenticate with Copilot (device flow):

```bash
xzatoma auth --provider copilot
```

Run a one-off prompt:

```bash
xzatoma run --prompt "Evaluate this repository and produce a short documentation outline"
```

Run with a different provider for a single invocation:

```bash
XZATOMA_PROVIDER=ollama xzatoma run --plan examples/quickstart_plan.yaml
```

Enable debug logging to troubleshoot:

```bash
RUST_LOG=debug xzatoma run --plan examples/quickstart_plan.yaml
```

## Troubleshooting & tips

- If the agent reports authentication errors for Copilot, re-run `xzatoma auth --provider copilot`.
- If the agent cannot reach Ollama, confirm `XZATOMA_OLLAMA_HOST` or the `provider.ollama.host` config field is correct.
- Use `--allow-dangerous` only after understanding which operations are being automated.
- Validate plan files locally with `xzatoma run --plan` — parsing and basic validation are performed prior to execution. Common plan errors include missing `name` or missing step `action`.

## See also

- Quickstart tutorial: `../tutorials/quickstart.md`
- Create workflows: `../how-to/create_workflows.md`
- Generate documentation: `../how-to/generate_documentation.md`
- Configuration reference: `configuration.md`
- Workflow format: `workflow_format.md`

---

Last updated: 2026-01-24
