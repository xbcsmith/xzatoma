# Phase 5: Provider Config Headers Implementation

## Overview

This document explains the Phase 5 changes applied to the Ollama and Copilot
provider implementations. Two independent features were added:

1. Ollama provider applies `num_ctx` from config to every request body.
2. Copilot provider applies `editor_version` and `initiator` as HTTP headers on
   all outbound API requests.

---

## Ollama: `num_ctx` in Request Body

### Problem

The Ollama API accepts an `options` object in each chat completion request body.
One of the most useful options is `num_ctx`, which sets the context window size
in tokens. Previously, XZatoma never sent this field, so Ollama always used its
model-default context size regardless of what was configured.

### Solution

A new `OllamaOptions` struct was added to hold the serializable options:

```rust
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    num_ctx: Option<u32>,
}
```

A new `OllamaRequestFull` struct replaced the `OllamaRequest` type alias
(`ProviderRequest`) for constructing actual HTTP request bodies. It adds an
`options` field that is omitted from JSON serialization when `None`:

```rust
struct OllamaRequestFull {
    model: String,
    messages: Vec<OllamaMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ProviderTool>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}
```

Both the blocking `complete` and streaming `complete_streaming_with_callbacks`
paths now:

1. Extract `config.num_ctx` together with other config values in the single
   config read at the top of each method.
2. Construct `options` via
   `num_ctx.map(|n| OllamaOptions { num_ctx: Some(n) })`.
3. Build `OllamaRequestFull` instead of the old `OllamaRequest`.

When `num_ctx` is `None` (the default), the `options` key is absent from the
serialized JSON and Ollama uses its built-in default context window.

### Serialization contract

| `config.num_ctx` | Serialized JSON body                                                    |
| ---------------- | ----------------------------------------------------------------------- |
| `None`           | `{"model":...,"messages":...,"stream":...}` (no `options`)              |
| `Some(16384)`    | `{"model":...,"messages":...,"stream":...,"options":{"num_ctx":16384}}` |

---

## Copilot: `editor_version` and `X-Initiator` HTTP Headers

### Problem

All outbound Copilot API requests previously sent either `"vscode/1.85.0"` or
`"xzatoma/0.1.0"` as the `Editor-Version` header, hardcoded at call sites. The
`X-Initiator` header was never sent at all. Both values need to be configurable
so that operators can identify requests in Copilot backend telemetry and
routing.

### Solution

Two fields were added to `CopilotConfig` (already done in config changes):

- `editor_version: String` -- default `"vscode/1.95.0"`
- `initiator: String` -- default `"agent"`

A private helper method was added to `impl CopilotProvider`:

```rust
fn editor_headers(&self) -> Result<(String, String)> {
    let config = read_config_lock(&self.config)?;
    Ok((config.editor_version.clone(), config.initiator.clone()))
}
```

Every method that builds an outbound HTTP request was updated to:

1. Call `let (editor_version, initiator) = self.editor_headers()?;` before the
   request builder chain.
2. Replace the hardcoded `"Editor-Version"` string literal with
   `&editor_version`.
3. Add `.header("X-Initiator", &initiator)` immediately after.

### Methods updated

| Method                          | Requests updated              |
| ------------------------------- | ----------------------------- |
| `fetch_copilot_models`          | Initial request and 401 retry |
| `fetch_copilot_models_raw`      | Initial request               |
| `stream_response`               | Streaming request             |
| `stream_completion`             | Streaming request             |
| `complete_responses_blocking`   | Non-streaming request         |
| `complete_completions_blocking` | Initial request and 401 retry |

---

## Testing

### Ollama serialization tests

Two unit tests verify the serialization contract:

- `test_ollama_request_body_omits_options_when_num_ctx_is_none`: asserts that
  the `options` key is absent from the JSON body when `num_ctx` is `None`.
- `test_ollama_request_body_includes_num_ctx_when_set`: asserts that the body
  contains `"options": {"num_ctx": 16384}` when `num_ctx` is `Some(16384)`.

### Copilot header tests

Three unit tests verify the config/provider integration:

- `test_copilot_config_editor_version_default_is_vscode`: default is
  `"vscode/1.95.0"`.
- `test_copilot_config_initiator_default_is_agent`: default is `"agent"`.
- `test_copilot_provider_editor_headers_returns_config_values`: end-to-end check
  that custom values set in `CopilotConfig` are returned by `editor_headers()`.

---

## Files changed

| File                                    | Change                                                                                                                                |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `src/providers/ollama.rs`               | Added `OllamaOptions`, `OllamaRequestFull`; updated both request paths; fixed all test struct literals; added two serialization tests |
| `src/providers/copilot.rs`              | Added `editor_headers` method; replaced hardcoded header strings at 8 call sites; added three config/header tests                     |
| `src/commands/mod.rs`                   | Added `num_ctx: None` to `OllamaConfig` struct literal                                                                                |
| `src/providers/factory.rs`              | Added `num_ctx: None` to `OllamaConfig` struct literal in test helper                                                                 |
| `tests/integration_provider_factory.rs` | Added missing fields to `CopilotConfig` and `OllamaConfig` struct literals                                                            |
