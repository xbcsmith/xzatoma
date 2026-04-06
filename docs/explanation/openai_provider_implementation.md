# OpenAI Provider Implementation

## Overview

The `OpenAIProvider` implements the `Provider` trait for the OpenAI chat
completions API and any server that exposes a compatible interface. This
document describes the design decisions made during implementation, operational
characteristics such as streaming and caching behavior, and the relationship to
the existing `CopilotProvider` and `OllamaProvider`.

Phases completed:

- Phase 1: `OpenAIConfig` added to `src/config.rs` with env var mappings and
  validation.
- Phase 2: `src/providers/openai.rs` implemented with full `Provider` trait
  coverage.
- Phase 3: Factory functions `create_provider` and
  `create_provider_with_override` updated in `src/providers/mod.rs`.

## Implementation Files

| File                        | Purpose                                    |
| --------------------------- | ------------------------------------------ |
| `src/providers/openai.rs`   | Provider implementation and wire types     |
| `src/providers/mod.rs`      | Factory registration and re-exports        |
| `src/config.rs`             | `OpenAIConfig` struct and env var mappings |
| `config/openai_config.yaml` | Standalone example configuration           |

## Design Decisions

### OpenAI-Compatible by Design

The provider targets two API endpoints:

- `POST {base_url}/chat/completions` for text and tool-call completions
- `GET {base_url}/models` for model listing

Any server that implements these endpoints with compatible request and response
shapes can be used by setting `base_url` to the server address. Servers known to
be compatible include llama.cpp, vLLM, and Mistral.rs. The `base_url` defaults
to `https://api.openai.com/v1` for the hosted OpenAI API.

### Wire Types Use `content: Option<String>`

The shared `ProviderMessage` type uses `content: String` because the internal
conversation representation always has a content string. The OpenAI wire format
requires `content` to be `null` (not an empty string) for assistant messages
that carry only tool calls. Sending `content: ""` causes OpenAI and compatible
servers to reject or misinterpret the request.

`OpenAIProvider` defines its own wire types (`OpenAIWireMessage` and
`OpenAIResponseMessage`) with `content: Option<String>`. When converting from
`ProviderMessage` to the wire format, an empty string on an assistant message
that carries tool calls is mapped to `None`, which serializes to `null` in the
JSON body.

This is the key structural difference between the OpenAI wire layer and the
internal provider message representation, and the primary reason the provider
cannot reuse the shared `ProviderMessage` type directly for HTTP serialization.

### Separate Implementation Rather Than Extending CopilotProvider

GitHub Copilot uses OAuth device-flow authentication with its own token cache,
token refresh on 401, and keyring storage. Sharing that implementation with
simple Bearer-token or unauthenticated local servers would introduce unnecessary
coupling and complexity.

`OpenAIProvider` is intentionally minimal:

- No OAuth lifecycle
- No keyring dependency
- Bearer token injected once at construction from `OpenAIConfig.api_key`
- No automatic token refresh

### Streaming and the Tool-Use Exception

When `enable_streaming` is `true` and the request contains no tools, the
provider uses server-sent events (SSE) for streaming responses. When tools are
present in the request, the provider uses the non-streaming path regardless of
the `enable_streaming` setting.

Tool-call arguments may arrive split across many SSE deltas. Accumulating and
reassembling partial tool-call deltas reliably adds fragility. The non-streaming
path returns a complete structured response for tool calls, which simplifies the
agent executor loop and eliminates an entire class of partial-delta reassembly
bugs.

### Model List Cache

`list_models` calls `GET {base_url}/models` and stores the result in an
`Arc<RwLock<Option<(Vec<ModelInfo>, Instant)>>>`. The cache TTL is 300 seconds
(5 minutes). Subsequent calls within the TTL window return the cached list
without issuing a network request.

The cache also affects `set_model`. When called, the method first calls
`list_models` (which returns from cache if the cache is warm) to validate that
the requested model exists in the server's model list. The internal model field
is updated only if the requested model is found.

## Authentication Model

The `Authorization: Bearer <api_key>` header is included in every request only
when `api_key` is non-empty. This allows the provider to connect to local
inference servers that do not require authentication by leaving `api_key` as an
empty string, which is the default value.

The optional `organization_id` field, when present and non-empty, is sent as the
`OpenAI-Organization` request header. This header is required for users
accessing the hosted OpenAI API through an organizational account.

## Targeting a Local Inference Server

Set `base_url` to the server address and leave `api_key` empty:

```yaml
provider:
  type: openai
  openai:
    api_key: ""
    base_url: "http://localhost:8080/v1"
    model: "local-model"
    enable_streaming: true
```

The model name must match whatever the local server reports or expects. For
llama.cpp the model name is typically `"local-model"` or whatever was passed to
the binary at startup. For vLLM, use the Hugging Face model ID passed to the
vLLM server process. See `config/openai_config.yaml` for complete commented
examples for llama.cpp, vLLM, and Mistral.rs.

## Relationship to Other Providers

All three providers implement the same `Provider` trait defined in
`src/providers/base.rs` and are instantiated by the factory functions in
`src/providers/mod.rs`.

| Aspect            | CopilotProvider | OllamaProvider | OpenAIProvider |
| ----------------- | --------------- | -------------- | -------------- |
| Auth mechanism    | OAuth keyring   | None           | Bearer token   |
| Wire format       | OpenAI-compat   | Ollama JSON    | OpenAI         |
| Streaming format  | SSE             | JSON Lines     | SSE            |
| Model list source | Copilot API     | Ollama tags    | `/v1/models`   |
| Model cache TTL   | 300 seconds     | 60 seconds     | 300 seconds    |
| Tool-call path    | Non-streaming   | Non-streaming  | Non-streaming  |
| Auth header       | Bearer + OAuth  | None           | Bearer (opt.)  |

## Environment Variable Reference

All five `OpenAIConfig` fields can be overridden at runtime via environment
variables without modifying the configuration file.

| Variable                   | `OpenAIConfig` Field | Default                       |
| -------------------------- | -------------------- | ----------------------------- |
| `XZATOMA_OPENAI_API_KEY`   | `api_key`            | `""`                          |
| `XZATOMA_OPENAI_BASE_URL`  | `base_url`           | `"https://api.openai.com/v1"` |
| `XZATOMA_OPENAI_MODEL`     | `model`              | `"gpt-4o-mini"`               |
| `XZATOMA_OPENAI_ORG_ID`    | `organization_id`    | (not set)                     |
| `XZATOMA_OPENAI_STREAMING` | `enable_streaming`   | `"true"`                      |

## Tests

Unit tests are in the `#[cfg(test)] mod tests` block at the bottom of
`src/providers/openai.rs`. Factory tests are in `src/providers/mod.rs`. HTTP
interactions are mocked with `wiremock` so no network access is required.

The test suite covers:

- Provider construction and field accessors (`base_url`, `model`)
- Message conversion for all roles and tool-call shapes
- Bearer token injection when `api_key` is non-empty
- Absence of auth header when `api_key` is empty
- `OpenAI-Organization` header injection when `organization_id` is set
- Non-streaming completion path end-to-end
- SSE streaming completion path end-to-end
- Tool-call completion path using the non-streaming path
- Model listing with a fresh request
- Model listing returning from cache on a second call (cache hit)
- Model switching for a valid model name
- Model switching rejection for an invalid model name
- Factory `create_provider("openai", ...)` returning `Ok`
- Factory `create_provider_with_override(..., Some("openai"), None)` returning
  `Ok`
- Factory `create_provider_with_override(..., Some("openai"), Some("gpt-4o"))`
  returning `Ok`

See `docs/explanation/openai_provider_phase2_implementation.md` for the full
Phase 2 implementation notes and design deviation log.
