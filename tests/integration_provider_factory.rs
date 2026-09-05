//! Integration tests for Phase 2: Provider Factory and Instantiation
//!
//! Tests subagent provider override functionality including:
//! - Provider factory with overrides
//! - SubagentTool instantiation with different providers
//! - Nested subagent provider inheritance
//!
//! Provider construction now resolves the effective model against the
//! provider's live model list (see `xzatoma::providers::factory`), so these
//! tests use Ollama and OpenAI pointed at an unreachable local port with a
//! configured model: the model-list fetch fails, and since a model is
//! configured, resolution falls back to it (mirroring today's behavior when
//! a real server is briefly unreachable) without requiring a live server.
//! Copilot is deliberately not exercised here because
//! `CopilotProvider::authenticate()` reads/writes the real OS keyring and,
//! absent a cached token, performs a live GitHub OAuth device flow -- see
//! `tests/copilot_integration.rs` for the opt-in, keyring-gated pattern used
//! to test Copilot safely.

use std::sync::Arc;
use xzatoma::config::{
    AgentConfig, CopilotConfig, OllamaConfig, OpenAIConfig, ProviderConfig, SubagentConfig,
};
use xzatoma::providers::create_provider_with_override;
use xzatoma::tools::ToolRegistry;
use xzatoma::tools::subagent::SubagentTool;

/// A local port that is never listening in test/CI environments, used so
/// tests exercise a fast, local, deterministic connection failure instead of
/// reaching a real external host.
const UNREACHABLE_HOST: &str = "http://127.0.0.1:9";

/// Helper to create a test provider config. Defaults to Ollama pointed at an
/// unreachable host with a configured model.
fn create_test_provider_config() -> ProviderConfig {
    ProviderConfig {
        provider_type: "ollama".to_string(),
        copilot: CopilotConfig {
            model: "gpt-5.3-codex".to_string(),
            api_base: None,
            enable_streaming: true,
            enable_endpoint_fallback: true,
            reasoning_effort: None,
            include_reasoning: false,
            ..Default::default()
        },
        ollama: OllamaConfig {
            host: UNREACHABLE_HOST.to_string(),
            model: "llama3.2:3b".to_string(),
            request_timeout_seconds: 1,
            stream_idle_timeout_seconds: 120,
            num_ctx: None,
        },
        openai: OpenAIConfig {
            api_key: String::new(),
            base_url: UNREACHABLE_HOST.to_string(),
            model: "gpt-4.1-mini".to_string(),
            organization_id: None,
            enable_streaming: true,
            request_timeout_seconds: 1,
            stream_idle_timeout_seconds: 1,
            reasoning_effort: None,
        },
    }
}

/// Helper to create a test agent config
fn create_test_agent_config() -> AgentConfig {
    AgentConfig {
        max_turns: 5,
        subagent: SubagentConfig {
            max_depth: 3,
            default_max_turns: 3,
            ..SubagentConfig::default()
        },
        ..AgentConfig::default()
    }
}

#[tokio::test]
async fn test_create_provider_with_override_no_override() {
    let config = create_test_provider_config();

    // No overrides - should use config defaults
    let result = create_provider_with_override(&config, None, None).await;
    assert!(result.is_ok());

    let provider = result.unwrap();
    // Should use ollama provider from config
    assert!(!provider.get_current_model().is_empty());
}

#[tokio::test]
async fn test_create_provider_with_override_provider_only() {
    let config = create_test_provider_config();

    // Override to openai provider
    let result = create_provider_with_override(&config, Some("openai"), None).await;
    assert!(result.is_ok());

    let provider = result.unwrap();
    assert!(!provider.get_current_model().is_empty());
}

#[tokio::test]
async fn test_create_provider_with_override_both() {
    let config = create_test_provider_config();

    // Override both provider and model
    let result = create_provider_with_override(&config, Some("ollama"), Some("llama3.2:3b")).await;
    assert!(result.is_ok());

    let provider = result.unwrap();
    assert!(!provider.get_current_model().is_empty());
}

#[tokio::test]
async fn test_create_provider_with_override_model_only_ollama() {
    let config = create_test_provider_config();

    // Override model only (uses ollama from config)
    let result = create_provider_with_override(&config, None, Some("llama3.2:1b")).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_create_provider_with_override_invalid_provider() {
    let config = create_test_provider_config();

    // Invalid provider type
    let result = create_provider_with_override(&config, Some("invalid"), None).await;
    assert!(result.is_err());

    if let Err(e) = result {
        let err_msg = e.to_string();
        assert!(err_msg.contains("Unknown provider type"));
    }
}

#[tokio::test]
async fn test_subagent_tool_new_with_config_no_override() {
    let provider_config = create_test_provider_config();
    let agent_config = create_test_agent_config();

    // Create parent provider
    let parent_provider = create_provider_with_override(&provider_config, None, None)
        .await
        .expect("Failed to create parent provider");
    let parent_provider_arc = Arc::from(parent_provider);

    // Create subagent tool without override (should share parent provider)
    let result = SubagentTool::new_with_config(
        parent_provider_arc,
        &provider_config,
        agent_config,
        ToolRegistry::new(),
        0,
    )
    .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_subagent_tool_new_with_config_provider_override() {
    let provider_config = create_test_provider_config();
    let mut agent_config = create_test_agent_config();

    // Configure subagent to use openai provider
    agent_config.subagent.provider = Some("openai".to_string());

    // Create parent provider (ollama)
    let parent_provider = create_provider_with_override(&provider_config, None, None)
        .await
        .expect("Failed to create parent provider");
    let parent_provider_arc = Arc::from(parent_provider);

    // Create subagent tool with provider override
    let result = SubagentTool::new_with_config(
        parent_provider_arc,
        &provider_config,
        agent_config,
        ToolRegistry::new(),
        0,
    )
    .await;

    assert!(result.is_ok());
    // Subagent should have created its own openai provider instance
}

#[tokio::test]
async fn test_subagent_tool_new_with_config_model_override() {
    let provider_config = create_test_provider_config();
    let mut agent_config = create_test_agent_config();

    // Configure subagent to use different model with same provider
    agent_config.subagent.model = Some("llama3.2:1b".to_string());

    // Create parent provider
    let parent_provider = create_provider_with_override(&provider_config, None, None)
        .await
        .expect("Failed to create parent provider");
    let parent_provider_arc = Arc::from(parent_provider);

    // Create subagent tool with model override
    let result = SubagentTool::new_with_config(
        parent_provider_arc,
        &provider_config,
        agent_config,
        ToolRegistry::new(),
        0,
    )
    .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_subagent_tool_new_with_config_provider_and_model_override() {
    let provider_config = create_test_provider_config();
    let mut agent_config = create_test_agent_config();

    // Configure subagent to use openai with specific model
    agent_config.subagent.provider = Some("openai".to_string());
    agent_config.subagent.model = Some("gpt-4.1-mini".to_string());

    // Create parent provider (ollama)
    let parent_provider = create_provider_with_override(&provider_config, None, None)
        .await
        .expect("Failed to create parent provider");
    let parent_provider_arc = Arc::from(parent_provider);

    // Create subagent tool with both overrides
    let result = SubagentTool::new_with_config(
        parent_provider_arc,
        &provider_config,
        agent_config,
        ToolRegistry::new(),
        0,
    )
    .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_subagent_tool_new_with_config_invalid_provider_override() {
    let provider_config = create_test_provider_config();
    let mut agent_config = create_test_agent_config();

    // Configure subagent with invalid provider
    agent_config.subagent.provider = Some("invalid_provider".to_string());

    // Create parent provider
    let parent_provider = create_provider_with_override(&provider_config, None, None)
        .await
        .expect("Failed to create parent provider");
    let parent_provider_arc = Arc::from(parent_provider);

    // Create subagent tool should fail
    let result = SubagentTool::new_with_config(
        parent_provider_arc,
        &provider_config,
        agent_config,
        ToolRegistry::new(),
        0,
    )
    .await;

    assert!(result.is_err());
    if let Err(e) = result {
        let err_msg = e.to_string();
        assert!(err_msg.contains("Unknown provider type"));
    }
}

#[test]
fn test_subagent_config_defaults() {
    let config = SubagentConfig::default();

    // Verify defaults from Phase 1
    assert_eq!(config.provider, None);
    assert_eq!(config.model, None);
    assert!(!config.chat_enabled);
}

#[tokio::test]
async fn test_provider_override_ollama_to_openai() {
    let mut provider_config = create_test_provider_config();
    provider_config.provider_type = "ollama".to_string();

    // Parent uses ollama
    let parent_provider = create_provider_with_override(&provider_config, None, None)
        .await
        .expect("Failed to create parent provider");

    // Subagent overrides to openai
    let subagent_provider =
        create_provider_with_override(&provider_config, Some("openai"), Some("gpt-4.1-mini"))
            .await
            .expect("Failed to create subagent provider");

    // Both should be valid but different providers
    assert!(!parent_provider.get_current_model().is_empty());
    assert!(!subagent_provider.get_current_model().is_empty());
}

#[tokio::test]
async fn test_provider_override_openai_to_ollama() {
    let mut provider_config = create_test_provider_config();
    provider_config.provider_type = "openai".to_string();

    // Parent uses openai
    let parent_provider = create_provider_with_override(&provider_config, None, None)
        .await
        .expect("Failed to create parent provider");

    // Subagent overrides to ollama
    let subagent_provider =
        create_provider_with_override(&provider_config, Some("ollama"), Some("llama3.2:3b"))
            .await
            .expect("Failed to create subagent provider");

    // Both should be valid but different providers
    assert!(!parent_provider.get_current_model().is_empty());
    assert!(!subagent_provider.get_current_model().is_empty());
}

#[tokio::test]
async fn test_model_override_same_provider() {
    let provider_config = create_test_provider_config();

    // Create provider with default model
    let default_provider = create_provider_with_override(&provider_config, None, None)
        .await
        .expect("Failed to create default provider");

    // Create provider with model override
    let custom_provider =
        create_provider_with_override(&provider_config, None, Some("llama3.2:1b"))
            .await
            .expect("Failed to create custom provider");

    // Both should be valid ollama providers
    assert!(!default_provider.get_current_model().is_empty());
    assert!(!custom_provider.get_current_model().is_empty());
}

#[tokio::test]
async fn test_backward_compatibility_no_subagent_config() {
    let provider_config = create_test_provider_config();
    let agent_config = AgentConfig::default();

    // Default agent config has no provider override
    assert_eq!(agent_config.subagent.provider, None);
    assert_eq!(agent_config.subagent.model, None);

    // Create parent provider
    let parent_provider = create_provider_with_override(&provider_config, None, None)
        .await
        .expect("Failed to create parent provider");
    let parent_provider_arc = Arc::from(parent_provider);

    // SubagentTool should work with default config (no override)
    let result = SubagentTool::new_with_config(
        parent_provider_arc,
        &provider_config,
        agent_config,
        ToolRegistry::new(),
        0,
    )
    .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_multiple_subagent_tools_different_providers() {
    let provider_config = create_test_provider_config();
    let mut agent_config1 = create_test_agent_config();
    let mut agent_config2 = create_test_agent_config();

    // First subagent uses ollama
    agent_config1.subagent.provider = Some("ollama".to_string());

    // Second subagent uses openai
    agent_config2.subagent.provider = Some("openai".to_string());

    // Create parent provider
    let parent_provider = create_provider_with_override(&provider_config, None, None)
        .await
        .expect("Failed to create parent provider");
    let parent_provider_arc = Arc::from(parent_provider);

    // Create two subagent tools with different providers
    let tool1 = SubagentTool::new_with_config(
        Arc::clone(&parent_provider_arc),
        &provider_config,
        agent_config1,
        ToolRegistry::new(),
        0,
    )
    .await;

    let tool2 = SubagentTool::new_with_config(
        Arc::clone(&parent_provider_arc),
        &provider_config,
        agent_config2,
        ToolRegistry::new(),
        0,
    )
    .await;

    assert!(tool1.is_ok());
    assert!(tool2.is_ok());
}
