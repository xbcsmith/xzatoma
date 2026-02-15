# Configuration Reference

## Overview

This document describes XZatoma's configuration model: where configuration is read from, the key schema used in the YAML config file, environment variables that override configuration values, and validation rules you should be aware of.

Key points:

- Default config path used by the CLI is `config/config.yaml`. You may override it with `--config <path>`.
- `Config::load(path, cli)` semantics:

1.  If a config file exists at `path`, it is parsed; otherwise defaults are used.
2.  Environment variables are applied (override file values).
3.  CLI-level overrides are applied last (where implemented).

- Sensitive credentials are not recommended to be stored directly in config files; use provider auth flows (e.g., Copilot device flow caches tokens in the system keyring).

---

## Default config path & loading

By default, the CLI uses the path `config/config.yaml` unless you provide a different path with the `--config` option when invoking `xzatoma`.

Example:

```bash
# Use a custom config location
xzatoma --config ~/.config/xzatoma/config.yaml run --plan examples/quickstart_plan.yaml
```

If the file at the given path does not exist, the application will fall back to sensible defaults and then apply any environment variable overrides.

---

## Top-level keys (schema summary)

The configuration is organized into a small number of top-level sections. The schema shown here is a concise reference — consult `src/config.rs` for the canonical Rust types and defaults if you need exact types or additional fields.

- `provider` — Provider selection and provider-specific settings
- `provider_type` (string) — e.g., `copilot` or `ollama`
- `copilot` — Copilot-specific configuration (e.g., `model`, optional `api_base`)
- `ollama` — Ollama-specific configuration (e.g., `host`, `model`)

- `agent` — Agent runtime configuration
- `max_turns` (integer) — Max conversation turns (default: 50)
- `timeout_seconds` (integer) — Per-run timeout (default: 600)
- `conversation` — Conversation context management settings
  - `max_tokens` (integer) — Maximum context window size in tokens (default: 100000)
  - `min_retain_turns` (integer) — Minimum conversation turns to retain (default: 5)
  - `prune_threshold` (float) — Token threshold for pruning (0.0-1.0, default: 0.8)
  - `warning_threshold` (float) — Token usage threshold for warnings (0.0-1.0, default: 0.85)
  - `auto_summary_threshold` (float) — Token threshold for automatic summarization (0.0-1.0, default: 0.90)
  - `summary_model` (string, optional) — Model to use for generating summaries
- `tools` — Tool-related limits (file read sizes, grep limits, fetch limits)
- `terminal` — Terminal/command validator settings
- `chat` — Chat-specific defaults (default mode, safety flags, etc.)

- `repository`
- `clone_depth` (integer) — `git` clone depth used when scanning repositories
- `ignore_patterns` (array of strings) — globs to exclude (e.g., `node_modules`)

- `documentation`
- `output_dir` (string) — where generated docs are written
- `categories` (array of strings) — categories to generate (Diataxis: `tutorials`, `how_to`, `reference`, `explanation`)

---

## Conversation Context Window Management

XZatoma manages conversation context windows to prevent exceeding AI model token limits. Configure these settings to control how your conversation history is retained and summarized:

- `warning_threshold`: Token usage percentage to trigger warnings (0.0-1.0, default: 0.85)

  - When conversation exceeds this percentage, XZatoma shows a warning in chat mode
  - In run mode, this triggers automatic summarization if `auto_summary_threshold` is configured

- `auto_summary_threshold`: Token usage percentage to trigger automatic summarization (0.0-1.0, default: 0.90)

  - In run mode, when conversation exceeds this percentage, old turns are automatically summarized
  - The summarization reduces token usage while preserving conversation context
  - Chat mode requires manual summarization using `/context summary`

- `summary_model`: Model to use for generating summaries (optional, defaults to current model)

  - Useful for cost optimization: use a cheaper/faster model for summaries
  - Example: use `gpt-4o-mini` for Copilot or `mistral:latest` for Ollama
  - If not specified, uses the same model configured for main conversations

- `prune_threshold`: Token threshold to trigger conversation pruning (0.0-1.0, default: 0.8)

  - Before summarizing, older turns are pruned if conversation exceeds this threshold
  - Helps reduce unnecessary context from very old turns

- `min_retain_turns`: Minimum number of conversation turns to always keep (default: 5)
  - Even during summarization, this many recent turns are always retained
  - Ensures recent context is preserved even in summarized conversations

### Example Configuration

```yaml
agent:
  conversation:
    max_tokens: 100000
    min_retain_turns: 5
    prune_threshold: 0.8

    # Context window management
    warning_threshold: 0.85 # Warn at 85% full
    auto_summary_threshold: 0.90 # Auto-summarize at 90% full


    # Use cheaper model for summaries (optional)
    # summary_model: "gpt-4o-mini"  # For Copilot provider
```

---

## Example configuration (YAML)

```yaml
# config/config.yaml (example)
provider:
  provider_type: copilot # or 'ollama'
  copilot:
    model: gpt-5-mini
    # api_base: https://internal-copilot-host.example
  ollama:
    host: localhost:11434
    model: qwen3

agent:
  max_turns: 50
  timeout_seconds: 600
  # conversation, tools, terminal, and chat have nested defaults you may customize
  conversation:
    max_tokens: 100000
    min_retain_turns: 5
    prune_threshold: 0.8
    warning_threshold: 0.85
    auto_summary_threshold: 0.90
    # summary_model: "gpt-4o-mini"  # Optional: use cheaper model for summaries

repository:
  clone_depth: 1
  ignore_patterns:
    - node_modules
    - target
    - .git

documentation:
  output_dir: docs
  categories:
    - tutorials
    - how_to
    - explanation
    - reference
```

---

## Environment variables

A set of environment variables is supported to override configuration values. Environment variables take precedence over values loaded from the file (but are overridden by CLI-level overrides where implemented).

Common environment variables:

- `XZATOMA_PROVIDER`
  Example: `export XZATOMA_PROVIDER=copilot`

- `XZATOMA_COPILOT_MODEL`
  Example: `export XZATOMA_COPILOT_MODEL=gpt-5-mini`

- `XZATOMA_OLLAMA_HOST`
  Example: `export XZATOMA_OLLAMA_HOST=localhost:11434`

- `XZATOMA_OLLAMA_MODEL`
  Example: `export XZATOMA_OLLAMA_MODEL=qwen3`

- `XZATOMA_MAX_TURNS`
  Example: `export XZATOMA_MAX_TURNS=100`

- `XZATOMA_TIMEOUT_SECONDS`
  Example: `export XZATOMA_TIMEOUT_SECONDS=900`

- `XZATOMA_EXECUTION_MODE`
  Example: `export XZATOMA_EXECUTION_MODE=restricted_autonomous`
  Supported values (case-insensitive):
- `interactive`
- `restricted_autonomous`
- `full_autonomous`

Context window management environment variables:

- `XZATOMA_CONTEXT_MAX_TOKENS`
  Example: `export XZATOMA_CONTEXT_MAX_TOKENS=150000`
  Override maximum tokens allowed in conversation context

- `XZATOMA_CONTEXT_WARNING_THRESHOLD`
  Example: `export XZATOMA_CONTEXT_WARNING_THRESHOLD=0.85`
  Token usage percentage to trigger warnings (0.0-1.0)

- `XZATOMA_CONTEXT_AUTO_SUMMARY_THRESHOLD`
  Example: `export XZATOMA_CONTEXT_AUTO_SUMMARY_THRESHOLD=0.90`
  Token usage percentage to trigger automatic summarization (0.0-1.0)

- `XZATOMA_CONTEXT_SUMMARY_MODEL`
  Example: `export XZATOMA_CONTEXT_SUMMARY_MODEL=gpt-4o-mini`
  Model to use for generating conversation summaries

Usage example (one-off):

```bash
XZATOMA_PROVIDER=ollama XZATOMA_OLLAMA_HOST=localhost:11434 xzatoma run --plan plans/generate_docs.yaml
```

---

## Validation rules & common errors

`Config::load` applies validation to ensure configuration is sane. Typical checks include:

- `provider_type` must be a known provider (e.g., `copilot`, `ollama`). An unknown provider produces a validation error.
- Numeric limits (e.g., `max_turns`, `timeout_seconds`) must be positive where applicable.
- Plan/agent runtime settings that fall outside acceptable ranges will be rejected by `Config::validate`.

Common issues and corrective actions:

- "Config file not found" — the CLI will warn and use defaults. Pass `--config` with the correct path to use a custom config file.
- "Invalid provider" — fix `provider.provider_type` or set `XZATOMA_PROVIDER`.
- "Numeric validation errors" — verify integer fields are positive and within expected ranges.

When you receive validation errors, fix the config file (or use environment variables for temporary overrides) and re-run the command.

---

## Security & secrets handling

- Avoid embedding secrets (API keys or tokens) directly in repo files. Prefer provider authentication flows (for example, `xzatoma auth --provider copilot` uses a device-code flow and caches tokens in the system keyring).
- If you must store secrets locally, place them in a secure location (outside version control) and ensure they are not committed.

---

## Programmatic usage (Rust)

Loading and merging configuration programmatically:

```rust
use xzatoma::cli::Cli;
use xzatoma::config::Config;

let cli = Cli::parse_args(); // parse CLI args
let cfg = Config::load("config/config.yaml", &cli)?;
cfg.validate()?;
```

This mirrors what the CLI does when resolving the effective configuration: read file → apply environment vars → apply CLI overrides.

---

## Troubleshooting tips

- Enable verbose/diagnostic logging for more insight:

```bash
RUST_LOG=debug xzatoma run --plan examples/quickstart_plan.yaml
```

- If provider features fail due to authentication, run:

```bash
xzatoma auth --provider copilot
```

- If a value set via environment variable appears not to take effect, verify spelling and that the variable is exported in the same shell session running `xzatoma`.

---

## See also

- How to configure providers: `../how-to/configure_providers.md`
- CLI reference (how to pass `--config` and other options): `../reference/cli.md`
- Workflow / Plan format (how `generate_documentation` and other plan-driven features may consume `context`): `../reference/workflow_format.md`

---

Last updated: 2026-01-24
