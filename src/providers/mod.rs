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
pub mod capabilities;
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
    CompletionResponse, FinishReason, FunctionCall, ImagePromptError, ImagePromptPart,
    ImagePromptSource, Message, ModelCapability, ModelInfo, ModelInfoSummary,
    MultimodalPromptInput, PromptInputError, PromptInputPart, ProviderCapabilities,
    ProviderFunction, ProviderFunctionCall, ProviderImagePromptPart, ProviderImagePromptSource,
    ProviderMessage, ProviderMessageContentPart, ProviderMessageContentParts, ProviderPromptInput,
    ProviderPromptInputPart, ProviderRequest, ProviderTextPromptPart, ProviderTool,
    ProviderToolCall, TextPromptPart, TokenUsage, ToolCall, convert_tools_from_json,
    messages_contain_image_content, validate_message_sequence,
};

// ---------------------------------------------------------------------------
// Provider trait (from trait_mod.rs)
// ---------------------------------------------------------------------------

pub use trait_mod::Provider;

// ---------------------------------------------------------------------------
// Factory (from factory.rs)
// ---------------------------------------------------------------------------

pub use factory::{ProviderFactory, create_provider, create_provider_with_override};

// ---------------------------------------------------------------------------
// Cache helpers (from cache.rs)
// ---------------------------------------------------------------------------

pub use cache::{MODEL_CACHE_TTL_SECS, ModelCache, is_cache_valid, new_model_cache};
pub use capabilities::{
    ollama_model_supports_vision, openai_model_supports_vision, provider_model_supports_vision,
};

// ---------------------------------------------------------------------------
// Provider implementations
// ---------------------------------------------------------------------------

pub use copilot::CopilotProvider;
pub use ollama::OllamaProvider;
pub use openai::OpenAIProvider;
