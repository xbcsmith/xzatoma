//! Compatibility re-export shim for `src/providers/base.rs`.
//!
//! The types and trait that previously lived in this file have been moved to
//! dedicated submodules:
//!
//! | Item                   | New canonical location            |
//! | ---------------------- | --------------------------------- |
//! | Domain types           | `crate::providers::types`         |
//! | `Provider` trait       | `crate::providers::trait_mod`     |
//!
//! This file re-exports everything so that existing code paths
//! (e.g. `use xzatoma::providers::base::ProviderTool`) continue to compile
//! without modification while the rest of the codebase migrates to the new
//! canonical paths.
//!
//! Prefer importing from `crate::providers` directly rather than from this
//! module.

pub use super::trait_mod::Provider;
pub use super::types::{
    convert_tools_from_json, messages_contain_image_content, validate_message_sequence,
    CompletionResponse, FunctionCall, ImagePromptPart, ImagePromptSource, Message, ModelCapability,
    ModelInfo, ModelInfoSummary, MultimodalPromptInput, PromptInputPart, ProviderCapabilities,
    ProviderFunction, ProviderFunctionCall, ProviderImagePromptPart, ProviderImagePromptSource,
    ProviderMessage, ProviderMessageContentPart, ProviderMessageContentParts, ProviderPromptInput,
    ProviderPromptInputPart, ProviderRequest, ProviderTextPromptPart, ProviderTool,
    ProviderToolCall, TextPromptPart, TokenUsage, ToolCall,
};
