# Ollama Vision Capability Fix Implementation

## Problem

In ACP agent mode, when Zed connected xzatoma to an Ollama model that supports
vision, xzatoma incorrectly rejected image prompts with:

```text
provider 'ollama' with model '<name>' does not support image input
```

This happened even for models that the Ollama server itself reported as
vision-capable via its `/api/show` endpoint (e.g. `llama3.2-vision`,
`minicpm-v3`, or any third-party multimodal model not on the static name list).

## Root Cause

Vision capability was checked in three places, all using a **static name-based
allowlist** (`ollama_model_supports_vision` in `src/providers/capabilities.rs`):

```text
"vision" -> model.contains("llava") || model.contains("bakllava") || ...
```

1. **ACP pre-flight check**
   (`src/acp/prompt_input.rs::validate_provider_supports_prompt_input`): Called
   from `enqueue_prompt` in `stdio.rs` before the prompt is even queued. Used
   `provider_model_supports_vision` (static allowlist). Any model not on the
   list was rejected here before reaching the provider.

2. **Ollama `complete()`** (`src/providers/ollama.rs`): Checked
   `ollama_model_supports_vision` (static allowlist).

3. **Ollama `complete_streaming_with_callbacks()`** (`src/providers/ollama.rs`):
   Same static check.

Meanwhile, `build_model_info_from_show_response` in `ollama.rs` **correctly**
read the `capabilities` array from the Ollama `/api/show` response and stored
`ModelCapability::Vision` in the `ModelInfo` cached in `model_cache`. This cache
was never consulted for the vision gating decision.

## Fix

### 1. Cache-aware vision check in `OllamaProvider`

Added `fn model_has_vision_capability(&self, model: &str) -> bool` to
`OllamaProvider`:

- Reads the `Arc<RwLock<model_cache>>` (non-blocking, no network call).
- If the model is found in the cache and its `ModelInfo` carries
  `ModelCapability::Vision`, returns `true` unconditionally.
- Falls back to the static `ollama_model_supports_vision` allowlist on cache
  miss (e.g. before the first `list_models` call) or lock failure.

Updated `complete()` and `complete_streaming_with_callbacks()` to use this
helper instead of the static allowlist.

### 2. `model_supports_vision` method on the `Provider` trait

Added a default
`fn model_supports_vision(&self, provider_name: &str, model_name: &str) -> bool`
to the `Provider` trait in `src/providers/trait_mod.rs`. The default delegates
to the static `provider_model_supports_vision` allowlist, preserving existing
behaviour for Copilot and OpenAI providers.

`OllamaProvider` overrides this method to delegate to
`model_has_vision_capability`, so any call through `&dyn Provider` also gets
cache-aware results.

### 3. Runtime override in the ACP pre-flight check

Changed `validate_provider_supports_prompt_input` signature to accept
`vision_override: Option<bool>`:

```rust
pub fn validate_provider_supports_prompt_input(
    provider_name: &str,
    model_name: &str,
    input: &MultimodalPromptInput,
    vision_override: Option<bool>,
) -> Result<()>
```

When `vision_override` is `Some(v)`, that value is used directly. When it is
`None`, the function falls back to the static allowlist as before.

In `enqueue_prompt` (`src/acp/stdio.rs`), before calling the validation
function, the code now tries to read the runtime vision capability from the
provider's model cache using non-blocking `try_lock`:

```rust
let vision_override = session
    .try_lock()
    .ok()
    .and_then(|session_guard| {
        session_guard.xzatoma_agent.try_lock().ok().map(|agent_guard| {
            agent_guard
                .provider()
                .model_supports_vision(&provider_name, &model_name)
        })
    });
```

`try_lock` is non-blocking: if the agent mutex is held by the prompt worker
(i.e. a previous prompt is still executing), the override is `None` and the
static allowlist is used as a fallback. In the common case (agent idle when a
new prompt arrives) the lock succeeds and the correct cached value is used.

## Files Changed

| File                         | Change                                                                                                                                                                                                                   |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `src/providers/ollama.rs`    | Added `model_has_vision_capability` helper; updated `complete` and `complete_streaming_with_callbacks` to use it; added `model_supports_vision` override in `impl Provider for OllamaProvider`; added two new unit tests |
| `src/providers/trait_mod.rs` | Added default `model_supports_vision` method to `Provider` trait                                                                                                                                                         |
| `src/acp/prompt_input.rs`    | Added `vision_override: Option<bool>` parameter to `validate_provider_supports_prompt_input`; updated two existing tests to pass `None`; added two new tests covering the override behaviour                             |
| `src/acp/stdio.rs`           | In `enqueue_prompt`, populate `vision_override` via non-blocking `try_lock` on the session agent and pass it to `validate_provider_supports_prompt_input`                                                                |

## Invariants Preserved

- The static allowlist remains the authoritative fallback when the model cache
  is unpopulated.
- No network calls are made in the vision gating paths.
- Copilot and OpenAI providers are unaffected (their `model_supports_vision`
  default delegates to the existing static allowlist logic).
- The `--skip providers::copilot --skip mcp::auth` test constraint is respected;
  no new keyring access is introduced.
