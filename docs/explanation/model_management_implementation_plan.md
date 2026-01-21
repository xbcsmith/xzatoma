---
description: Implementation plan for model management and provider capabilities
created: 2025-01-XX
status: draft
---

# Model Management Implementation Plan

## Overview

This plan outlines the implementation of comprehensive model management features for XZatoma, including:

1. **Model Listing** - List available models per provider (CLI and chat mode)
2. **Model Switching** - Dynamic model switching in chat mode
3. **Model Capability Detection** - Detect and display model capabilities (context window size, features)
4. **Token Usage Tracking** - Track token usage per session and display context window information

These features will be implemented at the provider abstraction layer to ensure consistency across all providers (Copilot, Ollama, and future providers).

## Current State Analysis

### Existing Infrastructure

- **Provider Trait** (`src/providers/base.rs`): Defines the `Provider` trait with a `complete()` method
- **Copilot Provider** (`src/providers/copilot.rs`): Implements GitHub Copilot integration with configurable model
- **Ollama Provider** (`src/providers/ollama.rs`): Implements Ollama integration with configurable model
- **Configuration System** (`src/config.rs`): Supports provider-specific configuration including model selection
- **Conversation Management** (`src/agent/conversation.rs`): Implements token counting (chars/4 heuristic) and pruning
- **Chat Mode** (`src/chat_mode.rs`): Provides interactive chat with special commands
- **Special Commands** (`src/commands/special_commands.rs`): Handles slash commands in chat mode
- **CLI Structure** (`src/cli.rs`): Defines command-line interface with subcommands

### Identified Issues

1. **No Model Discovery**: Cannot list available models from providers
2. **Static Model Selection**: Model is set at startup via config, cannot change during chat
3. **Missing Capability Information**: No visibility into model capabilities (context window, features)
4. **Opaque Token Usage**: Token counting exists but not exposed to users
5. **Provider Interface Too Minimal**: `Provider` trait lacks methods for metadata queries
6. **Heuristic Token Counting**: Uses chars/4 approximation instead of provider-specific tokenization

## Implementation Phases

### Phase 1: Enhanced Provider Trait and Metadata

Extend the provider abstraction to support model discovery and capability queries.

#### Task 1.1: Define Provider Metadata Structures

Create new types in `src/providers/base.rs` to represent model information:

- `ModelInfo` struct with fields: `name`, `display_name`, `context_window`, `supports_tools`, `supports_streaming`, `provider_specific` (HashMap)
- `ModelCapability` enum for feature flags (LongContext, FunctionCalling, Vision, etc.)
- `TokenUsage` struct with fields: `prompt_tokens`, `completion_tokens`, `total_tokens`
- `ProviderCapabilities` struct with fields: `supports_model_listing`, `supports_token_counts`, `supports_streaming`

#### Task 1.2: Extend Provider Trait

Add new async methods to the `Provider` trait in `src/providers/base.rs`:

- `async fn list_models(&self) -> Result<Vec<ModelInfo>>` - Return available models
- `async fn get_model_info(&self, model_name: &str) -> Result<ModelInfo>` - Get specific model details
- `fn get_current_model(&self) -> &str` - Return currently selected model name
- `fn get_provider_capabilities(&self) -> ProviderCapabilities` - Return provider-level capabilities
- `fn set_model(&mut self, model_name: String) -> Result<()>` - Change current model (if supported)

Modify `complete()` to optionally return token usage:

- Add `TokenUsage` to response or create new `CompletionResponse` struct containing both `Message` and `TokenUsage`

#### Task 1.3: Update Message/Response Types

Modify or create response types in `src/providers/base.rs`:

- Create `CompletionResponse` struct with `message: Message` and `usage: Option<TokenUsage>`
- Update `Provider::complete()` return type from `Result<Message>` to `Result<CompletionResponse>`

#### Task 1.4: Testing Requirements

- Unit tests for new structs (ModelInfo, TokenUsage serialization/deserialization)
- Mock provider implementing extended trait
- Test default implementations where applicable

#### Task 1.5: Deliverables

- Updated `src/providers/base.rs` with new types and trait methods
- Documentation for new API surface
- Unit tests for metadata structures

#### Task 1.6: Success Criteria

- Provider trait compiles with new methods
- All existing providers still compile (may use default/stub implementations)
- New types have comprehensive documentation
- Tests achieve >80% coverage

### Phase 2: Copilot Provider Implementation

Implement the extended provider interface for GitHub Copilot.

#### Task 2.1: Implement Model Listing for Copilot

Update `src/providers/copilot.rs`:

- Add `list_models()` implementation returning hardcoded list of supported Copilot models
- Known models: `gpt-4`, `gpt-4-turbo`, `gpt-3.5-turbo`, `claude-3.5-sonnet`, `claude-sonnet-4.5`, `o1-preview`, `o1-mini`
- Include context window sizes (e.g., gpt-4-turbo: 128k, claude-sonnet-4.5: 200k)
- Mark all as supporting tools/function calling

#### Task 2.2: Extract Token Usage from Copilot Response

Modify `complete()` in `src/providers/copilot.rs`:

- Parse `usage` field from `CopilotResponse` (already has `CopilotUsage` struct)
- Return `CompletionResponse` with extracted token counts
- Handle cases where usage is not provided

#### Task 2.3: Implement Model Switching for Copilot

Add to `CopilotProvider`:

- Wrap `config` field in `Arc<RwLock<CopilotConfig>>` for interior mutability (Decision from Question 3)
- Implement `set_model(&self, ...)` to update config model name with write lock
- Implement `get_current_model(&self)` to return current model with read lock
- Validate model name against supported models from `list_models()`

#### Task 2.4: Testing Requirements

- Test model listing returns expected models
- Test token usage extraction from responses
- Test model switching updates internal state
- Integration test with actual Copilot API (optional, may require auth)

#### Task 2.5: Deliverables

- Fully implemented extended provider interface for Copilot
- Updated tests in `src/providers/copilot.rs`

#### Task 2.6: Success Criteria

- `CopilotProvider` implements all new trait methods
- Token usage accurately extracted from API responses
- Model switching works without requiring provider recreation
- All tests pass

### Phase 3: Ollama Provider Implementation

Implement the extended provider interface for Ollama.

#### Task 3.1: Implement Model Listing for Ollama

Update `src/providers/ollama.rs`:

- Add `list_models()` using Ollama's `/api/tags` endpoint
- Parse response to extract model names and metadata
- Map Ollama model info to `ModelInfo` struct
- Handle connection errors gracefully (offline Ollama)
- Implement 5-minute cache with `Arc<RwLock<Option<(Vec<ModelInfo>, Instant)>>>` (Decision from Question 4)
- Return cached results if fresh, fetch and update cache if stale

#### Task 3.2: Implement Model Details for Ollama

Add to `OllamaProvider`:

- Implement `get_model_info()` using `/api/show` endpoint
- Extract context window size from model details (parameter_size, num_ctx)
- Detect tool support capability from model metadata
- Cache model info to avoid repeated API calls

#### Task 3.3: Extract Token Usage from Ollama Response

Modify `complete()` in `src/providers/ollama.rs`:

- Parse `prompt_eval_count` and `eval_count` from `OllamaResponse` (already available)
- Return `CompletionResponse` with token counts
- Calculate total tokens as sum of prompt and completion

#### Task 3.4: Implement Model Switching for Ollama

Add to `OllamaProvider`:

- Wrap `config` field in `Arc<RwLock<OllamaConfig>>` for interior mutability (Decision from Question 3)
- Implement `set_model(&self, ...)` to update model name with write lock
- Validate model exists by checking against `list_models()` (may trigger cache refresh)
- Implement `get_current_model(&self)` with read lock
- Invalidate model list cache on successful switch (Decision from Question 4)

#### Task 3.5: Testing Requirements

- Test model listing (requires mock Ollama server or test fixtures)
- Test model info retrieval
- Test token usage extraction
- Test model switching validation

#### Task 3.6: Deliverables

- Fully implemented extended provider interface for Ollama
- Updated tests with mock responses
- Error handling for offline scenarios

#### Task 3.7: Success Criteria

- `OllamaProvider` implements all new trait methods
- Model listing works with live Ollama instance
- Token usage extracted from responses
- Graceful degradation when Ollama is unavailable

### Phase 4: Agent Integration

Update the agent to use the new provider capabilities.

#### Task 4.1: Update Agent to Track Token Usage

Modify `src/agent/core.rs`:

- Update `execute()` to accumulate token usage from each completion
- Add session-level token tracking (total prompt, completion, cumulative)
- Expose token usage through new method `get_token_usage() -> TokenUsage`

#### Task 4.2: Update Conversation with Provider Token Counts

Modify `src/agent/conversation.rs`:

- Add option to use provider-reported token counts instead of heuristic
- Keep heuristic as fallback when provider doesn't report tokens (chars/4)
- Update `token_count` field based on actual usage when available
- Add method `update_from_provider_usage(&mut self, usage: &TokenUsage)`
- Prefer provider-reported counts over heuristic when available (Decision from Question 1)

#### Task 4.3: Expose Context Window Information

Add to `src/agent/core.rs` and `src/agent/conversation.rs`:

- Add method `get_context_info() -> ContextInfo` with fields: max_tokens, used_tokens, remaining_tokens, percentage_used
- Query provider for context window size on initialization
- Update conversation max_tokens based on provider model info

#### Task 4.4: Testing Requirements

- Test token accumulation across multiple turns
- Test context window calculation
- Test fallback to heuristic when provider doesn't report tokens

#### Task 4.5: Deliverables

- Updated `Agent` with token tracking
- Updated `Conversation` with provider token support
- New `ContextInfo` type

#### Task 4.6: Success Criteria

- Agent accurately tracks cumulative token usage
- Context window information reflects provider capabilities
- Backward compatibility with providers that don't report tokens

### Phase 5: CLI Commands for Model Management

Add new CLI commands for model operations.

#### Task 5.1: Define Model Subcommand

Update `src/cli.rs`:

- Add `Models` variant to `Commands` enum
- Add nested subcommands: `List`, `Info`, `Current`
- Options for `List`: `--provider <name>` to filter by provider
- Options for `Info`: `--model <name>` and `--provider <name>`

#### Task 5.2: Implement Model List Command

Create `src/commands/models.rs`:

- Function `list_models(config: &Config, provider_name: Option<&str>)`
- Load specified provider or default from config
- Call `provider.list_models()` and format output
- Display table with columns: Model Name, Context Window, Supports Tools
- Handle errors (provider unavailable, authentication required)

#### Task 5.3: Implement Model Info Command

Add to `src/commands/models.rs`:

- Function `show_model_info(config: &Config, model_name: &str, provider_name: Option<&str>)`
- Load provider and call `get_model_info()`
- Display detailed information: name, context window, capabilities, provider-specific details
- Pretty-print JSON for provider_specific metadata

#### Task 5.4: Implement Current Model Command

Add to `src/commands/models.rs`:

- Function `show_current_model(config: &Config, provider_name: Option<&str>)`
- Load provider and call `get_current_model()`
- Display current model and configuration source (config file, env var, default)

#### Task 5.5: Wire Up CLI Handler

Update `src/main.rs`:

- Add handler for `Commands::Models` variant
- Route to appropriate function based on subcommand
- Format and display output

#### Task 5.6: Testing Requirements

- Test CLI parsing for model commands
- Integration tests with mock providers
- Test output formatting

#### Task 5.7: Deliverables

- New `src/commands/models.rs` module
- Updated CLI with model management commands
- User documentation for new commands

#### Task 5.8: Success Criteria

- `xzatoma models list` works with both providers
- `xzatoma models info <name>` shows detailed model information
- `xzatoma models current` displays active model
- Error messages are helpful and actionable

### Phase 6: Chat Mode Model Management

Add model management capabilities to interactive chat mode.

#### Task 6.1: Add Model Special Commands

Update `src/commands/special_commands.rs`:

- Add `ListModels` variant to `SpecialCommand` enum
- Add `SwitchModel(String)` variant
- Add `ShowModelInfo` variant
- Add `ShowContextInfo` variant
- Parse `/models list`, `/model <name>`, `/model info`, `/context` commands

#### Task 6.2: Implement Model Listing in Chat

Update `src/commands/chat_mode.rs`:

- Handle `SpecialCommand::ListModels`
- Call `agent.provider.list_models()`
- Format and display model list in chat
- Highlight current model

#### Task 6.3: Implement Model Switching in Chat

Update `src/commands/chat_mode.rs`:

- Handle `SpecialCommand::SwitchModel(name)`
- Validate model is available via `list_models()`
- Get new model's context window from `get_model_info(name)`
- Check if current conversation tokens exceed new context window (Decision from Question 2)
- If exceeds: Display warning "Current conversation (X tokens) exceeds new model context (Y tokens). Z messages will be pruned. Continue? [y/N]"
- Require user confirmation before proceeding with switch
- On confirm: Call `agent.provider.set_model(name)`, update conversation `max_tokens`, trigger immediate pruning
- Display confirmation with new model, context window, and tokens pruned (if any)

#### Task 6.4: Implement Context Window Display

Add to chat mode:

- Handle `SpecialCommand::ShowContextInfo`
- Call `agent.get_context_info()`
- Display: Current tokens used, Total context window, Remaining tokens, Percentage used
- Visual indicator (progress bar or percentage) for context usage
- Colorize output (green <60%, yellow 60-85%, red >85%)

#### Task 6.5: Update Chat Prompt Display

Modify chat prompt to show context usage:

- Add optional context indicator to prompt
- Implement `format_token_count(used, total, format)` supporting multiple formats (Decision from Question 5)
- Default format: `[Planning|Write] [Safe|YOLO] [1.2k/8k] >`
- Alternative formats: `[1234/8000]`, `[15%]`, or `[1.2k/8k | 15%]` based on config
- Apply color coding: green (<60% used), yellow (60-85%), red (>85%)
- Make display configurable via `show_context_in_prompt` in chat config

#### Task 6.6: Testing Requirements

- Test model switching in chat mode
- Test context info display
- Test model listing in chat
- Test error handling for invalid model names

#### Task 6.7: Deliverables

- Updated special commands with model management
- Chat mode handlers for model operations
- Enhanced prompt with context information
- Updated help text with new commands

#### Task 6.8: Success Criteria

- `/models list` shows available models in chat
- `/model <name>` successfully switches models
- `/context` displays accurate context window information
- Chat prompt shows real-time context usage
- Switching models doesn't lose conversation history

### Phase 7: Configuration and Documentation

Complete configuration support and user documentation.

#### Task 7.1: Update Configuration Schema

Update `src/config.rs`:

- Add `show_context_in_prompt: bool` to `ChatConfig` (default: true)
- Add `context_display_format` enum: `Tokens | HumanFriendly | Percentage | Both` (default: HumanFriendly) (Decision from Question 5)
- Add `default_models: HashMap<String, String>` for provider -> model name mappings
- Add `model_cache_ttl_seconds: u64` (default: 300 for 5 minutes) (Decision from Question 4)
- Add validation for model names in config
- Add helper function to parse context display format from config strings

#### Task 7.2: Create Reference Documentation

Create `docs/reference/models.md`:

- Document all supported models per provider
- List context window sizes
- Document model capabilities
- Include performance characteristics where known
- Update regularly as providers add models

#### Task 7.3: Create How-To Guides

Create `docs/how-to/manage-models.md`:

- Guide for listing available models
- Guide for switching models in chat
- Guide for monitoring context usage
- Best practices for model selection
- Troubleshooting common issues

#### Task 7.4: Update CLI Help Text

Update command help in `src/cli.rs` and special command help in `src/commands/special_commands.rs`:

- Add examples for model commands
- Document context window display
- Add warnings about model switching

#### Task 7.5: Create API Documentation

Add to `docs/reference/provider-api.md`:

- Document extended Provider trait
- Example implementations
- Integration guide for new providers

#### Task 7.6: Testing Requirements

- Validate all documentation examples work
- Test configuration loading with model settings

#### Task 7.7: Deliverables

- Complete user documentation
- API documentation for providers
- Updated configuration schema
- Updated help text

#### Task 7.8: Success Criteria

- Documentation covers all features
- Examples are tested and working
- Configuration is well-documented
- Help text is comprehensive and accurate

## Open Questions

### Question 1: Token Counting Strategy ✅ RESOLVED

Should we implement provider-specific tokenizers or continue with heuristic fallback?

**Option A**: Use provider-reported tokens only (require all providers to report)
**Option B**: Implement tiktoken for Copilot, use Ollama's counts, keep heuristic fallback
**Option C**: Always use heuristic for consistency across providers

**Decision**: Option B - Use provider-reported tokens when available, heuristic as fallback

**Implementation Impact**:

- Phase 2 (Copilot): Extract token usage from API response `usage` field
- Phase 3 (Ollama): Use `prompt_eval_count` and `eval_count` from response
- Phase 4 (Agent): Update `Conversation::update_from_provider_usage()` to prefer provider counts
- Keep chars/4 heuristic for providers that don't report or when unavailable

### Question 2: Model Switching Behavior ✅ RESOLVED

What happens to conversation history when switching models with different context windows?

**Option A**: Keep all messages, rely on pruning if new model has smaller window
**Option B**: Immediately prune to fit new model's window
**Option C**: Warn user and require confirmation if history won't fit

**Decision**: Option C - Warn and confirm, then apply Option B

**Implementation Impact**:

- Phase 6 (Chat Mode): Add model switch confirmation logic in `src/commands/chat_mode.rs`
- Calculate if current conversation tokens exceed new model's context window
- Display warning: "Current conversation (X tokens) exceeds new model context (Y tokens). Z messages will be pruned. Continue? [y/N]"
- On confirmation, update conversation `max_tokens` and trigger immediate pruning
- Preserve conversation if user declines

### Question 3: Provider Mutability ✅ RESOLVED

How should we handle provider state mutation for model switching?

**Option A**: Require `&mut self` for Provider trait methods (breaking change)
**Option B**: Use interior mutability (RefCell/RwLock) in providers
**Option C**: Recreate provider with new model (copy conversation)

**Decision**: Option B - Interior mutability maintains clean API

**Implementation Impact**:

- Phase 1: Define `set_model(&self, ...)` (not `&mut self`) in Provider trait
- Phase 2 (Copilot): Wrap `config` field in `Arc<RwLock<CopilotConfig>>` in `CopilotProvider`
- Phase 3 (Ollama): Wrap `config` field in `Arc<RwLock<OllamaConfig>>` in `OllamaProvider`
- Thread-safe model switching without breaking existing Agent/Provider API
- Agent maintains `Arc<dyn Provider>` without changes

### Question 4: Model Discovery Caching ✅ RESOLVED

Should we cache model lists to reduce API calls?

**Option A**: Cache indefinitely (until restart)
**Option B**: Cache with TTL (e.g., 1 hour)
**Option C**: No caching, always fetch fresh
**Option D**: Cache in config file, update on-demand

**Decision**: Option B - Cache with 5-minute TTL for balance

**Implementation Impact**:

- Phase 2 (Copilot): No caching needed (hardcoded model list)
- Phase 3 (Ollama): Add cache fields to `OllamaProvider`: `model_cache: Arc<RwLock<Option<(Vec<ModelInfo>, Instant)>>>`
- Check cache age in `list_models()`, fetch if >5 minutes old
- Cache invalidation on `set_model()` success (confirms Ollama is reachable)
- Graceful fallback to cached data if Ollama unreachable and cache exists

### Question 5: Context Display Format ✅ RESOLVED

What's the best way to display context usage in chat prompt?

**Option A**: `[1234/8000]` - Raw token counts
**Option B**: `[1.2k/8k]` - Human-friendly (k/M suffixes)
**Option C**: `[15%]` - Percentage only
**Option D**: `[●●●○○○○○]` - Visual bar
**Option E**: Configurable, default to Option B

**Decision**: Option E with B as default

**Implementation Impact**:

- Phase 7 (Config): Add `context_display_format` enum to `ChatConfig`: `Tokens | HumanFriendly | Percentage | Both`
- Default to `HumanFriendly` format
- Phase 6 (Chat): Implement formatting function `format_token_count(used, total, format)` in `src/chat_mode.rs`
- Apply color coding: green (<60%), yellow (60-85%), red (>85%)
- Update prompt: `[Planning] [Safe] [1.2k/8k] >` or `[Planning] [Safe] [15%] >` based on config

## Dependencies and Prerequisites

- Rust async runtime (tokio) - already present
- Serde for JSON parsing - already present
- HTTP client (reqwest) - already present for Ollama
- Configuration system - already present
- No new external dependencies required

## Risks and Mitigations

### Risk 1: Breaking Changes to Provider Trait

**Impact**: High - All providers must be updated
**Mitigation**: Provide default implementations where possible, phase rollout

### Risk 2: Ollama Offline/Unavailable

**Impact**: Medium - Model operations fail if Ollama not running
**Mitigation**: Graceful error handling, cache last-known model list

### Risk 3: Token Count Accuracy

**Impact**: Low - Heuristic may be inaccurate for some models
**Mitigation**: Use provider-reported counts when available, document limitations

### Risk 4: Model Switching Mid-Conversation

**Impact**: Medium - May cause confusion or lose context
**Mitigation**: Clear warnings, require confirmation, preserve history

## Timeline Estimate

- **Phase 1**: 2-3 days (foundational trait changes)
- **Phase 2**: 2-3 days (Copilot implementation)
- **Phase 3**: 2-3 days (Ollama implementation)
- **Phase 4**: 2 days (Agent integration)
- **Phase 5**: 2 days (CLI commands)
- **Phase 6**: 3 days (Chat mode integration)
- **Phase 7**: 2 days (Documentation)

**Total**: 15-18 days (3-4 weeks)

## Success Metrics

1. **Functionality**: All CLI and chat commands work as specified
2. **Test Coverage**: >80% coverage for new code
3. **Documentation**: Complete reference and how-to guides
4. **User Experience**: Clear, helpful output and error messages
5. **Performance**: Model operations complete in <2 seconds
6. **Reliability**: Graceful handling of offline providers

## Future Enhancements

- Model performance benchmarking and recommendations
- Cost tracking per model/session
- Model capability auto-detection for new providers
- Streaming token usage updates
- Model recommendation based on prompt complexity
- Multi-provider model comparison
- Token usage visualization/export
