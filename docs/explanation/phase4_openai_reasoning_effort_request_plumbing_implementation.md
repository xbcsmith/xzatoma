# Phase 4: OpenAIConfig.reasoning_effort and Request Plumbing Implementation

## Overview

Phase 4 threads the `reasoning_effort` configuration value that was stored in
`OpenAIConfig` (added during Phase 3) all the way through to outgoing HTTP
request bodies. It also wires the field to its canonical environment variable so
operators can control chain-of-thought depth without editing a YAML file.

After Phase 4:

- `XZATOMA_OPENAI_REASONING_EFFORT=high cargo run -- chat` sends
  `"reasoning_effort":"high"` to o-series models.
- Non-reasoning models receive no extra field (the value is omitted when
  `None`).
- The `--thinking-effort` CLI flags introduced in Phase 6 will flow through
  `set_thinking_effort` into the same config field and therefore also reach the
  wire automatically.

## Changes

### Task 4.1: `OpenAIConfig.reasoning_effort` (already present)

The field was added to `src/config.rs` during Phase 3 with the correct serde
attribute and `Default` initializer. No further change was required.

### Task 4.2: `XZATOMA_OPENAI_REASONING_EFFORT` Environment Variable

Added to the OpenAI section of `Config::apply_env_vars` in `src/config.rs`,
placed after the existing `XZATOMA_OPENAI_REQUEST_TIMEOUT` handler so the block
remains in alphabetical order:

```src/config.rs#L1441-1449
if let Ok(val) = std::env::var("XZATOMA_OPENAI_REASONING_EFFORT") {
    if val == "none" {
        self.provider.openai.reasoning_effort = None;
    } else {
        self.provider.openai.reasoning_effort = Some(val);
    }
}
```

Setting the variable to the literal string `"none"` explicitly clears the field
so operators can override a value that was baked into a config file.

### Task 4.3: Thread `reasoning_effort` Through `OpenAIRequest`

Two locations in `src/providers/openai.rs` were updated.

**`OpenAIRequest` struct** - a new optional field was added with
`#[serde(skip_serializing_if = "Option::is_none")]` so it is absent from the
JSON body when `None`. This keeps non-reasoning model requests clean:

```src/providers/openai.rs#L61-70
/// Reasoning effort for o-series reasoning models.
///
/// Accepted values: `"low"`, `"medium"`, `"high"`. Omitted from the
/// request body when `None` so non-reasoning models are unaffected.
#[serde(skip_serializing_if = "Option::is_none")]
reasoning_effort: Option<String>,
```

**`Provider::complete` implementation** - the config read guard was extended to
also capture `reasoning_effort`, then the value is forwarded to the request
struct:

```src/providers/openai.rs#L1154-1168
let (model, enable_streaming, reasoning_effort) = {
    let config = self.config.read().map_err(|_| {
        XzatomaError::Provider("Failed to acquire read lock on config".to_string())
    })?;
    (
        config.model.clone(),
        config.enable_streaming,
        config.reasoning_effort.clone(),
    )
};
```

```src/providers/openai.rs#L1178-1185
let request = OpenAIRequest {
    model,
    messages: self.convert_messages(messages)?,
    tools: openai_tools,
    stream: use_streaming,
    reasoning_effort,
};
```

Both the streaming and non-streaming paths share the same `OpenAIRequest`
struct, so both automatically carry the field.

## Tests Added

### `src/config.rs`

| Test function                                                   | Verifies                                                        |
| --------------------------------------------------------------- | --------------------------------------------------------------- |
| `test_openai_config_reasoning_effort_defaults_none`             | `OpenAIConfig::default()` has `reasoning_effort: None`          |
| `test_openai_config_deserialize_reasoning_effort`               | YAML key `reasoning_effort:` deserializes correctly             |
| `test_apply_env_vars_overrides_openai_reasoning_effort`         | Env var `XZATOMA_OPENAI_REASONING_EFFORT=medium` sets the field |
| `test_apply_env_vars_openai_reasoning_effort_none_clears_field` | Env var value `"none"` clears a previously set value            |

### `src/providers/openai.rs`

| Test function                                            | Verifies                                                 |
| -------------------------------------------------------- | -------------------------------------------------------- |
| `test_openai_request_reasoning_effort_omitted_when_none` | Field is absent from serialized JSON when `None`         |
| `test_openai_request_reasoning_effort_included_when_set` | Field is present with the correct value when `Some(...)` |

## Design Decisions

**Skip serializing when `None`**: The OpenAI API does not error on unknown
fields, but sending `"reasoning_effort": null` could trigger unexpected model
behavior on providers that forward the field verbatim. Omitting the field
entirely is the safer default.

**Env var `"none"` clears the field**: A YAML config may contain
`reasoning_effort: high`. Without the `"none"` escape hatch, an operator using
environment variables would have no way to suppress that value without editing
the file.

**Single read lock, three values**: All three config values (`model`,
`enable_streaming`, `reasoning_effort`) are captured under a single read lock
acquisition to avoid TOCTOU races and minimize lock contention.

## Quality Gate Results

```text
cargo fmt --all              -- OK
cargo check --all-targets --all-features  -- OK
cargo clippy --all-targets --all-features -- -D warnings  -- OK (no warnings)
cargo test providers::openai::tests  -- 53 passed, 0 failed, 18 ignored
cargo test config::tests::test_openai_config_reasoning_effort  -- 1 passed
cargo test config::tests::test_apply_env_vars_overrides_openai_reasoning_effort  -- 1 passed
cargo test config::tests::test_apply_env_vars_openai_reasoning_effort_none_clears_field  -- 1 passed
cargo test providers::openai::tests::test_openai_request_reasoning_effort  -- 2 passed
```
