//! Provider factory for XZatoma.
//!
//! This module contains [`ProviderFactory`], a unit struct whose associated
//! functions create boxed [`Provider`] instances from a provider-type string
//! and a [`ProviderConfig`]. It is the single authoritative place for:
//!
//! - Constructing any supported provider
//! - Storing the keyring service and user constants used for credential storage
//! - Resolving the effective model to use when none is configured, or when
//!   the configured model isn't actually available from the provider (see
//!   [`resolve_effective_model`])
//!
//! Free-function wrappers (`create_provider` and `create_provider_with_override`)
//! are re-exported from this module so that existing call sites do not need to
//! change.

use crate::config::ProviderConfig;
use crate::error::{Result, XzatomaError};

use super::copilot::CopilotProvider;
use super::ollama::OllamaProvider;
use super::openai::OpenAIProvider;
use super::trait_mod::Provider;
use super::types::ModelInfo;

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
// Model resolution
// ---------------------------------------------------------------------------

/// Returns `true` when `err` indicates that the provider's model-listing
/// endpoint itself is missing or unsupported (HTTP 404, 405, or 501), as
/// opposed to a transient failure (network error, timeout, 5xx, auth
/// failure).
///
/// This is the "error out" signal: callers should fail hard rather than
/// silently continuing with a configured model, since there's no way to
/// validate that model against the provider at all.
fn is_models_endpoint_missing(err: &XzatomaError) -> bool {
    matches!(
        err,
        XzatomaError::ProviderHttpStatus { status, .. }
            if *status == reqwest::StatusCode::NOT_FOUND
                || *status == reqwest::StatusCode::METHOD_NOT_ALLOWED
                || *status == reqwest::StatusCode::NOT_IMPLEMENTED
    )
}

/// Picks the "latest" model from a non-empty list of models returned by a
/// provider's model-listing API.
///
/// Prefers a recency hint attached to [`ModelInfo::provider_specific`]:
/// `"modified_at"` (an RFC 3339 timestamp, populated by the Ollama provider
/// from `/api/tags`) or `"created"` (Unix epoch seconds, populated by the
/// OpenAI provider from `/models` when the server provides it), taking the
/// maximum. Falls back to the first entry in the provider's returned list
/// order when no model carries a parseable timestamp — this is the only
/// option available for providers that expose no recency metadata at all
/// (e.g. GitHub Copilot).
///
/// Returns `None` only when `models` is empty.
fn pick_latest_model(models: &[ModelInfo]) -> Option<String> {
    let by_timestamp = models
        .iter()
        .filter_map(|model| {
            let timestamp = model
                .provider_specific
                .get("modified_at")
                .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
                .map(|dt| dt.timestamp())
                .or_else(|| {
                    model
                        .provider_specific
                        .get("created")
                        .and_then(|v| v.parse::<i64>().ok())
                })?;
            Some((timestamp, model.name.clone()))
        })
        .max_by_key(|(timestamp, _)| *timestamp)
        .map(|(_, name)| name);

    by_timestamp.or_else(|| models.first().map(|m| m.name.clone()))
}

/// Resolves the effective model to use for a freshly constructed provider.
///
/// `configured` is the model configured for this provider (empty string
/// means "not specified"). `provider` is used to fetch the provider's
/// current model list.
///
/// Behavior:
/// - Models fetched, non-empty, `configured` present in the list → keep
///   `configured`.
/// - Models fetched, non-empty, `configured` empty or not present in the
///   list → log an error (only when `configured` was non-empty, since that
///   means the configured model doesn't exist) and pick the latest model via
///   [`pick_latest_model`].
/// - Models fetched but empty (provider reachable, nothing available) → keep
///   `configured` if non-empty; error if empty (nothing to fall back to).
/// - Fetch failed because the endpoint itself is missing/unsupported (see
///   [`is_models_endpoint_missing`]) → hard error, regardless of
///   `configured`.
/// - Fetch failed for another (transient) reason → warn and keep
///   `configured` if non-empty; hard error if empty (no list to pick
///   "latest" from, and nothing configured to fall back to).
///
/// # Errors
///
/// Returns an error when no usable model can be determined, per the rules
/// above.
async fn resolve_effective_model(
    provider_label: &str,
    provider: &dyn Provider,
    configured: &str,
) -> Result<String> {
    match provider.fetch_models().await {
        Ok(models) if models.is_empty() => {
            if configured.is_empty() {
                Err(XzatomaError::Provider(format!(
                    "{provider_label}: no model configured and the provider reported no available models"
                )))
            } else {
                Ok(configured.to_string())
            }
        }
        Ok(models) => {
            if !configured.is_empty() {
                if models.iter().any(|m| m.name == configured) {
                    return Ok(configured.to_string());
                }
                tracing::error!(
                    provider = provider_label,
                    configured_model = configured,
                    "configured model not found in provider's model list; selecting the latest available model instead"
                );
            }
            pick_latest_model(&models).ok_or_else(|| {
                XzatomaError::Provider(format!(
                    "{provider_label}: could not determine a default model from the provider's model list"
                ))
            })
        }
        Err(error) if is_models_endpoint_missing(&error) => Err(XzatomaError::Provider(format!(
            "{provider_label}: models endpoint is not available: {error}"
        ))),
        Err(error) if !configured.is_empty() => {
            tracing::warn!(
                provider = provider_label,
                error = %error,
                configured_model = configured,
                "failed to query provider for available models; continuing with configured model"
            );
            Ok(configured.to_string())
        }
        Err(error) => Err(error),
    }
}

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
/// # async fn example() -> xzatoma::error::Result<()> {
/// let config = ProviderConfig {
///     provider_type: "ollama".to_string(),
///     copilot: CopilotConfig::default(),
///     ollama: OllamaConfig::default(),
///     openai: OpenAIConfig::default(),
/// };
///
/// let provider = ProviderFactory::create_provider("ollama", &config).await?;
/// # Ok(())
/// # }
/// ```
pub struct ProviderFactory;

impl ProviderFactory {
    /// Create a provider instance based on a type string and configuration.
    ///
    /// After construction, the effective model is resolved via
    /// [`resolve_effective_model`]: if `config`'s model for the resolved
    /// provider type is unset, or doesn't exist on the provider, the
    /// provider's model list is queried and the latest available model is
    /// selected instead.
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
    /// Returns an error if `provider_type` is not a recognised value, if
    /// provider initialisation fails (e.g. missing credentials, bad config),
    /// or if the effective model cannot be resolved (see
    /// [`resolve_effective_model`]).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzatoma::providers::ProviderFactory;
    /// use xzatoma::config::{ProviderConfig, CopilotConfig, OllamaConfig, OpenAIConfig};
    ///
    /// # async fn example() -> xzatoma::error::Result<()> {
    /// let config = ProviderConfig {
    ///     provider_type: "ollama".to_string(),
    ///     copilot: CopilotConfig::default(),
    ///     ollama: OllamaConfig::default(),
    ///     openai: OpenAIConfig::default(),
    /// };
    /// let provider = ProviderFactory::create_provider("ollama", &config).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_provider(
        provider_type: &str,
        config: &ProviderConfig,
    ) -> Result<Box<dyn Provider>> {
        match provider_type {
            "copilot" => {
                let mut provider = CopilotProvider::new(config.copilot.clone())?;
                let resolved =
                    resolve_effective_model("copilot", &provider, &config.copilot.model).await?;
                provider.set_model(&resolved);
                Ok(Box::new(provider))
            }
            "ollama" => {
                let mut provider = OllamaProvider::new(config.ollama.clone())?;
                let resolved =
                    resolve_effective_model("ollama", &provider, &config.ollama.model).await?;
                provider.set_model(&resolved);
                Ok(Box::new(provider))
            }
            "openai" => {
                let mut provider = OpenAIProvider::new(config.openai.clone())?;
                let resolved =
                    resolve_effective_model("openai", &provider, &config.openai.model).await?;
                provider.set_model(&resolved);
                Ok(Box::new(provider))
            }
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
    /// After applying `provider_override`/`model_override`, the effective
    /// model is resolved via [`resolve_effective_model`] exactly as in
    /// [`ProviderFactory::create_provider`].
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
    /// - The effective model cannot be resolved (see
    ///   [`resolve_effective_model`])
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzatoma::providers::ProviderFactory;
    /// use xzatoma::config::{ProviderConfig, CopilotConfig, OllamaConfig, OpenAIConfig};
    ///
    /// # async fn example() -> xzatoma::error::Result<()> {
    /// let config = ProviderConfig {
    ///     provider_type: "copilot".to_string(),
    ///     copilot: CopilotConfig::default(),
    ///     ollama: OllamaConfig::default(),
    ///     openai: OpenAIConfig::default(),
    /// };
    ///
    /// // Use default provider from config
    /// let default_provider = ProviderFactory::create_provider_with_override(&config, None, None).await?;
    ///
    /// // Override to Ollama with a specific model
    /// let ollama_provider = ProviderFactory::create_provider_with_override(
    ///     &config,
    ///     Some("ollama"),
    ///     Some("llama3.2:3b"),
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_provider_with_override(
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
                let mut provider = CopilotProvider::new(copilot_config.clone())?;
                let resolved =
                    resolve_effective_model("copilot", &provider, &copilot_config.model).await?;
                provider.set_model(&resolved);
                Ok(Box::new(provider))
            }
            "ollama" => {
                let mut ollama_config = config.ollama.clone();
                if let Some(model) = model_override {
                    ollama_config.model = model.to_string();
                }
                let mut provider = OllamaProvider::new(ollama_config.clone())?;
                let resolved =
                    resolve_effective_model("ollama", &provider, &ollama_config.model).await?;
                provider.set_model(&resolved);
                Ok(Box::new(provider))
            }
            "openai" => {
                let mut openai_config = config.openai.clone();
                if let Some(model) = model_override {
                    openai_config.model = model.to_string();
                }
                let mut provider = OpenAIProvider::new(openai_config.clone())?;
                let resolved =
                    resolve_effective_model("openai", &provider, &openai_config.model).await?;
                provider.set_model(&resolved);
                Ok(Box::new(provider))
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
/// Returns an error if provider type is invalid, initialisation fails, or
/// the effective model cannot be resolved
pub async fn create_provider(
    provider_type: &str,
    config: &ProviderConfig,
) -> Result<Box<dyn Provider>> {
    ProviderFactory::create_provider(provider_type, config).await
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
/// Returns an error if the provider type is invalid, initialisation fails, or
/// the effective model cannot be resolved
///
/// # Examples
///
/// ```no_run
/// use xzatoma::providers::create_provider_with_override;
/// use xzatoma::config::{ProviderConfig, CopilotConfig, OllamaConfig, OpenAIConfig};
///
/// # async fn example() -> xzatoma::error::Result<()> {
/// let config = ProviderConfig {
///     provider_type: "copilot".to_string(),
///     copilot: CopilotConfig::default(),
///     ollama: OllamaConfig::default(),
///     openai: OpenAIConfig::default(),
/// };
///
/// // Use default provider from config
/// let default_provider = create_provider_with_override(&config, None, None).await?;
///
/// // Override to use Ollama instead
/// let ollama_provider = create_provider_with_override(
///     &config,
///     Some("ollama"),
///     None,
/// ).await?;
///
/// // Override provider and model
/// let custom_provider = create_provider_with_override(
///     &config,
///     Some("ollama"),
///     Some("llama3.2:3b"),
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub async fn create_provider_with_override(
    config: &ProviderConfig,
    provider_override: Option<&str>,
    model_override: Option<&str>,
) -> Result<Box<dyn Provider>> {
    ProviderFactory::create_provider_with_override(config, provider_override, model_override).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CopilotConfig, OllamaConfig, OpenAIConfig};
    use crate::providers::types::ModelCapability;
    use async_trait::async_trait;

    /// A local port that is never listening in test/CI environments, used so
    /// tests exercise a fast, local, deterministic connection failure instead
    /// of reaching a real external host.
    const UNREACHABLE_HOST: &str = "http://127.0.0.1:9";

    fn unreachable_ollama_config(model: &str) -> OllamaConfig {
        OllamaConfig {
            host: UNREACHABLE_HOST.to_string(),
            model: model.to_string(),
            request_timeout_seconds: 1,
            stream_idle_timeout_seconds: 120,
        }
    }

    fn unreachable_openai_config(model: &str) -> OpenAIConfig {
        OpenAIConfig {
            api_key: String::new(),
            base_url: UNREACHABLE_HOST.to_string(),
            model: model.to_string(),
            organization_id: None,
            enable_streaming: true,
            request_timeout_seconds: 1,
            stream_idle_timeout_seconds: 1,
            reasoning_effort: None,
        }
    }

    // Note: CopilotProvider::authenticate() reads/writes the real OS keyring
    // and, absent a cached token, performs a live GitHub OAuth device flow.
    // That makes it unsafe and environment-dependent to exercise end-to-end
    // in unit tests (it could read or clobber a developer's real cached
    // Copilot token). Dispatch/override-application coverage below uses
    // Ollama and OpenAI instead, whose fetch failures degrade safely and
    // deterministically to "keep the configured model" when a model is
    // configured and the endpoint is merely unreachable. The resolution
    // algorithm itself (`resolve_effective_model`, `pick_latest_model`) is
    // tested directly further below against an in-memory mock `Provider`
    // that never touches real credentials or the network.

    #[tokio::test]
    async fn test_create_provider_invalid_type() {
        let config = ProviderConfig {
            provider_type: "invalid".to_string(),
            copilot: CopilotConfig::default(),
            ollama: OllamaConfig::default(),
            openai: OpenAIConfig::default(),
        };

        let result = create_provider("invalid", &config).await;
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

    #[tokio::test]
    async fn test_create_provider_with_override_default() {
        let config = ProviderConfig {
            provider_type: "ollama".to_string(),
            copilot: CopilotConfig::default(),
            ollama: unreachable_ollama_config("llama3.2:3b"),
            openai: OpenAIConfig::default(),
        };

        // No overrides - should use config defaults. The configured model is
        // kept because the (unreachable) model list can't be fetched.
        let result = create_provider_with_override(&config, None, None).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().get_current_model(), "llama3.2:3b");
    }

    #[tokio::test]
    async fn test_create_provider_with_override_provider_only() {
        let config = ProviderConfig {
            provider_type: "openai".to_string(),
            copilot: CopilotConfig::default(),
            ollama: unreachable_ollama_config("llama3.2:3b"),
            openai: OpenAIConfig::default(),
        };

        // Override provider to ollama
        let result = create_provider_with_override(&config, Some("ollama"), None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_provider_with_override_provider_and_model() {
        let config = ProviderConfig {
            provider_type: "openai".to_string(),
            copilot: CopilotConfig::default(),
            ollama: unreachable_ollama_config("llama3.2:3b"),
            openai: OpenAIConfig::default(),
        };

        // Override both provider and model
        let result =
            create_provider_with_override(&config, Some("ollama"), Some("llama3.2:3b")).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().get_current_model(), "llama3.2:3b");
    }

    #[tokio::test]
    async fn test_create_provider_with_override_model_only() {
        let config = ProviderConfig {
            provider_type: "ollama".to_string(),
            copilot: CopilotConfig::default(),
            ollama: unreachable_ollama_config("llama3.2:3b"),
            openai: OpenAIConfig::default(),
        };

        // Override model only (uses config provider type)
        let result = create_provider_with_override(&config, None, Some("llama3.2:1b")).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().get_current_model(), "llama3.2:1b");
    }

    #[tokio::test]
    async fn test_create_provider_with_override_invalid_provider() {
        let config = ProviderConfig {
            provider_type: "ollama".to_string(),
            copilot: CopilotConfig::default(),
            ollama: OllamaConfig::default(),
            openai: OpenAIConfig::default(),
        };

        // Invalid provider override
        let result = create_provider_with_override(&config, Some("invalid"), None).await;
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

    #[tokio::test]
    async fn test_create_provider_with_override_ollama_model() {
        let config = ProviderConfig {
            provider_type: "ollama".to_string(),
            copilot: CopilotConfig::default(),
            ollama: unreachable_ollama_config("llama3.2:latest"),
            openai: OpenAIConfig::default(),
        };

        // Override to ollama with custom model
        let result =
            create_provider_with_override(&config, Some("ollama"), Some("gemma2:2b")).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().get_current_model(), "gemma2:2b");
    }

    #[tokio::test]
    async fn test_create_provider_openai() {
        let config = ProviderConfig {
            provider_type: "openai".to_string(),
            copilot: CopilotConfig::default(),
            ollama: OllamaConfig::default(),
            openai: unreachable_openai_config("gpt-4.1-mini"),
        };

        let result = create_provider("openai", &config).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().get_current_model(), "gpt-4.1-mini");
    }

    #[tokio::test]
    async fn test_create_provider_with_override_to_openai() {
        let config = ProviderConfig {
            provider_type: "ollama".to_string(),
            copilot: CopilotConfig::default(),
            ollama: OllamaConfig::default(),
            openai: unreachable_openai_config("gpt-4.1-mini"),
        };

        // Override from ollama config to openai
        let result = create_provider_with_override(&config, Some("openai"), None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_provider_with_override_openai_model() {
        let config = ProviderConfig {
            provider_type: "ollama".to_string(),
            copilot: CopilotConfig::default(),
            ollama: OllamaConfig::default(),
            openai: unreachable_openai_config("gpt-4.1-mini"),
        };

        // Override to openai with custom model
        let result = create_provider_with_override(&config, Some("openai"), Some("gpt-4.1")).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().get_current_model(), "gpt-4.1");
    }

    #[tokio::test]
    async fn test_create_provider_no_model_configured_and_endpoint_unreachable_errors() {
        // No configured model and the model list can't be fetched at all:
        // there's nothing to fall back to, so this must be a hard error.
        let config = ProviderConfig {
            provider_type: "ollama".to_string(),
            copilot: CopilotConfig::default(),
            ollama: unreachable_ollama_config(""),
            openai: OpenAIConfig::default(),
        };

        let result = create_provider("ollama", &config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_provider_factory_create_provider_invalid() {
        let config = ProviderConfig {
            provider_type: "unknown".to_string(),
            copilot: CopilotConfig::default(),
            ollama: OllamaConfig::default(),
            openai: OpenAIConfig::default(),
        };
        let result = ProviderFactory::create_provider("unknown", &config).await;
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

    #[tokio::test]
    async fn test_provider_factory_create_provider_with_override_invalid() {
        let config = ProviderConfig {
            provider_type: "ollama".to_string(),
            copilot: CopilotConfig::default(),
            ollama: OllamaConfig::default(),
            openai: OpenAIConfig::default(),
        };
        let result =
            ProviderFactory::create_provider_with_override(&config, Some("unknown"), None).await;
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

    #[tokio::test]
    async fn test_unknown_provider_error_message_contains_all_supported_types() {
        // Dispatch on an unrecognised provider type happens before any
        // provider is constructed or any network call is made.
        let config = ProviderConfig {
            provider_type: "xyz".to_string(),
            copilot: CopilotConfig::default(),
            ollama: OllamaConfig::default(),
            openai: OpenAIConfig::default(),
        };

        let result = ProviderFactory::create_provider("xyz", &config).await;
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
            ProviderFactory::create_provider_with_override(&config, Some("xyz"), None).await;
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

    // -----------------------------------------------------------------
    // resolve_effective_model / pick_latest_model tests
    // -----------------------------------------------------------------

    /// Minimal in-memory mock provider so `resolve_effective_model` can be
    /// tested directly without touching the network, credentials, or the
    /// OS keyring.
    struct MockProvider {
        models: Result<Vec<ModelInfo>>,
    }

    #[async_trait]
    impl Provider for MockProvider {
        fn is_authenticated(&self) -> bool {
            true
        }

        fn current_model(&self) -> Option<&str> {
            None
        }

        fn set_model(&mut self, _model: &str) {}

        async fn fetch_models(&self) -> Result<Vec<ModelInfo>> {
            match &self.models {
                Ok(models) => Ok(models.clone()),
                Err(error) => Err(clone_error(error)),
            }
        }

        async fn complete(
            &self,
            _messages: &[crate::providers::Message],
            _tools: &[serde_json::Value],
        ) -> Result<crate::providers::CompletionResponse> {
            unimplemented!("not exercised by resolve_effective_model tests")
        }
    }

    fn clone_error(error: &XzatomaError) -> XzatomaError {
        match error {
            XzatomaError::ProviderHttpStatus {
                provider,
                endpoint,
                status,
                response,
            } => XzatomaError::ProviderHttpStatus {
                provider: provider.clone(),
                endpoint: endpoint.clone(),
                status: *status,
                response: response.clone(),
            },
            other => XzatomaError::Provider(other.to_string()),
        }
    }

    fn model_with_metadata(name: &str, key: &str, value: &str) -> ModelInfo {
        let mut info = ModelInfo::new(name, name, 8192);
        info.add_capability(ModelCapability::Streaming);
        info.set_provider_metadata(key, value);
        info
    }

    #[tokio::test]
    async fn test_resolve_effective_model_unset_picks_latest() {
        let provider = MockProvider {
            models: Ok(vec![
                model_with_metadata("model-old", "modified_at", "2024-01-01T00:00:00Z"),
                model_with_metadata("model-new", "modified_at", "2024-06-01T00:00:00Z"),
            ]),
        };
        let resolved = resolve_effective_model("mock", &provider, "")
            .await
            .expect("resolution should succeed");
        assert_eq!(resolved, "model-new");
    }

    #[tokio::test]
    async fn test_resolve_effective_model_configured_present_is_kept() {
        let provider = MockProvider {
            models: Ok(vec![
                model_with_metadata("model-old", "modified_at", "2024-01-01T00:00:00Z"),
                model_with_metadata("model-new", "modified_at", "2024-06-01T00:00:00Z"),
            ]),
        };
        let resolved = resolve_effective_model("mock", &provider, "model-old")
            .await
            .expect("resolution should succeed");
        assert_eq!(resolved, "model-old");
    }

    #[tokio::test]
    async fn test_resolve_effective_model_configured_absent_falls_back_to_latest() {
        let provider = MockProvider {
            models: Ok(vec![
                model_with_metadata("model-old", "modified_at", "2024-01-01T00:00:00Z"),
                model_with_metadata("model-new", "modified_at", "2024-06-01T00:00:00Z"),
            ]),
        };
        let resolved = resolve_effective_model("mock", &provider, "does-not-exist")
            .await
            .expect("resolution should succeed");
        assert_eq!(resolved, "model-new");
    }

    #[tokio::test]
    async fn test_resolve_effective_model_empty_list_and_no_configured_errors() {
        let provider = MockProvider { models: Ok(vec![]) };
        let result = resolve_effective_model("mock", &provider, "").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resolve_effective_model_empty_list_keeps_configured() {
        let provider = MockProvider { models: Ok(vec![]) };
        let resolved = resolve_effective_model("mock", &provider, "my-model")
            .await
            .expect("resolution should succeed");
        assert_eq!(resolved, "my-model");
    }

    #[tokio::test]
    async fn test_resolve_effective_model_endpoint_missing_errors_even_with_configured() {
        let provider = MockProvider {
            models: Err(XzatomaError::ProviderHttpStatus {
                provider: "mock".to_string(),
                endpoint: "models".to_string(),
                status: reqwest::StatusCode::NOT_FOUND,
                response: String::new(),
            }),
        };
        let result = resolve_effective_model("mock", &provider, "my-model").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resolve_effective_model_transient_failure_keeps_configured() {
        let provider = MockProvider {
            models: Err(XzatomaError::Provider("connection reset".to_string())),
        };
        let resolved = resolve_effective_model("mock", &provider, "my-model")
            .await
            .expect("resolution should succeed");
        assert_eq!(resolved, "my-model");
    }

    #[tokio::test]
    async fn test_resolve_effective_model_transient_failure_and_no_configured_errors() {
        let provider = MockProvider {
            models: Err(XzatomaError::Provider("connection reset".to_string())),
        };
        let result = resolve_effective_model("mock", &provider, "").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_pick_latest_model_prefers_timestamp_over_list_order() {
        let models = vec![
            model_with_metadata("model-newer", "created", "2000"),
            model_with_metadata("model-older", "created", "1000"),
        ];
        assert_eq!(pick_latest_model(&models), Some("model-newer".to_string()));
    }

    #[test]
    fn test_pick_latest_model_falls_back_to_list_order_without_timestamps() {
        let models = vec![
            ModelInfo::new("first", "first", 8192),
            ModelInfo::new("second", "second", 8192),
        ];
        assert_eq!(pick_latest_model(&models), Some("first".to_string()));
    }

    #[test]
    fn test_pick_latest_model_empty_list_returns_none() {
        assert_eq!(pick_latest_model(&[]), None);
    }

    #[test]
    fn test_is_models_endpoint_missing_matches_404_405_501() {
        for status in [
            reqwest::StatusCode::NOT_FOUND,
            reqwest::StatusCode::METHOD_NOT_ALLOWED,
            reqwest::StatusCode::NOT_IMPLEMENTED,
        ] {
            let err = XzatomaError::ProviderHttpStatus {
                provider: "mock".to_string(),
                endpoint: "models".to_string(),
                status,
                response: String::new(),
            };
            assert!(is_models_endpoint_missing(&err));
        }
    }

    #[test]
    fn test_is_models_endpoint_missing_does_not_match_other_statuses() {
        for status in [
            reqwest::StatusCode::UNAUTHORIZED,
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            reqwest::StatusCode::SERVICE_UNAVAILABLE,
        ] {
            let err = XzatomaError::ProviderHttpStatus {
                provider: "mock".to_string(),
                endpoint: "models".to_string(),
                status,
                response: String::new(),
            };
            assert!(!is_models_endpoint_missing(&err));
        }
        assert!(!is_models_endpoint_missing(&XzatomaError::Provider(
            "connection reset".to_string()
        )));
    }
}
