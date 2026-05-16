//! Provider module for XZatoma
//!
//! This module contains the AI provider abstraction and implementations
//! for GitHub Copilot, Ollama, and OpenAI.
//!
//! ## Module Layout
//!
//! | Submodule      | Contents                                              |
//! | -------------- | ----------------------------------------------------- |
//! | `types`        | All shared domain types and wire-format structs       |
//! | `trait_mod`    | The `Provider` trait                                  |
//! | `factory`      | `ProviderFactory` and backward-compatible free funcs  |
//! | `copilot`      | GitHub Copilot provider implementation                |
//! | `ollama`       | Ollama provider implementation                        |
//! | `openai`       | OpenAI provider implementation                        |

pub mod cache;
pub mod copilot;
pub mod factory;
pub mod ollama;
pub mod openai;
pub mod trait_mod;
pub mod types;

// ---------------------------------------------------------------------------
// Domain types (from types.rs)
// ---------------------------------------------------------------------------

pub use types::{
    convert_tools_from_json, messages_contain_image_content, validate_message_sequence,
    CompletionResponse, FinishReason, FunctionCall, ImagePromptError, ImagePromptPart,
    ImagePromptSource, Message, ModelCapability, ModelInfo, ModelInfoSummary,
    MultimodalPromptInput, PromptInputError, PromptInputPart, ProviderCapabilities,
    ProviderFunction, ProviderFunctionCall, ProviderImagePromptPart, ProviderImagePromptSource,
    ProviderMessage, ProviderMessageContentPart, ProviderMessageContentParts, ProviderPromptInput,
    ProviderPromptInputPart, ProviderRequest, ProviderTextPromptPart, ProviderTool,
    ProviderToolCall, TextPromptPart, TokenUsage, ToolCall,
};

// ---------------------------------------------------------------------------
// Provider trait (from trait_mod.rs)
// ---------------------------------------------------------------------------

pub use trait_mod::Provider;

// ---------------------------------------------------------------------------
// Factory (from factory.rs)
// ---------------------------------------------------------------------------

pub use factory::{create_provider, create_provider_with_override, ProviderFactory};

// ---------------------------------------------------------------------------
// Cache helpers (from cache.rs)
// ---------------------------------------------------------------------------

pub use cache::{is_cache_valid, new_model_cache, ModelCache, MODEL_CACHE_TTL_SECS};

// ---------------------------------------------------------------------------
// Provider implementations
// ---------------------------------------------------------------------------

pub use copilot::CopilotProvider;
pub use ollama::OllamaProvider;
pub use openai::OpenAIProvider;
