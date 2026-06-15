# Phase 2: System Prompt Resolver Implementation

## Overview

Phase 2 introduces the resolution logic that determines the active system prompt
for every XZatoma agent session. The new `src/agent/system_prompt.rs` module
provides a `resolve` function that applies a fixed precedence order across the
four configuration channels.

## Resolution Precedence

The resolver applies sources in the following order, highest priority first:

1. Plan file `system_prompt` field
2. `--system-prompt` CLI flag
3. `agent.system_prompt` configuration field (also set by
   `XZATOMA_SYSTEM_PROMPT`)
4. No override — callers fall back to the mode-specific default prompt

The `Default` variant of `SystemPromptSource` is reserved for callers that
construct a fallback from the mode-specific base prompt and is never returned by
`resolve` itself.

## New Types

### `SystemPromptSource`

An enum that identifies which configuration channel supplied the active prompt.
Used in trace-level log output so operators can determine the effective source
at runtime.

### `ResolvedSystemPrompt`

A struct that pairs the resolved prompt text with its source. Returned by
`resolve` when at least one non-blank input is present.

### `resolve`

```rust
pub fn resolve(
    plan_prompt: Option<&str>,
    cli_flag: Option<&str>,
    config_prompt: Option<&str>,
) -> Option<ResolvedSystemPrompt>
```

Blank (whitespace-only) strings are treated as absent at every precedence level,
consistent with the validation rules introduced in Phase 1.

## Trace Logging Stubs

Phase 2 adds `tracing::trace!` calls at the startup of each LLM-facing mode
(`chat`, `run`, `agent`). These stubs log the effective system prompt value
after the CLI override has been merged into `config.agent.system_prompt`.

Phase 3 replaces these stubs with full resolver-based logging that also
incorporates the plan file prompt and reports `SystemPromptSource`.

## Module Location

`src/agent/system_prompt.rs` is a public submodule of `src/agent/`. Key types
are re-exported from `src/agent/mod.rs`:

```rust
pub use system_prompt::{resolve, ResolvedSystemPrompt, SystemPromptSource};
```

They are accessible from the crate root as:

```rust
use xzatoma::agent::system_prompt::{resolve, ResolvedSystemPrompt, SystemPromptSource};
```

## Test Coverage

Fourteen unit tests cover:

- Precedence: plan wins over CLI and config; CLI wins over config; config used
  alone.
- Blank-string handling: each position treated as absent when whitespace-only.
- All-absent input returns `None`.
- Struct field access, `Clone`, and `Debug` formatting.
- `SystemPromptSource` equality and inequality.
