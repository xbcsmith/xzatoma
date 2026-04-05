# OpenAI Provider Phase 2 Implementation

## Overview

Phase 2 of the OpenAI support implementation adds `src/providers/openai.rs`, a
full implementation of the `Provider` trait targeting the OpenAI Chat
Completions API. The module supports both SSE streaming and non-streaming
completion paths, tool calling, model listing with a 5-minute cache, model
switching, and `Bearer` token authorization. Any server that implements the
OpenAI Chat Completions API (llama.cpp, vLLM, Mistral.rs, Candle-vLLM) can be
targeted by overriding the `base_url` field in `OpenAIConfig`.

## Files Modified

| File                      | Change                                                                            |
| ------------------------- | --------------------------------------------------------------------------------- |
| `src/providers/openai.rs` | Created; full provider implementation and 22 tests                                |
| `src/providers/mod.rs`    | Added `pub mod openai` and `pub use openai::OpenAIProvider`; updated doc comments |
| `src/lib.rs`              | Updated module-level doc to mention OpenAI provider                               |

## Design Decisions

### Wire Types are Separate from Shared Provider Types

The Ollama provider aliases `ProviderMessage`, `ProviderRequest`,
`ProviderToolCall`, and `ProviderFunctionCall` because Ollama's JSON schema is
structurally identical to those shared types. OpenAI's schema differs in one
critical way: the `content` field in assistant messages must be `null` (not an
empty string) when the message contains only tool calls. The shared
`ProviderMessage.content` is `String`, not `Option<String>`. Reusing it for
OpenAI would require inserting empty strings where the API expects `null`,
causing serialization to diverge from the OpenAI spec.

All OpenAI wire types therefore use `Option<String>` for `content` fields and
are defined as private structs local to the module:

- `OpenAIRequest` - request body for `POST /v1/chat/completions`
- `OpenAIMessage` - single message with nullable `content`
- `OpenAIToolCall` / `OpenAIFunctionCall` - tool call structure in requests and
  responses
- `OpenAIResponse` / `OpenAIChoice` / `OpenAIUsage` - non-streaming response
- `OpenAIModelsResponse` / `OpenAIModelEntry` - `/v1/models` response
- `OpenAIStreamChunk` / `OpenAIStreamChoice` / `OpenAIStreamDelta` - SSE deltas
- `OpenAIStreamToolCallDelta` / `OpenAIStreamFunctionDelta` - incremental tool
  call data

### Streaming Path Selection

The `complete` method selects the path according to the following rule:

```text
use_streaming = config.enable_streaming AND tools.is_empty()
```

When tools are present the non-streaming path is always used. Streaming tool
calls arrive as incremental JSON fragments across many SSE chunks. Accumulating
them correctly requires a complete buffering layer; using the non-streaming path
is simpler and fully reliable. This decision is intentional and documented in
both the module-level doc comment and the `complete` method doc comment.

The `stream` field in the request body is set to `use_streaming` so the value
sent to the server always matches the code path taken. A test
(`test_complete_with_tools_uses_non_streaming_path`) verifies that a request
with `enable_streaming: true` and a non-empty tool list sends `"stream":false`
in the body.

### SSE Streaming Implementation

The streaming parser operates at the byte level to avoid allocating a full
string buffer per network chunk. Bytes are accumulated in a `line_buf: Vec<u8>`
until a `\n` byte is encountered. Each complete line is then processed:

1. Leading whitespace is stripped.
2. Blank lines and lines starting with `:` (SSE comment lines) are skipped.
3. Lines starting with `data:` are extracted and the payload is trimmed.
4. A payload of `[DONE]` breaks the outer stream loop via a labeled
   `break 'stream`.
5. Any other payload is deserialized as `OpenAIStreamChunk`. Parse errors are
   logged at `DEBUG` level and skipped so a single malformed chunk does not
   abort the stream.

Tool call deltas are accumulated in a `HashMap<u32, AccumulatedToolCall>` keyed
by the delta `index`. Each entry collects the `id` (set on the first delta),
`name` (set on the first function delta), and appended `arguments_buf`. After
the stream ends, entries are sorted by index and converted to `ToolCall`
structs.

Token usage is not included in the streaming `CompletionResponse` because the
OpenAI SSE event stream does not carry usage counters. The non-streaming path
does map `OpenAIUsage` to `TokenUsage`.

### Model Cache

`list_models` results are cached for 300 seconds (5 minutes) in
`model_cache: ModelCache` where `ModelCache` is a type alias for
`Arc<RwLock<Option<(Vec<ModelInfo>, Instant)>>>`. The same alias pattern is used
by `OllamaProvider`. The Clippy `type_complexity` lint is satisfied by
introducing the alias rather than suppressing the lint.

On cache hit, the method returns the cached `Vec<ModelInfo>` immediately without
making an HTTP request. The test `test_list_models_cache_hit` verifies this
using a wiremock mock with `expect(1)`: the mock is mounted to respond exactly
once, `list_models` is called twice, and when the mock server is dropped at the
end of the test it asserts that exactly one HTTP request was received.

### Authorization Headers

`build_request_headers` constructs the `HeaderMap` for all requests:

- `Content-Type: application/json` is always inserted.
- `Authorization: Bearer <api_key>` is inserted only when `api_key` is
  non-empty. This allows the provider to target local inference servers that
  require no authentication without sending a `Bearer` token.
- `OpenAI-Organization: <org_id>` is inserted only when `organization_id` is
  `Some` and non-empty.

The header name for the organization ID is `openai-organization` (lowercase),
which is the canonical HTTP/1.1 form used by `reqwest::header::HeaderName`.

### ProviderCapabilities

The capability flags returned by `get_provider_capabilities` are:

| Flag                       | Value   | Reason                                                                                                      |
| -------------------------- | ------- | ----------------------------------------------------------------------------------------------------------- |
| `supports_model_listing`   | `true`  | `/v1/models` endpoint is available                                                                          |
| `supports_model_details`   | `false` | The models list returns only `id` and `owned_by`; there is no per-model detail endpoint in the standard API |
| `supports_model_switching` | `true`  | `set_model` updates the config under a write lock after validating the name against the model list          |
| `supports_token_counts`    | `true`  | Non-streaming responses include `usage`                                                                     |
| `supports_streaming`       | `true`  | SSE streaming path is implemented                                                                           |

## Testing

All 22 tests are in the `mod tests` block inside `src/providers/openai.rs`.
Tests that require no network access (construction, message conversion,
capability checks) are synchronous `#[test]` functions. Tests that make HTTP
calls use `wiremock::MockServer` started in-process via `#[tokio::test]`.

| Test                                               | Category       | What it checks                                               |
| -------------------------------------------------- | -------------- | ------------------------------------------------------------ |
| `test_openai_provider_creation`                    | Construction   | `new` returns `Ok` with a default config                     |
| `test_openai_provider_base_url`                    | Accessor       | `base_url()` returns the default URL                         |
| `test_openai_provider_model`                       | Accessor       | `model()` returns `"gpt-4o-mini"`                            |
| `test_convert_messages_basic`                      | Conversion     | Single user message maps correctly                           |
| `test_convert_messages_with_tool_calls`            | Conversion     | Tool call fields are preserved                               |
| `test_convert_messages_drops_orphan_tool`          | Conversion     | `validate_message_sequence` drops orphan tool messages       |
| `test_convert_messages_preserves_valid_tool_pair`  | Conversion     | Paired assistant and tool messages are both kept             |
| `test_convert_response_message_text`               | Conversion     | Text response produces `Message::assistant`                  |
| `test_convert_response_message_with_tools`         | Conversion     | Tool call response produces `Message::assistant_with_tools`  |
| `test_convert_tools`                               | Conversion     | `convert_tools_from_json` produces two `ProviderTool` values |
| `test_get_current_model`                           | Accessor       | Returns `"gpt-4o-mini"` from default config                  |
| `test_provider_capabilities`                       | Capabilities   | All five flags match the specification                       |
| `test_complete_non_streaming`                      | HTTP           | Non-streaming completion returns content and usage           |
| `test_complete_streaming`                          | HTTP / SSE     | Accumulated SSE content equals `"Hello world"`               |
| `test_list_models`                                 | HTTP           | Two sorted models with correct capabilities are returned     |
| `test_set_model_valid`                             | HTTP           | `set_model` updates `get_current_model`                      |
| `test_set_model_invalid`                           | HTTP           | `set_model` returns `Err` for an unknown model               |
| `test_complete_with_tools_uses_non_streaming_path` | HTTP           | Tools force `"stream":false` even when streaming is enabled  |
| `test_bearer_token_sent_in_header`                 | HTTP / Headers | `Authorization: Bearer test-key` header is present           |
| `test_no_auth_header_when_api_key_empty`           | HTTP / Headers | No `Authorization` header when `api_key` is empty            |
| `test_org_header_sent_when_set`                    | HTTP / Headers | `openai-organization` header is present when configured      |
| `test_list_models_cache_hit`                       | HTTP / Cache   | Second `list_models` call does not hit the API               |

## Quality Gate Results

All four mandatory quality gates passed after implementation:

```text
cargo fmt --all                                        -- no diff
cargo check --all-targets --all-features               -- no errors
cargo clippy --all-targets --all-features -D warnings  -- no warnings
cargo test --all-features --lib -- providers::openai   -- 22/22 passed
```

The `type_complexity` Clippy lint was resolved by introducing the `ModelCache`
type alias, matching the pattern already used by `OllamaProvider`.

## Deviations from the Implementation Plan

The plan listed `ProviderFunctionCall` and `ProviderToolCall` in the required
imports. Those types belong to the Ollama/Copilot shared wire format and are not
used anywhere in the OpenAI provider implementation. Importing unused items
would cause `cargo clippy -D warnings` to fail. The imports were therefore
omitted. `ProviderTool` was added instead because it is the type used for the
`tools: Vec<ProviderTool>` field in `OpenAIRequest`.

The test tool JSON in `test_convert_tools` and
`test_complete_with_tools_uses_non_streaming_path` uses the flat tool-registry
format (`{ "name": ..., "description": ..., "parameters": ... }`) rather than
the nested OpenAI function-call format. This is required because
`convert_tools_from_json` (defined in `src/providers/base.rs`) expects the flat
format from the XZatoma tool registry, not the OpenAI request schema.
