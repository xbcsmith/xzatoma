//! Test utilities for XZatoma
//!
//! This module provides common test utilities including temporary directory
//! management, test file creation, and assertion helpers.

use crate::config::Config;
use crate::error::{Result, XzatomaError};
use crate::providers::{CompletionResponse, Message, ModelInfo, Provider, ProviderCapabilities};
use async_trait::async_trait;
use std::path::PathBuf;
use tempfile::TempDir;

/// Create a temporary directory for testing
///
/// # Returns
///
/// Returns a TempDir that will be cleaned up when dropped
///
/// # Examples
///
/// ```
/// use xzatoma::test_utils::temp_dir;
///
/// let dir = temp_dir();
/// let path = dir.path();
/// // Use the temporary directory
/// ```
pub fn temp_dir() -> TempDir {
    TempDir::new().expect("Failed to create temporary directory")
}

/// Create a test file with the given content
///
/// # Arguments
///
/// * `dir` - Directory to create the file in
/// * `name` - Name of the file
/// * `content` - Content to write to the file
///
/// # Returns
///
/// Returns the path to the created file
///
/// # Panics
///
/// Panics if file creation or writing fails
///
/// # Examples
///
/// ```
/// use xzatoma::test_utils::{temp_dir, create_test_file};
///
/// let dir = temp_dir();
/// let file_path = create_test_file(&dir, "test.txt", "content");
/// ```
pub fn create_test_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, content).expect("Failed to write test file");
    path
}

/// Assert that an error contains the expected message
///
/// # Arguments
///
/// * `result` - Result to check
/// * `expected` - Expected error message substring
///
/// # Panics
///
/// Panics if the result is Ok or if the error doesn't contain the expected message
///
/// # Examples
///
/// ```
/// use xzatoma::test_utils::assert_error_contains;
/// use xzatoma::error::XzatomaError;
///
/// let result: Result<(), XzatomaError> = Err(XzatomaError::Config("invalid".to_string()));
/// assert_error_contains(result, "invalid");
/// ```
pub fn assert_error_contains<T>(result: std::result::Result<T, XzatomaError>, expected: &str) {
    match result {
        Ok(_) => panic!("Expected error containing '{}' but got Ok", expected),
        Err(e) => {
            let error_msg = e.to_string();
            assert!(
                error_msg.contains(expected),
                "Error message '{}' does not contain '{}'",
                error_msg,
                expected
            );
        }
    }
}

/// Create a test configuration with default values
///
/// # Returns
///
/// Returns a Config instance suitable for testing
///
/// # Examples
///
/// ```
/// use xzatoma::test_utils::test_config;
///
/// let config = test_config();
/// assert_eq!(config.provider.provider_type, "copilot");
/// ```
pub fn test_config() -> Config {
    Config::default()
}

/// Create a test configuration YAML string
///
/// # Returns
///
/// Returns a YAML string with test configuration
pub fn test_config_yaml() -> String {
    r#"
provider:
  type: ollama
  copilot:
    model: gpt-5.3-codex
  ollama:
    host: http://localhost:11434
    model: llama3.2:latest

agent:
  max_turns: 10
  timeout_seconds: 60
  conversation:
    max_tokens: 10000
    min_retain_turns: 3
    prune_threshold: 0.8
  tools:
    max_output_size: 1048576
    max_file_read_size: 10485760
  terminal:
    default_mode: restricted_autonomous
    timeout_seconds: 30
    max_stdout_bytes: 1048576
    max_stderr_bytes: 262144
"#
    .to_string()
}

/// Builder for configurable test provider instances.
///
/// Creates lightweight `TestProvider` instances that implement the `Provider`
/// trait for use in unit tests. The builder pattern lets each test configure
/// only the behavior it cares about without boilerplate.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::test_utils::TestProviderBuilder;
/// use xzatoma::providers::Provider;
///
/// let provider = TestProviderBuilder::new()
///     .with_model("test-model")
///     .with_completion("Hello from test!")
///     .build();
///
/// assert_eq!(provider.get_current_model(), "test-model".to_string());
/// assert!(provider.is_authenticated());
/// ```
pub struct TestProviderBuilder {
    model: String,
    authenticated: bool,
    completion_text: String,
}

impl TestProviderBuilder {
    /// Create a new test provider builder with sensible defaults.
    ///
    /// Defaults:
    /// - `model`: `"test-model"`
    /// - `authenticated`: `true`
    /// - `completion_text`: `"test response"`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzatoma::test_utils::TestProviderBuilder;
    ///
    /// let builder = TestProviderBuilder::new();
    /// ```
    pub fn new() -> Self {
        Self {
            model: "test-model".to_string(),
            authenticated: true,
            completion_text: "test response".to_string(),
        }
    }

    /// Set the model name returned by the provider.
    ///
    /// # Arguments
    ///
    /// * `model` - Model name string
    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    /// Set whether the provider reports as authenticated.
    ///
    /// # Arguments
    ///
    /// * `authenticated` - Authentication state
    pub fn with_authenticated(mut self, authenticated: bool) -> Self {
        self.authenticated = authenticated;
        self
    }

    /// Set the text content returned by `complete`.
    ///
    /// # Arguments
    ///
    /// * `text` - Completion response text
    pub fn with_completion(mut self, text: &str) -> Self {
        self.completion_text = text.to_string();
        self
    }

    /// Build and return a configured `TestProvider`.
    pub fn build(self) -> TestProvider {
        TestProvider {
            model: self.model,
            authenticated: self.authenticated,
            completion_text: self.completion_text,
        }
    }
}

impl Default for TestProviderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// A configurable test implementation of the `Provider` trait.
///
/// Used in unit tests to simulate provider behavior without making real HTTP
/// calls. Create instances via [`TestProviderBuilder`].
///
/// # Examples
///
/// ```no_run
/// use xzatoma::test_utils::TestProviderBuilder;
/// use xzatoma::providers::{Provider, Message};
///
/// # tokio_test::block_on(async {
/// let provider = TestProviderBuilder::new()
///     .with_completion("Hello!")
///     .build();
///
/// let messages = vec![Message::user("Hi")];
/// let response = provider.complete(&messages, &[]).await.unwrap();
/// assert_eq!(response.message.content.as_deref().unwrap_or(""), "Hello!");
/// # });
/// ```
pub struct TestProvider {
    model: String,
    authenticated: bool,
    completion_text: String,
}

#[async_trait]
impl Provider for TestProvider {
    fn is_authenticated(&self) -> bool {
        self.authenticated
    }

    fn current_model(&self) -> Option<&str> {
        Some(&self.model)
    }

    fn set_model(&mut self, model: &str) {
        self.model = model.to_string();
    }

    async fn fetch_models(&self) -> Result<Vec<ModelInfo>> {
        Ok(vec![ModelInfo::new(
            self.model.clone(),
            self.model.clone(),
            4096,
        )])
    }

    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[serde_json::Value],
    ) -> Result<CompletionResponse> {
        let message = Message::assistant(&self.completion_text);
        Ok(CompletionResponse::new(message))
    }

    fn get_current_model(&self) -> String {
        self.model.clone()
    }

    fn get_provider_capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temp_dir_creation() {
        let dir = temp_dir();
        assert!(dir.path().exists());
    }

    #[test]
    fn test_create_test_file() {
        let dir = temp_dir();
        let path = create_test_file(&dir, "test.txt", "content");
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "content");
    }

    #[test]
    fn test_assert_error_contains_success() {
        let result: std::result::Result<(), XzatomaError> =
            Err(XzatomaError::Config("test error message".to_string()));
        assert_error_contains(result, "test error");
    }

    #[test]
    #[should_panic(expected = "Expected error containing")]
    fn test_assert_error_contains_ok() {
        let result: std::result::Result<(), XzatomaError> = Ok(());
        assert_error_contains(result, "error");
    }

    #[test]
    #[should_panic(expected = "does not contain")]
    fn test_assert_error_contains_wrong_message() {
        let result: std::result::Result<(), XzatomaError> =
            Err(XzatomaError::Config("different error".to_string()));
        assert_error_contains(result, "not present");
    }

    #[test]
    fn test_test_config() {
        let config = test_config();
        assert_eq!(config.provider.provider_type, "copilot");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_test_config_yaml() {
        let yaml = test_config_yaml();
        assert!(yaml.contains("provider:"));
        assert!(yaml.contains("agent:"));
        let config: Config = serde_yaml::from_str(&yaml).unwrap();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_test_provider_builder_default_authenticated() {
        let provider = TestProviderBuilder::new().build();
        assert!(provider.is_authenticated());
    }

    #[test]
    fn test_test_provider_builder_with_model() {
        let provider = TestProviderBuilder::new()
            .with_model("custom-model")
            .build();
        assert_eq!(provider.get_current_model(), "custom-model".to_string());
    }

    #[test]
    fn test_test_provider_builder_with_unauthenticated() {
        let provider = TestProviderBuilder::new().with_authenticated(false).build();
        assert!(!provider.is_authenticated());
    }

    #[tokio::test]
    async fn test_test_provider_complete_returns_configured_text() {
        let provider = TestProviderBuilder::new()
            .with_completion("configured response")
            .build();
        let messages = vec![Message::user("hello")];
        let response = provider.complete(&messages, &[]).await.unwrap();
        assert_eq!(
            response.message.content.as_deref().unwrap_or(""),
            "configured response"
        );
    }

    #[tokio::test]
    async fn test_test_provider_fetch_models_returns_configured_model() {
        let provider = TestProviderBuilder::new().with_model("my-model").build();
        let models = provider.fetch_models().await.unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name, "my-model");
    }

    #[test]
    fn test_test_provider_builder_default() {
        let builder = TestProviderBuilder::default();
        let provider = builder.build();
        assert_eq!(provider.get_current_model(), "test-model".to_string());
    }
}
