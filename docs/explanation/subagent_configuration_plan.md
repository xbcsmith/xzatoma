# Subagent Configuration Enhancement Implementation Plan

## Overview

This plan implements configurable provider and model selection for subagents, along with chat mode controls for enabling/disabling subagent delegation. The implementation allows users to specify different models and providers for subagent execution (e.g., using gpt-5.3-codex for subagents while the main agent uses gpt-5.3-codex), and provides explicit control over when subagents are available in chat mode.

## Current State Analysis

### Existing Infrastructure

XZatoma currently has:

- **Subagent tool infrastructure** (`src/tools/subagent.rs`, `src/tools/parallel_subagent.rs`) - Working subagent delegation system
- **Provider abstraction** (`src/providers/base.rs`) - Trait-based provider system supporting multiple backends (Copilot, Ollama)
- **Configuration system** (`src/config.rs`) - Comprehensive YAML-based configuration with environment variable and CLI overrides
- **SubagentConfig** - Existing configuration structure for quotas, depth limits, telemetry, and persistence
- **Chat mode infrastructure** (`src/chat_mode.rs`) - Working chat mode system with planning/write modes
- **Tool registry filtering** - Existing capability to selectively enable/disable tools via `clone_without()` and `clone_without_parallel()`
- **Provider sharing** - Subagents already share the parent's provider via `Arc<dyn Provider>`

### Identified Issues

1. **Hard-coded provider sharing** - Subagents always inherit the parent agent's provider and model with no configuration override
2. **No model selection** - Cannot specify different models for subagent execution (e.g., cheaper/faster models for delegation)
3. **No provider override** - Cannot use different providers for subagents (e.g., Ollama for subagents, Copilot for main)
4. **Always-enabled in chat** - Subagent tool is always registered in chat mode, even when not needed
5. **No explicit control** - No way for users to request subagent enablement via prompt patterns or commands
6. **Missing configuration schema** - No configuration structure for per-subagent provider/model settings
7. **Limited testing** - No tests for provider override or selective enablement scenarios

## Implementation Phases

### Phase 1: Configuration Schema and Parsing

**Goal**: Add configuration structure for subagent-specific provider and model selection

#### Task 1.1: Extend SubagentConfig Structure

**Files**: `src/config.rs`

Add new fields to `SubagentConfig`:

```rust
pub struct SubagentConfig {
    // ... existing fields ...

    /// Optional provider override for subagents
    /// If None, subagents use parent agent's provider
    /// If Some("copilot" | "ollama"), creates dedicated provider instance
    #[serde(default)]
    pub provider: Option<String>,

    /// Optional model override for subagents
    /// If None, uses provider's default model
    /// If Some("model-name"), overrides the model for subagent provider
    #[serde(default)]
    pub model: Option<String>,

    /// Enable subagents in chat mode by default
    /// If false, requires explicit prompt pattern or command to enable
    #[serde(default = "default_chat_enabled")]
    pub chat_enabled: bool,
}

fn default_chat_enabled() -> bool {
    false // Disabled by default per requirements
}
```

**Implementation details**:
- Add serde deserialize support for optional fields
- Maintain backward compatibility (None = current behavior)
- Add validation in `Config::validate()` to ensure valid provider types
- Update default implementation

#### Task 1.2: Update Configuration File Schema

**Files**: `config/config.yaml`

Add subagent provider/model configuration section:

```yaml
agent:
  subagent:
    # ... existing fields ...

    # Optional: Override provider for subagents
    # provider: copilot  # or "ollama"

    # Optional: Override model for subagents
    # model: gpt-5.3-codex

    # Enable subagents in chat mode by default
    chat_enabled: false
```

**Implementation details**:
- Document all new fields with inline comments
- Provide examples for common use cases
- Update example configurations in docs

#### Task 1.3: Configuration Validation

**Files**: `src/config.rs`

Add validation logic in `Config::validate()`:

```rust
impl Config {
    pub fn validate(&self) -> Result<()> {
        // ... existing validation ...

        // Validate subagent provider if specified
        if let Some(ref provider) = self.agent.subagent.provider {
            if provider != "copilot" && provider != "ollama" {
                return Err(XzatomaError::Config(
                    format!("Invalid subagent provider: {}", provider)
                ));
            }
        }

        // Validate model is specified if provider is set
        // (or use provider defaults)

        Ok(())
    }
}
```

#### Task 1.4: Testing Requirements

**Files**: `src/config.rs` (tests module)

Test coverage:
- Deserialize configuration with subagent provider override
- Deserialize configuration with subagent model override
- Deserialize configuration with both provider and model
- Deserialize configuration with neither (backward compatibility)
- Validation rejects invalid provider types
- Validation accepts valid provider types
- Default values applied correctly

#### Task 1.5: Deliverables

- Extended `SubagentConfig` with provider/model fields
- Updated `config/config.yaml` with documented examples
- Validation logic for new configuration fields
- Unit tests for configuration parsing and validation
- Updated configuration documentation

#### Task 1.6: Success Criteria

- Configuration file parses with new optional fields
- Backward compatibility maintained (existing configs still work)
- Validation catches invalid provider/model combinations
- All tests pass with >80% coverage
- Documentation clearly explains configuration options

### Phase 2: Provider Factory and Instantiation

**Goal**: Implement provider creation logic for subagent-specific configurations

#### Task 2.1: Provider Factory Function

**Files**: `src/providers/mod.rs`

Create provider factory function:

```rust
/// Create a provider instance from configuration
///
/// # Arguments
///
/// * `provider_type` - Provider type ("copilot" or "ollama")
/// * `config` - Main configuration containing provider settings
/// * `model_override` - Optional model name override
///
/// # Returns
///
/// Arc-wrapped provider instance configured with specified model
pub fn create_provider_with_override(
    provider_type: &str,
    config: &Config,
    model_override: Option<&str>,
) -> Result<Arc<dyn Provider>> {
    match provider_type {
        "copilot" => {
            let mut copilot_config = config.provider.copilot.clone();
            if let Some(model) = model_override {
                copilot_config.model = model.to_string();
            }
            let provider = CopilotProvider::new(copilot_config)?;
            Ok(Arc::new(provider))
        }
        "ollama" => {
            let mut ollama_config = config.provider.ollama.clone();
            if let Some(model) = model_override {
                ollama_config.model = model.to_string();
            }
            let provider = OllamaProvider::new(ollama_config)?;
            Ok(Arc::new(provider))
        }
        _ => Err(XzatomaError::Provider(
            format!("Unknown provider type: {}", provider_type)
        )),
    }
}
```

**Implementation details**:
- Extract existing provider creation logic from `commands/mod.rs`
- Support model override via configuration cloning
- Return `Arc<dyn Provider>` for sharing with subagents
- Handle authentication for Copilot if needed

#### Task 2.2: SubagentTool Provider Selection

**Files**: `src/tools/subagent.rs`

Update `SubagentTool::new()` signature and implementation:

```rust
impl SubagentTool {
    pub fn new(
        provider: Arc<dyn Provider>,
        config: AgentConfig,
        parent_registry: ToolRegistry,
        current_depth: usize,
    ) -> Self {
        // Determine which provider to use for subagents
        let subagent_provider = if let Some(ref provider_type) = config.subagent.provider {
            // Create dedicated provider for subagents
            match create_provider_with_override(
                provider_type,
                &full_config, // Need to pass full config
                config.subagent.model.as_deref(),
            ) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        "Failed to create subagent provider, using parent: {}",
                        e
                    );
                    Arc::clone(&provider)
                }
            }
        } else {
            // Use parent's provider
            Arc::clone(&provider)
        };

        // ... rest of initialization with subagent_provider ...
    }
}
```

**Challenge**: `SubagentTool::new()` needs access to full `Config` to create provider instances, but currently only receives `AgentConfig`. Need to refactor signature.

#### Task 2.3: Update Tool Registration

**Files**: `src/commands/mod.rs`

Update subagent tool registration to pass full configuration:

```rust
// Register subagent tool for task delegation
let subagent_tool = SubagentTool::new(
    Arc::clone(&provider),
    config.clone(), // Pass full config instead of just agent config
    tools.clone(),
    0,
);
tools.register("subagent", Arc::new(subagent_tool));
```

#### Task 2.4: Testing Requirements

**Files**: `src/tools/subagent.rs` (tests), `src/providers/mod.rs` (tests)

Test coverage:
- Provider factory creates Copilot provider with model override
- Provider factory creates Ollama provider with model override
- SubagentTool uses parent provider when no override configured
- SubagentTool creates dedicated provider when override configured
- Provider override configuration is properly cloned and isolated
- Error handling when provider creation fails
- Model override is applied correctly to provider configuration

#### Task 2.5: Deliverables

- Provider factory function in `src/providers/mod.rs`
- Updated `SubagentTool::new()` with provider selection logic
- Fallback to parent provider on configuration errors
- Unit tests for provider factory and selection
- Integration tests for end-to-end provider override

#### Task 2.6: Success Criteria

- Subagents can use different provider than parent
- Subagents can use different model within same provider
- Graceful fallback when provider creation fails
- No regression in existing subagent functionality
- All tests pass with >80% coverage

### Phase 3: Chat Mode Subagent Control

**Goal**: Implement opt-in subagent enablement in chat mode with prompt pattern detection

#### Task 3.1: Tool Registry Builder Updates

**Files**: `src/tools/registry_builder.rs`

Add chat mode aware tool registration:

```rust
impl ToolRegistryBuilder {
    /// Build tools for chat mode
    ///
    /// # Arguments
    ///
    /// * `chat_mode` - Current chat mode (Planning or Write)
    /// * `subagents_enabled` - Whether to include subagent tools
    pub fn build_for_chat(
        &mut self,
        chat_mode: ChatMode,
        subagents_enabled: bool,
    ) -> ToolRegistry {
        let mut tools = ToolRegistry::new();

        // Add mode-appropriate tools
        match chat_mode {
            ChatMode::Planning => {
                tools.register("read_file", ...);
                tools.register("list_directory", ...);
                // ... other read-only tools
            }
            ChatMode::Write => {
                // All tools
            }
        }

        // Conditionally add subagent tools
        if subagents_enabled {
            tools.register("subagent", ...);
            tools.register("parallel_subagent", ...);
        }

        tools
    }
}
```

#### Task 3.2: Chat State Tracking

**Files**: `src/chat_mode.rs`

Add subagent enablement tracking to chat state:

```rust
pub struct ChatModeState {
    pub chat_mode: ChatMode,
    pub safety_mode: SafetyMode,
    pub subagents_enabled: bool, // New field
}

impl ChatModeState {
    pub fn new(chat_mode: ChatMode, safety_mode: SafetyMode) -> Self {
        Self {
            chat_mode,
            safety_mode,
            subagents_enabled: false, // Disabled by default
        }
    }

    pub fn enable_subagents(&mut self) {
        self.subagents_enabled = true;
    }

    pub fn disable_subagents(&mut self) {
        self.subagents_enabled = false;
    }

    pub fn toggle_subagents(&mut self) -> bool {
        self.subagents_enabled = !self.subagents_enabled;
        self.subagents_enabled
    }
}
```

#### Task 3.3: Prompt Pattern Detection

**Files**: `src/commands/mod.rs`

Add prompt analysis for subagent keywords:

```rust
/// Detect if user prompt requests subagent functionality
///
/// Looks for patterns like:
/// - "use subagents to..."
/// - "with subagent delegation..."
/// - "spawn subagents for..."
/// - "delegate to subagents..."
fn should_enable_subagents(prompt: &str) -> bool {
    let lower = prompt.to_lowercase();
    lower.contains("subagent")
        || lower.contains("delegate to")
        || lower.contains("spawn agent")
        || lower.contains("parallel task")
}

// In chat loop:
if !mode_state.subagents_enabled && should_enable_subagents(&trimmed) {
    mode_state.enable_subagents();
    println!("Enabling subagent delegation for this request");

    // Rebuild tool registry with subagents
    agent = rebuild_agent_with_subagents(...)?;
}
```

#### Task 3.4: Special Commands

**Files**: `src/commands/special_commands.rs`

Add `/subagents` command:

```rust
pub enum SpecialCommand {
    // ... existing variants ...

    /// Enable or disable subagent delegation
    ToggleSubagents(bool), // true = enable, false = disable
}

// In parse_special_command():
"/subagents on" | "/subagents enable" => {
    Ok(SpecialCommand::ToggleSubagents(true))
}
"/subagents off" | "/subagents disable" => {
    Ok(SpecialCommand::ToggleSubagents(false))
}
"/subagents" => {
    // Show current status
    Ok(SpecialCommand::ShowSubagentStatus)
}
```

#### Task 3.5: Agent Reconstruction

**Files**: `src/commands/mod.rs`

Add function to rebuild agent with updated tool registry:

```rust
fn rebuild_agent_tools(
    agent: &mut Agent,
    mode_state: &ChatModeState,
    config: &Config,
    working_dir: &Path,
    provider: Arc<dyn Provider>,
) -> Result<()> {
    let tools = build_tools_for_mode(
        mode_state,
        config,
        working_dir,
        mode_state.subagents_enabled,
    )?;

    // Update agent's tool registry
    agent.set_tools(tools)?;

    Ok(())
}
```

**Note**: May require adding `set_tools()` method to `Agent` struct.

#### Task 3.6: Testing Requirements

**Files**: `src/commands/mod.rs` (tests), `src/chat_mode.rs` (tests), `src/commands/special_commands.rs` (tests)

Test coverage:
- Subagents disabled by default in chat mode
- Prompt pattern detection correctly identifies subagent requests
- `/subagents on` command enables subagent tools
- `/subagents off` command disables subagent tools
- Tool registry correctly rebuilt when enabling/disabling
- Status display shows current subagent enablement state
- Configuration `chat_enabled: true` starts with subagents enabled
- Subagent enablement persists across mode switches

#### Task 3.7: Deliverables

- Chat mode state tracking for subagent enablement
- Prompt pattern detection for automatic enablement
- `/subagents` special command for manual control
- Tool registry rebuilding when enablement changes
- Status display integration
- Comprehensive test suite

#### Task 3.8: Success Criteria

- Subagents disabled by default in chat mode
- User can enable via prompt mention or command
- Tool registry dynamically updates without restarting
- Clear feedback when subagents are enabled/disabled
- All tests pass with >80% coverage
- Documentation updated with usage examples

### Phase 4: Documentation and User Experience

**Goal**: Provide comprehensive documentation and improve user experience for new features

#### Task 4.1: Configuration Documentation

**Files**: `docs/how-to/configure_subagents.md`

Create comprehensive guide covering:
- Overview of subagent configuration options
- Provider override configuration examples
- Model override configuration examples
- Chat mode enablement configuration
- Use case examples (cost optimization, speed optimization, provider mixing)
- Troubleshooting common configuration errors

#### Task 4.2: Chat Mode Usage Guide

**Files**: `docs/how-to/use_subagents_in_chat.md`

Create user guide covering:
- How to enable subagents in chat mode
- Prompt patterns that trigger automatic enablement
- Using `/subagents` command
- Checking subagent status
- Best practices for delegating tasks
- Performance considerations
- Cost implications of provider/model choices

#### Task 4.3: Help Text Updates

**Files**: `src/commands/special_commands.rs`

Update `print_help()` to include:

```rust
SUBAGENT DELEGATION:
  /subagents          - Show subagent enablement status
  /subagents on       - Enable subagent delegation
  /subagents off      - Disable subagent delegation
  /subagents enable   - Same as /subagents on
  /subagents disable  - Same as /subagents off

NOTE: Subagents allow delegating tasks to separate agent instances.
Mention "subagents" in your prompt to auto-enable.
```

#### Task 4.4: Status Display Enhancement

**Files**: `src/commands/mod.rs`

Update status display to show subagent configuration:

```rust
fn print_status_display(
    mode_state: &ChatModeState,
    tool_count: usize,
    conversation_len: usize,
    subagent_config: &SubagentConfig,
) {
    println!("=== Current Status ===");
    println!("Mode: {}", mode_state.chat_mode);
    println!("Safety: {}", mode_state.safety_mode);
    println!("Subagents: {}", if mode_state.subagents_enabled {
        "Enabled"
    } else {
        "Disabled"
    });

    if let Some(ref provider) = subagent_config.provider {
        println!("Subagent Provider: {}", provider);
    }
    if let Some(ref model) = subagent_config.model {
        println!("Subagent Model: {}", model);
    }

    // ... rest of status ...
}
```

#### Task 4.5: Examples and Recipes

**Files**: `docs/tutorials/subagent_configuration_examples.md`

Create tutorial with practical examples:

1. Cost-optimized setup (cheap model for subagents)
2. Speed-optimized setup (fast local model for subagents)
3. Provider mixing (Copilot main, Ollama subagents)
4. Chat workflow examples (enabling for specific tasks)
5. Configuration file templates for common scenarios

#### Task 4.6: Testing Requirements

**Files**: Documentation review checklist

Quality checks:
- All examples tested and verified working
- Screenshots or command output included
- Cross-references between documents accurate
- Terminology consistent across all docs
- Code examples follow project conventions
- Troubleshooting section covers common errors

#### Task 4.7: Deliverables

- Configuration guide in `docs/how-to/`
- Chat mode usage guide in `docs/how-to/`
- Updated help text in special commands
- Enhanced status display with subagent info
- Tutorial with practical examples
- Updated main README.md with feature summary

#### Task 4.8: Success Criteria

- Users can configure subagent provider/model without code changes
- Chat mode subagent control is intuitive and discoverable
- Documentation covers all common use cases
- Help text accessible and comprehensive
- Examples work as documented
- No questions left unanswered in docs

### Phase 5: Testing and Validation

**Goal**: Comprehensive testing across all scenarios and edge cases

#### Task 5.1: Integration Tests

**Files**: `tests/subagent_configuration_integration.rs`

Create integration tests:

```rust
#[tokio::test]
async fn test_subagent_provider_override_copilot() {
    // Main agent uses Ollama, subagents use Copilot
}

#[tokio::test]
async fn test_subagent_model_override() {
    // Main uses gpt-5.3-codex, subagents use gpt-3.5-turbo
}

#[tokio::test]
async fn test_chat_subagent_disabled_by_default() {
    // Verify subagent tool not available initially
}

#[tokio::test]
async fn test_chat_subagent_prompt_detection() {
    // Verify prompt with "subagent" enables tool
}

#[tokio::test]
async fn test_chat_subagent_command_toggle() {
    // Verify /subagents command works
}
```

#### Task 5.2: Configuration Validation Tests

**Files**: `src/config.rs` (tests)

Test configuration edge cases:
- Invalid provider type in subagent config
- Model specified without provider
- Provider override with invalid credentials
- Configuration file syntax errors
- Environment variable overrides
- CLI argument overrides

#### Task 5.3: Error Handling Tests

**Files**: Various test modules

Test error scenarios:
- Provider creation failure falls back to parent
- Model not available on specified provider
- Authentication failure for subagent provider
- Network errors during subagent provider init
- Quota exhaustion with custom provider
- Tool registry rebuild failures

#### Task 5.4: Performance Tests

**Files**: `tests/subagent_performance.rs`

Verify performance characteristics:
- Provider override doesn't add significant latency
- Tool registry rebuild is fast enough for chat UX
- Memory usage reasonable with multiple providers
- Concurrent subagent execution with different providers
- Provider pooling and reuse working correctly

#### Task 5.5: Backward Compatibility Tests

**Files**: `tests/backward_compatibility.rs`

Ensure no regressions:
- Existing configurations work without changes
- Default behavior unchanged when new fields omitted
- Subagent execution identical when no override specified
- API compatibility maintained for programmatic usage
- Tool registry behavior unchanged in non-chat contexts

#### Task 5.6: Testing Requirements

Manual testing checklist:
- [ ] Test with Copilot main, Ollama subagents
- [ ] Test with Ollama main, Copilot subagents
- [ ] Test with same provider, different models
- [ ] Test chat mode prompt detection
- [ ] Test `/subagents` command in interactive session
- [ ] Test configuration file hot-reload (if supported)
- [ ] Test with invalid configurations
- [ ] Test authentication flows for both providers
- [ ] Verify quota tracking with provider override
- [ ] Verify metrics collection with provider override

#### Task 5.7: Deliverables

- Comprehensive integration test suite
- Configuration validation tests
- Error handling tests
- Performance benchmarks
- Backward compatibility tests
- Manual testing checklist and results
- Test coverage report (target >80%)

#### Task 5.8: Success Criteria

- All automated tests pass
- Test coverage >80% for new code
- No performance regressions
- Backward compatibility maintained
- Manual testing checklist completed
- All edge cases covered
- Error messages helpful and actionable

## Configuration Examples

### Example 1: Cost-Optimized (Cheap Model for Subagents)

```yaml
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

agent:
  subagent:
    # Use cheaper model for delegation
    model: gpt-3.5-turbo
    chat_enabled: false
    max_executions: 20
    max_total_tokens: 50000
```

**Use case**: Main agent uses powerful model for analysis, subagents use cheaper model for simple tasks.

### Example 2: Provider Mixing (Copilot Main, Ollama Subagents)

```yaml
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex
  ollama:
    host: http://localhost:11434
    model: llama3.2:latest

agent:
  subagent:
    # Use local Ollama for subagents (no API costs)
    provider: ollama
    model: llama3.2:latest
    chat_enabled: false
    max_depth: 2
```

**Use case**: Main agent uses cloud provider, subagents use free local models.

### Example 3: Speed-Optimized (Fast Model for Subagents)

```yaml
provider:
  type: ollama
  ollama:
    host: http://localhost:11434
    model: llama3.2:latest

agent:
  subagent:
    # Use faster model for quick delegation
    model: gemma2:2b
    chat_enabled: false
    default_max_turns: 5
```

**Use case**: Use larger model for main reasoning, smaller/faster model for quick tasks.

### Example 4: Chat Mode with Manual Enablement

```yaml
provider:
  type: copilot
  copilot:
    model: gpt-5.3-codex

agent:
  subagent:
    # Same provider, but control when available
    chat_enabled: false  # Must explicitly enable in chat
    max_executions: 10
```

**Usage**:
```
[PLANNING][SAFE] >>> /subagents on
Subagent delegation enabled

[PLANNING][SAFE] >>> Use subagents to analyze @src/
[Agent delegates tasks to subagents...]
```

## Migration Guide

### For Existing Users

No changes required for existing configurations. New features are opt-in:

1. **No provider override**: Subagents use parent's provider (current behavior)
2. **No model override**: Subagents use provider's default model (current behavior)
3. **chat_enabled default**: `false` (new behavior - more conservative)

To enable new features, add to your `config.yaml`:

```yaml
agent:
  subagent:
    provider: ollama  # Optional: override provider
    model: llama3.2:latest  # Optional: override model
    chat_enabled: true  # Optional: enable by default in chat
```

### Configuration Migration Steps

1. Backup existing `config.yaml`
2. Review new subagent configuration options
3. Decide on provider/model strategy for your use case
4. Update configuration file with desired overrides
5. Test with simple subagent delegation task
6. Monitor costs/performance with new configuration
7. Adjust quotas if using different provider/model

## Security Considerations

### Provider Credentials

- **Separate credentials**: If using different providers, ensure both are properly authenticated
- **Copilot tokens**: Subagent provider needs valid GitHub token
- **Ollama access**: Ensure subagent can reach Ollama host
- **Credential isolation**: Provider instances are isolated, credentials not shared

### Resource Quotas

- **Set conservative limits**: Different models have different cost/speed characteristics
- **Monitor usage**: Track subagent executions with metrics
- **Test small**: Start with low quotas when testing new configurations
- **Rate limiting**: Be aware of provider-specific rate limits

### Configuration Validation

- **Validate early**: Configuration errors caught at startup, not runtime
- **Fail safe**: Invalid provider config falls back to parent provider
- **Log warnings**: Configuration issues logged for debugging
- **Test configs**: Validate configuration in development before production

## Performance Considerations

### Provider Initialization

- **Lazy initialization**: Providers created on-demand, not at startup
- **Caching**: Provider instances reused across subagent executions
- **Connection pooling**: HTTP clients pool connections efficiently
- **Authentication caching**: Tokens cached to avoid repeated auth flows

### Chat Mode Dynamics

- **Tool registry rebuild**: Fast operation, minimal impact on chat UX
- **Prompt detection**: Simple string matching, negligible overhead
- **State tracking**: Minimal memory overhead for enablement flag
- **Provider switching**: Only creates provider when configuration specifies override

### Memory Usage

- **Provider sharing**: Parent and subagents share provider via Arc when no override
- **Provider isolation**: Dedicated provider instance when override specified
- **Registry cloning**: Tool registry efficiently cloned with Arc references
- **Conversation isolation**: Each subagent has independent conversation (intentional)

## Open Questions and Decisions

### Question 1: Provider Instance Lifecycle

**Options**:
A. Create provider instance at SubagentTool construction
B. Create provider lazily on first subagent execution
C. Create provider per subagent execution (expensive)

**Decision**: Option A - Create at tool construction for predictable initialization and better error reporting

### Question 2: Configuration Hot-Reload

**Options**:
A. Support configuration reload without restarting agent
B. Require agent restart for configuration changes
C. Hybrid: Some fields hot-reloadable, others require restart

**Decision**: Option B - Keep it simple, require restart for configuration changes

### Question 3: Chat Mode Default Enablement

**Options**:
A. Subagents disabled by default (require opt-in)
B. Subagents enabled by default (current behavior)
C. Configuration-driven default (let user choose)

**Decision**: Option A (per requirements) - Disabled by default, explicit enablement required

### Question 4: Prompt Detection Sensitivity

**Options**:
A. Strict matching (only "subagent" keyword)
B. Fuzzy matching (variations like "sub-agent", "sub agent")
C. Semantic matching (LLM-based intent detection)

**Decision**: Option B - Reasonable fuzzy matching without LLM overhead

### Question 5: Provider Authentication

**Options**:
A. Separate authentication for subagent provider
B. Reuse parent authentication when possible
C. Require pre-authentication before starting

**Decision**: Option A - Separate authentication, allows mixing authenticated and local providers

## Risk Mitigation

### Risk 1: Provider Creation Failures

**Impact**: Subagent tool unavailable, delegation fails
**Mitigation**: Fallback to parent provider with warning log
**Testing**: Unit tests for provider factory error cases

### Risk 2: Configuration Complexity

**Impact**: Users confused by many options
**Mitigation**: Comprehensive documentation, sensible defaults, examples
**Testing**: Documentation review, user testing feedback

### Risk 3: Cost Overruns

**Impact**: Unexpected API costs with different models
**Mitigation**: Clear documentation, quota enforcement, cost examples
**Testing**: Integration tests with quota validation

### Risk 4: Authentication Errors

**Impact**: Subagent provider not authenticated, execution fails
**Mitigation**: Clear error messages, authentication guides
**Testing**: Tests with missing/invalid credentials

### Risk 5: Performance Regression

**Impact**: Chat mode feels sluggish with tool registry rebuilds
**Mitigation**: Performance testing, optimize rebuild path
**Testing**: Benchmark tool registry operations

## Success Metrics

### Functional Metrics

- All configuration fields parsed correctly
- Provider override working for both Copilot and Ollama
- Model override applied to provider instances
- Chat mode enablement working as expected
- Prompt detection accurately identifies subagent requests
- Special commands properly toggle enablement

### Quality Metrics

- Test coverage >80% for new code
- Zero regressions in existing functionality
- All edge cases handled gracefully
- Error messages clear and actionable
- Documentation complete and accurate

### Performance Metrics

- Provider creation <500ms
- Tool registry rebuild <50ms
- Prompt detection <1ms
- Memory overhead <10MB per provider instance
- No performance regression in existing paths

### User Experience Metrics

- Configuration intuitive for new users
- Help text discoverable and helpful
- Error messages guide users to solutions
- Documentation answers common questions
- Examples work as documented

## Timeline Estimate

- **Phase 1**: 2-3 days (Configuration schema and parsing)
- **Phase 2**: 3-4 days (Provider factory and instantiation)
- **Phase 3**: 3-4 days (Chat mode subagent control)
- **Phase 4**: 2-3 days (Documentation and UX)
- **Phase 5**: 2-3 days (Testing and validation)

**Total**: 12-17 days for complete implementation

**Dependencies**: Phases must be completed sequentially (each builds on previous)

## References

- Current subagent implementation: `src/tools/subagent.rs`
- Provider abstraction: `src/providers/base.rs`
- Configuration system: `src/config.rs`
- Chat mode infrastructure: `src/chat_mode.rs`
- Tool registry: `src/tools/mod.rs`
- Requirements: `docs/explanation/need_fixes.md`
