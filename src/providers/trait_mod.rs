//! Provider trait definition for the XZatoma provider layer.
//!
//! This module defines the `Provider` trait that all AI provider
//! implementations must satisfy. The trait methods cover conversation
//! completion, model listing, model switching, capability querying, and
//! summary retrieval. Default implementations are provided for every method
//! except `complete` so that minimal providers need only implement the core
//! completion call.

use crate::error::Result;
use async_trait::async_trait;

use super::types::{
    CompletionResponse, Message, ModelInfo, ModelInfoSummary, ProviderCapabilities,
};

/// Provider trait for AI providers
///
/// All AI providers (Copilot, Ollama, OpenAI, etc.) must implement this trait.
/// The trait provides a common interface for completing conversations with tool
/// support and model management capabilities.
///
/// Only `complete` is required; every other method has a default implementation
/// that returns an appropriate error or falls back to a simpler method.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::providers::{Provider, Message, CompletionResponse};
/// use xzatoma::error::Result;
/// use async_trait::async_trait;
///
/// struct MyProvider;
///
/// #[async_trait]
/// impl Provider for MyProvider {
///     async fn complete(
///         &self,
///         messages: &[Message],
///         tools: &[serde_json::Value],
///     ) -> Result<CompletionResponse> {
///         Ok(CompletionResponse::new(Message::assistant("Response")))
///     }
/// }
/// ```
#[async_trait]
pub trait Provider: Send + Sync {
    /// Completes a conversation with the given messages and available tools
    ///
    /// # Arguments
    ///
    /// * `messages` - Conversation history
    /// * `tools` - Available tools for the assistant to use (as JSON schemas)
    ///
    /// # Returns
    ///
    /// Returns the assistant's response message along with token usage information
    ///
    /// # Errors
    ///
    /// Returns error if the API call fails or response is invalid
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[serde_json::Value],
    ) -> Result<CompletionResponse>;

    /// List available models for this provider
    ///
    /// # Returns
    ///
    /// Returns a vector of available models
    ///
    /// # Errors
    ///
    /// Returns error if the provider does not support model listing
    /// or if the API call fails
    ///
    /// # Default Implementation
    ///
    /// The default implementation returns an error indicating that
    /// model listing is not supported by this provider.
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        Err(crate::error::XzatomaError::Provider(
            "Model listing is not supported by this provider".to_string(),
        ))
    }

    /// Get detailed information about a specific model
    ///
    /// # Arguments
    ///
    /// * `model_name` - Name/identifier of the model
    ///
    /// # Returns
    ///
    /// Returns detailed information about the model
    ///
    /// # Errors
    ///
    /// Returns error if the provider does not support detailed model info
    /// or if the model is not found
    ///
    /// # Default Implementation
    ///
    /// The default implementation returns an error indicating that
    /// detailed model information is not supported.
    async fn get_model_info(&self, _model_name: &str) -> Result<ModelInfo> {
        Err(crate::error::XzatomaError::Provider(
            "Detailed model information is not supported by this provider".to_string(),
        ))
    }

    /// Get the name of the currently active model
    ///
    /// # Returns
    ///
    /// Returns the name of the currently active model
    ///
    /// # Errors
    ///
    /// Returns error if the current model cannot be determined
    ///
    /// # Default Implementation
    ///
    /// The default implementation returns a generic unavailable message.
    fn get_current_model(&self) -> Result<String> {
        Err(crate::error::XzatomaError::Provider(
            "Current model information is not available from this provider".to_string(),
        ))
    }

    /// Get the capabilities of this provider
    ///
    /// # Returns
    ///
    /// Returns the provider's capabilities
    ///
    /// # Default Implementation
    ///
    /// The default implementation returns a capabilities struct with
    /// all features disabled.
    fn get_provider_capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::default()
    }

    /// Change the active model (if supported)
    ///
    /// # Arguments
    ///
    /// * `model_name` - Name of the model to switch to
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the model was switched successfully
    ///
    /// # Errors
    ///
    /// Returns error if the provider does not support model switching
    /// or if the model is not found
    ///
    /// # Default Implementation
    ///
    /// The default implementation returns an error indicating that
    /// model switching is not supported.
    async fn set_model(&mut self, _model_name: String) -> Result<()> {
        Err(crate::error::XzatomaError::Provider(
            "Model switching is not supported by this provider".to_string(),
        ))
    }

    /// List models with full summary data
    ///
    /// # Returns
    ///
    /// Returns a vector of models with extended summary information
    ///
    /// # Errors
    ///
    /// Returns error if the provider does not support model listing
    /// or if the API call fails
    ///
    /// # Default Implementation
    ///
    /// The default implementation converts basic `ModelInfo` to
    /// `ModelInfoSummary`. Providers can override this to supply additional
    /// summary fields.
    async fn list_models_summary(&self) -> Result<Vec<ModelInfoSummary>> {
        let models = self.list_models().await?;
        Ok(models
            .into_iter()
            .map(ModelInfoSummary::from_model_info)
            .collect())
    }

    /// Get model info with full summary data
    ///
    /// # Arguments
    ///
    /// * `model_name` - Name/identifier of the model
    ///
    /// # Returns
    ///
    /// Returns detailed model information with extended summary data
    ///
    /// # Errors
    ///
    /// Returns error if the provider does not support detailed model info
    /// or if the model is not found
    ///
    /// # Default Implementation
    ///
    /// The default implementation converts basic `ModelInfo` to
    /// `ModelInfoSummary`. Providers can override this to supply additional
    /// summary fields.
    async fn get_model_info_summary(&self, model_name: &str) -> Result<ModelInfoSummary> {
        let info = self.get_model_info(model_name).await?;
        Ok(ModelInfoSummary::from_model_info(info))
    }
}

#[cfg(test)]
mod tests {
    use super::Provider;
    use crate::error::Result;
    use crate::providers::{CompletionResponse, Message};
    use async_trait::async_trait;

    #[test]
    fn test_default_list_models_error() {
        struct MockProvider;

        #[async_trait]
        impl Provider for MockProvider {
            async fn complete(
                &self,
                _messages: &[Message],
                _tools: &[serde_json::Value],
            ) -> Result<CompletionResponse> {
                Ok(CompletionResponse::new(Message::assistant("test")))
            }
        }

        // SAFETY: Runtime::new only fails on OS resource exhaustion, which
        // cannot occur in a well-behaved test environment.
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let provider = MockProvider;
            let result = provider.list_models().await;
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_default_get_model_info_error() {
        struct MockProvider;

        #[async_trait]
        impl Provider for MockProvider {
            async fn complete(
                &self,
                _messages: &[Message],
                _tools: &[serde_json::Value],
            ) -> Result<CompletionResponse> {
                Ok(CompletionResponse::new(Message::assistant("test")))
            }
        }

        // SAFETY: Runtime::new only fails on OS resource exhaustion.
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let provider = MockProvider;
            let result = provider.get_model_info("gpt-4").await;
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_default_get_current_model_error() {
        struct MockProvider;

        #[async_trait]
        impl Provider for MockProvider {
            async fn complete(
                &self,
                _messages: &[Message],
                _tools: &[serde_json::Value],
            ) -> Result<CompletionResponse> {
                Ok(CompletionResponse::new(Message::assistant("test")))
            }
        }

        let provider = MockProvider;
        let result = provider.get_current_model();
        assert!(result.is_err());
    }

    #[test]
    fn test_default_set_model_error() {
        struct MockProvider;

        #[async_trait]
        impl Provider for MockProvider {
            async fn complete(
                &self,
                _messages: &[Message],
                _tools: &[serde_json::Value],
            ) -> Result<CompletionResponse> {
                Ok(CompletionResponse::new(Message::assistant("test")))
            }
        }

        // SAFETY: Runtime::new only fails on OS resource exhaustion.
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let mut provider = MockProvider;
            let result = provider.set_model("gpt-4".to_string()).await;
            assert!(result.is_err());
        });
    }
}
