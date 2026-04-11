# Provider Trait Phase 2 Implementation

## Overview

Phase 2 of the Provider trait redesign formalises the contract between XZatoma
and its AI provider backends. The changes make authentication status and model
management first-class concerns of the trait, remove a footgun (async validation
inside `set_model`), and introduce two new streaming-related default methods.
All existing provider implementations and downstream callers have been updated
to satisfy the new contract.

---

## Motivation

The Phase 1 `Provider` trait had several weaknesses:

- `get_current_model` returned `Result<String>`, forcing every call site to
  handle an error that was almost always impossible in practice.
- `set_model` was `async` and performed remote API validation, mixing a
  side-effectful network call into what should be a simple in-memory setter.
  This made it impossible to call from synchronous code and made callers
  responsible for a poorly-defined two-step flow.
- There was no standard way to ask a provider whether it held valid credentials.
- There was no distinction between "the authoritative network fetch" and "the
  cache-aware list accessor", so `list_models` overrides mixed caching concerns
  with fetching concerns inside a single method.
- Streaming support was not visible through the trait interface.

---

## Changes to the Trait

### Required Methods Added

Four methods are now required on every `Provider` implementation.

#### `fn is_authenticated(&self) -> bool`

Returns `true` when the provider holds valid stored credentials. The semantics
are provider-specific:

- `CopilotProvider` checks whether the system keyring contains a non-expired
  Copilot token (expiry threshold: 300 seconds of headroom).
- `OpenAIProvider` checks whether `config.api_key` is non-empty. Local servers
  that require no auth will correctly return `false`; this is expected and is
  not an error condition.
- `OllamaProvider` always returns `true` because Ollama requires no credentials.

#### `fn current_model(&self) -> Option<&str>`

Returns a borrowed reference to the currently active model name, or `None` if no
model is configured. Providers that store the model name behind a `RwLock` (all
three built-in providers) cannot return a reference that outlives the lock
guard, so they return `None` and rely on the `get_current_model` override
instead. Providers that store the model name as a plain field can return
`Some(&self.model)` directly.

#### `fn set_model(&mut self, model: &str)`

Sets the active model name in memory. No network validation is performed.
Callers that need to verify that a model exists should call `list_models` first,
inspect the returned list, and then call `set_model` with a known-good name.

This replaces the old
`async fn set_model(&mut self, model_name: String) -> Result<()>` which
performed a full remote validation round-trip. The validation logic was moved to
the responsibility of the caller to keep `set_model` simple, synchronous, and
infallible.

All three built-in providers implement this as a write-lock acquire on their
inner `RwLock<ProviderConfig>`, assignment of `config.model`, and lock release.

#### `async fn fetch_models(&self) -> Result<Vec<ModelInfo>>`

Fetches the list of available models from the remote API unconditionally. This
is the canonical implementation method. The default `list_models` delegates to
`fetch_models`, allowing providers to satisfy the listing contract by
implementing only this method.

Providers that want a cache in front of `fetch_models` (Ollama, OpenAI) keep
their own `list_models` override, which checks the cache and calls their private
`fetch_models_from_api` helper (Ollama) or an inline fetch path (OpenAI). Their
`fetch_models` implementation therefore delegates back to the overridden
`list_models`.

The Copilot provider implements `fetch_models` as a direct call to
`fetch_copilot_models`, removing the old `list_models` override entirely.

### Required Method Removed

`async fn set_model(&mut self, model_name: String) -> Result<()>` was the old
default-returns-error method. It is replaced by the synchronous required method
described above.

### Provided Methods Changed

#### `fn get_current_model(&self) -> String`

Previously `fn get_current_model(&self) -> Result<String>`. The return type is
now a plain `String`. The default implementation calls
`self.current_model().unwrap_or("none").to_string()`.

The sentinel value `"none"` is returned when `current_model` returns `None`. All
three built-in providers override `get_current_model` to read from their
internal `RwLock<Config>` and return `"none"` only if the lock is poisoned,
which is not expected during normal operation.

The `# Errors` section has been removed from the doc comment because the method
is now infallible.

#### `async fn list_models(&self) -> Result<Vec<ModelInfo>>`

The default implementation now calls `self.fetch_models().await` instead of
returning an error. Providers that implement `fetch_models` automatically get a
working `list_models` for free.

The doc comment note has been updated: "The default delegates to `fetch_models`.
Providers do NOT override this method; implement `fetch_models` instead."
Providers that need caching in front of the network call (Ollama, OpenAI) are
permitted to keep their `list_models` override as an exception.

### Provided Methods Added

#### `fn supports_streaming(&self) -> bool`

Defaults to `false`. Providers that support SSE streaming should override this
to return `true`. `CopilotProvider` and `OpenAIProvider` both set this to `true`
via their `get_provider_capabilities` return value; the new `supports_streaming`
method surfaces the same information through a simpler interface that does not
require constructing a `ProviderCapabilities` struct.

#### `async fn chat_completion_stream(&self, messages, tools) -> Result<CompletionResponse>`

Defaults to `self.complete(messages, tools).await`. Providers that implement
true SSE streaming may override this to return a streamed response. The default
ensures that code paths which call `chat_completion_stream` work correctly even
against providers that have not implemented SSE streaming.

---

## Implementation Notes for Lock-Based Providers

All three built-in providers store their configuration (including the model
name) inside an `Arc<RwLock<ProviderConfig>>`. This makes implementing
`current_model(&self) -> Option<&str>` impossible: a `&str` returned from within
a lock guard cannot outlive the guard, and the guard is dropped at the end of
the method body.

The chosen solution is:

1. Implement `current_model` to return `None`.
2. Override `get_current_model` to acquire the read lock, clone the model
   string, and return it. The fallback `"none"` is returned only if the lock is
   poisoned.

This approach is consistent across all three providers and is documented in the
method-level comments. Callers that only need the current model name as an owned
`String` should use `get_current_model`. Code that needs a `&str` and can
guarantee the provider stores its model as a plain field may use
`current_model`.

---

## Call Site Migrations

### `get_current_model` Call Sites

Every call site that used `provider.get_current_model()?` or
`provider.get_current_model().unwrap()` has been updated to use the return value
directly as a `String`. Sites that previously used `.ok()` to convert
`Result<String>` into `Option<String>` now explicitly check whether the returned
string equals the sentinel `"none"`:

```xzatoma/src/commands/mod.rs#L683-L696
let current_model: Option<String> = {
    let m = agent.provider().get_current_model();
    if m == "none" {
        None
    } else {
        Some(m)
    }
};
```

Sites that matched on `Ok(model_name)` from a `Result` have been simplified to a
direct binding followed by a conditional or a direct call to
`get_model_info(&model_name)`.

### `set_model` Call Sites

The single production call site in `commands/mod.rs` that previously wrote:

```xzatoma/src/commands/mod.rs#L1379-L1380
new_provider.set_model(&model_info.name);
```

The model validity is guaranteed at that point because `model_info` was obtained
by calling `list_models` earlier in the same function, so removing the
validation from `set_model` does not reduce safety.

---

## Test Updates

### Tests in `trait_mod.rs`

All `MockProvider` structs inside the test module now implement the four new
required methods. Two tests were renamed and their bodies updated:

| Old name                               | New name                                          | Change                                                                               |
| -------------------------------------- | ------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `test_default_get_current_model_error` | `test_default_get_current_model_returns_sentinel` | Asserts the return value equals `"none"` instead of checking `is_err()`.             |
| `test_default_set_model_error`         | `test_default_set_model_noop`                     | Calls `set_model` synchronously and verifies no panic occurs; no `Result` to assert. |

Five new tests were added:

| Test                                                | What it verifies                                                                                                                                                                                                                             |
| --------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `test_is_authenticated_through_trait_ref`           | Two `MockProvider` variants returning `true` and `false` are accessed via `&dyn Provider`; the correct boolean is returned from each.                                                                                                        |
| `test_current_model_through_trait_ref`              | A `ModelProvider { model: String }` struct returns `Some(&self.model)` from `current_model`; boxed as `Box<dyn Provider>`; `current_model()` and `get_current_model()` both return the expected value.                                       |
| `test_set_model_through_trait_ref`                  | A `MutProvider { model: String }` struct has `set_model` update `self.model`; boxed as `Box<dyn Provider>`; calling `set_model("new-model")` via the box and then reading `current_model()` and `get_current_model()` returns `"new-model"`. |
| `test_supports_streaming_default_is_false`          | A minimal `MockProvider` accessed via `&dyn Provider` returns `false` from `supports_streaming`.                                                                                                                                             |
| `test_chat_completion_stream_delegates_to_complete` | A `MockProvider` whose `complete` returns a known response is called via `chat_completion_stream`; the response content matches the known value.                                                                                             |

### Tests in Provider Implementation Files

Provider-specific tests were updated to match the new API:

- `get_current_model().unwrap()` was simplified to `get_current_model()`.
- `set_model(String).await` was replaced with `set_model(&str)`.
- Tests for the old validating `set_model` were converted to tests of the
  in-memory setter (no mock server needed, no `#[ignore]`).

### Integration Test Files

Mock providers in integration test files
(`tests/conversation_persistence_integration.rs`,
`tests/integration_history_tool_integrity.rs`, `tests/mcp_sampling_test.rs`)
received stub implementations of the four new required methods.

`tests/integration_provider_factory.rs` assertions of the form
`provider.get_current_model().is_ok()` were changed to
`!provider.get_current_model().is_empty()`.

---

## Files Modified

| File                                            | Change summary                                                                                                                                   |
| ----------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `src/providers/trait_mod.rs`                    | Full rewrite: new required methods, updated defaults, new tests                                                                                  |
| `src/providers/copilot.rs`                      | Added required methods; changed `get_current_model` and `set_model` signatures; updated tests and doc examples                                   |
| `src/providers/ollama.rs`                       | Added required methods; changed `get_current_model` and `set_model` signatures; added `#[allow(dead_code)]` to `invalidate_cache`; updated tests |
| `src/providers/openai.rs`                       | Added required methods; changed `get_current_model` and `set_model` signatures; updated tests                                                    |
| `src/agent/core.rs`                             | Removed closure fallback on `get_current_model`; added required methods to `MockProvider` in tests                                               |
| `src/commands/models.rs`                        | Removed `?` operator from `get_current_model` call                                                                                               |
| `src/commands/mod.rs`                           | Fixed all `get_current_model` call sites; fixed `set_model` call; added required methods to `TestProvider` in tests                              |
| `src/mcp/sampling.rs`                           | Replaced old Provider overrides with new required methods in `MockProvider`                                                                      |
| `src/tools/subagent.rs`                         | Added required methods to `MockProvider` in tests                                                                                                |
| `tests/conversation_persistence_integration.rs` | Added required methods to `MockProvider`                                                                                                         |
| `tests/integration_history_tool_integrity.rs`   | Added required methods to `TrackingMockProvider`                                                                                                 |
| `tests/integration_provider_factory.rs`         | Fixed `get_current_model().is_ok()` assertions                                                                                                   |
| `tests/mcp_sampling_test.rs`                    | Replaced old Provider overrides with new required methods in `MockProvider`                                                                      |
