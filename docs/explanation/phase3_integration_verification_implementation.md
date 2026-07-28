# Phase 3: Integration Verification and Documentation

## Overview

Phase 3 completes the ACP Features Implementation Plan by verifying that the
changes from Phase 1 (Core Mode Selector Fix) and Phase 2 (Context Window
Display) are correctly integrated, and by updating the documentation so
operators and contributors can understand and use the new features.

## What Phase 3 Covers

Phase 3 has no Rust code changes. It is entirely documentation and verification:

- End-to-end Zed verification checklist for the mode selector and context window
  bar.
- Documentation updates across three files.
- Full quality gate run confirming all code and documentation standards are met.

## Documentation Changes

### `docs/how-to/zed_acp_agent_setup.md`

Two new sections were added between the existing "Step 4: Enable or disable
vision support" section and the "Troubleshooting" section.

#### Session Mode Selector

Documents the four XZatoma session modes that Zed renders in its agent panel
mode selector dropdown:

| Mode ID           | Terminal access | Confirmations |
| ----------------- | --------------- | ------------- |
| `planning`        | None            | Always        |
| `write`           | Safe only       | Always        |
| `safe`            | Safe only       | Always (Zed)  |
| `full_autonomous` | Unrestricted    | Never         |

The section explains how to switch modes via the Zed UI and how to confirm mode
changes are applied using `RUST_LOG=xzatoma::acp=debug`.

#### Context Window Bar

Documents the two mechanisms that keep the Zed context window bar updated:

1. A `UsageUpdate` session notification sent after every `EndTurn`.
2. `PromptResponse.usage` populated with token counts in the response payload.

Explains the two-tier token counting approach (provider-reported usage preferred
over heuristic fallback) and provides a bar-fill interpretation table so
operators know when to start a new session.

### `docs/reference/acp_configuration.md`

A new "Session config options" subsection was added to the Stdio ACP
configuration section. It documents `session_mode` as a runtime ACP protocol
option with its four accepted values and their effects.

A deprecation note was added: `terminal_execution` is no longer advertised as a
standalone config option. Terminal execution mode is now controlled exclusively
through `session_mode`.

A clarifying note was added after the Stdio configuration example block
confirming that session config options are runtime controls set through the ACP
protocol, not YAML configuration fields.

### `docs/reference/acp_api.md`

A new "Stdio Session Config Options" section was added before "Related
Documents". It summarises `session_mode` for readers of the HTTP API reference
and cross-references `acp_configuration.md` for full details.

## End-to-End Zed Verification Checklist

Manual verification steps for `xzatoma agent` in Zed:

### Session Mode Selector

1. Open a new session in Zed. The Mode Selector dropdown should show: Planning,
   Write, Safe, Full Autonomous.
2. Select Full Autonomous. Terminal commands should run without confirmation
   prompts.
3. Switch back to Planning. Terminal commands should be blocked.

To confirm mode notifications are sent, run with:

```json
{
  "env": {
    "RUST_LOG": "xzatoma::acp=debug"
  }
}
```

Look for `ConfigOptionUpdate` and `CurrentModeUpdate` in the log output.

### Context Window Bar

1. Send a prompt. The context window bar should update after the response
   completes, showing `used / max` tokens.
2. Run with `RUST_LOG=xzatoma::acp=debug` and look for:
   - `"ACP stdio: sending initial context window usage update"` at session
     creation.
   - `"post-turn usage update"` after each completed prompt.

## Quality Gates

All four quality gate commands passed with zero errors and zero warnings:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features --lib -- acp::   # 339 passed
```

All modified Markdown files passed `markdownlint` and `prettier` checks.

## Files Changed

- `docs/how-to/zed_acp_agent_setup.md` - Session Mode Selector and Context
  Window Bar sections added.
- `docs/reference/acp_configuration.md` - `session_mode` config option
  documented, `terminal_execution` removal noted, runtime-vs-YAML distinction
  clarified.
- `docs/reference/acp_api.md` - Stdio Session Config Options section added.
- `docs/explanation/acp_features_implementation.md` - Passed linting and
  formatting checks (`markdownlint` and `prettier`).
