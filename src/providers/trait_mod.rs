//! Provider trait definition for the XZatoma provider layer.
//!
//! This module defines the `Provider` trait that all AI provider
//! implementations must satisfy. The trait methods cover conversation
//! completion, model listing, model switching, capability querying, and
//! summary retrieval.
//!
//! Required methods are `complete`, `is_authenticated`, `current_model`,
//! `set_model`, and `fetch_models`. All remaining methods have default
//! implementations that delegate to the required methods or return safe
//! fallbacks.

use crate::error::Result;
use async_trait::async_trait;

use super::types::{
    CompletionResponse, Message, ModelInfo, ModelInfoSummary, ProviderCapabilities,
};

/// Provider trait for AI providers.
///
/// All AI providers (Copilot, Ollama, OpenAI, etc.) must implement this trait.
/// The trait provides a common interface for completing conversations with tool
/// support and model management capabilities.
///
/// The required methods are `complete`, `is_authenticated`, `current_model`,
/// `set_model`, and `fetch_models`. Every other method has a default
/// implementation that either delegates to a required method or returns a
/// safe fallback.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::providers::{Provider, Message, CompletionResponse, ModelInfo};
/// use xzatoma::error::Result;
/// use async_trait::async_trait;
///
/// struct MyProvider {
///     model: String,
/// }
///
/// #[async_trait]
/// impl Provider for MyProvider {
///     fn is_authenticated(&self) -> bool {
///         true
///     }
///
///     fn current_model(&self) -> Option<&str> {
///         Some(&self.model)
///     }
///
///     fn set_model(&mut self, model: &str) {
///         self.model = model.to_string();
///     }
///
///     async fn fetch_models(&self) -> Result<Vec<ModelInfo>> {
///         Ok(vec![])
///     }
///
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
    /// Returns `true` if this provider has valid stored credentials.
    fn is_authenticated(&self) -> bool;

    /// Returns a borrowed reference to the currently active model name, or
    /// `None` if no model is configured.
    fn current_model(&self) -> Option<&str>;

    /// Set the active model in memory without any API validation. Callers
    /// that need model-existence validation should call `list_models` before
    /// calling this method.
    fn set_model(&mut self, model: &str);

    /// Fetch the list of available models from the remote API. This is the
    /// canonical implementation method; `list_models` provides a default that
    /// delegates here.
    ///
    /// # Returns
    ///
    /// Returns a vector of available models.
    ///
    /// # Errors
    ///
    /// Returns error if the API call fails or the provider does not support
    /// model listing.
    async fn fetch_models(&self) -> Result<Vec<ModelInfo>>;

    /// Completes a conversation with the given messages and available tools.
    ///
    /// # Arguments
    ///
    /// * `messages` - Conversation history
    /// * `tools` - Available tools for the assistant to use (as JSON schemas)
    ///
    /// # Returns
    ///
    /// Returns the assistant's response message along with token usage
    /// information.
    ///
    /// # Errors
    ///
    /// Returns error if the API call fails or response is invalid.
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[serde_json::Value],
    ) -> Result<CompletionResponse>;

    /// List available models for this provider.
    ///
    /// # Returns
    ///
    /// Returns a vector of available models.
    ///
    /// # Errors
    ///
    /// Returns error if the provider does not support model listing or if the
    /// API call fails.
    ///
    /// # Default Implementation
    ///
    /// The default delegates to `fetch_models`. Providers do NOT override this
    /// method; implement `fetch_models` instead.
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        self.fetch_models().await
    }

    /// Get detailed information about a specific model.
    ///
    /// # Arguments
    ///
    /// * `model_name` - Name/identifier of the model
    ///
    /// # Returns
    ///
    /// Returns detailed information about the model.
    ///
    /// # Errors
    ///
    /// Returns error if the provider does not support detailed model info or
    /// if the model is not found.
    ///
    /// # Default Implementation
    ///
    /// The default implementation returns an error indicating that detailed
    /// model information is not supported.
    async fn get_model_info(&self, _model_name: &str) -> Result<ModelInfo> {
        Err(crate::error::XzatomaError::Provider(
            "Detailed model information is not supported by this provider".to_string(),
        ))
    }

    /// Get the name of the currently active model.
    ///
    /// # Returns
    ///
    /// Returns the name of the currently active model as an owned `String`, or
    /// the sentinel value `"none"` if no model is configured.
    ///
    /// # Default Implementation
    ///
    /// The default implementation delegates to `current_model`, returning
    /// `"none"` when `current_model` returns `None`.
    fn get_current_model(&self) -> String {
        self.current_model().unwrap_or("none").to_string()
    }

    /// Returns `true` if this provider supports SSE streaming completions.
    /// Defaults to `false`. Providers that support streaming should override
    /// this.
    fn supports_streaming(&self) -> bool {
        false
    }

    /// Perform a streaming chat completion. The default implementation
    /// delegates to `complete`. Providers that implement true SSE streaming
    /// may override this.
    ///
    /// # Arguments
    ///
    /// * `messages` - Conversation history
    /// * `tools` - Available tools for the assistant to use (as JSON schemas)
    ///
    /// # Returns
    ///
    /// Returns the assistant's response.
    ///
    /// # Errors
    ///
    /// Returns error if the underlying `complete` call fails.
    async fn chat_completion_stream(
        &self,
        messages: &[Message],
        tools: &[serde_json::Value],
    ) -> Result<CompletionResponse> {
        self.complete(messages, tools).await
    }

    /// Get the capabilities of this provider.
    ///
    /// # Returns
    ///
    /// Returns the provider's capabilities.
    ///
    /// # Default Implementation
    ///
    /// The default implementation returns a capabilities struct with all
    /// features disabled.
    fn get_provider_capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::default()
    }

    /// List models with full summary data.
    ///
    /// # Returns
    ///
    /// Returns a vector of models with extended summary information.
    ///
    /// # Errors
    ///
    /// Returns error if the provider does not support model listing or if the
    /// API call fails.
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

    /// Get model info with full summary data.
    ///
    /// # Arguments
    ///
    /// * `model_name` - Name/identifier of the model
    ///
    /// # Returns
    ///
    /// Returns detailed model information with extended summary data.
    ///
    /// # Errors
    ///
    /// Returns error if the provider does not support detailed model info or
    /// if the model is not found.
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
    use crate::providers::{CompletionResponse, Message, ModelInfo};
    use async_trait::async_trait;

    #[test]
    fn test_default_list_models_error() {
        struct MockProvider;

        #[async_trait]
        impl Provider for MockProvider {
            fn is_authenticated(&self) -> bool {
                false
            }

            fn current_model(&self) -> Option<&str> {
                None
            }

            fn set_model(&mut self, _model: &str) {}

            async fn fetch_models(&self) -> Result<Vec<ModelInfo>> {
                Err(crate::error::XzatomaError::Provider(
                    "not supported".to_string(),
                ))
            }

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
            fn is_authenticated(&self) -> bool {
                false
            }

            fn current_model(&self) -> Option<&str> {
                None
            }

            fn set_model(&mut self, _model: &str) {}

            async fn fetch_models(&self) -> Result<Vec<ModelInfo>> {
                Err(crate::error::XzatomaError::Provider(
                    "not supported".to_string(),
                ))
            }

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
            let result = provider.get_model_info("gpt-4").await;
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_default_get_current_model_returns_sentinel() {
        struct MockProvider;

        #[async_trait]
        impl Provider for MockProvider {
            fn is_authenticated(&self) -> bool {
                false
            }

            fn current_model(&self) -> Option<&str> {
                None
            }

            fn set_model(&mut self, _model: &str) {}

            async fn fetch_models(&self) -> Result<Vec<ModelInfo>> {
                Err(crate::error::XzatomaError::Provider(
                    "not supported".to_string(),
                ))
            }

            async fn complete(
                &self,
                _messages: &[Message],
                _tools: &[serde_json::Value],
            ) -> Result<CompletionResponse> {
                Ok(CompletionResponse::new(Message::assistant("test")))
            }
        }

        let provider = MockProvider;
        assert_eq!(provider.get_current_model(), "none");
    }

    #[test]
    fn test_default_set_model_noop() {
        struct MockProvider;

        #[async_trait]
        impl Provider for MockProvider {
            fn is_authenticated(&self) -> bool {
                false
            }

            fn current_model(&self) -> Option<&str> {
                None
            }

            fn set_model(&mut self, _model: &str) {}

            async fn fetch_models(&self) -> Result<Vec<ModelInfo>> {
                Err(crate::error::XzatomaError::Provider(
                    "not supported".to_string(),
                ))
            }

            async fn complete(
                &self,
                _messages: &[Message],
                _tools: &[serde_json::Value],
            ) -> Result<CompletionResponse> {
                Ok(CompletionResponse::new(Message::assistant("test")))
            }
        }

        let mut provider = MockProvider;
        provider.set_model("any-model");
        // No assertion on a Result because set_model returns (); verifies the
        // call completes without panicking.
    }

    #[test]
    fn test_is_authenticated_through_trait_ref() {
        struct AuthedMock;
        struct UnauthMock;

        #[async_trait]
        impl Provider for AuthedMock {
            fn is_authenticated(&self) -> bool {
                true
            }

            fn current_model(&self) -> Option<&str> {
                None
            }

            fn set_model(&mut self, _model: &str) {}

            async fn fetch_models(&self) -> Result<Vec<ModelInfo>> {
                Err(crate::error::XzatomaError::Provider(
                    "not supported".to_string(),
                ))
            }

            async fn complete(
                &self,
                _messages: &[Message],
                _tools: &[serde_json::Value],
            ) -> Result<CompletionResponse> {
                Ok(CompletionResponse::new(Message::assistant("test")))
            }
        }

        #[async_trait]
        impl Provider for UnauthMock {
            fn is_authenticated(&self) -> bool {
                false
            }

            fn current_model(&self) -> Option<&str> {
                None
            }

            fn set_model(&mut self, _model: &str) {}

            async fn fetch_models(&self) -> Result<Vec<ModelInfo>> {
                Err(crate::error::XzatomaError::Provider(
                    "not supported".to_string(),
                ))
            }

            async fn complete(
                &self,
                _messages: &[Message],
                _tools: &[serde_json::Value],
            ) -> Result<CompletionResponse> {
                Ok(CompletionResponse::new(Message::assistant("test")))
            }
        }

        let authed: &dyn Provider = &AuthedMock;
        let unauthed: &dyn Provider = &UnauthMock;
        assert!(authed.is_authenticated());
        assert!(!unauthed.is_authenticated());
    }

    #[test]
    fn test_current_model_through_trait_ref() {
        struct ModelProvider {
            model: String,
        }

        #[async_trait]
        impl Provider for ModelProvider {
            fn is_authenticated(&self) -> bool {
                false
            }

            fn current_model(&self) -> Option<&str> {
                Some(&self.model)
            }

            fn set_model(&mut self, model: &str) {
                self.model = model.to_string();
            }

            async fn fetch_models(&self) -> Result<Vec<ModelInfo>> {
                Err(crate::error::XzatomaError::Provider(
                    "not supported".to_string(),
                ))
            }

            async fn complete(
                &self,
                _messages: &[Message],
                _tools: &[serde_json::Value],
            ) -> Result<CompletionResponse> {
                Ok(CompletionResponse::new(Message::assistant("test")))
            }
        }

        let boxed: Box<dyn Provider> = Box::new(ModelProvider {
            model: "gpt-4o".to_string(),
        });
        assert_eq!(boxed.current_model(), Some("gpt-4o"));
        assert_eq!(boxed.get_current_model(), "gpt-4o");
    }

    #[test]
    fn test_set_model_through_trait_ref() {
        struct MutProvider {
            model: String,
        }

        #[async_trait]
        impl Provider for MutProvider {
            fn is_authenticated(&self) -> bool {
                false
            }

            fn current_model(&self) -> Option<&str> {
                Some(&self.model)
            }

            fn set_model(&mut self, model: &str) {
                self.model = model.to_string();
            }

            async fn fetch_models(&self) -> Result<Vec<ModelInfo>> {
                Err(crate::error::XzatomaError::Provider(
                    "not supported".to_string(),
                ))
            }

            async fn complete(
                &self,
                _messages: &[Message],
                _tools: &[serde_json::Value],
            ) -> Result<CompletionResponse> {
                Ok(CompletionResponse::new(Message::assistant("test")))
            }
        }

        let mut boxed: Box<dyn Provider> = Box::new(MutProvider {
            model: "old-model".to_string(),
        });
        boxed.set_model("new-model");
        assert_eq!(boxed.current_model(), Some("new-model"));
        assert_eq!(boxed.get_current_model(), "new-model");
    }

    #[test]
    fn test_supports_streaming_default_is_false() {
        struct MockProvider;

        #[async_trait]
        impl Provider for MockProvider {
            fn is_authenticated(&self) -> bool {
                false
            }

            fn current_model(&self) -> Option<&str> {
                None
            }

            fn set_model(&mut self, _model: &str) {}

            async fn fetch_models(&self) -> Result<Vec<ModelInfo>> {
                Err(crate::error::XzatomaError::Provider(
                    "not supported".to_string(),
                ))
            }

            async fn complete(
                &self,
                _messages: &[Message],
                _tools: &[serde_json::Value],
            ) -> Result<CompletionResponse> {
                Ok(CompletionResponse::new(Message::assistant("test")))
            }
        }

        let provider: &dyn Provider = &MockProvider;
        assert!(!provider.supports_streaming());
    }

    #[test]
    fn test_chat_completion_stream_delegates_to_complete() {
        struct MockProvider;

        #[async_trait]
        impl Provider for MockProvider {
            fn is_authenticated(&self) -> bool {
                false
            }

            fn current_model(&self) -> Option<&str> {
                None
            }

            fn set_model(&mut self, _model: &str) {}

            async fn fetch_models(&self) -> Result<Vec<ModelInfo>> {
                Err(crate::error::XzatomaError::Provider(
                    "not supported".to_string(),
                ))
            }

            async fn complete(
                &self,
                _messages: &[Message],
                _tools: &[serde_json::Value],
            ) -> Result<CompletionResponse> {
                Ok(CompletionResponse::new(Message::assistant(
                    "stream-delegate-response",
                )))
            }
        }

        // SAFETY: Runtime::new only fails on OS resource exhaustion, which
        // cannot occur in a well-behaved test environment.
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let provider = MockProvider;
            let messages = vec![Message::user("hello")];
            let tools: Vec<serde_json::Value> = vec![];
            let result = provider.chat_completion_stream(&messages, &tools).await;
            assert!(result.is_ok());
            let response = result.unwrap();
            assert_eq!(
                response.message.content,
                Some("stream-delegate-response".to_string())
            );
        });
    }
}
