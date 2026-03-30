# CLI Reference

## Overview

This document describes the `xzatoma` command-line interface: global options,
subcommands, flags, and common usage patterns. XZatoma is an autonomous AI agent
CLI that runs plans (workflows), provides interactive chat, handles provider
authentication, and can manage and inspect AI models.

Primary commands:

- `chat` — start interactive agent chat
- `run` — execute a plan file or a single prompt
- `auth` — perform provider authentication flows
- `models` — inspect and manage provider models
- `history` — inspect and manage conversation history
- `watch` — watch a Kafka topic and process events
- `mcp` — manage MCP (Model Context Protocol) servers
- `acp` — manage and run the ACP (Agent Communication Protocol) server
- `skills` — discover, validate, and manage agent skills
- `replay` — replay and inspect saved conversations

Default config file: `config/config.yaml` (the CLI's `--config`/`-c` option
defaults to this path).

## Usage

```text
xzatoma [OPTIONS] <SUBCOMMAND>
```

Global options:

- `-c, --config <PATH>` — path to configuration file (default:
  `config/config.yaml`)
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

```text
xzatoma chat [--provider <name>] [--mode <planning|write>] [-s|--safe]
```

Options:

- `-p, --provider <name>` — temporarily override the configured provider (e.g.,
  `copilot`, `ollama`)
- `-m, --mode <planning|write>` — chat mode; defaults to `planning`. Modes
  control whether the agent operates in read-only planning mode or in write mode
  (which may propose changes).
- `-s, --safe` — enable safety mode; the agent will confirm potentially
  dangerous operations

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

Execute a plan file or run a single prompt. The `run` command constructs a task
from the plan (or prompt) and sends it to the configured provider; the agent may
execute tools available in the environment (file ops, terminal, etc.).

Synopsis:

```text
xzatoma run [--plan <PATH>] [--prompt <TEXT>] [--allow-dangerous]
```

Options:

- `-p, --plan <PATH>` — path to a plan file (YAML, JSON or Markdown). The plan
  is validated before execution.
- `--prompt <TEXT>` — direct prompt to execute; mutually exclusive with `--plan`
  (one of them must be provided).
- `--allow-dangerous` — escalate execution mode to `FullAutonomous` (use with
  caution).

Notes:

- The `run` subcommand does not include a `--provider` flag. To override the
  provider for a single execution, set the `XZATOMA_PROVIDER` environment
  variable:

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

```text
xzatoma auth [--provider <name>]
```

Options:

- `-p, --provider <name>` — provider to authenticate with (e.g., `copilot`,
  `ollama`)

Behavior:

- `copilot`: initiates a GitHub OAuth device flow. A verification URL and code
  will be printed. After you complete the device flow, tokens are cached
  (keyring) for subsequent runs.
- `ollama`: typically requires no cloud authentication; ensure your local or
  remote Ollama instance is reachable and configured.

Example:

```bash
# Start Copilot device flow:
xzatoma auth --provider copilot
```

### models

Commands for inspecting and managing provider models.

Subcommands:

- `xzatoma models list [--provider <name>] [--json] [--summary]` — list
  available models, optionally filtered by provider. Use `--json` to produce
  pretty-printed JSON suitable for scripting/export. Use `--summary` to produce
  a compact, human-readable summary table (columns: Model Name, Display Name,
  Context Window, State, Tool Calls, Vision). Combine both (`--json --summary`)
  to serialize the summary format as JSON.
- `xzatoma models info --model <id> [--provider <name>] [--json] [--summary]` —
  show detailed information about a specific model. Use `--json` for pretty JSON
  output; `--summary` shows a compact summary view. Combine both to get summary
  JSON output.
- `xzatoma models current [--provider <name>]` — show currently active model for
  a provider

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
xzatoma models info --model gpt-5.3-codex

# Get summary info for a model
xzatoma models info --model gpt-5.3-codex --summary

# Export model info as JSON
xzatoma models info --model gpt-5.3-codex --json > gpt53codex_info.json

# Show current model for Copilot
xzatoma models current --provider copilot
```

### history

Commands for inspecting and managing conversation history.

Subcommands:

- `xzatoma history list` — list all saved conversations with metadata
- `xzatoma history show --id <id> [--raw] [--limit N]` — show detailed
  message-level history for a conversation
- `xzatoma history delete --id <id>` — delete a saved conversation

#### history list

List all saved conversations with metadata (ID, title, model, message count,
last updated).

Synopsis:

```text
xzatoma history list
```

Output: Table showing conversation ID (first 8 chars), title, model used, number
of messages, and timestamp of last update.

Examples:

```bash
# List all conversations
xzatoma history list
```

#### history show

Display detailed message-level history for a specific conversation. Supports
formatted output (default) or raw JSON export, with optional message limiting.

Synopsis:

```text
xzatoma history show --id <id> [--raw] [--limit N]
```

Options:

- `-i, --id <ID>` — conversation ID to display (required)
- `-r, --raw` — output raw JSON format instead of formatted display
- `-n, --limit <N>` — show only the last N messages (default: all)

Output:

- **Default (formatted):** Color-coded message display with role, content
  preview, and metadata
- **Raw JSON:** Complete message objects including tool calls, tool results, and
  all message fields

Examples:

```bash
# Show full conversation history
xzatoma history show --id abc123def456

# Show last 10 messages only
xzatoma history show --id abc123def456 --limit 10

# Export as JSON for scripting or analysis
xzatoma history show --id abc123def456 --raw > history.json

# Combine options: show last 5 messages as JSON
xzatoma history show --id abc123def456 --raw --limit 5 > recent_messages.json
```

#### history delete

Delete a saved conversation permanently. This action cannot be undone.

Synopsis:

```text
xzatoma history delete --id <id>
```

Options:

- `-i, --id <ID>` — conversation ID to delete (required)

Examples:

```bash
# Delete a conversation
xzatoma history delete --id abc123def456
```

### watch

Watch a Kafka topic for events and process them using the configured watcher
backend. Supports both the XZepr-specific backend and a generic regex-matching
backend.

Synopsis:

```text
xzatoma watch [OPTIONS]
```

Options:

- `--topic <TOPIC>` — Kafka topic to watch (overrides config)
- `-e, --event-types <LIST>` — event types to process (comma-separated)
- `-f, --filter-config <PATH>` — filter configuration file (YAML)
- `-l, --log-file <PATH>` — log output file (defaults to STDOUT only)
- `--json-logs` — enable JSON-formatted logging (default: true)
- `--watcher-type <TYPE>` — watcher backend type: `xzepr` (default) or `generic`
- `--group-id <ID>` — Kafka consumer group ID (overrides config)
- `--output-topic <TOPIC>` — output topic for publishing results (generic
  watcher only; defaults to input topic)
- `--create-topics` — create missing Kafka topics automatically at watcher
  startup
- `--action <REGEX>` — generic matcher: regex pattern for the action field
  (case-insensitive)
- `--name <REGEX>` — generic matcher: regex pattern for the name field
  (case-insensitive)
- `--match-version <REGEX>` — generic matcher: regex pattern for the version
  field (case-insensitive)
- `--brokers <ADDRS>` — Kafka broker addresses (comma-separated, overrides
  config)
- `--dry-run` — dry run mode (parse but do not execute plans)

Examples:

```bash
# Watch with XZepr backend (default)
xzatoma watch --topic xzepr.events

# Watch with generic backend
xzatoma watch --watcher-type generic --topic plans.events --action deploy

# Dry-run mode
xzatoma watch --dry-run --watcher-type generic --topic plans.events

# Override brokers
xzatoma watch --brokers kafka-1:9092,kafka-2:9092 --topic events
```

### mcp

Commands for managing MCP (Model Context Protocol) servers.

Synopsis:

```text
xzatoma mcp <SUBCOMMAND>
```

Subcommands:

- `xzatoma mcp list` — list configured MCP servers. Shows server IDs, transport
  type, enabled/disabled status, and when `auto_connect` is enabled, shows live
  connection state and tool counts.

Examples:

```bash
# List configured MCP servers
xzatoma mcp list
```

### acp

Commands for managing and running the ACP (Agent Communication Protocol) server.

Synopsis:

```text
xzatoma acp <SUBCOMMAND>
```

Subcommands:

- `xzatoma acp serve [--host <HOST>] [--port <PORT>] [--base-path <PATH>] [--root-compatible]`
  — start the ACP server
- `xzatoma acp config` — show current ACP configuration
- `xzatoma acp runs [--session-id <ID>] [--limit <N>]` — list recent ACP agent
  runs
- `xzatoma acp validate --manifest <PATH>` — validate an ACP agent manifest file

#### acp serve

Start the ACP server, exposing agent capabilities over HTTP.

Synopsis:

```text
xzatoma acp serve [--host <HOST>] [--port <PORT>] [--base-path <PATH>] [--root-compatible]
```

Options:

- `--host <HOST>` — host to bind to (default: `127.0.0.1`)
- `--port <PORT>` — port to listen on (default: `8090`)
- `--base-path <PATH>` — base path prefix for all routes (default: `/`)
- `--root-compatible` — enable v0.2.0+ root-compatible routing

#### acp config

Show the current ACP configuration.

Synopsis:

```text
xzatoma acp config
```

#### acp runs

List recent ACP agent runs with optional filtering.

Synopsis:

```text
xzatoma acp runs [--session-id <ID>] [--limit <N>]
```

Options:

- `--session-id <ID>` — filter runs by session ID
- `--limit <N>` — maximum number of runs to show

#### acp validate

Validate an ACP agent manifest file.

Synopsis:

```text
xzatoma acp validate --manifest <PATH>
```

Options:

- `--manifest <PATH>` — path to the manifest file to validate

#### acp examples

```bash
# Start ACP server with defaults
xzatoma acp serve

# Start ACP server on custom port
xzatoma acp serve --port 9000 --host 0.0.0.0

# Show ACP configuration
xzatoma acp config

# List recent runs
xzatoma acp runs --limit 20

# Validate a manifest
xzatoma acp validate --manifest agent_manifest.yaml
```

### skills

Commands for discovering, validating, and managing agent skills.

Synopsis:

```text
xzatoma skills <SUBCOMMAND>
```

Subcommands:

- `xzatoma skills list` — list valid loaded visible skills
- `xzatoma skills validate` — validate all discovered skills
- `xzatoma skills show --name <NAME>` — show details for a specific skill
- `xzatoma skills paths` — show skill discovery paths and their status
- `xzatoma skills trust <SUBCOMMAND>` — manage skill trust

#### skills list

List all valid, loaded, and visible skills.

Synopsis:

```text
xzatoma skills list
```

#### skills validate

Validate all discovered skills, reporting any errors found.

Synopsis:

```text
xzatoma skills validate
```

#### skills show

Show detailed information for a specific skill.

Synopsis:

```text
xzatoma skills show --name <NAME>
```

Options:

- `--name <NAME>` — name of the skill to display

#### skills paths

Show skill discovery paths and their status (whether they exist and are
accessible).

Synopsis:

```text
xzatoma skills paths
```

#### skills trust

Manage skill trust configuration.

Subcommands:

- `xzatoma skills trust show` — show current trust configuration
- `xzatoma skills trust add --path <PATH>` — add a path to the trusted list
- `xzatoma skills trust remove --path <PATH>` — remove a path from the trusted
  list

#### skills examples

```bash
# List available skills
xzatoma skills list

# Validate all skills
xzatoma skills validate

# Show a specific skill
xzatoma skills show --name my_skill

# Show discovery paths
xzatoma skills paths

# Manage trust
xzatoma skills trust show
xzatoma skills trust add --path ./custom_skills
xzatoma skills trust remove --path ./old_skills
```

### replay

Replay and inspect saved conversations from the conversation database.

Synopsis:

```text
xzatoma replay [OPTIONS]
```

Options:

- `-i, --id <ID>` — conversation ID to replay
- `-l, --list` — list all conversations
- `--db-path <PATH>` — path to conversation database (default:
  `~/.xzatoma/conversations.db`)
- `--limit <N>` — limit for list results (default: `10`)
- `--offset <N>` — offset for pagination (default: `0`)
- `-t, --tree` — show conversation tree (with nested subagents)

Examples:

```bash
# List saved conversations
xzatoma replay --list

# Replay a specific conversation
xzatoma replay --id abc123

# Show conversation tree
xzatoma replay --id abc123 --tree

# Paginate through conversations
xzatoma replay --list --limit 20 --offset 10
```

## Environment variables and configuration precedence

Configuration is loaded from the file specified by `--config` (default
`config/config.yaml`), then environment variables are applied, and finally CLI
overrides (where implemented) are applied.

Common environment variables:

- `XZATOMA_PROVIDER` — override provider (e.g., `copilot` or `ollama`)
- `XZATOMA_COPILOT_MODEL` — default model name for Copilot
- `XZATOMA_OLLAMA_HOST` — host (and port) for Ollama (e.g., `localhost:11434`)
- `XZATOMA_OLLAMA_MODEL` — default model for Ollama
- `XZATOMA_MAX_TURNS` — agent max turns
- `XZATOMA_TIMEOUT_SECONDS` — agent timeout in seconds
- `XZATOMA_EXECUTION_MODE` — execution mode (`interactive`,
  `restricted_autonomous`, or `full_autonomous`)

Use environment variables for short-lived overrides or CI-based configuration.
Use the configuration file for persistent, repo-specific settings.

See `docs/reference/configuration.md` for the full configuration schema.

## Execution modes

The agent supports multiple execution modes that influence safety and
validation:

- `Interactive` — human-in-the-loop
- `RestrictedAutonomous` — limited autonomous operations (safer defaults)
- `FullAutonomous` — full autonomy (dangerous; requires explicit consent via
  `--allow-dangerous`)

The mode affects validators used by tools such as the terminal to prevent
accidental dangerous commands.

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
- Non-zero — an error occurred (validation error, provider error, tool error,
  etc.)
- When running under `cargo run`, the binary exit code is propagated to the
  shell.

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

- If the agent reports authentication errors for Copilot, re-run
  `xzatoma auth --provider copilot`.
- If the agent cannot reach Ollama, confirm `XZATOMA_OLLAMA_HOST` or the
  `provider.ollama.host` config field is correct.
- Use `--allow-dangerous` only after understanding which operations are being
  automated.
- Validate plan files locally with `xzatoma run --plan` — parsing and basic
  validation are performed prior to execution. Common plan errors include
  missing `name` or missing step `action`.

## See also

- Quickstart tutorial: `../tutorials/quickstart.md`
- Create workflows: `../how-to/create_workflows.md`
- Generate documentation: `../how-to/generate_documentation.md`
- Configuration reference: `configuration.md`
- Workflow format: `workflow_format.md`
- Skills reference: `skills.md`
- ACP reference: `acp.md`

---

Last updated: 2026-01-24
