# OpenAI Provider Support Implementation Plan

## Overview

This plan adds an OpenAI-compatible provider to XZatoma, enabling users to
target OpenAI's hosted API as well as any inference server that exposes an
OpenAI-compatible REST surface. Compatible servers include llama.cpp, vLLM,
Candle-vLLM, and Mistral.rs. The implementation extends the existing provider
abstraction in `src/providers/` with a new `openai.rs` module, adds
`OpenAIConfig` to `src/config.rs`, wires the provider into the factory functions
in `src/providers/mod.rs`, and documents the feature across `docs/explanation/`,
`docs/how-to/`, and `docs/reference/`.

The approach mirrors the patterns already established by `OllamaProvider` and
`CopilotProvider` so that any downstream code that calls the `Provider` trait
works without modification. A configurable `base_url` field is the mechanism
that makes the same provider code work for both the public OpenAI API and local
inference servers.

## Current State Analysis

### Existing Infrastructure

| Area                 | File                                     | Symbol or Responsibility                                                        | Relevance                                                                                                                                                           |
| -------------------- | ---------------------------------------- | ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Provider trait       | `src/providers/base.rs`                  | `Provider`                                                                      | Defines `complete`, `list_models`, `get_model_info`, `get_current_model`, `get_provider_capabilities`, `set_model`, `list_models_summary`, `get_model_info_summary` |
| Provider factory     | `src/providers/mod.rs`                   | `create_provider`, `create_provider_with_override`                              | Creates boxed provider instances from a type string                                                                                                                 |
| Copilot provider     | `src/providers/copilot.rs`               | `CopilotProvider`                                                               | Reference implementation for auth, SSE streaming, and tool calling                                                                                                  |
| Ollama provider      | `src/providers/ollama.rs`                | `OllamaProvider`                                                                | Reference implementation for REST completion and model listing                                                                                                      |
| Provider config      | `src/config.rs`                          | `ProviderConfig` at L40                                                         | Struct holding `provider_type`, `copilot: CopilotConfig`, and `ollama: OllamaConfig`                                                                                |
| Config validation    | `src/config.rs`                          | `Config::validate` at L1597                                                     | Hardcodes `valid_providers = ["copilot", "ollama"]` at exactly two locations: L1605 and L1714                                                                       |
| Env var application  | `src/config.rs`                          | `Config::apply_env_vars` at L966                                                | Maps `XZATOMA_PROVIDER`, `XZATOMA_COPILOT_MODEL`, `XZATOMA_OLLAMA_HOST`, `XZATOMA_OLLAMA_MODEL` to config fields                                                    |
| Subagent validation  | `src/config.rs`                          | `Config::validate` subagent block at L1714                                      | Independently validates `agent.subagent.provider` against the same hardcoded list                                                                                   |
| Shared message types | `src/providers/base.rs`                  | `ProviderMessage`, `ProviderTool`, `ProviderRequest`, `convert_tools_from_json` | Reusable wire-format types that both existing providers already use                                                                                                 |
| Shared message types | `src/providers/base.rs`                  | `validate_message_sequence`                                                     | Strips orphaned tool-result messages before any provider sends them                                                                                                 |
| HTTP client          | `Cargo.toml`                             | `reqwest 0.11`                                                                  | Already present with `json`, `rustls-tls`, and `stream` features enabled                                                                                            |
| Mock HTTP            | `Cargo.toml`                             | `wiremock 0.5` in `[dev-dependencies]`                                          | Used by existing provider tests for HTTP mocking                                                                                                                    |
| Library doc          | `src/lib.rs`                             | Module-level comment at L1                                                      | Lists "Copilot, Ollama" as the provider implementations; must be updated                                                                                            |
| Factory doc          | `src/providers/mod.rs`                   | `create_provider` doc comment                                                   | Lists valid types as `"copilot"` or `"ollama"` in `# Arguments`; must be updated                                                                                    |
| Config example       | `config/config.yaml`                     | `provider.type` comment at L7                                                   | Comment reads `"copilot" or "ollama"`; must be updated to include `"openai"`                                                                                        |
| Config reference     | `docs/reference/configuration.md`        | Provider Configuration section at L62                                           | Lists accepted values as `copilot` and `ollama` only; must be updated                                                                                               |
| Provider reference   | `docs/reference/provider_abstraction.md` | Provider Comparison table                                                       | Lists OpenAI as "API Reference Only"; must be updated to "Implemented"                                                                                              |
| Provider how-to      | `docs/how-to/configure_providers.md`     | OpenAI section at L13                                                           | References non-existent `OPENAI_API_KEY` env var; must be updated to `XZATOMA_OPENAI_API_KEY` and related vars                                                      |
| Models reference     | `docs/reference/models.md`               | Ollama and Copilot sections only                                                | No OpenAI section; must be added                                                                                                                                    |

### Identified Issues

| ID  | Issue                                                                                  | Impact                                                                            |
| --- | -------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| 1   | `valid_providers` is hardcoded at L1605 and L1714 in `Config::validate`                | Adding a new provider requires editing validation in two places; easy to miss one |
| 2   | `ProviderConfig` has no `openai` field                                                 | OpenAI configuration cannot be loaded from a config file or env vars              |
| 3   | `create_provider` and `create_provider_with_override` have no `"openai"` arm           | Requesting the OpenAI provider produces an error at runtime                       |
| 4   | No `XZATOMA_OPENAI_*` env vars are mapped in `apply_env_vars`                          | Users cannot configure the provider through environment variables                 |
| 5   | No `openai.rs` module exists in `src/providers/`                                       | There is no implementation to call                                                |
| 6   | `docs/reference/provider_abstraction.md` lists OpenAI as "API Reference Only"          | Documentation is inaccurate once the provider is implemented                      |
| 7   | `docs/how-to/configure_providers.md` references `OPENAI_API_KEY` and `OPENAI_HOST`     | Wrong env var names; the XZatoma prefix is `XZATOMA_OPENAI_*`                     |
| 8   | `config/config.yaml` comment says `"copilot" or "ollama"` only                         | Users inspecting the example file will not discover the openai option             |
| 9   | `src/lib.rs` module doc says "Copilot, Ollama" as provider implementations             | Library documentation is incomplete                                               |
| 10  | `create_provider` and `create_provider_with_override` doc comments list only two types | Callers reading the API docs will not know "openai" is a valid type string        |
| 11  | `docs/reference/configuration.md` Provider section lists only `copilot` and `ollama`   | Reference documentation is incomplete                                             |
| 12  | `docs/reference/models.md` has no OpenAI section                                       | Users have no reference for available OpenAI models                               |

## Explicit First-Release Decisions

The following decisions are locked for this plan and must not be changed during
implementation.

| Decision Area        | Decision                                                                                                                           | Status       |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------------------- | ------------ |
| API surface          | Target `POST /v1/chat/completions` and `GET /v1/models` only                                                                       | REQUIRED     |
| Compatibility        | A single `base_url` field makes the provider work with any OpenAI-compatible server                                                | REQUIRED     |
| Authentication       | Bearer token via `Authorization: Bearer <api_key>` header; `api_key` may be an empty string for local servers that require no auth | REQUIRED     |
| Streaming            | SSE streaming must be supported via `stream: true` in the Chat Completions request body                                            | REQUIRED     |
| Tool calling         | Tool calling must be supported using the standard OpenAI function calling JSON schema                                              | REQUIRED     |
| Config key name      | The YAML key for the new config block is `openai` inside the `provider` section                                                    | REQUIRED     |
| Env var prefix       | All new env vars use the `XZATOMA_OPENAI_` prefix                                                                                  | REQUIRED     |
| Default base URL     | `https://api.openai.com/v1`                                                                                                        | REQUIRED     |
| Default model        | `gpt-4o-mini`                                                                                                                      | REQUIRED     |
| API key source       | Config field `openai.api_key` or env var `XZATOMA_OPENAI_API_KEY`; env var takes precedence over config file when both are set     | REQUIRED     |
| Organization support | Optional `organization_id` field sent as `OpenAI-Organization` header when non-empty                                               | REQUIRED     |
| Streaming path       | Use non-streaming (single JSON response) path when the request includes tools, to avoid partial tool-call accumulation issues      | REQUIRED     |
| Reasoning fields     | Not implemented in this phase                                                                                                      | OUT OF SCOPE |
| Embeddings           | Not implemented in this phase                                                                                                      | OUT OF SCOPE |
| Images/vision input  | Not implemented in this phase                                                                                                      | OUT OF SCOPE |
| Fine-tuning          | Not implemented in this phase                                                                                                      | OUT OF SCOPE |

## Implementation Phases

### Phase 1: Configuration Schema

Add `OpenAIConfig` to `src/config.rs`, wire it into `ProviderConfig`, expand
validation to accept `"openai"`, map five new env vars, and update the
`config/config.yaml` example file.

#### Task 1.1: Add OpenAIConfig Struct

Add a new `OpenAIConfig` struct to `src/config.rs` immediately after the closing
brace of `impl Default for OllamaConfig` at L155. Follow the same documentation
and default-function patterns used by `OllamaConfig` at L130-155.

The struct must contain the following fields with the listed defaults.

| Field              | Type             | Default                       | Description                                                             |
| ------------------ | ---------------- | ----------------------------- | ----------------------------------------------------------------------- |
| `api_key`          | `String`         | `""` (empty string)           | Bearer token for authentication; empty is valid for local servers       |
| `base_url`         | `String`         | `"https://api.openai.com/v1"` | Base URL for all API requests; override to target llama.cpp, vLLM, etc. |
| `model`            | `String`         | `"gpt-4o-mini"`               | Model identifier sent in the request body                               |
| `organization_id`  | `Option<String>` | `None`                        | Optional organization identifier sent as `OpenAI-Organization` header   |
| `enable_streaming` | `bool`           | `true`                        | Use SSE streaming for responses when no tools are present               |

Provide private helper functions `default_openai_api_key`,
`default_openai_base_url`, and `default_openai_model` that return the default
values listed above. Implement `Default for OpenAIConfig` using those helpers,
following the identical pattern used by `impl Default for OllamaConfig` at
L148-155 of `src/config.rs`.

The struct must be decorated with
`#[derive(Debug, Clone, Serialize, Deserialize)]`. Every field must have a
`#[serde(default = "...")]` or `#[serde(default)]` annotation. Every field must
have a `///` doc comment. The struct itself must have a full Rustdoc block
including `# Examples` with a runnable code snippet.

#### Task 1.2: Add openai Field to ProviderConfig

Add the following field to the `ProviderConfig` struct at L40 of
`src/config.rs`, alongside the existing `copilot` and `ollama` fields:

```text
#[serde(default)]
pub openai: OpenAIConfig,
```

Update the Rustdoc block comment above `ProviderConfig` to list `"openai"` as a
valid value for `provider_type`.

#### Task 1.3: Update Config Validation

Edit `Config::validate` in `src/config.rs` to add `"openai"` to both
`valid_providers` arrays. There are exactly two occurrences to update.

- First occurrence: `let valid_providers = ["copilot", "ollama"];` at L1605.
  Change to `let valid_providers = ["copilot", "ollama", "openai"];`.
- Second occurrence: `let valid_providers = ["copilot", "ollama"];` at L1714,
  inside the `if let Some(ref provider) = self.agent.subagent.provider` block.
  Change to `let valid_providers = ["copilot", "ollama", "openai"];`.

Both occurrences must be updated. The error messages produced by these blocks
already interpolate the slice contents via `.join(", ")`, so the error messages
will automatically include `"openai"` after the change without any other edits.

#### Task 1.4: Add OpenAI Env Var Mappings

Add the following mappings to `Config::apply_env_vars` in `src/config.rs`,
immediately after the last `XZATOMA_OLLAMA_MODEL` block that ends at
approximately L983. Follow the placement and style of the existing Ollama env
var block at L979-983.

| Env Var                    | Config Field                            | Parse Rule                           |
| -------------------------- | --------------------------------------- | ------------------------------------ |
| `XZATOMA_OPENAI_API_KEY`   | `self.provider.openai.api_key`          | Assign string value directly         |
| `XZATOMA_OPENAI_BASE_URL`  | `self.provider.openai.base_url`         | Assign string value directly         |
| `XZATOMA_OPENAI_MODEL`     | `self.provider.openai.model`            | Assign string value directly         |
| `XZATOMA_OPENAI_ORG_ID`    | `self.provider.openai.organization_id`  | Set to `Some(value)` when var is set |
| `XZATOMA_OPENAI_STREAMING` | `self.provider.openai.enable_streaming` | Call `parse_env_bool(&value)`        |

For `XZATOMA_OPENAI_STREAMING`, use the same `match parse_env_bool(...)` guard
pattern used by `XZATOMA_SKILLS_ENABLED` at approximately L1010-1015 of
`src/config.rs`. Emit a `tracing::warn!` on invalid values.

#### Task 1.5: Update config/config.yaml

Edit `config/config.yaml` to make two changes.

Change 1: Update the provider type comment at L7 from:

```text
# Provider type: "copilot" or "ollama"
```

to:

```text
# Provider type: "copilot", "ollama", or "openai"
```

Change 2: Add a commented-out openai block immediately after the closing line of
the `ollama:` block. The block must show all five `OpenAIConfig` fields with
explanatory comments and must be completely commented out so the file parses as
valid YAML using the existing `ollama` provider:

```text
  # OpenAI (and OpenAI-compatible) provider configuration
  # Uncomment the openai block and set type: openai to activate
  # openai:
  #   api_key: ""              # Set via XZATOMA_OPENAI_API_KEY env var
  #   base_url: "https://api.openai.com/v1"  # Override for local servers
  #   model: gpt-4o-mini
  #   organization_id:         # Optional; set via XZATOMA_OPENAI_ORG_ID
  #   enable_streaming: true
```

#### Task 1.6: Testing Requirements

Write tests in the `mod tests` block of `src/config.rs` for:

- `test_openai_config_defaults` - verifies all five field defaults match the
  specification in Task 1.1
- `test_openai_config_deserialize_all_fields` - round-trips a full YAML block
  through `serde_yaml::from_str` with all five fields explicitly set
- `test_openai_config_deserialize_omitted_fields_use_defaults` - verifies that a
  YAML block containing only `model: gpt-4o` falls back to defaults for all
  other fields
- `test_apply_env_vars_overrides_openai_fields` - sets all five env vars via
  `EnvVarGuard::set`, calls `apply_env_vars`, and asserts each field is updated
- `test_config_validation_accepts_openai_provider` - builds a `Config` with
  `provider.provider_type = "openai"` and asserts `validate()` returns `Ok`
- `test_config_validation_accepts_openai_subagent_override` - sets
  `config.agent.subagent.provider = Some("openai".to_string())` and asserts
  `validate()` returns `Ok`

Use the `EnvVarGuard` pattern defined at L1982-2003 in `src/config.rs` for all
env var isolation in these tests.

#### Task 1.7: Deliverables

- `OpenAIConfig` struct in `src/config.rs` with all five fields, private default
  helpers, `impl Default`, and full Rustdoc including a runnable example
- `openai` field added to `ProviderConfig` with `#[serde(default)]`
- `ProviderConfig` Rustdoc updated to list `"openai"` as a valid provider type
- Both `valid_providers` slices updated to `["copilot", "ollama", "openai"]` at
  L1605 and L1714
- Five env var mappings added to `apply_env_vars`
- `config/config.yaml` updated with corrected comment and commented-out openai
  block
- Six new tests passing

#### Task 1.8: Success Criteria

- `cargo fmt --all` produces no diff
- `cargo check --all-targets --all-features` produces no errors
- `cargo clippy --all-targets --all-features -- -D warnings` produces no
  warnings
- `cargo test --all-features` passes all new and existing tests
- A config file containing an `openai` section with all five fields set is
  accepted by `Config::validate` without error when `provider.type` is
  `"openai"`
- `config/config.yaml` parses without error when loaded by `Config::from_file`

---

### Phase 2: Provider Implementation

Create `src/providers/openai.rs` implementing the full `Provider` trait for the
OpenAI Chat Completions API. Update `src/providers/mod.rs` and `src/lib.rs` to
export the new provider and update their documentation.

#### Task 2.1: Create src/providers/openai.rs

Create a new file `src/providers/openai.rs`. The file must begin with a
module-level `//!` doc comment describing the module, followed by all `use`
declarations, then the wire types, then the provider struct and impl blocks.

##### Required Imports

The file must import the following items:

```text
use crate::config::OpenAIConfig;
use crate::error::{Result, XzatomaError};
use crate::providers::{
    convert_tools_from_json, validate_message_sequence, CompletionResponse, FunctionCall,
    Message, ModelCapability, ModelInfo, Provider, ProviderCapabilities, ProviderFunctionCall,
    ProviderToolCall, TokenUsage, ToolCall,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
```

##### Struct and Construction

Define `pub struct OpenAIProvider` with three private fields:

- `client: Client`
- `config: Arc<RwLock<OpenAIConfig>>`
- `model_cache: Arc<RwLock<Option<(Vec<ModelInfo>, Instant)>>>`

Implement `pub fn new(config: OpenAIConfig) -> Result<Self>` following the same
`Client::builder` pattern used by `OllamaProvider::new` at L165 of
`src/providers/ollama.rs`. Set the timeout to 120 seconds and the user agent to
`"xzatoma/0.1.0"`. Emit a `tracing::info!` line on successful construction that
logs `base_url` and `model`. Return the error variant
`XzatomaError::Provider(format!("Failed to create HTTP client: {}", e))` if the
builder fails.

Implement the following public accessor helpers, each with a full Rustdoc block
and a runnable `# Examples` section:

- `pub fn base_url(&self) -> String` - reads `config.base_url` under the read
  lock
- `pub fn model(&self) -> String` - reads `config.model` under the read lock

##### Wire Types

Define private structs for the OpenAI Chat Completions request and response. All
structs must derive `Debug`, `Clone`, `Serialize`, and `Deserialize`. Fields
that should be absent from serialized JSON when empty or `None` must use the
appropriate `#[serde(skip_serializing_if = "...")]` attributes.

**Important distinction**: `ProviderMessage.content` in `src/providers/base.rs`
is a `String`. The OpenAI API requires `content` to be nullable (`null`) for
messages that contain only tool calls. All OpenAI wire types in this file must
therefore use `Option<String>` for `content` fields rather than reusing
`ProviderMessage` directly.

Required types:

- `OpenAIRequest` - maps to the `POST /v1/chat/completions` request body;
  fields: `model: String`, `messages: Vec<OpenAIMessage>`,
  `tools: Vec<ProviderTool>` with
  `#[serde(skip_serializing_if = "Vec::is_empty")]`, `stream: bool`
- `OpenAIMessage` - fields: `role: String`, `content: Option<String>` with
  `#[serde(skip_serializing_if = "Option::is_none")]`,
  `tool_calls: Option<Vec<OpenAIToolCall>>` with
  `#[serde(skip_serializing_if = "Option::is_none")]`,
  `tool_call_id: Option<String>` with
  `#[serde(skip_serializing_if = "Option::is_none")]`
- `OpenAIToolCall` - fields: `id: String`, `r#type: String` (always `"function"`
  when serialized), `function: OpenAIFunctionCall`
- `OpenAIFunctionCall` - fields: `name: String`, `arguments: String`
- `OpenAIResponse` - fields: `choices: Vec<OpenAIChoice>`,
  `usage: Option<OpenAIUsage>`, `model: Option<String>`
- `OpenAIChoice` - fields: `message: OpenAIMessage`,
  `finish_reason: Option<String>`
- `OpenAIUsage` - fields: `prompt_tokens: u32`, `completion_tokens: u32`,
  `total_tokens: u32`
- `OpenAIModelsResponse` - fields: `data: Vec<OpenAIModelEntry>`
- `OpenAIModelEntry` - fields: `id: String`, `owned_by: Option<String>`

For the SSE streaming path, define one additional type:

- `OpenAIStreamChoice` - fields: `delta: OpenAIStreamDelta`,
  `finish_reason: Option<String>`, `index: u32`
- `OpenAIStreamDelta` - fields: `content: Option<String>` with
  `#[serde(skip_serializing_if = "Option::is_none")]`,
  `tool_calls: Option<Vec<OpenAIStreamToolCallDelta>>` with
  `#[serde(skip_serializing_if = "Option::is_none")]`
- `OpenAIStreamToolCallDelta` - fields: `index: u32`, `id: Option<String>`,
  `r#type: Option<String>`, `function: Option<OpenAIStreamFunctionDelta>`
- `OpenAIStreamFunctionDelta` - fields: `name: Option<String>`,
  `arguments: Option<String>`
- `OpenAIStreamChunk` - fields: `choices: Vec<OpenAIStreamChoice>`

##### Message Conversion

Implement a private
`fn convert_messages(&self, messages: &[Message]) -> Vec<OpenAIMessage>` method
on `OpenAIProvider`. Steps:

1. Call `validate_message_sequence(messages)` to obtain a validated slice.
2. Map each validated `Message` to `OpenAIMessage` as follows:
   - Set `role` from `message.role`
   - Set `content` to `message.content.clone()` (already `Option<String>`)
   - If `message.tool_calls` is `Some`, convert each `ToolCall` to
     `OpenAIToolCall`: set `id`, set `r#type` to `"function"`, set
     `function.name` from `tc.function.name`, set `function.arguments` from
     `tc.function.arguments`
   - Set `tool_call_id` from `message.tool_call_id`
3. Skip messages where both `content` is `None` and `tool_calls` is `None`.
4. Return the collected `Vec<OpenAIMessage>`.

Implement a private
`fn convert_response_message(&self, msg: OpenAIMessage) -> Message` method.
Steps:

1. If `msg.tool_calls` is `Some` and non-empty, convert each `OpenAIToolCall` to
   `ToolCall`: set `id` from `tc.id`, set `function.name` from
   `tc.function.name`, set `function.arguments` from `tc.function.arguments`.
   Return `Message::assistant_with_tools(converted_calls)`.
2. Otherwise return `Message::assistant(msg.content.unwrap_or_default())`.

This mirrors the pattern in `OllamaProvider::convert_response_message` at
approximately L279 of `src/providers/ollama.rs`.

##### HTTP Helpers

Implement a private
`fn build_request_headers(&self) -> Result<reqwest::header::HeaderMap>`. This
helper must:

1. Insert `Content-Type: application/json`.
2. Read `api_key` from the config under the read lock. If `api_key` is
   non-empty, insert `Authorization: Bearer <api_key>`.
3. Read `organization_id` from the config. If `Some` and non-empty, insert
   `OpenAI-Organization: <org_id>`.
4. Return the completed `HeaderMap`.

Map header insertion errors to `XzatomaError::Provider(format!(...))`.

Implement a private
`async fn post_completions(&self, request: &OpenAIRequest) -> Result<CompletionResponse>`.
This method performs the non-streaming path:

1. Build headers by calling `self.build_request_headers()`.
2. Build the URL as `format!("{}/chat/completions", self.base_url())`.
3. POST the request body as JSON using
   `self.client.post(&url).headers(headers).json(request).send().await`.
4. If the response status is not success, return
   `Err(XzatomaError::Provider(format!("HTTP {}: {}", status, body)))`.
5. Deserialize the body as `OpenAIResponse`.
6. Take the first choice from `response.choices`. If empty, return
   `Err(XzatomaError::Provider("No choices in response".to_string()))`.
7. Convert the choice message using
   `self.convert_response_message(choice.message)`.
8. Map `response.usage` to `TokenUsage` and set it on the `CompletionResponse`.
9. Set `response.model` on the `CompletionResponse`.
10. Return the `CompletionResponse`.

Implement a private
`async fn post_completions_streaming(&self, request: &OpenAIRequest) -> Result<CompletionResponse>`.
This method performs the SSE streaming path:

1. Build headers as above. Set `Accept: text/event-stream` in addition.
2. POST the same endpoint with the request body.
3. Read the response body as a byte stream using `response.bytes_stream()`.
4. Process lines from the stream as follows:
   - Collect bytes into a line buffer until a newline character is encountered.
   - Strip leading whitespace from each line.
   - Skip blank lines and lines that start with `:` (SSE comment lines).
   - For lines starting with `data:`, extract the payload after the `data:`
     prefix.
   - If the payload is exactly `[DONE]`, break out of the stream loop.
   - Otherwise, attempt to deserialize the payload as `OpenAIStreamChunk`. On
     parse error, log a `tracing::debug!` and continue to the next line.
   - For each chunk, process `chunk.choices[0].delta`:
     - If `delta.content` is `Some`, append it to a `content_buf: String`.
     - If `delta.tool_calls` is `Some`, accumulate tool call deltas by `index`
       into a `HashMap<u32, AccumulatedToolCall>` where `AccumulatedToolCall`
       holds `id`, `name`, and `arguments_buf`. On each delta, update the map
       entry by appending to `arguments_buf` and setting `id`/`name` if not yet
       set.
5. After the loop, assemble the final `CompletionResponse`:
   - If the tool call map is non-empty, call `Message::assistant_with_tools`
     with the accumulated calls converted to `ToolCall` structs.
   - Otherwise call `Message::assistant(content_buf)`.
6. Return the `CompletionResponse` with no `TokenUsage` (streaming does not
   return token counts in the SSE event stream).

Note: `AccumulatedToolCall` is a private struct local to this function or
defined as a private helper struct in the module.

##### Provider Trait Implementation

Implement `Provider for OpenAIProvider`. The impl block must be annotated with
`#[async_trait]`. Every method must have a `///` doc comment. Behavior
requirements:

- `complete`: Determine the path as follows. If `enable_streaming` is `true` AND
  the `tools` argument is empty, call `self.post_completions_streaming(...)`.
  Otherwise call `self.post_completions(...)`. This rule means tool-use requests
  always use the non-streaming path to avoid partial tool-call accumulation.
- `list_models`: Build the URL as `format!("{}/models", self.base_url())`. GET
  the URL with the request headers. Deserialize the response as
  `OpenAIModelsResponse`. Convert each `OpenAIModelEntry` to `ModelInfo` using
  `ModelInfo::new(entry.id.clone(), entry.id.clone(), 0)`. Add the
  `ModelCapability::Completion` and `ModelCapability::FunctionCalling`
  capabilities to each entry. Sort the results by `name` before returning. Cache
  the result in `self.model_cache` with a 300-second TTL. On cache hit, return
  the cached list.
- `get_model_info`: Call `self.list_models(&[]).await` and find the entry where
  `info.name == model_name`. Return `Ok(info)` or
  `Err(XzatomaError::Provider(format!("Model not found: {}", model_name)))`.
- `get_current_model`: Read `config.model` under the read lock and return it.
- `get_provider_capabilities`: Return `ProviderCapabilities` with
  `supports_model_listing: true`, `supports_model_details: false`,
  `supports_model_switching: true`, `supports_token_counts: true`,
  `supports_streaming: true`.
- `set_model`: Call `self.list_models(&[]).await` to verify the model exists. If
  not found, return an error. Otherwise write
  `config.model = model_name.to_string()` under the write lock and return
  `Ok(())`.
- `list_models_summary` and `get_model_info_summary`: Use the default trait
  implementations already provided in `src/providers/base.rs`.

#### Task 2.2: Export the New Provider

Edit `src/providers/mod.rs` to add the following two lines after the
`pub mod ollama;` and `pub use ollama::OllamaProvider;` lines:

```text
pub mod openai;
pub use openai::OpenAIProvider;
```

#### Task 2.3: Update Module Documentation

Make the following documentation-only edits. No logic changes.

Edit 1: In `src/lib.rs` at L1, update the module-level `//!` doc comment. Locate
the line that reads:

```text
//! - `providers`: AI provider abstraction and implementations (Copilot, Ollama)
```

Change it to:

```text
//! - `providers`: AI provider abstraction and implementations (Copilot, Ollama, OpenAI)
```

Edit 2: In `src/providers/mod.rs`, update the `# Arguments` section of the doc
comment above `create_provider`. The line that reads:

```text
/// * `provider_type` - Type of provider ("copilot" or "ollama")
```

must be changed to:

```text
/// * `provider_type` - Type of provider ("copilot", "ollama", or "openai")
```

Edit 3: In `src/providers/mod.rs`, update the `# Arguments` section of the doc
comment above `create_provider_with_override`. The line that reads:

```text
/// * `provider_override` - Optional provider type override ("copilot" or "ollama")
```

must be changed to:

```text
/// * `provider_override` - Optional provider type override ("copilot", "ollama", or "openai")
```

#### Task 2.4: Testing Requirements

Write a `mod tests` block inside `src/providers/openai.rs` with the following
tests. Use `wiremock` (in `[dev-dependencies]` in `Cargo.toml`) to mock HTTP
responses for all tests that make network calls.

| Test Name                                          | Condition                                          | Assertion                                                         |
| -------------------------------------------------- | -------------------------------------------------- | ----------------------------------------------------------------- |
| `test_openai_provider_creation`                    | Valid `OpenAIConfig`                               | `new` returns `Ok`                                                |
| `test_openai_provider_base_url`                    | Default config                                     | `base_url()` returns `"https://api.openai.com/v1"`                |
| `test_openai_provider_model`                       | Default config                                     | `model()` returns `"gpt-4o-mini"`                                 |
| `test_convert_messages_basic`                      | Single user message                                | Produces one `OpenAIMessage` with correct role and content        |
| `test_convert_messages_with_tool_calls`            | Assistant message with tool calls                  | Tool call fields are preserved in output                          |
| `test_convert_messages_drops_orphan_tool`          | Tool result without preceding assistant tool call  | Orphan is dropped via `validate_message_sequence`                 |
| `test_convert_messages_preserves_valid_tool_pair`  | Paired assistant tool call and tool result message | Both messages are present in output                               |
| `test_convert_response_message_text`               | `OpenAIMessage` with text content                  | Returns `Message::assistant` with the same content                |
| `test_convert_response_message_with_tools`         | `OpenAIMessage` with tool calls                    | Returns `Message::assistant_with_tools` with converted calls      |
| `test_convert_tools`                               | Two tool JSON values                               | Produces two `ProviderTool` values via `convert_tools_from_json`  |
| `test_get_current_model`                           | Default config                                     | Returns `"gpt-4o-mini"`                                           |
| `test_provider_capabilities`                       | Any config                                         | All five capability flags match the spec in Task 2.1              |
| `test_complete_non_streaming`                      | Mock completions endpoint returns valid JSON       | Returns `CompletionResponse` with correct content                 |
| `test_complete_streaming`                          | Mock completions endpoint returns SSE stream       | Returns `CompletionResponse` with accumulated content             |
| `test_list_models`                                 | Mock models endpoint returns two entries           | `list_models` returns two sorted `ModelInfo` values               |
| `test_set_model_valid`                             | Model exists in mock model list                    | `set_model` returns `Ok` and `get_current_model` returns new name |
| `test_set_model_invalid`                           | Model does not exist in mock model list            | `set_model` returns `Err`                                         |
| `test_complete_with_tools_uses_non_streaming_path` | `enable_streaming: true`, request has tools        | Non-streaming endpoint is called; streaming endpoint is not       |
| `test_bearer_token_sent_in_header`                 | `api_key` is non-empty                             | Request contains `Authorization: Bearer <key>` header             |
| `test_no_auth_header_when_api_key_empty`           | `api_key` is empty string                          | No `Authorization` header is sent                                 |
| `test_org_header_sent_when_set`                    | `organization_id` is `Some("myorg".to_string())`   | Request contains `OpenAI-Organization: myorg` header              |
| `test_list_models_cache_hit`                       | `list_models` called twice; mock returns once      | Second call returns cached result without hitting the mock        |

#### Task 2.5: Deliverables

- `src/providers/openai.rs` implementing all wire types, streaming delta types,
  message converters, HTTP helpers, and the full `Provider` trait with
  `#[async_trait]`
- `pub mod openai;` and `pub use openai::OpenAIProvider;` in
  `src/providers/mod.rs`
- Module doc, `create_provider` doc, and `create_provider_with_override` doc
  updated in `src/providers/mod.rs` and `src/lib.rs`
- All 22 tests in `src/providers/openai.rs` passing

#### Task 2.6: Success Criteria

- `cargo fmt --all` produces no diff
- `cargo check --all-targets --all-features` produces no errors
- `cargo clippy --all-targets --all-features -- -D warnings` produces no
  warnings
- `cargo test --all-features` passes including all new provider tests
- A manually constructed `OpenAIProvider` targeting a wiremock server can
  complete a request with and without tool schemas

---

### Phase 3: Factory Registration and Integration

Wire `OpenAIProvider` into the two factory functions, update all existing tests
that construct `ProviderConfig` literals, and add integration-level factory
tests.

#### Task 3.1: Update create_provider

In `src/providers/mod.rs`, add an `"openai"` arm to `create_provider`. Place the
arm between the `"ollama"` arm and the catch-all error arm:

```text
"openai" => Ok(Box::new(openai::OpenAIProvider::new(config.openai.clone())?)),
```

#### Task 3.2: Update create_provider_with_override

In `src/providers/mod.rs`, add an `"openai"` arm to
`create_provider_with_override`. The arm must clone `config.openai`, apply the
`model_override` if `Some` by setting `openai_config.model = model.to_string()`,
then construct and box the provider:

```text
"openai" => {
    let mut openai_config = config.openai.clone();
    if let Some(model) = model_override {
        openai_config.model = model.to_string();
    }
    Ok(Box::new(OpenAIProvider::new(openai_config)?))
}
```

#### Task 3.3: Update ProviderConfig Literals in Existing Tests

The `ProviderConfig` struct in `src/config.rs` will have a new `openai` field
after Phase 1. All existing test code that constructs `ProviderConfig` using
struct literal syntax will fail to compile until the new field is added.

Search for all occurrences of `ProviderConfig {` in `src/providers/mod.rs` using
the project search tool. For each occurrence, add
`openai: OpenAIConfig::default()` to the literal. Also add
`use crate::config::OpenAIConfig;` to the test module imports if it is not
already present.

The affected tests in `src/providers/mod.rs` are:

- `test_create_provider_invalid_type`
- `test_create_provider_with_override_default`
- `test_create_provider_with_override_provider_only`
- `test_create_provider_with_override_provider_and_model`
- `test_create_provider_with_override_model_only`
- `test_create_provider_with_override_invalid_provider`
- `test_create_provider_with_override_copilot_model`
- `test_create_provider_with_override_ollama_model`

Also check `src/config.rs` for any test that constructs `ProviderConfig` with a
struct literal and add `openai: OpenAIConfig::default()` to those as well.

Also check the doc example inside the `create_provider_with_override` doc
comment. The example constructs
`ProviderConfig { provider_type: ..., copilot: ..., ollama: ... }`. This example
is marked `no_run` so it will not fail compilation, but it must be updated
anyway to include `openai: OpenAIConfig::default()` to remain accurate.

#### Task 3.4: Add Config Example File

Create `config/openai_config.yaml` with a complete, standalone configuration
that demonstrates all `OpenAIConfig` fields. The file must include a comment on
every field explaining its effect. The file must be valid YAML that parses
without error when loaded by `Config::from_file`.

The file must contain at minimum:

1. A primary `provider` block with `type: openai` and a fully populated
   `openai:` sub-block showing all five fields.
2. A comment section showing example configurations for llama.cpp, vLLM, and
   Mistral.rs as commented-out YAML blocks. Each block must show a `base_url`
   pointing to a local address (e.g., `http://localhost:8080/v1`), a
   representative `model` name, and `api_key: ""`.
3. A minimal `agent:` block to satisfy the loader's required fields.

#### Task 3.5: Testing Requirements

Write new tests in the `mod tests` block of `src/providers/mod.rs`:

| Test Name                                         | Condition                                  | Assertion    |
| ------------------------------------------------- | ------------------------------------------ | ------------ |
| `test_create_provider_openai`                     | `provider_type = "openai"`                 | Returns `Ok` |
| `test_create_provider_with_override_to_openai`    | Override to `"openai"` from copilot config | Returns `Ok` |
| `test_create_provider_with_override_openai_model` | Override to `"openai"` with model override | Returns `Ok` |

#### Task 3.6: Deliverables

- `"openai"` arm in `create_provider` in `src/providers/mod.rs`
- `"openai"` arm in `create_provider_with_override` in `src/providers/mod.rs`
- All `ProviderConfig` struct literals in existing tests updated with
  `openai: OpenAIConfig::default()`
- `create_provider_with_override` doc example updated with the new field
- `config/openai_config.yaml` with full examples for hosted and local servers
- Three new factory tests passing

#### Task 3.7: Success Criteria

- `cargo fmt --all` produces no diff
- `cargo test --all-features` passes all tests including factory tests
- `cargo clippy --all-targets --all-features -- -D warnings` produces no
  warnings
- `config/openai_config.yaml` parses without error when loaded by
  `Config::from_file`

---

### Phase 4: Documentation

Update and create all documentation files required to make the feature
discoverable and accurately described.

#### Task 4.1: Create Implementation Summary

Create `docs/explanation/openai_provider_implementation.md`. The document must
describe:

- Design decisions made during implementation
- How to target a local inference server using `base_url`
- Authentication model and how to disable auth for local servers
- Streaming behavior and the tool-use exception (non-streaming when tools
  present)
- How `content: Option<String>` differs from `ProviderMessage.content: String`
  and why this matters for tool-use messages
- Relationship to the existing `CopilotProvider` and `OllamaProvider`
- The 300-second model list cache and its effect on `set_model` behavior
- Env var reference table for all five `XZATOMA_OPENAI_*` variables

#### Task 4.2: Update docs/reference/configuration.md

File `docs/reference/configuration.md` exists at
`docs/reference/configuration.md`. Make the following additions.

Addition 1: In the Provider Configuration section at approximately L62, add
`openai` to the accepted values list under the `type` field:

```text
- `type`
  - Type: string
  - Accepted values:
    - `copilot`
    - `ollama`
    - `openai`
```

Addition 2: Add a new sub-section `### OpenAI Configuration` after the existing
`### Ollama Configuration` sub-section. The sub-section must contain a
`#### Fields` table listing all five `OpenAIConfig` fields with their types,
defaults, env var overrides, and descriptions. The sub-section must also contain
a YAML code block showing a complete `provider:` block with `type: openai`.

#### Task 4.3: Update docs/reference/provider_abstraction.md

File `docs/reference/provider_abstraction.md` exists at
`docs/reference/provider_abstraction.md`. Make the following changes.

Change 1: In the Provider Comparison table, change the OpenAI row from:

```text
| OpenAI    | API Reference Only | Bearer Token   | api.openai.com        | gpt-4o            | 128K            | SSE        | Native            |
```

to:

```text
| OpenAI    | Implemented        | Bearer Token   | api.openai.com/v1     | gpt-4o-mini       | Model-dependent | SSE        | OpenAI-compatible |
```

Change 2: In the introductory paragraph under `**API reference only**`, remove
OpenAI from the list. If Anthropic is the only remaining entry, update the
paragraph to reflect that.

Change 3: In the File Layout section at the bottom of the document, update the
directory listing to add `openai.rs`:

```text
providers/
├── base.rs         # Provider trait and base types
├── copilot.rs       # GitHub Copilot (OAuth) implementation
├── ollama.rs        # Ollama local/remote provider
├── openai.rs        # OpenAI and OpenAI-compatible provider
└── mod.rs          # Module root and re-exports
```

Change 4: In the Environment Variables section, replace the existing OpenAI
block (which incorrectly shows `OPENAI_API_KEY`, `OPENAI_HOST`, and
`OPENAI_TIMEOUT`) with:

```text
OpenAI (and OpenAI-compatible servers)

export XZATOMA_OPENAI_API_KEY="sk-..."          # Required for hosted API
export XZATOMA_OPENAI_BASE_URL="https://api.openai.com/v1"  # Default
export XZATOMA_OPENAI_MODEL="gpt-4o-mini"       # Default model
export XZATOMA_OPENAI_ORG_ID="org-..."          # Optional
export XZATOMA_OPENAI_STREAMING="true"          # Default
```

#### Task 4.4: Update docs/how-to/configure_providers.md

File `docs/how-to/configure_providers.md` exists at
`docs/how-to/configure_providers.md`. The file currently contains an OpenAI
section that references `OPENAI_API_KEY`, `OPENAI_HOST`, and `OPENAI_TIMEOUT`,
which are not XZatoma environment variable names and will not work. Make the
following changes.

Change 1: Replace the Quickstart one-liner for OpenAI at approximately L13:

```text
export OPENAI_API_KEY="sk-..."
xzatoma chat --provider openai
```

with:

```text
export XZATOMA_OPENAI_API_KEY="sk-..."
xzatoma chat --provider openai
```

Change 2: Replace the OpenAI entry in the Supported Providers and Auth Methods
section. The replacement text must state: Bearer API key via
`XZATOMA_OPENAI_API_KEY`; optional `XZATOMA_OPENAI_BASE_URL` for compatible
local servers; `XZATOMA_OPENAI_MODEL` for model selection. Remove references to
`OPENAI_HOST` and `OPENAI_TIMEOUT`.

Change 3: Replace the OpenAI entry in the Environment Variables section:

```text
export OPENAI_API_KEY="sk-..."
# Optional
export OPENAI_HOST="https://api.openai.com"
export OPENAI_TIMEOUT="600" # seconds
```

with:

```text
export XZATOMA_OPENAI_API_KEY="sk-..."
export XZATOMA_OPENAI_BASE_URL="https://api.openai.com/v1"  # default; override for local servers
export XZATOMA_OPENAI_MODEL="gpt-4o-mini"   # default model
export XZATOMA_OPENAI_ORG_ID="org-..."      # optional
export XZATOMA_OPENAI_STREAMING="true"      # default
```

Change 4: Add a new sub-section after the Copilot section titled
"OpenAI-Compatible Local Servers" that shows three example configurations: one
for llama.cpp (`http://localhost:8080/v1`), one for vLLM
(`http://localhost:8000/v1`), and one for Mistral.rs
(`http://localhost:1234/v1`). Each example must show the
`XZATOMA_OPENAI_BASE_URL`, `XZATOMA_OPENAI_MODEL`, and
`XZATOMA_OPENAI_API_KEY=""` env vars.

#### Task 4.5: Update docs/reference/models.md

File `docs/reference/models.md` exists at `docs/reference/models.md`. Add a new
`## OpenAI` section at the end of the file. The section must list the default
model (`gpt-4o-mini`), note that the full model list is returned dynamically by
`GET /v1/models`, and provide a reference table of commonly used models with
their approximate context windows:

| Model Name      | Context Window | Notes                   |
| --------------- | -------------- | ----------------------- |
| `gpt-4o-mini`   | 128K tokens    | Default; cost-efficient |
| `gpt-4o`        | 128K tokens    | Full capability         |
| `gpt-4-turbo`   | 128K tokens    | Previous generation     |
| `gpt-3.5-turbo` | 16K tokens     | Fastest; lowest cost    |

Note that for local inference servers (llama.cpp, vLLM, Mistral.rs), the model
name is server-specific and must match the model loaded on the server.

#### Task 4.6: Create How-To Guide for OpenAI-Compatible Providers

Create `docs/how-to/use_openai_compatible_providers.md`. The document must
include step-by-step instructions for:

1. Configuring XZatoma to use OpenAI's hosted API (via env var and via config
   file)
2. Configuring XZatoma to use a local llama.cpp server
3. Configuring XZatoma to use a local vLLM server
4. Configuring XZatoma to use a local Mistral.rs server
5. Setting the API key via env var instead of config file
6. Switching model without changing the config file

Each section must show a complete YAML snippet and the equivalent env var
alternative. Env var examples must use the `XZATOMA_OPENAI_*` prefix.

#### Task 4.7: Deliverables

- `docs/explanation/openai_provider_implementation.md` created
- `docs/reference/configuration.md` updated with openai accepted value and new
  sub-section
- `docs/reference/provider_abstraction.md` updated: comparison table,
  introductory paragraph, file layout, and environment variables section
- `docs/how-to/configure_providers.md` updated: corrected env var names,
  corrected Quickstart, new local server sub-section
- `docs/reference/models.md` updated with OpenAI section
- `docs/how-to/use_openai_compatible_providers.md` created

#### Task 4.8: Success Criteria

- All created and modified Markdown files pass
  `markdownlint --fix --config .markdownlint.json <file>` without errors after
  fix
- All created and modified Markdown files pass
  `prettier --write --parser markdown --prose-wrap always <file>` without errors
- The how-to guide `use_openai_compatible_providers.md` contains at least one
  complete YAML snippet per section
- `docs/reference/configuration.md` no longer lists `copilot` and `ollama` as
  the only accepted provider type values

---

## Implementation Order

Phases must be executed in strict sequence. A later phase must not begin until
all quality gates for the previous phase pass.

| Order | Phase                            | Blocking Gate                                                          |
| ----- | -------------------------------- | ---------------------------------------------------------------------- |
| 1     | Phase 1: Configuration Schema    | `cargo fmt`, `cargo check`, `cargo clippy`, `cargo test` all pass      |
| 2     | Phase 2: Provider Implementation | `cargo fmt`, `cargo check`, `cargo clippy`, `cargo test` all pass      |
| 3     | Phase 3: Factory Registration    | `cargo fmt`, `cargo check`, `cargo clippy`, `cargo test` all pass      |
| 4     | Phase 4: Documentation           | All Markdown lint and format checks pass on all created/modified files |

## File Inventory

The following files must be created or modified as part of this plan. No other
files may be modified unless a compilation error requires it.

| Action | File                                                 | Reason                                                                                                           |
| ------ | ---------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| Modify | `src/config.rs`                                      | Add `OpenAIConfig`, add `openai` field to `ProviderConfig`, update validation, add env var mappings, add 6 tests |
| Modify | `config/config.yaml`                                 | Update provider type comment; add commented-out openai block                                                     |
| Create | `src/providers/openai.rs`                            | New provider implementation with full `Provider` trait                                                           |
| Modify | `src/providers/mod.rs`                               | Export `OpenAIProvider`, add `"openai"` arms in both factory functions, update doc comments, update tests        |
| Modify | `src/lib.rs`                                         | Update module doc comment to list OpenAI as an implemented provider                                              |
| Create | `config/openai_config.yaml`                          | Standalone example config for OpenAI and local inference servers                                                 |
| Create | `docs/explanation/openai_provider_implementation.md` | Implementation summary following Diataxis explanation format                                                     |
| Modify | `docs/reference/configuration.md`                    | Add `openai` to accepted values; add OpenAI configuration sub-section                                            |
| Modify | `docs/reference/provider_abstraction.md`             | Update comparison table, introductory paragraph, file layout, and env var section                                |
| Modify | `docs/how-to/configure_providers.md`                 | Correct wrong env var names; add local server sub-section                                                        |
| Modify | `docs/reference/models.md`                           | Add OpenAI section with model reference table                                                                    |
| Create | `docs/how-to/use_openai_compatible_providers.md`     | New how-to guide for OpenAI and compatible local servers                                                         |

## Quality Gates

All of the following commands must pass before any phase is marked complete.

```text
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
markdownlint --fix --config .markdownlint.json <file>
prettier --write --parser markdown --prose-wrap always <file>
```

Test coverage for `src/providers/openai.rs` and the new blocks in
`src/config.rs` and `src/providers/mod.rs` must exceed 80%.
