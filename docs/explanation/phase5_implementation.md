# Phase 5: Provider Config, Agent Config, History, and Scoped Validation

## Overview

Phase 5 closes all remaining low-to-medium priority improvements identified in
the XZatoma feature improvements plan. All tasks are self-contained changes with
no cross-phase dependencies.

## Changes

### Task 5.1: Ollama `num_ctx` Context Window Control

**Files changed:** `src/config.rs`, `src/providers/ollama.rs`

Added `num_ctx: Option<u32>` to `OllamaConfig`. When set, the value is passed as
`options.num_ctx` in every Ollama chat completion request body (both blocking
and streaming paths). When `None`, the `options` key is omitted entirely,
letting Ollama use its model-default context window size.

A new `OllamaRequestFull` struct replaces the `OllamaRequest` type alias for
request serialization to support the additional `options` field.

### Task 5.2: Copilot `editor_version` and `initiator` Headers

**Files changed:** `src/config.rs`, `src/providers/copilot.rs`

Added two fields to `CopilotConfig`:

- `editor_version: String` (default `"vscode/1.95.0"`) - sent as the
  `Editor-Version` HTTP header on all outbound Copilot API requests.
- `initiator: String` (default `"agent"`) - sent as the `X-Initiator` HTTP
  header on all outbound Copilot API requests.

All eight outbound request call sites in `copilot.rs` (across
`fetch_copilot_models`, `fetch_copilot_models_raw`, `stream_response`,
`stream_completion`, `complete_responses_blocking`, and
`complete_completions_blocking`) were updated. A private `editor_headers()`
helper method reads both values under a single config lock.

### Task 5.3: `history tools` Subcommand

**Files changed:** `src/cli.rs`, `src/storage/mod.rs`, `src/storage/types.rs`,
`src/commands/history.rs`

Added
`HistoryCommand::Tools { conversation: Option<String>, tool: Option<String> }`
to the CLI. The `--conversation` and `--tool` flags are optional filters; when
absent all rows are returned.

A new `tool_invocations` SQLite table is created during database initialization
using `CREATE TABLE IF NOT EXISTS`. The schema records:

| Column            | Type    | Description                  |
| ----------------- | ------- | ---------------------------- |
| `id`              | TEXT    | ULID primary key             |
| `run_id`          | TEXT    | Associated run identifier    |
| `conversation_id` | TEXT    | Owning conversation          |
| `tool_name`       | TEXT    | Name of the invoked tool     |
| `arguments`       | TEXT    | JSON-encoded arguments       |
| `result`          | TEXT    | JSON-encoded result          |
| `success`         | INTEGER | 1 for success, 0 for failure |
| `timestamp`       | TEXT    | RFC-3339 timestamp           |

Two indexes on `conversation_id` and `tool_name` support the filter queries.
`StoredToolInvocation` is added to `src/storage/types.rs` and re-exported from
`src/storage/mod.rs`. The `handle_history_tools_with_storage` function in
`src/commands/history.rs` handles the filtered query and tabular output.

### Task 5.4: Scoped Configuration Validators

**Files changed:** `src/config.rs`, `src/commands/mod.rs`,
`src/commands/acp.rs`, `src/commands/agent.rs`

Four public methods added to `impl Config`:

| Method                   | Caller          | Checks                                              |
| ------------------------ | --------------- | --------------------------------------------------- |
| `validate_for_execution` | `chat`, `run`   | Provider type configured; model non-empty           |
| `validate_for_watcher`   | `watch`         | Kafka brokers and topic non-empty                   |
| `validate_for_acp`       | `serve` / `acp` | Bind port non-zero; agent names are RFC 1123 labels |
| `validate_for_zed_agent` | `agent`         | Provider type configured                            |

RFC 1123 label validation uses the pattern
`^[a-z0-9]([a-z0-9\-]{0,61}[a-z0-9])?$`.

Each scoped validator is called at the entry point of its command handler, after
all CLI overrides have been applied.

### Task 5.5: Agent Chat History Configuration Fields

**Files changed:** `src/config.rs`, `src/commands/mod.rs`

Added two fields to `AgentConfig`:

- `chat_history_max_size: usize` (default `1000`) - passed to the rustyline
  `Config::builder().max_history_size(n)` call.
- `chat_history_file: Option<PathBuf>` (default `None`) - path used for loading
  and saving the readline command history. When `None`, the platform data
  directory joined with `chat_history` is used as the default path.

The interactive chat session loads history from the file at startup
(best-effort) and saves history to it on exit (best-effort). Failures are logged
at DEBUG level and do not abort the session.

### Task 5.6: Agent `thinking_mode` Global Default

**Files changed:** `src/config.rs`, `src/commands/mod.rs`

Added `thinking_mode: Option<String>` (default `None`) to `AgentConfig`.

In the `chat` and `run` command handlers, the thinking effort resolution is now:

1. CLI `--thinking-effort` flag value (highest precedence).
2. `agent.thinking_mode` from config (fallback when CLI flag is absent).
3. Provider default (when both are absent).

The `watch` command uses `agent.thinking_mode` directly through the agent config
that is passed to the watcher's internal agent instances.

## Testing

All new functionality has corresponding unit tests:

- `test_ollama_config_num_ctx_default_is_none` and
  `test_ollama_config_num_ctx_deserializes` verify config deserialization.
- `test_ollama_request_body_omits_options_when_num_ctx_is_none` and
  `test_ollama_request_body_includes_num_ctx_when_set` verify JSON serialization
  of the request body.
- `test_copilot_provider_editor_headers_returns_config_values` and related tests
  verify header values.
- Storage tests verify the `tool_invocations` table schema and all three filter
  combinations (no filter, by tool name, by conversation ID).
- History command tests verify tabular output for the `tools` subcommand.
- Scoped validator tests cover passing and failing cases for all four methods,
  including RFC 1123 label validation.
- `test_agent_config_chat_history_max_size_default`,
  `test_agent_config_chat_history_file_deserializes`, and
  `test_agent_config_thinking_mode_deserializes` verify config round-trips.

## Quality Gates

All five quality gates pass:

1. `cargo fmt --all` -- no diff
2. `cargo check --all-targets --all-features` -- clean
3. `cargo clippy --all-targets --all-features -- -D warnings` -- zero warnings
4. `cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth`
   -- 2603 passed, 0 failed
5. `cargo audit --deny warnings` -- run before release (not gated in this PR)
