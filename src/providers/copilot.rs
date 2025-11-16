//! GitHub Copilot provider implementation for XZatoma
//!
//! This module implements the Provider trait for GitHub Copilot.
//! Full implementation will be completed in Phase 4.

use crate::config::CopilotConfig;
use crate::error::{Result, XzatomaError};
use crate::providers::{Message, Provider};
use async_trait::async_trait;

/// GitHub Copilot provider
///
/// This provider connects to GitHub Copilot's API to generate
/// completions with tool calling support.
pub struct CopilotProvider {
    config: CopilotConfig,
    #[allow(dead_code)]
    client: reqwest::Client,
}

impl CopilotProvider {
    /// Create a new Copilot provider instance
    ///
    /// # Arguments
    ///
    /// * `config` - Copilot configuration
    ///
    /// # Returns
    ///
    /// Returns a new CopilotProvider instance
    ///
    /// # Errors
    ///
    /// Returns error if client initialization fails
    pub fn new(config: CopilotConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent("xzatoma/0.1.0")
            .build()
            .map_err(|e| XzatomaError::Provider(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self { config, client })
    }

    /// Get the configured model name
    pub fn model(&self) -> &str {
        &self.config.model
    }
}

#[async_trait]
impl Provider for CopilotProvider {
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[serde_json::Value],
    ) -> Result<Message> {
        // Placeholder implementation
        // Full implementation will be in Phase 4
        Err(
            XzatomaError::Provider("Copilot provider not yet implemented (Phase 4)".to_string())
                .into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_copilot_provider_creation() {
        let config = CopilotConfig {
            model: "gpt-4o".to_string(),
        };
        let provider = CopilotProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn test_copilot_provider_model() {
        let config = CopilotConfig {
            model: "gpt-4o".to_string(),
        };
        let provider = CopilotProvider::new(config).unwrap();
        assert_eq!(provider.model(), "gpt-4o");
    }

    #[tokio::test]
    async fn test_copilot_complete_not_implemented() {
        let config = CopilotConfig {
            model: "gpt-4o".to_string(),
        };
        let provider = CopilotProvider::new(config).unwrap();
        let messages = vec![Message::user("test")];
        let tools = vec![];

        let result = provider.complete(&messages, &tools).await;
        assert!(result.is_err());
    }
}
