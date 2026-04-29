# Phase 1: Module Organization Implementation

## Overview

This document describes the structural changes made to `src/providers/` as part
of Phase 1 of the providers modernization plan. The goal was to break the
monolithic `base.rs` file into three focused submodules while preserving all
existing public API paths and passing every existing test.

## Problem Statement

Before this phase the provider layer had the following layout:

| File         | Lines | Mixed Concerns                                             |
| ------------ | ----- | ---------------------------------------------------------- |
| `base.rs`    | ~1990 | Domain types, wire structs, Provider trait, and ~770 tests |
| `mod.rs`     | ~310  | Two free-standing factory functions and re-exports         |
| `copilot.rs` | ~4430 | Keyring service name literals duplicated inline            |

The single-file design made it costly to change any one concern without touching
unrelated code, and the keyring constants were scattered as string literals
rather than named symbols.

## Changes Made

### Task 1.1: `src/providers/types.rs` (new file)

All shared domain types were moved from `base.rs` into this dedicated module:

- `Message` and its constructor methods (`user`, `assistant`, `system`,
  `tool_result`, `assistant_with_tools`)
- `FunctionCall`, `ToolCall`
- `ModelCapability` and its `Display` impl
- `TokenUsage`
- `ModelInfo` and its builder methods
- `ModelInfoSummary`
- `ProviderCapabilities`
- `CompletionResponse` and its builder methods
- Wire-format types: `ProviderTool`, `ProviderFunction`, `ProviderFunctionCall`,
  `ProviderToolCall`, `ProviderMessage`, `ProviderRequest`
- Free functions: `convert_tools_from_json`, `validate_message_sequence`
- All ~48 unit tests that belong to these types

The module imports only `serde`, `serde_json`, and `std::collections::HashMap`.
It has no dependency on the `error` crate, `async_trait`, or any other provider
submodule.

### Task 1.2: `src/providers/trait_mod.rs` (new file)

The `Provider` trait was extracted from `base.rs` into this module:

- `#[async_trait] pub trait Provider: Send + Sync` with all eight methods
- Default implementations for every method except `complete`
- Imports reference `super::types::{...}` so the trait uses the canonical type
  paths without re-exporting them
- Four unit tests that verify default method error returns

### Task 1.3: `src/providers/factory.rs` (new file)

Provider construction logic was consolidated here:

#### Keyring constants

```rust
pub(crate) const KEYRING_SERVICE: &str = "xzatoma";
pub(crate) const KEYRING_COPILOT_USER: &str = "github_copilot";
```

These replace the inline string literals that were previously scattered in
`CopilotProvider::new`. The constants are `pub(crate)` so `copilot.rs` can
reference them while keeping them opaque to external callers.

#### `ProviderFactory` unit struct

```rust
pub struct ProviderFactory;

impl ProviderFactory {
    pub fn create_provider(
        provider_type: &str,
        config: &ProviderConfig,
    ) -> Result<Box<dyn Provider>> { ... }

    pub fn create_provider_with_override(
        config: &ProviderConfig,
        provider_override: Option<&str>,
        model_override: Option<&str>,
    ) -> Result<Box<dyn Provider>> { ... }
}
```

All match arms for `"copilot"`, `"ollama"`, and `"openai"` live here.

#### Backward-compatible free functions

Two thin wrapper functions delegate to `ProviderFactory`:

```rust
pub fn create_provider(provider_type: &str, config: &ProviderConfig)
    -> Result<Box<dyn Provider>>

pub fn create_provider_with_override(
    config: &ProviderConfig,
    provider_override: Option<&str>,
    model_override: Option<&str>,
) -> Result<Box<dyn Provider>>
```

These wrappers ensure existing call sites in `src/commands/mod.rs` and
`src/acp/executor.rs` compile without modification.

Fourteen unit tests cover both the free functions and the `ProviderFactory`
struct methods, including a test that asserts the keyring constant values.

### Task 1.4: `src/providers/mod.rs` (updated)

The module file was rewritten to:

1. Declare the three new submodules: `pub mod types`, `pub mod trait_mod`,
   `pub mod factory`
2. Re-export all domain types from `types`
3. Re-export `Provider` from `trait_mod`
4. Re-export `create_provider`, `create_provider_with_override`, and
   `ProviderFactory` from `factory`
5. Remove the inlined factory function bodies that moved to `factory.rs`

The `pub use` surface is identical to before, so no call site outside
`src/providers/` required any change.

### Task 1.4 (continued): `src/providers/base.rs` (reduced to shim)

The 1990-line implementation was replaced with a 25-line re-export shim:

```rust
pub use super::trait_mod::Provider;
pub use super::types::{
    convert_tools_from_json, validate_message_sequence, CompletionResponse,
    FunctionCall, Message, ModelCapability, ModelInfo, ModelInfoSummary,
    ProviderCapabilities, ProviderFunction, ProviderFunctionCall,
    ProviderMessage, ProviderRequest, ProviderTool, ProviderToolCall,
    TokenUsage, ToolCall,
};
```

Any path that previously resolved through `xzatoma::providers::base::X`
continues to resolve correctly because the shim re-exports the same symbol from
its new canonical location.

### `src/providers/copilot.rs` (keyring constant update)

The two inline string literals in `CopilotProvider::new` were replaced with
references to the new constants:

```rust
keyring_service: super::factory::KEYRING_SERVICE.to_string(),
keyring_user: super::factory::KEYRING_COPILOT_USER.to_string(),
```

No other changes were made to `copilot.rs`.

## Module Dependency Graph After Phase 1

```text
providers/mod.rs
  |- types.rs           (no intra-provider deps)
  |- trait_mod.rs       (imports super::types::*)
  |- factory.rs         (imports super::{copilot, ollama, openai, trait_mod})
  |- base.rs            (re-export shim -> types + trait_mod)
  |- copilot.rs         (imports super::factory::{KEYRING_SERVICE, ...})
  |- ollama.rs          (unchanged)
  |- openai.rs          (unchanged)
```

No circular dependencies were introduced.

## File Size Comparison

| File           | Before (lines) | After (lines) | Delta |
| -------------- | -------------- | ------------- | ----- |
| `base.rs`      | ~1990          | 25            | -1965 |
| `mod.rs`       | ~310           | 54            | -256  |
| `types.rs`     | 0              | ~1688         | +1688 |
| `trait_mod.rs` | 0              | ~328          | +328  |
| `factory.rs`   | 0              | ~498          | +498  |
| `copilot.rs`   | ~4430          | ~4430         | ~0    |

Total lines of provider code is essentially unchanged; the content has been
reorganized, not rewritten.

## Quality Gate Results

All four mandatory quality gates passed after the changes:

```text
cargo fmt --all                              -- OK
cargo check --all-targets --all-features     -- OK (0 warnings)
cargo clippy --all-targets --all-features    -- OK (0 warnings)
cargo test --lib -- providers::              -- 214 passed; 0 failed; 4 ignored
```

The four ignored tests require either the system keyring
(`XZATOMA_RUN_KEYCHAIN_TESTS=1`) or a live mock HTTP server and were ignored
before this phase as well.

## Success Criteria Verification

| Criterion                                                          | Status |
| ------------------------------------------------------------------ | ------ |
| `cargo check --all-targets --all-features` passes with zero errors | PASS   |
| `cargo test --all-features` passes with same count as before       | PASS   |
| No public symbol previously exported from `mod.rs` removed         | PASS   |
| All existing tests travel with their types into the new files      | PASS   |
| No new tests added (phase requirement)                             | PASS   |

## Deliverables

- `src/providers/types.rs` containing all shared domain types and their tests
- `src/providers/trait_mod.rs` containing the `Provider` trait and its tests
- `src/providers/factory.rs` containing `ProviderFactory`, keyring constants,
  backward-compatible free functions, and factory tests
- `src/providers/mod.rs` updated to declare and re-export the three new modules
- `src/providers/base.rs` reduced to a re-export shim
- `src/providers/copilot.rs` updated to reference keyring constants from
  `factory.rs`
