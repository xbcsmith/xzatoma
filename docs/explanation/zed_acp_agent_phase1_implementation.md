# Zed ACP Agent Phase 1 Implementation

## Overview

This document summarizes Phase 1 of the Zed ACP agent command implementation for
XZatoma. Phase 1 establishes the dependency, CLI, command dispatch, tracing
safety, and module boundaries required for a future Zed-compatible ACP stdio
transport.

The new user-facing entry point is `xzatoma agent`. This command is intended to
be launched by Zed or another ACP-compatible client as a subprocess. Future
phases will use stdin/stdout for newline-delimited JSON-RPC messages through the
`agent-client-protocol` SDK.

Phase 1 intentionally does not implement the ACP handshake, sessions, prompt
execution, queueing, cancellation, or vision handling. It provides the safe
scaffold those phases will build on.

## Changes Implemented

### Dependency foundation

Phase 1 adds the ACP stdio SDK dependency:

- `agent-client-protocol = "0.11.1"`

The existing `tokio-util` dependency is updated to include the `compat` feature
alongside `codec`. This prepares the project to wrap Tokio stdin/stdout in the
futures-compatible IO types expected by the ACP SDK `ByteStreams` transport.

### CLI command

A new top-level `Commands::Agent` variant is added to `src/cli.rs`.

The command is invoked as:

- `xzatoma agent`

It accepts these flags:

- `--provider <provider>`: override the configured provider for this subprocess.
- `--model <model>`: override the selected provider model.
- `--allow-dangerous`: enable full autonomous terminal execution for this
  subprocess.
- `--working-dir <path>`: provide a fallback workspace root when the ACP client
  does not provide one.

The command documentation states that this is the ACP stdio subprocess entry
point for Zed or another ACP-compatible client, distinct from the existing HTTP
ACP server command.

### Command handler

A new `src/commands/agent.rs` module provides the CLI-facing handler for
`xzatoma agent`.

The handler is intentionally thin. It converts CLI flags into
`AcpStdioAgentOptions` and delegates to the ACP stdio module. It must not write
human-readable output to stdout because stdout is reserved for the future ACP
JSON-RPC protocol stream.

### ACP stdio module boundary

A new `src/acp/stdio.rs` module defines the Phase 1 transport boundary.

It provides:

- `AcpStdioAgentOptions`
- `run_stdio_agent`
- `apply_stdio_agent_options`

The module applies provider, model, and safety overrides to an in-memory
configuration clone and validates the effective configuration. It logs startup
state through tracing and returns successfully without writing to stdout.

This module is exposed from `src/acp/mod.rs`.

### Command registration and dispatch

The new command module is registered in `src/commands/mod.rs`.

`src/main.rs` dispatches `Commands::Agent` to the new handler, passing provider,
model, dangerous-mode, working-directory, and loaded configuration values.

### Stdio safety

`src/main.rs` now configures tracing to write to stderr explicitly.

This is required because future ACP stdio traffic will use stdout for protocol
frames. Any tracing output, banners, or status messages on stdout would corrupt
the JSON-RPC stream.

### Tests

Phase 1 adds CLI parser coverage for:

- `xzatoma agent`
- `xzatoma agent --provider ollama`
- `xzatoma agent --provider copilot --model gpt-4o`
- `xzatoma agent --provider openai --model gpt-4o --allow-dangerous`
- `xzatoma agent --working-dir /tmp/xzatoma-zed-workspace`

It also adds command-level startup tests verifying that `xzatoma agent` does not
write to stdout for valid startup scenarios and that invalid provider errors are
reported on stderr only.

The new ACP stdio scaffold includes unit tests for option construction,
provider/model override application, dangerous-mode override behavior, and
configuration validation.

## Design Decisions

### Keep `xzatoma agent` separate from `xzatoma acp serve`

XZatoma already has an HTTP ACP surface through `xzatoma acp serve`. Zed uses a
subprocess protocol over stdin/stdout, so the new command is intentionally a
separate entry point.

This avoids mixing HTTP server concerns with stdio JSON-RPC framing.

### Keep command handling thin

The `src/commands/agent.rs` module is only responsible for CLI-level concerns.
Protocol behavior belongs under `src/acp/stdio.rs`.

This keeps the module boundary consistent with the rest of XZatoma: command
modules dispatch behavior, while ACP modules own ACP-specific transport and
protocol logic.

### Preserve stdout for protocol frames

All human-readable output must avoid stdout in the `xzatoma agent` path. Phase 1
enforces this by configuring tracing to stderr and by making the scaffold return
without printing.

Future phases should continue this rule: only ACP JSON-RPC frames may be written
to stdout.

### Apply overrides in memory only

Provider, model, and dangerous-mode options are applied to the loaded
configuration for the current subprocess only. They do not mutate configuration
files.

This matches existing CLI behavior and keeps Zed launch settings isolated from
persistent configuration.

## Current Limitations

Phase 1 is scaffolding only. The following are not yet implemented:

- ACP stdio transport over stdin/stdout
- `InitializeRequest`
- `NewSessionRequest`
- session registry
- prompt queues
- `PromptRequest`
- `CancelNotification`
- text prompt execution
- vision prompt conversion
- session persistence and resume
- model advertisement to Zed

These items are planned for later phases in the Zed ACP agent command plan.

## Files Changed

### New files

- `src/commands/agent.rs`
- `src/acp/stdio.rs`
- `tests/agent_cli.rs`
- `docs/explanation/zed_acp_agent_phase1_implementation.md`

### Modified files

- `Cargo.toml`
- `Cargo.lock`
- `src/cli.rs`
- `src/main.rs`
- `src/commands/mod.rs`
- `src/acp/mod.rs`

## Validation Notes

The Phase 1 implementation should be validated with the required project quality
gates:

1. `cargo fmt --all`
2. `cargo check --all-targets --all-features`
3. `cargo clippy --all-targets --all-features -- -D warnings`
4. `cargo test --all-features`

This Markdown document should also be formatted and linted with the project
Markdown tooling.
