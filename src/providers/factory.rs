//! Provider factory for XZatoma.
//!
//! This module contains [`ProviderFactory`], a unit struct whose associated
//! functions create boxed [`Provider`] instances from a provider-type string
//! and a [`ProviderConfig`]. It is the single authoritative place for:
//!
//! - Constructing any supported provider
//! - Storing the keyring service and user constants used for credential storage
//!
//! Free-function wrappers (`create_provider` and `create_provider_with_override`)
//! are re-exported from this module so that existing call sites do not need to
//! change.

use crate::config::ProviderConfig;
use crate::error::Result;

use super::copilot::CopilotProvider;
use super::ollama::OllamaProvider;
use super::openai::OpenAIProvider;
use super::trait_mod::Provider;

// ---------------------------------------------------------------------------
// Keyring credential constants
// ---------------------------------------------------------------------------

/// System keyring service name used for all XZatoma credential storage.
///
/// Centralised here so that every module that reads from or writes to the
/// keyring uses an identical service name. Currently used by
/// [`CopilotProvider`] to persist OAuth tokens.
pub(crate) const KEYRING_SERVICE: &str = "xzatoma";

/// Keyring user name for the GitHub Copilot OAuth token entry.
pub(crate) const KEYRING_COPILOT_USER: &str = "github_copilot";

// ---------------------------------------------------------------------------
// ProviderFactory
// ---------------------------------------------------------------------------

/// Unit struct that groups provider construction logic.
///
/// All methods are free associated functions (no `self`) so callers can use
/// them without holding an instance:
///
/// ```no_run
/// use xzatoma::providers::ProviderFactory;
/// use xzatoma::config::{ProviderConfig, CopilotConfig, OllamaConfig, OpenAIConfig};
///
/// # fn example() -> xzatoma::error::Result<()> {
/// let config = ProviderConfig {
///     provider_type: "ollama".to_string(),
///     copilot: CopilotConfig::default(),
///     ollama: OllamaConfig::default(),
///     openai: OpenAIConfig::default(),
/// };
///
/// let provider = ProviderFactory::create_provider("ollama", &config)?;
/// # Ok(())
/// # }
/// ```
pub struct ProviderFactory;

impl ProviderFactory {
    /// Create a provider instance based on a type string and configuration.
    ///
    /// # Arguments
    ///
    /// * `provider_type` - One of `"copilot"`, `"ollama"`, or `"openai"`
    /// * `config` - Full provider configuration
    ///
    /// # Returns
    ///
    /// Returns a heap-allocated [`Provider`] trait object
    ///
    /// # Errors
    ///
    /// Returns an error if `provider_type` is not a recognised value or if
    /// provider initialisation fails (e.g. missing credentials, bad config).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzatoma::providers::ProviderFactory;
    /// use xzatoma::config::{ProviderConfig, CopilotConfig, OllamaConfig, OpenAIConfig};
    ///
    /// # fn example() -> xzatoma::error::Result<()> {
    /// let config = ProviderConfig {
    ///     provider_type: "ollama".to_string(),
    ///     copilot: CopilotConfig::default(),
    ///     ollama: OllamaConfig::default(),
    ///     openai: OpenAIConfig::default(),
    /// };
    /// let provider = ProviderFactory::create_provider("ollama", &config)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn create_provider(
        provider_type: &str,
        config: &ProviderConfig,
    ) -> Result<Box<dyn Provider>> {
        match provider_type {
            "copilot" => Ok(Box::new(CopilotProvider::new(config.copilot.clone())?)),
            "ollama" => Ok(Box::new(OllamaProvider::new(config.ollama.clone())?)),
            "openai" => Ok(Box::new(OpenAIProvider::new(config.openai.clone())?)),
            _ => Err(crate::error::XzatomaError::Provider(format!(
                "Unknown provider type: '{}'. Supported types are: copilot, ollama, openai",
                provider_type
            ))),
        }
    }

    /// Create a provider instance with optional type and model overrides.
    ///
    /// Used primarily for subagent instantiation where the subagent may require
    /// a different provider or model than the parent agent.
    ///
    /// # Arguments
    ///
    /// * `config` - Full provider configuration containing all provider settings
    /// * `provider_override` - Optional provider type override; falls back to
    ///   `config.provider_type` when `None`
    /// * `model_override` - Optional model name override applied on top of the
    ///   provider-specific config
    ///
    /// # Returns
    ///
    /// Returns a heap-allocated [`Provider`] trait object configured with the
    /// specified or default settings
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The resolved provider type is not recognised
    /// - Provider initialisation fails (authentication, network, etc.)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzatoma::providers::ProviderFactory;
    /// use xzatoma::config::{ProviderConfig, CopilotConfig, OllamaConfig, OpenAIConfig};
    ///
    /// # fn example() -> xzatoma::error::Result<()> {
    /// let config = ProviderConfig {
    ///     provider_type: "copilot".to_string(),
    ///     copilot: CopilotConfig::default(),
    ///     ollama: OllamaConfig::default(),
    ///     openai: OpenAIConfig::default(),
    /// };
    ///
    /// // Use default provider from config
    /// let default_provider = ProviderFactory::create_provider_with_override(&config, None, None)?;
    ///
    /// // Override to Ollama with a specific model
    /// let ollama_provider = ProviderFactory::create_provider_with_override(
    ///     &config,
    ///     Some("ollama"),
    ///     Some("llama3.2:3b"),
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn create_provider_with_override(
        config: &ProviderConfig,
        provider_override: Option<&str>,
        model_override: Option<&str>,
    ) -> Result<Box<dyn Provider>> {
        let provider_type = provider_override.unwrap_or(&config.provider_type);

        match provider_type {
            "copilot" => {
                let mut copilot_config = config.copilot.clone();
                if let Some(model) = model_override {
                    copilot_config.model = model.to_string();
                }
                Ok(Box::new(CopilotProvider::new(copilot_config)?))
            }
            "ollama" => {
                let mut ollama_config = config.ollama.clone();
                if let Some(model) = model_override {
                    ollama_config.model = model.to_string();
                }
                Ok(Box::new(OllamaProvider::new(ollama_config)?))
            }
            "openai" => {
                let mut openai_config = config.openai.clone();
                if let Some(model) = model_override {
                    openai_config.model = model.to_string();
                }
                Ok(Box::new(OpenAIProvider::new(openai_config)?))
            }
            _ => Err(crate::error::XzatomaError::Provider(format!(
                "Unknown provider type: '{}'. Supported types are: copilot, ollama, openai",
                provider_type
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// Backward-compatible free-function wrappers
// ---------------------------------------------------------------------------

/// Create a provider instance based on configuration.
///
/// This is a thin wrapper around [`ProviderFactory::create_provider`] kept for
/// backward compatibility with call sites that import the free function
/// directly from `crate::providers`.
///
/// # Arguments
///
/// * `provider_type` - One of `"copilot"`, `"ollama"`, or `"openai"`
/// * `config` - Provider configuration
///
/// # Returns
///
/// Returns a boxed provider instance
///
/// # Errors
///
/// Returns an error if provider type is invalid or initialisation fails
pub fn create_provider(provider_type: &str, config: &ProviderConfig) -> Result<Box<dyn Provider>> {
    ProviderFactory::create_provider(provider_type, config)
}

/// Create a provider instance with optional overrides for subagents.
///
/// This is a thin wrapper around
/// [`ProviderFactory::create_provider_with_override`] kept for backward
/// compatibility with call sites that import the free function directly from
/// `crate::providers`.
///
/// # Arguments
///
/// * `config` - Full provider configuration containing all provider settings
/// * `provider_override` - Optional provider type override
/// * `model_override` - Optional model name override
///
/// # Returns
///
/// Returns a boxed provider instance configured with the specified or default
/// settings
///
/// # Errors
///
/// Returns an error if the provider type is invalid or initialisation fails
///
/// # Examples
///
/// ```no_run
/// use xzatoma::providers::create_provider_with_override;
/// use xzatoma::config::{ProviderConfig, CopilotConfig, OllamaConfig, OpenAIConfig};
///
/// # fn example() -> xzatoma::error::Result<()> {
/// let config = ProviderConfig {
///     provider_type: "copilot".to_string(),
///     copilot: CopilotConfig::default(),
///     ollama: OllamaConfig::default(),
///     openai: OpenAIConfig::default(),
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
///     Some("llama3.2:3b"),
/// )?;
/// # Ok(())
/// # }
/// ```
pub fn create_provider_with_override(
    config: &ProviderConfig,
    provider_override: Option<&str>,
    model_override: Option<&str>,
) -> Result<Box<dyn Provider>> {
    ProviderFactory::create_provider_with_override(config, provider_override, model_override)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CopilotConfig, OllamaConfig, OpenAIConfig};

    #[test]
    fn test_create_provider_invalid_type() {
        let config = ProviderConfig {
            provider_type: "invalid".to_string(),
            copilot: CopilotConfig::default(),
            ollama: OllamaConfig::default(),
            openai: OpenAIConfig::default(),
        };

        let result = create_provider("invalid", &config);
        assert!(result.is_err());
        // SAFETY: asserted is_err() above, so err() is guaranteed Some
        let err_msg = result.err().unwrap().to_string();
        assert!(
            err_msg.contains("copilot"),
            "Error message should contain 'copilot'"
        );
        assert!(
            err_msg.contains("ollama"),
            "Error message should contain 'ollama'"
        );
        assert!(
            err_msg.contains("openai"),
            "Error message should contain 'openai'"
        );
    }

    #[test]
    fn test_create_provider_with_override_default() {
        let config = ProviderConfig {
            provider_type: "copilot".to_string(),
            copilot: CopilotConfig {
                model: "gpt-5.3-codex".to_string(),
                api_base: None,
                enable_streaming: true,
                enable_endpoint_fallback: true,
                reasoning_effort: None,
                include_reasoning: false,
            },
            ollama: OllamaConfig::default(),
            openai: OpenAIConfig::default(),
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
            openai: OpenAIConfig::default(),
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
            openai: OpenAIConfig::default(),
        };

        // Override both provider and model
        let result = create_provider_with_override(&config, Some("ollama"), Some("llama3.2:3b"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_provider_with_override_model_only() {
        let config = ProviderConfig {
            provider_type: "copilot".to_string(),
            copilot: CopilotConfig {
                model: "gpt-5.3-codex".to_string(),
                api_base: None,
                enable_streaming: true,
                enable_endpoint_fallback: true,
                reasoning_effort: None,
                include_reasoning: false,
            },
            ollama: OllamaConfig::default(),
            openai: OpenAIConfig::default(),
        };

        // Override model only (uses config provider type)
        let result = create_provider_with_override(&config, None, Some("gpt-5.1-codex-mini"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_provider_with_override_invalid_provider() {
        let config = ProviderConfig {
            provider_type: "copilot".to_string(),
            copilot: CopilotConfig::default(),
            ollama: OllamaConfig::default(),
            openai: OpenAIConfig::default(),
        };

        // Invalid provider override
        let result = create_provider_with_override(&config, Some("invalid"), None);
        assert!(result.is_err());
        // SAFETY: asserted is_err() above, so err() is guaranteed Some
        let err_msg = result.err().unwrap().to_string();
        assert!(
            err_msg.contains("copilot"),
            "Error should mention 'copilot'"
        );
        assert!(err_msg.contains("ollama"), "Error should mention 'ollama'");
        assert!(err_msg.contains("openai"), "Error should mention 'openai'");
    }

    #[test]
    fn test_create_provider_with_override_copilot_model() {
        let config = ProviderConfig {
            provider_type: "copilot".to_string(),
            copilot: CopilotConfig {
                model: "gpt-5.3-codex".to_string(),
                api_base: None,
                enable_streaming: true,
                enable_endpoint_fallback: true,
                reasoning_effort: None,
                include_reasoning: false,
            },
            ollama: OllamaConfig::default(),
            openai: OpenAIConfig::default(),
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
                request_timeout_seconds: 600,
            },
            openai: OpenAIConfig::default(),
        };

        // Override to ollama with custom model
        let result = create_provider_with_override(&config, Some("ollama"), Some("gemma2:2b"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_provider_openai() {
        let config = ProviderConfig {
            provider_type: "openai".to_string(),
            copilot: CopilotConfig::default(),
            ollama: OllamaConfig::default(),
            openai: OpenAIConfig::default(),
        };

        let result = create_provider("openai", &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_provider_with_override_to_openai() {
        let config = ProviderConfig {
            provider_type: "copilot".to_string(),
            copilot: CopilotConfig::default(),
            ollama: OllamaConfig::default(),
            openai: OpenAIConfig::default(),
        };

        // Override from copilot config to openai
        let result = create_provider_with_override(&config, Some("openai"), None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_provider_with_override_openai_model() {
        let config = ProviderConfig {
            provider_type: "copilot".to_string(),
            copilot: CopilotConfig::default(),
            ollama: OllamaConfig::default(),
            openai: OpenAIConfig::default(),
        };

        // Override to openai with custom model
        let result = create_provider_with_override(&config, Some("openai"), Some("gpt-4o"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_provider_factory_create_provider_invalid() {
        let config = ProviderConfig {
            provider_type: "unknown".to_string(),
            copilot: CopilotConfig::default(),
            ollama: OllamaConfig::default(),
            openai: OpenAIConfig::default(),
        };
        let result = ProviderFactory::create_provider("unknown", &config);
        assert!(result.is_err());
        // SAFETY: asserted is_err() above, so err() is guaranteed Some
        let err_msg = result.err().unwrap().to_string();
        assert!(
            err_msg.contains("copilot"),
            "Error message should contain 'copilot'"
        );
        assert!(
            err_msg.contains("ollama"),
            "Error message should contain 'ollama'"
        );
        assert!(
            err_msg.contains("openai"),
            "Error message should contain 'openai'"
        );
    }

    #[test]
    fn test_provider_factory_create_provider_with_override_invalid() {
        let config = ProviderConfig {
            provider_type: "copilot".to_string(),
            copilot: CopilotConfig::default(),
            ollama: OllamaConfig::default(),
            openai: OpenAIConfig::default(),
        };
        let result = ProviderFactory::create_provider_with_override(&config, Some("unknown"), None);
        assert!(result.is_err());
        // SAFETY: asserted is_err() above, so err() is guaranteed Some
        let err_msg = result.err().unwrap().to_string();
        assert!(
            err_msg.contains("copilot"),
            "Error message should contain 'copilot'"
        );
        assert!(
            err_msg.contains("ollama"),
            "Error message should contain 'ollama'"
        );
        assert!(
            err_msg.contains("openai"),
            "Error message should contain 'openai'"
        );
    }

    #[test]
    fn test_unknown_provider_error_message_contains_all_supported_types() {
        let config = ProviderConfig {
            provider_type: "xyz".to_string(),
            copilot: CopilotConfig::default(),
            ollama: OllamaConfig::default(),
            openai: OpenAIConfig::default(),
        };

        let result = ProviderFactory::create_provider("xyz", &config);
        assert!(result.is_err());
        // SAFETY: asserted is_err() above, so err() is guaranteed Some
        let err_msg = result.err().unwrap().to_string();
        assert!(
            err_msg.contains("copilot"),
            "Error message should contain 'copilot', got: {}",
            err_msg
        );
        assert!(
            err_msg.contains("ollama"),
            "Error message should contain 'ollama', got: {}",
            err_msg
        );
        assert!(
            err_msg.contains("openai"),
            "Error message should contain 'openai', got: {}",
            err_msg
        );

        let result_override =
            ProviderFactory::create_provider_with_override(&config, Some("xyz"), None);
        assert!(result_override.is_err());
        // SAFETY: asserted is_err() above, so err() is guaranteed Some
        let err_msg_override = result_override.err().unwrap().to_string();
        assert!(
            err_msg_override.contains("copilot"),
            "Error message should contain 'copilot', got: {}",
            err_msg_override
        );
        assert!(
            err_msg_override.contains("ollama"),
            "Error message should contain 'ollama', got: {}",
            err_msg_override
        );
        assert!(
            err_msg_override.contains("openai"),
            "Error message should contain 'openai', got: {}",
            err_msg_override
        );
    }

    #[test]
    fn test_keyring_constants_are_correct() {
        assert_eq!(KEYRING_SERVICE, "xzatoma");
        assert_eq!(KEYRING_COPILOT_USER, "github_copilot");
    }
}
