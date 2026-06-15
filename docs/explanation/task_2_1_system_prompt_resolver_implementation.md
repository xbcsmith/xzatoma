# Task 2.1: System Prompt Resolver Implementation

## Overview

Task 2.1 creates `src/agent/system_prompt.rs` and wires it into `src/agent/mod.rs`.
The module encapsulates all precedence logic that determines the active system
prompt for an XZatoma agent session.

## Files Changed

| File | Change |
|------|--------|
| `src/agent/system_prompt.rs` | New file -- full implementation |
| `src/agent/mod.rs` | Added `pub mod system_prompt` and re-exports |

## New Public API

### `SystemPromptSource` (enum)

Identifies which configuration channel supplied the active prompt. Used in
trace-level logging so operators can determine the effective source at runtime.

Variants:

- `Plan` -- prompt came from a plan file's `system_prompt` field
- `CliFlag` -- prompt came from the `--system-prompt` CLI flag
- `Config` -- prompt came from `config.agent.system_prompt` or the
  `XZATOMA_SYSTEM_PROMPT` environment variable (written by `apply_env_vars`)
- `Default` -- reserved for callers that construct a mode-specific fallback;
  never returned by `resolve`

### `ResolvedSystemPrompt` (struct)

Pairs the resolved prompt text (`text: String`) with its origin
(`source: SystemPromptSource`). Derives `Debug`, `Clone`, `PartialEq`, `Eq`.

### `resolve` (function)

```rust
pub fn resolve(
    plan_prompt: Option<&str>,
    cli_flag: Option<&str>,
    config_prompt: Option<&str>,
) -> Option<ResolvedSystemPrompt>
```

Applies the following precedence order (highest to lowest):

1. `plan_prompt`
2. `cli_flag`
3. `config_prompt`

Any input that is `None` or contains only whitespace is treated as absent and
skipped. Returns `None` when all three inputs are absent.

## Re-exports

Key types are re-exported from `src/agent/mod.rs` and are accessible as:

```rust
use xzatoma::agent::system_prompt::{resolve, ResolvedSystemPrompt, SystemPromptSource};
```

## Implementation Notes

The blank-string guard uses `.filter(|v| !v.trim().is_empty())` applied
directly to each `Option<&str>` argument. This avoids a closure with an
explicit lifetime annotation while keeping the intent clear.

## Test Coverage

Fourteen unit tests in the `tests` submodule cover:

- Precedence: plan wins over CLI and config; CLI wins over config alone.
- Blank-string handling: each position treated as absent when whitespace-only.
- All-absent and all-blank inputs return `None`.
- `ResolvedSystemPrompt` field access, `Clone`, and equality.
- `SystemPromptSource` `Debug` output and equality across all four variants.

## Quality Gates

All four gates passed at the time of implementation:

```text
cargo fmt --all                              -- ok
cargo check --all-targets --all-features    -- ok
cargo clippy --all-targets --all-features   -- ok (0 warnings)
cargo test --all-features --lib             -- 2310 passed, 0 failed
```
