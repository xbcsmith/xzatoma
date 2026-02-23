//! Integration tests for Phase 2: Provider Factory and Instantiation
//!
//! Tests subagent provider override functionality including:
//! - Provider factory with overrides
//! - SubagentTool instantiation with different providers
//! - Nested subagent provider inheritance

use std::sync::Arc;
use xzatoma::config::{AgentConfig, CopilotConfig, OllamaConfig, ProviderConfig, SubagentConfig};
use xzatoma::providers::create_provider_with_override;
use xzatoma::tools::subagent::SubagentTool;
use xzatoma::tools::ToolRegistry;

/// Helper to create a test provider config
fn create_test_provider_config() -> ProviderConfig {
    ProviderConfig {
        provider_type: "copilot".to_string(),
        copilot: CopilotConfig {
            model: "gpt-5.3-codex".to_string(),
            api_base: None,
            enable_streaming: true,
            enable_endpoint_fallback: true,
            reasoning_effort: None,
            include_reasoning: false,
        },
        ollama: OllamaConfig {
            host: "http://localhost:11434".to_string(),
            model: "llama3.2:3b".to_string(),
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

#[test]
fn test_create_provider_with_override_no_override() {
    let config = create_test_provider_config();

    // No overrides - should use config defaults
    let result = create_provider_with_override(&config, None, None);
    assert!(result.is_ok());

    let provider = result.unwrap();
    // Should use copilot provider from config
    assert!(provider.get_current_model().is_ok());
}

#[test]
fn test_create_provider_with_override_provider_only() {
    let config = create_test_provider_config();

    // Override to ollama provider
    let result = create_provider_with_override(&config, Some("ollama"), None);
    assert!(result.is_ok());

    let provider = result.unwrap();
    assert!(provider.get_current_model().is_ok());
}

#[test]
fn test_create_provider_with_override_both() {
    let config = create_test_provider_config();

    // Override both provider and model
    let result = create_provider_with_override(&config, Some("ollama"), Some("llama3.2:3b"));
    assert!(result.is_ok());

    let provider = result.unwrap();
    assert!(provider.get_current_model().is_ok());
}

#[test]
fn test_create_provider_with_override_model_only_copilot() {
    let config = create_test_provider_config();

    // Override model only (uses copilot from config)
    let result = create_provider_with_override(&config, None, Some("gpt-5.1-codex-mini"));
    assert!(result.is_ok());
}

#[test]
fn test_create_provider_with_override_invalid_provider() {
    let config = create_test_provider_config();

    // Invalid provider type
    let result = create_provider_with_override(&config, Some("invalid"), None);
    assert!(result.is_err());

    if let Err(e) = result {
        let err_msg = e.to_string();
        assert!(err_msg.contains("Unknown provider type"));
    }
}

#[test]
fn test_subagent_tool_new_with_config_no_override() {
    let provider_config = create_test_provider_config();
    let agent_config = create_test_agent_config();

    // Create parent provider
    let parent_provider = create_provider_with_override(&provider_config, None, None)
        .expect("Failed to create parent provider");
    let parent_provider_arc = Arc::from(parent_provider);

    // Create subagent tool without override (should share parent provider)
    let result = SubagentTool::new_with_config(
        parent_provider_arc,
        &provider_config,
        agent_config,
        ToolRegistry::new(),
        0,
    );

    assert!(result.is_ok());
}

#[test]
fn test_subagent_tool_new_with_config_provider_override() {
    let provider_config = create_test_provider_config();
    let mut agent_config = create_test_agent_config();

    // Configure subagent to use ollama provider
    agent_config.subagent.provider = Some("ollama".to_string());

    // Create parent provider (copilot)
    let parent_provider = create_provider_with_override(&provider_config, None, None)
        .expect("Failed to create parent provider");
    let parent_provider_arc = Arc::from(parent_provider);

    // Create subagent tool with provider override
    let result = SubagentTool::new_with_config(
        parent_provider_arc,
        &provider_config,
        agent_config,
        ToolRegistry::new(),
        0,
    );

    assert!(result.is_ok());
    // Subagent should have created its own ollama provider instance
}

#[test]
fn test_subagent_tool_new_with_config_model_override() {
    let provider_config = create_test_provider_config();
    let mut agent_config = create_test_agent_config();

    // Configure subagent to use different model with same provider
    agent_config.subagent.model = Some("gpt-5.1-codex-mini".to_string());

    // Create parent provider
    let parent_provider = create_provider_with_override(&provider_config, None, None)
        .expect("Failed to create parent provider");
    let parent_provider_arc = Arc::from(parent_provider);

    // Create subagent tool with model override
    let result = SubagentTool::new_with_config(
        parent_provider_arc,
        &provider_config,
        agent_config,
        ToolRegistry::new(),
        0,
    );

    assert!(result.is_ok());
}

#[test]
fn test_subagent_tool_new_with_config_provider_and_model_override() {
    let provider_config = create_test_provider_config();
    let mut agent_config = create_test_agent_config();

    // Configure subagent to use ollama with specific model
    agent_config.subagent.provider = Some("ollama".to_string());
    agent_config.subagent.model = Some("llama3.2:3b".to_string());

    // Create parent provider (copilot)
    let parent_provider = create_provider_with_override(&provider_config, None, None)
        .expect("Failed to create parent provider");
    let parent_provider_arc = Arc::from(parent_provider);

    // Create subagent tool with both overrides
    let result = SubagentTool::new_with_config(
        parent_provider_arc,
        &provider_config,
        agent_config,
        ToolRegistry::new(),
        0,
    );

    assert!(result.is_ok());
}

#[test]
fn test_subagent_tool_new_with_config_invalid_provider_override() {
    let provider_config = create_test_provider_config();
    let mut agent_config = create_test_agent_config();

    // Configure subagent with invalid provider
    agent_config.subagent.provider = Some("invalid_provider".to_string());

    // Create parent provider
    let parent_provider = create_provider_with_override(&provider_config, None, None)
        .expect("Failed to create parent provider");
    let parent_provider_arc = Arc::from(parent_provider);

    // Create subagent tool should fail
    let result = SubagentTool::new_with_config(
        parent_provider_arc,
        &provider_config,
        agent_config,
        ToolRegistry::new(),
        0,
    );

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

#[test]
fn test_provider_override_copilot_to_ollama() {
    let mut provider_config = create_test_provider_config();
    provider_config.provider_type = "copilot".to_string();

    // Parent uses copilot
    let parent_provider = create_provider_with_override(&provider_config, None, None)
        .expect("Failed to create parent provider");

    // Subagent overrides to ollama
    let subagent_provider =
        create_provider_with_override(&provider_config, Some("ollama"), Some("llama3.2:3b"))
            .expect("Failed to create subagent provider");

    // Both should be valid but different providers
    assert!(parent_provider.get_current_model().is_ok());
    assert!(subagent_provider.get_current_model().is_ok());
}

#[test]
fn test_provider_override_ollama_to_copilot() {
    let mut provider_config = create_test_provider_config();
    provider_config.provider_type = "ollama".to_string();

    // Parent uses ollama
    let parent_provider = create_provider_with_override(&provider_config, None, None)
        .expect("Failed to create parent provider");

    // Subagent overrides to copilot
    let subagent_provider = create_provider_with_override(
        &provider_config,
        Some("copilot"),
        Some("gpt-5.1-codex-mini"),
    )
    .expect("Failed to create subagent provider");

    // Both should be valid but different providers
    assert!(parent_provider.get_current_model().is_ok());
    assert!(subagent_provider.get_current_model().is_ok());
}

#[test]
fn test_model_override_same_provider() {
    let provider_config = create_test_provider_config();

    // Create provider with default model
    let default_provider = create_provider_with_override(&provider_config, None, None)
        .expect("Failed to create default provider");

    // Create provider with model override
    let custom_provider =
        create_provider_with_override(&provider_config, None, Some("gpt-5.1-codex-mini"))
            .expect("Failed to create custom provider");

    // Both should be valid copilot providers
    assert!(default_provider.get_current_model().is_ok());
    assert!(custom_provider.get_current_model().is_ok());
}

#[test]
fn test_backward_compatibility_no_subagent_config() {
    let provider_config = create_test_provider_config();
    let agent_config = AgentConfig::default();

    // Default agent config has no provider override
    assert_eq!(agent_config.subagent.provider, None);
    assert_eq!(agent_config.subagent.model, None);

    // Create parent provider
    let parent_provider = create_provider_with_override(&provider_config, None, None)
        .expect("Failed to create parent provider");
    let parent_provider_arc = Arc::from(parent_provider);

    // SubagentTool should work with default config (no override)
    let result = SubagentTool::new_with_config(
        parent_provider_arc,
        &provider_config,
        agent_config,
        ToolRegistry::new(),
        0,
    );

    assert!(result.is_ok());
}

#[test]
fn test_multiple_subagent_tools_different_providers() {
    let provider_config = create_test_provider_config();
    let mut agent_config1 = create_test_agent_config();
    let mut agent_config2 = create_test_agent_config();

    // First subagent uses copilot
    agent_config1.subagent.provider = Some("copilot".to_string());

    // Second subagent uses ollama
    agent_config2.subagent.provider = Some("ollama".to_string());

    // Create parent provider
    let parent_provider = create_provider_with_override(&provider_config, None, None)
        .expect("Failed to create parent provider");
    let parent_provider_arc = Arc::from(parent_provider);

    // Create two subagent tools with different providers
    let tool1 = SubagentTool::new_with_config(
        Arc::clone(&parent_provider_arc),
        &provider_config,
        agent_config1,
        ToolRegistry::new(),
        0,
    );

    let tool2 = SubagentTool::new_with_config(
        Arc::clone(&parent_provider_arc),
        &provider_config,
        agent_config2,
        ToolRegistry::new(),
        0,
    );

    assert!(tool1.is_ok());
    assert!(tool2.is_ok());
}
