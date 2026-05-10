# Phase 3: `set_thinking_effort` on the Provider Trait

## Overview

This document describes the implementation of Phase 3 of the thinking mode
feature for XZatoma. Phase 3 adds a `set_thinking_effort` method to the
`Provider` trait so that callers can control the reasoning intensity of models
that support configurable chain-of-thought output at runtime.

## Deliverables

| Deliverable                                                | File                         | Status   |
| ---------------------------------------------------------- | ---------------------------- | -------- |
| `set_thinking_effort` default no-op on `Provider` trait    | `src/providers/trait_mod.rs` | Complete |
| `CopilotProvider` override sets `config.reasoning_effort`  | `src/providers/copilot.rs`   | Complete |
| `OpenAIProvider` override sets `config.reasoning_effort`   | `src/providers/openai.rs`    | Complete |
| `reasoning_effort: Option<String>` added to `OpenAIConfig` | `src/config.rs`              | Complete |
| `OllamaProvider` inherits default no-op (no change needed) | `src/providers/ollama.rs`    | Complete |
| All 6 required tests passing                               | Multiple files               | Complete |

## Design Decisions

### Default No-Op Signature

The method signature is:

```rust
fn set_thinking_effort(&self, _effort: Option<&str>) -> crate::error::Result<()> {
    Ok(())
}
```

`&self` rather than `&mut self` is used because both `CopilotProvider` and
`OpenAIProvider` hold their configuration behind `Arc<RwLock<Config>>`, allowing
interior mutability without a mutable receiver. This means the method can be
called through `&dyn Provider` trait objects, which is necessary for the ACP
session observer wiring in Phase 5.

### Effort String Convention

The method accepts `Option<&str>` where:

- `Some("low" | "medium" | "high")` activates a reasoning level for Copilot.
- `Some("low" | "medium" | "high")` maps to the OpenAI `reasoning_effort`
  request field for o-series models.
- `None` clears any previously set effort, reverting to the provider default.
- The special value `"none"` is handled by callers (Phase 5 Task 5.5) before
  reaching these methods; they convert `"none"` to `None`.

### OpenAIConfig.reasoning_effort Added as Phase 3 Prerequisite

Phase 3 Task 3.3 depends on `OpenAIConfig` having a `reasoning_effort` field.
Rather than block on Phase 4, `reasoning_effort: Option<String>` was added to
`OpenAIConfig` in this phase:

- Annotated `#[serde(default)]` so existing YAML configs without the field
  deserialise correctly with `None`.
- Added to the `Default` impl with value `None`.
- The environment variable binding (`XZATOMA_OPENAI_REASONING_EFFORT`) and
  request-body threading are left to Phase 4.

### Lock Error Handling

Both overrides acquire the config write lock with `.write().map_err(...)`,
converting a poisoned-lock error into an `XzatomaError::Provider` string rather
than using `unwrap()` or `expect()`.

## Modified Files

### src/providers/trait_mod.rs

Added `set_thinking_effort` between `get_provider_capabilities` and
`list_models_summary`. The method has a full doc comment with a runnable example
that creates a minimal `NoOpProvider` implementor to verify the default
behaviour.

### src/providers/copilot.rs

Added override inside `impl Provider for CopilotProvider` after
`get_provider_capabilities`:

```rust
fn set_thinking_effort(&self, effort: Option<&str>) -> crate::error::Result<()> {
    let mut config = self.config.write().map_err(|_| {
        crate::error::XzatomaError::Provider(
            "Failed to acquire write lock on CopilotConfig".to_string(),
        )
    })?;
    config.reasoning_effort = effort.map(str::to_string);
    tracing::debug!("Copilot thinking effort set to: {:?}", config.reasoning_effort);
    Ok(())
}
```

### src/providers/openai.rs

Added override inside `impl Provider for OpenAIProvider` after
`get_provider_capabilities`, mirroring the Copilot implementation but targeting
`OpenAIConfig`.

All 12 inline `OpenAIConfig { ... }` struct literals in the test module were
updated to include `reasoning_effort: None`. The `make_config` helper and both
doc-comment examples were updated as well.

### src/config.rs

Added `reasoning_effort: Option<String>` to `OpenAIConfig`:

```rust
/// Reasoning effort level for OpenAI o-series reasoning models.
///
/// Accepted values: `"low"`, `"medium"`, `"high"`. Set to `None` to use
/// the model default. Has no effect on non-reasoning models.
///
/// Set at runtime via `Provider::set_thinking_effort` or the
/// `XZATOMA_OPENAI_REASONING_EFFORT` environment variable (Phase 4).
#[serde(default)]
pub reasoning_effort: Option<String>,
```

## Test Coverage

### src/providers/trait_mod.rs

| Test name                                          | Behaviour verified                                        |
| -------------------------------------------------- | --------------------------------------------------------- |
| `test_set_thinking_effort_default_impl_returns_ok` | Default no-op returns `Ok(())` for both `Some` and `None` |

### src/providers/copilot.rs

| Test name                                               | Behaviour verified                                    |
| ------------------------------------------------------- | ----------------------------------------------------- |
| `test_set_thinking_effort_stores_effort_in_config`      | Effort string is written to `config.reasoning_effort` |
| `test_set_thinking_effort_none_clears_reasoning_effort` | `None` clears a previously set effort                 |
| `test_set_thinking_effort_returns_ok_for_valid_effort`  | Returns `Ok(())` for all three valid effort strings   |

### src/providers/openai.rs

| Test name                                                      | Behaviour verified                                      |
| -------------------------------------------------------------- | ------------------------------------------------------- |
| `test_set_thinking_effort_stores_effort_in_openai_config`      | Effort string stored in `OpenAIConfig.reasoning_effort` |
| `test_set_thinking_effort_none_clears_openai_reasoning_effort` | `None` clears a previously set effort                   |

## Quality Gate Results

All four mandatory quality gates pass:

```text
cargo fmt --all                                        OK
cargo check --all-targets --all-features               OK
cargo clippy --all-targets --all-features -D warnings  OK
cargo test --all-features --lib                        2058 passed, 0 failed
```

Provider-scoped test run:

```text
cargo test --all-features --lib -- providers           292 passed, 0 failed
```

Doc tests (includes the new trait_mod example):

```text
cargo test --all-features --doc -- providers            81 passed, 0 failed
```
