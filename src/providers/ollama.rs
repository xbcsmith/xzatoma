//! Ollama provider implementation for XZatoma
//!
//! This module implements the Provider trait for Ollama.
//! Full implementation will be completed in Phase 4.

use crate::config::OllamaConfig;
use crate::error::{Result, XzatomaError};
use crate::providers::{Message, Provider};

use async_trait::async_trait;

/// Ollama provider
///
/// This provider connects to a local Ollama server to generate
/// completions with tool calling support.
pub struct OllamaProvider {
    config: OllamaConfig,
    #[allow(dead_code)]
    client: reqwest::Client,
}

impl OllamaProvider {
    /// Create a new Ollama provider instance
    ///
    /// # Arguments
    ///
    /// * `config` - Ollama configuration
    ///
    /// # Returns
    ///
    /// Returns a new OllamaProvider instance
    ///
    /// # Errors
    ///
    /// Returns error if client initialization fails
    pub fn new(config: OllamaConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent("xzatoma/0.1.0")
            .build()
            .map_err(|e| XzatomaError::Provider(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self { config, client })
    }

    /// Get the configured Ollama host
    pub fn host(&self) -> &str {
        &self.config.host
    }

    /// Get the configured model name
    pub fn model(&self) -> &str {
        &self.config.model
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[serde_json::Value],
    ) -> Result<Message> {
        // Placeholder implementation
        // Full implementation will be in Phase 4
        Err(
            XzatomaError::Provider("Ollama provider not yet implemented (Phase 4)".to_string())
                .into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_provider_creation() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "qwen2.5-coder".to_string(),
        };
        let provider = OllamaProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn test_ollama_provider_host() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "qwen2.5-coder".to_string(),
        };
        let provider = OllamaProvider::new(config).unwrap();
        assert_eq!(provider.host(), "http://localhost:11434");
    }

    #[test]
    fn test_ollama_provider_model() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "qwen2.5-coder".to_string(),
        };
        let provider = OllamaProvider::new(config).unwrap();
        assert_eq!(provider.model(), "qwen2.5-coder");
    }

    #[tokio::test]
    async fn test_ollama_complete_not_implemented() {
        let config = OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "qwen2.5-coder".to_string(),
        };
        let provider = OllamaProvider::new(config).unwrap();
        let messages = vec![Message::user("test")];
        let tools = vec![];

        let result = provider.complete(&messages, &tools).await;
        assert!(result.is_err());
    }
}
