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

use crate::config::{CopilotConfig, OllamaConfig, ProviderConfig};
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

/// Create a provider instance with optional overrides for subagents
///
/// This factory function creates provider instances with configurable
/// provider type and model overrides. It is primarily used for subagent
/// instantiation where the subagent may need a different provider or model
/// than the parent agent.
///
/// # Arguments
///
/// * `config` - Full provider configuration containing all provider settings
/// * `provider_override` - Optional provider type override ("copilot" or "ollama")
/// * `model_override` - Optional model name override
///
/// # Returns
///
/// Returns a boxed provider instance configured with the specified or default settings
///
/// # Errors
///
/// Returns error if:
/// - Provider type is invalid
/// - Provider initialization fails (authentication, network, etc.)
/// - Model override is specified without provider override
///
/// # Examples
///
/// ```no_run
/// use xzatoma::providers::create_provider_with_override;
/// use xzatoma::config::{ProviderConfig, CopilotConfig, OllamaConfig};
///
/// # fn example() -> xzatoma::error::Result<()> {
/// let config = ProviderConfig {
///     provider_type: "copilot".to_string(),
///     copilot: CopilotConfig::default(),
///     ollama: OllamaConfig::default(),
/// };
///
/// // Use default provider from config
/// let default_provider = create_provider_with_override(&config, None, None)?;
///
/// // Override to use Ollama instead
/// let ollama_provider = create_provider_with_override(
///     &config,
///     Some("ollama"),
///     None,
/// )?;
///
/// // Override provider and model
/// let custom_provider = create_provider_with_override(
///     &config,
///     Some("ollama"),
///     Some("llama3.2:latest"),
/// )?;
/// # Ok(())
/// # }
/// ```
pub fn create_provider_with_override(
    config: &ProviderConfig,
    provider_override: Option<&str>,
    model_override: Option<&str>,
) -> Result<Box<dyn Provider>> {
    // Determine which provider to use
    let provider_type = provider_override.unwrap_or(&config.provider_type);

    match provider_type {
        "copilot" => {
            // Create Copilot config with optional model override
            let mut copilot_config = config.copilot.clone();
            if let Some(model) = model_override {
                copilot_config.model = model.to_string();
            }

            Ok(Box::new(CopilotProvider::new(copilot_config)?))
        }
        "ollama" => {
            // Create Ollama config with optional model override
            let mut ollama_config = config.ollama.clone();
            if let Some(model) = model_override {
                ollama_config.model = model.to_string();
            }

            Ok(Box::new(OllamaProvider::new(ollama_config)?))
        }
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

    #[test]
    fn test_create_provider_with_override_default() {
        let config = ProviderConfig {
            provider_type: "copilot".to_string(),
            copilot: CopilotConfig {
                model: "gpt-5-mini".to_string(),
                api_base: None,
            },
            ollama: OllamaConfig::default(),
        };

        // No overrides - should use config defaults
        let result = create_provider_with_override(&config, None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_provider_with_override_provider_only() {
        let config = ProviderConfig {
            provider_type: "copilot".to_string(),
            copilot: CopilotConfig::default(),
            ollama: OllamaConfig::default(),
        };

        // Override provider to ollama
        let result = create_provider_with_override(&config, Some("ollama"), None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_provider_with_override_provider_and_model() {
        let config = ProviderConfig {
            provider_type: "copilot".to_string(),
            copilot: CopilotConfig::default(),
            ollama: OllamaConfig::default(),
        };

        // Override both provider and model
        let result = create_provider_with_override(&config, Some("ollama"), Some("llama3.2:1b"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_provider_with_override_model_only() {
        let config = ProviderConfig {
            provider_type: "copilot".to_string(),
            copilot: CopilotConfig {
                model: "gpt-5-mini".to_string(),
                api_base: None,
            },
            ollama: OllamaConfig::default(),
        };

        // Override model only (uses config provider type)
        let result = create_provider_with_override(&config, None, Some("gpt-3.5-turbo"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_provider_with_override_invalid_provider() {
        let config = ProviderConfig {
            provider_type: "copilot".to_string(),
            copilot: CopilotConfig::default(),
            ollama: OllamaConfig::default(),
        };

        // Invalid provider override
        let result = create_provider_with_override(&config, Some("invalid"), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_provider_with_override_copilot_model() {
        let config = ProviderConfig {
            provider_type: "copilot".to_string(),
            copilot: CopilotConfig {
                model: "gpt-5-mini".to_string(),
                api_base: None,
            },
            ollama: OllamaConfig::default(),
        };

        // Override to copilot with custom model
        let result = create_provider_with_override(&config, Some("copilot"), Some("gpt-3.5-turbo"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_provider_with_override_ollama_model() {
        let config = ProviderConfig {
            provider_type: "copilot".to_string(),
            copilot: CopilotConfig::default(),
            ollama: OllamaConfig {
                host: "http://localhost:11434".to_string(),
                model: "llama3.2:latest".to_string(),
            },
        };

        // Override to ollama with custom model
        let result = create_provider_with_override(&config, Some("ollama"), Some("gemma2:2b"));
        assert!(result.is_ok());
    }
}
