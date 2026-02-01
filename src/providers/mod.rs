//! Provider module for XZatoma
//!
//! This module contains the AI provider abstraction and implementations
//! for GitHub Copilot and Ollama.

// Phase 1: Allow unused code for placeholder implementations
#![allow(dead_code)]
#![allow(unused_imports)]

pub mod base;
pub mod copilot;
pub mod ollama;

pub use base::{
    validate_message_sequence, CompletionResponse, FunctionCall, Message, ModelCapability,
    ModelInfo, ModelInfoSummary, Provider, ProviderCapabilities, TokenUsage, ToolCall,
};
pub use copilot::CopilotProvider;
pub use ollama::OllamaProvider;

use crate::error::Result;

/// Create a provider instance based on configuration
///
/// # Arguments
///
/// * `provider_type` - Type of provider ("copilot" or "ollama")
/// * `config` - Provider configuration
///
/// # Returns
///
/// Returns a boxed provider instance
///
/// # Errors
///
/// Returns error if provider type is invalid or initialization fails
pub fn create_provider(
    provider_type: &str,
    config: &crate::config::ProviderConfig,
) -> Result<Box<dyn Provider>> {
    match provider_type {
        "copilot" => Ok(Box::new(copilot::CopilotProvider::new(
            config.copilot.clone(),
        )?)),
        "ollama" => Ok(Box::new(ollama::OllamaProvider::new(
            config.ollama.clone(),
        )?)),
        _ => Err(crate::error::XzatomaError::Provider(format!(
            "Unknown provider type: {}",
            provider_type
        ))
        .into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CopilotConfig, OllamaConfig, ProviderConfig};

    #[test]
    fn test_create_provider_invalid_type() {
        let config = ProviderConfig {
            provider_type: "invalid".to_string(),
            copilot: CopilotConfig::default(),
            ollama: OllamaConfig::default(),
        };

        let result = create_provider("invalid", &config);
        assert!(result.is_err());
    }
}
