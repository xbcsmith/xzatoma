# Phase 4 Task 4.3: Provider Cache Consolidation Implementation

## Summary

This document describes the consolidation of identical model cache type aliases
and TTL helpers from the Copilot, OpenAI, and Ollama provider modules into a
single shared `providers/cache.rs` module.

## Problem

Three provider modules each contained their own copy of the same cache
infrastructure:

| Location                | Duplicate                                                          |
| ----------------------- | ------------------------------------------------------------------ |
| `openai.rs` line 46     | `type ModelCache = Arc<RwLock<Option<(Vec<ModelInfo>, Instant)>>>` |
| `ollama.rs` line 51     | `type ModelCache = Arc<RwLock<Option<(Vec<ModelInfo>, Instant)>>>` |
| `openai.rs` list_models | `cached_at.elapsed() < Duration::from_secs(300)` inline            |
| `ollama.rs` impl block  | `fn is_cache_valid(cached_at: Instant) -> bool { ... }`            |
| `copilot.rs` line 107   | `const MODEL_CACHE_DURATION: Duration = Duration::from_secs(300)`  |

The magic number `300` appeared in three different places with no shared source
of truth. Any change to the TTL required three separate edits.

## Solution

A new `providers/cache.rs` module centralises all shared cache definitions:

- `ModelCache` type alias (`Arc<RwLock<Option<(Vec<ModelInfo>, Instant)>>>`)
- `MODEL_CACHE_TTL_SECS` constant (`u64 = 300`)
- `new_model_cache()` constructor function
- `is_cache_valid(cached_at: Instant) -> bool` helper function

The module is declared as `pub mod cache` in `providers/mod.rs` and its public
items are re-exported at the `providers` level.

## Files Changed

### New file: `src/providers/cache.rs`

Defines the canonical `ModelCache` type alias, `MODEL_CACHE_TTL_SECS` constant,
`new_model_cache()` constructor, and `is_cache_valid()` helper. All public items
carry full `///` doc comments with runnable examples. Includes five unit tests
covering: empty initial state, fresh validity, expired validity, TTL constant
value, and Arc pointer equality.

### Modified: `src/providers/mod.rs`

Added `pub mod cache;` to the module declarations block. Added a re-export block
that surfaces `ModelCache`, `MODEL_CACHE_TTL_SECS`, `new_model_cache`, and
`is_cache_valid` at the `crate::providers` level.

### Modified: `src/providers/openai.rs`

- Removed local `type ModelCache = ...` alias.
- Added
  `use crate::providers::cache::{is_cache_valid, new_model_cache, ModelCache};`.
- Replaced `Arc::new(RwLock::new(None))` in `new()` with `new_model_cache()`.
- Replaced inline `cached_at.elapsed() < Duration::from_secs(300)` in
  `list_models` with `is_cache_valid(*cached_at)`.

### Modified: `src/providers/ollama.rs`

- Removed local `type ModelCache = ...` alias.
- Added
  `use crate::providers::cache::{is_cache_valid, new_model_cache, ModelCache};`.
- Replaced `Arc::new(RwLock::new(None))` in `new()` with `new_model_cache()`.
- Removed `fn is_cache_valid(cached_at: Instant) -> bool` from
  `impl OllamaProvider`.
- Replaced `Self::is_cache_valid(*cached_at)` in `list_models` and
  `get_model_info` with the module-level `is_cache_valid(*cached_at)`.
- Updated `test_is_cache_valid_fresh` and `test_is_cache_valid_expired` to call
  the module-level `is_cache_valid` instead of `OllamaProvider::is_cache_valid`.

### Modified: `src/providers/copilot.rs`

- Added `use crate::providers::cache::MODEL_CACHE_TTL_SECS;`.
- Changed `const MODEL_CACHE_DURATION: Duration = Duration::from_secs(300)` to
  `Duration::from_secs(MODEL_CACHE_TTL_SECS)`.
- The `CopilotCache` struct and its `is_valid` method are left unchanged; they
  continue to reference the local `MODEL_CACHE_DURATION` constant.

## Design Decisions

### Why a dedicated `cache.rs` file instead of inlining in `types.rs`?

`types.rs` owns domain types such as `Message`, `ModelInfo`, and
`CompletionResponse`. Cache infrastructure is a provider-internal concern rather
than a domain type. Separating them keeps each file's responsibility narrow.

### Why not move `CopilotCache` into `cache.rs`?

`CopilotCache` also holds the raw `CopilotModelData` list and a validity method
tied to Copilot-specific logic. Moving it would introduce a Copilot-specific
type into the shared cache module, violating the principle of keeping shared
modules generic. Tying the TTL to `MODEL_CACHE_TTL_SECS` via the constant is
sufficient to align the behaviour.

### Why keep `Arc` and `RwLock` imports in provider files?

`Arc<RwLock<OllamaConfig>>` and `Arc<RwLock<OpenAIConfig>>` are still used for
the config field in each provider struct. The imports remain to avoid breaking
those usages.

## Quality Gate Results

All four mandatory gates passed after the changes:

```text
cargo fmt --all                                           -- ok
cargo check --all-targets --all-features                 -- ok
cargo clippy --all-targets --all-features -- -D warnings -- ok
cargo test --lib --all-features providers                -- 309 passed, 0 failed
```
