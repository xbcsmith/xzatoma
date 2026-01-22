---
description: Implementation plan for model management and provider capabilities
created: 2025-01-XX
status: updated-with-copilot-dynamic-fetching-lessons
last_updated: 2026-01-22
updated_by: AI implementation - Copilot dynamic model fetching completed
---

# Model Management Implementation Plan

## Overview

**IMPORTANT**: This plan has been updated with critical lessons learned from the actual implementation. See the "Lessons Learned from Implementation" section for issues that MUST be avoided in future implementations.

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

**Goal**: Implement dynamic model management for GitHub Copilot provider.

**CRITICAL**: Copilot MUST use dynamic API-based model fetching, NOT hardcoded lists.

#### Task 2.1: Implement Dynamic Model Fetching for Copilot

**IMPORTANT**: The Copilot provider must fetch models from the API at runtime. Hardcoded model lists are outdated and incorrect.

**Steps**:

1. Add API endpoint constant:

   ```rust
   const COPILOT_MODELS_URL: &str = "https://api.githubcopilot.com/models";
   ```

2. Create API response structures (see Lesson 0 for complete structure):

   - `CopilotModelsResponse` with `data: Vec<CopilotModelData>`
   - `CopilotModelData` with `id`, `name`, `capabilities`, `policy`
   - `CopilotModelCapabilities` with `limits` and `supports`
   - All fields optional with `#[serde(default)]`

3. Implement `fetch_copilot_models()` async method:

   - Authenticate with `self.authenticate().await?`
   - GET request to `COPILOT_MODELS_URL` with Bearer token
   - Filter to models with `policy.state == "enabled"`
   - Extract context window from `limits.max_context_window_tokens`
   - Add FunctionCalling capability if `supports.tool_calls == true`
   - Add Vision capability if `supports.vision == true`
   - Add LongContext if context_window > 32000
   - Return `Vec<ModelInfo>`

4. Update `list_models()` to call `fetch_copilot_models().await`

**Testing**:

- Create `testdata/models.json` with real Copilot API response
- Test parsing logic with testdata (not hardcoded model names)
- Verify enabled model filtering
- Verify capability extraction from API metadata

**DO NOT**:

- ❌ Create hardcoded `Vec<ModelInfo>` with static model list
- ❌ Assume models like `gpt-4`, `gpt-3.5-turbo` still exist
- ❌ Hardcode context windows or capabilities

#### Task 2.2: Implement Model Listing for Copilot (LEGACY - SEE 2.1)

Update `src/providers/copilot.rs`:

- Add `list_models()` implementation returning hardcoded list of supported Copilot models
- Known models: `gpt-4`, `gpt-4-turbo`, `gpt-3.5-turbo`, `claude-3.5-sonnet`, `claude-sonnet-4.5`, `o1-preview`, `o1-mini`
- Include context window sizes (e.g., gpt-4-turbo: 128k, claude-sonnet-4.5: 200k)
- Mark all as supporting tools/function calling

#### Task 2.3: Extract Token Usage from Copilot Response

Modify `complete()` in `src/providers/copilot.rs`:

- Parse `usage` field from `CopilotResponse` (already has `CopilotUsage` struct)
- Return `CompletionResponse` with extracted token counts
- Handle cases where usage is not provided

#### Task 2.4: Implement Model Switching for Copilot

**IMPORTANT**: Model switching must validate against dynamically fetched models, not hardcoded lists.

**Steps**:

1. Update `set_model()` to be async and fetch current models:

   ```rust
   async fn set_model(&mut self, model_name: String) -> Result<()> {
       let models = self.fetch_copilot_models().await?;
       // Validate and check capabilities
   }
   ```

2. Find requested model in fetched list
3. If not found, return error with list of available models
4. Check if model supports FunctionCalling capability
5. If no tool support, return error suggesting models that do
6. Update config if all validations pass

**Error Messages**:

- Model not found: "Model 'X' not found. Available models: A, B, C, ..."
- No tool support: "Model 'X' does not support tool calling. Try: A, B, C"

Add to `CopilotProvider`:

- Wrap `config` field in `Arc<RwLock<CopilotConfig>>` for interior mutability (Decision from Question 3)
- Implement `set_model(&self, ...)` to update config model name with write lock
- Implement `get_current_model(&self)` to return current model with read lock
- Validate model name against supported models from `list_models()`

#### Task 2.5: Testing Requirements

**CRITICAL**: Do not hardcode expected model names in tests (API changes).

**Required Tests**:

- Parse `testdata/models.json` successfully
- Extract enabled models (filter by `policy.state == "enabled"`)
- Extract context windows from API metadata
- Extract capabilities (tool_calls, vision) from API
- Verify `CopilotMessage` can deserialize without `content` field
- Test complete `CopilotResponse` deserialization with missing content

- Test model listing returns expected models
- Test token usage extraction from responses
- Test model switching updates internal state
- Integration test with actual Copilot API (optional, may require auth)

#### Task 2.6: Deliverables

- `src/providers/copilot.rs` - Dynamic model fetching implementation
- `testdata/models.json` - Real Copilot API response for testing
- `docs/explanation/copilot_dynamic_model_fetching.md` - Implementation documentation
- `docs/explanation/copilot_response_parsing_fix.md` - Content field parsing fix

- Fully implemented extended provider interface for Copilot
- Updated tests in `src/providers/copilot.rs`

#### Task 2.7: Success Criteria

- ✅ No hardcoded model lists in Copilot provider
- ✅ Models fetched from API at runtime
- ✅ Only enabled models shown to users
- ✅ Capabilities extracted from API metadata
- ✅ Model switching validates against current API data
- ✅ Tool calling requirement enforced
- ✅ All tests pass without hardcoded model names
- ✅ `/models` command shows actual available models

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
- **CRITICAL**: Use model family name (not metadata) to detect tool support - Ollama API does not expose tool capability directly
- **CRITICAL**: Only mark models that actually support tools - see `add_model_capabilities()` whitelist
- Cache model info to avoid repeated API calls

**Known Models with Tool Support** (as of 2025-01):

- `llama3.2`, `llama3.3` - Meta Llama 3.2/3.3 series
- `mistral`, `mistral-nemo` - Mistral AI models
- `firefunction` - Specialized function calling model
- `command-r`, `command-r-plus` - Cohere Command R series
- `granite3`, `granite4` - IBM Granite 3/4 series

**Models WITHOUT Tool Support** (do not mark as supporting FunctionCalling):

- `llama3` (original), `llama2` - Older Llama versions
- `gemma` - Google Gemma series
- `codellama` - Code-focused models
- `llava` - Vision-focused models
- Most other models unless explicitly verified

#### Task 3.3: Extract Token Usage from Ollama Response

Modify `complete()` in `src/providers/ollama.rs`:

- Parse `prompt_eval_count` and `eval_count` from `OllamaResponse` (already available)
- Return `CompletionResponse` with token counts
- Calculate total tokens as sum of prompt and completion

**CRITICAL**: Make response parsing flexible for varying model formats:

- Use `#[serde(default)]` for fields that may be missing (`id`, `arguments`, `content`)
- Use `#[serde(default = "default_tool_type")]` for `type` field (defaults to `"function"`)
- Generate unique IDs for tool calls with missing/empty `id` field using `call_{timestamp_ms}_{index}` format
- Handle empty content when models return tool-only responses

**Response Structure Flexibility**:

```rust
struct OllamaMessage {
    role: String,
    #[serde(default)]  // May be empty for tool-only responses
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaToolCall>>,
}

struct OllamaToolCall {
    #[serde(default)]  // May be empty, generate if needed
    id: String,
    #[serde(default = "default_tool_type")]  // May be missing
    r#type: String,
    function: OllamaFunctionCall,
}

struct OllamaFunctionCall {
    name: String,  // Required field
    #[serde(default)]  // May be empty for parameterless functions
    arguments: serde_json::Value,
}
```

This flexibility ensures compatibility with models like `granite4:latest` that may omit certain fields.

#### Task 3.4: Implement Model Switching for Ollama

Add to `OllamaProvider`:

- Wrap `config` field in `Arc<RwLock<OllamaConfig>>` for interior mutability (Decision from Question 3)
- Implement `set_model(&self, ...)` to update model name with write lock
- **CRITICAL**: Validate model exists by checking against `list_models()` (may trigger cache refresh)
- **CRITICAL**: Validate model supports tool calling before allowing switch - XZatoma requires FunctionCalling capability
- Return descriptive error if model lacks tool support: "Model 'X' does not support tool calling. XZatoma requires models with tool/function calling support. Try llama3.2:latest, llama3.3:latest, or mistral:latest instead."
- Implement `get_current_model(&self)` with read lock
- Invalidate model list cache on successful switch (Decision from Question 4)

**Validation Logic**:

```rust
async fn set_model(&mut self, model_name: String) -> Result<()> {
    let models = self.list_models().await?;
    let model_info = models.iter().find(|m| m.name == model_name);

    if model_info.is_none() {
        return Err(Error::ModelNotFound(model_name));
    }

    // CRITICAL: Check tool support
    if !model_info.unwrap().supports_capability(ModelCapability::FunctionCalling) {
        return Err(Error::ModelLacksToolSupport {
            model: model_name,
            suggestion: "llama3.2:latest, llama3.3:latest, or mistral:latest"
        });
    }

    // Proceed with switch...
}
```

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
- **CRITICAL**: Validate model supports tool calling - `set_model()` will reject models without FunctionCalling capability
- Get new model's context window from `get_model_info(name)`
- Check if current conversation tokens exceed new context window (Decision from Question 2)
- If exceeds: Display warning "Current conversation (X tokens) exceeds new model context (Y tokens). Z messages will be pruned. Continue? [y/N]"
- Require user confirmation before proceeding with switch
- On confirm: Call `agent.provider.set_model(name)`, update conversation `max_tokens`, trigger immediate pruning
- Display confirmation with new model, context window, and tokens pruned (if any)
- **CRITICAL**: Handle tool support error gracefully with helpful message suggesting alternative models

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
- **CRITICAL**: Set Ollama default model to `llama3.2:latest` (NOT `llama3:latest` - lacks tool support)
- **CRITICAL**: Never use `qwen2.5-coder` or any `qwen*` models - not standard Ollama models
- **CRITICAL**: Only reference approved Ollama models in documentation: `llama3.2:latest`, `llama3.3:latest`, `mistral:latest`, `granite3:latest`, `granite4:latest`

**Default Model Configuration**:

```yaml
provider:
  ollama:
    host: http://localhost:11434
    # MUST use llama3.2:latest or newer for tool support
    model: llama3.2:latest
```

#### Task 7.2: Create Reference Documentation

Create `docs/reference/model_management.md`:

- Document all supported models per provider
- **CRITICAL**: Clearly mark which models support tool calling (required for XZatoma)
- List context window sizes
- Document model capabilities (FunctionCalling, LongContext, Vision, etc.)
- Include performance characteristics where known
- Update regularly as providers add models
- **WARNING**: Explicitly state that models without FunctionCalling capability cannot be used with XZatoma
- Document Ollama's tool support limitations and which models are verified to work

**Required Content**:

- Core types: ModelInfo, ModelCapability, TokenUsage, ContextInfo, ProviderCapabilities
- Provider trait methods with signatures and error handling
- CLI commands reference
- Chat mode special commands reference
- Provider comparison (Copilot vs Ollama)
- Error handling patterns
- Integration examples

#### Task 7.3: Create How-To Guides

Create `docs/how-to/manage_models.md` (note: underscore, not hyphen):

- Guide for listing available models (CLI and chat mode)
- Guide for viewing detailed model information
- Guide for checking current model
- Understanding model capabilities (especially FunctionCalling requirement)
- Choosing the right model for different tasks
- Provider differences (Copilot vs Ollama)
- Best practices for model selection
- Troubleshooting common issues (model not found, lacks tool support, parsing errors)

Create `docs/how-to/switch_models.md` (note: underscore, not hyphen):

- Quick start examples
- Switching in interactive chat mode
- Understanding model switching behavior
- Conversation persistence and pruning
- Switching via configuration
- Switching between providers
- Advanced model switching strategies
- Troubleshooting model switch issues (tool support, context window)
- Best practices for model switching
- Complete workflow examples

**CRITICAL Documentation Standards**:

- Use lowercase_with_underscores.md for ALL filenames (NOT kebab-case)
- NO emojis anywhere in documentation
- All code blocks must specify language (`rust, `yaml, `bash, `text)
- Cross-reference using relative paths
- Include troubleshooting sections with actual error messages and solutions

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
- `docs/reference/model_management.md` - Comprehensive API reference (~645 lines)
- `docs/how-to/manage_models.md` - Model discovery and inspection guide (~418 lines)
- `docs/how-to/switch_models.md` - Model switching guide (~439 lines)
- Implementation summary in `docs/explanation/` documenting what was delivered
- Updated configuration schema
- Updated help text

#### Task 7.8: Success Criteria

- Documentation covers all features
- Examples are tested and working
- Configuration is well-documented
- Help text is comprehensive and accurate

## Lessons Learned from Implementation

This section documents critical issues discovered during implementation that future implementations must avoid.

### Lesson 0: Copilot Provider MUST Use Dynamic Model Fetching

**Issue**: Copilot provider used hardcoded list of models (`gpt-4`, `gpt-4-turbo`, `gpt-3.5-turbo`, `o1-preview`, etc.) that don't exist in the actual Copilot API.

**Impact**: `/models` command showed completely wrong models. Users couldn't see or use actual available models (GPT-5, Claude 4.5, Gemini 2.5, Grok).

**Resolution**: Implemented dynamic model fetching from `https://api.githubcopilot.com/models` API endpoint.

**Critical Requirements**:

- **NEVER use hardcoded model lists for Copilot** - the API changes frequently
- **ALWAYS fetch models from `/models` endpoint** at runtime
- Filter to models with `policy.state == "enabled"`
- Extract capabilities from API metadata (`supports.tool_calls`, `supports.vision`)
- Extract context window from `limits.max_context_window_tokens`
- Default model should be `gpt-5-mini` (current standard)
- Validate model exists and supports tool calling before allowing switch

**API Endpoint**:

```rust
const COPILOT_MODELS_URL: &str = "https://api.githubcopilot.com/models";
```

**Response Structure**:

```rust
#[derive(Debug, Deserialize)]
struct CopilotModelsResponse {
    data: Vec<CopilotModelData>,
}

#[derive(Debug, Deserialize)]
struct CopilotModelData {
    id: String,
    name: String,
    #[serde(default)]
    capabilities: Option<CopilotModelCapabilities>,
    #[serde(default)]
    policy: Option<CopilotModelPolicy>,
}

#[derive(Debug, Deserialize)]
struct CopilotModelPolicy {
    state: String,  // "enabled" or other
}

#[derive(Debug, Deserialize)]
struct CopilotModelCapabilities {
    #[serde(default)]
    limits: Option<CopilotModelLimits>,
    #[serde(default)]
    supports: Option<CopilotModelSupports>,
}

#[derive(Debug, Deserialize)]
struct CopilotModelLimits {
    #[serde(default)]
    max_context_window_tokens: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct CopilotModelSupports {
    #[serde(default)]
    tool_calls: Option<bool>,
    #[serde(default)]
    vision: Option<bool>,
}
```

**Implementation Pattern**:

```rust
async fn fetch_copilot_models(&self) -> Result<Vec<ModelInfo>> {
    let token = self.authenticate().await?;

    let response = self
        .client
        .get(COPILOT_MODELS_URL)
        .header("Authorization", format!("Bearer {}", token))
        .header("Editor-Version", "vscode/1.85.0")
        .send()
        .await?;

    let models_response: CopilotModelsResponse = response.json().await?;

    let mut models = Vec::new();
    for model_data in models_response.data {
        // Only include enabled models
        if let Some(policy) = &model_data.policy {
            if policy.state != "enabled" {
                continue;
            }
        }

        // Extract context window (default 128k if missing)
        let context_window = model_data
            .capabilities
            .as_ref()
            .and_then(|c| c.limits.as_ref())
            .and_then(|l| l.max_context_window_tokens)
            .unwrap_or(128000);

        let mut model_info = ModelInfo::new(
            &model_data.id,
            &model_data.name,
            context_window
        );

        // Add capabilities from API metadata
        if let Some(caps) = &model_data.capabilities {
            if let Some(supports) = &caps.supports {
                if supports.tool_calls.unwrap_or(false) {
                    model_info.add_capability(ModelCapability::FunctionCalling);
                }
                if supports.vision.unwrap_or(false) {
                    model_info.add_capability(ModelCapability::Vision);
                }
            }
        }

        // Infer LongContext for models >32k
        if context_window > 32000 {
            model_info.add_capability(ModelCapability::LongContext);
        }

        models.push(model_info);
    }

    Ok(models)
}

async fn list_models(&self) -> Result<Vec<ModelInfo>> {
    self.fetch_copilot_models().await
}

async fn set_model(&mut self, model_name: String) -> Result<()> {
    let models = self.fetch_copilot_models().await?;

    let model_info = models
        .iter()
        .find(|m| m.name == model_name)
        .ok_or_else(|| {
            let available: Vec<String> =
                models.iter().map(|m| m.name.clone()).collect();
            XzatomaError::Provider(format!(
                "Model '{}' not found. Available models: {}",
                model_name,
                available.join(", ")
            ))
        })?;

    if !model_info.supports_capability(ModelCapability::FunctionCalling) {
        return Err(XzatomaError::Provider(format!(
            "Model '{}' does not support tool calling",
            model_name
        )));
    }

    self.config.write()?.model = model_name;
    Ok(())
}
```

**Why Hardcoded Lists Fail**:

1. GitHub frequently adds new models (GPT-5, Claude 4.5, Gemini 2.5, Grok)
2. Old models are deprecated/removed (GPT-3.5, GPT-4 original)
3. Context windows change (GPT-5-mini: 264k, not the hardcoded 8k)
4. Capabilities change (new models add vision, structured outputs)
5. Model availability varies by user/organization

---

## Fix Summary (Applied)

To prevent repeated mistakes the Copilot provider implementation was changed to follow these concrete rules:

- Replace any hardcoded model lists with a runtime model discovery call to the Copilot models API (`/models`) and parse its response.
- Filter models to `policy.state == "enabled"` and extract `id`, `name`, capability flags (`supports.tool_calls`, `supports.vision`) and limits (`max_context_window_tokens`).
- Map capabilities into explicit `ModelCapability` flags (FunctionCalling, Vision, LongContext for >32k tokens) and use those flags to validate `set_model()` requests.
- Make deserialization resilient: use `#[serde(default)]` and optional fields so missing fields (e.g., `content` on function/tool call responses) do not cause hard failures.
- On authentication failures (HTTP 401):
  - First attempt a non-interactive refresh by exchanging any cached GitHub token for a new Copilot token and retry the request.
  - If refresh fails or no cached GitHub token exists, perform a best-effort cache invalidation (so `authenticate()` won't reuse stale tokens) and return an actionable `Authentication` error telling the user how to re-auth (e.g. `xzatoma auth --provider copilot`).
- Do not make UI/CLI behavior assumptions in provider internals — surface clear, actionable errors and leave interactive re-auth to an explicit command or user action.

## Guardrails / PR Checklist (MUST PASS before merging provider changes)

When making changes to providers or model management, PRs must include:

1. No hardcoded provider model lists.
   - Any list of models must come from a runtime-discovered source or a well-documented local cache with TTL.
2. Robust parsing tests
   - Add a fixture to `testdata/` (e.g., `testdata/models.json`) using a real API response shape and include unit tests that parse it.
   - Add parsing tests for edge cases: missing `content`, missing `id`, omitted `arguments`, and alternate shapes.
3. Auth resiliency tests
   - Unit tests for 401→Authentication mapping.
   - Integration tests (mocked) for 401 → non-interactive refresh → retry → success, and 401 → refresh fails → cache invalidation + actionable error.
4. Integration/mocked tests
   - Use a mock server (wiremock or similar) and ensure the code supports an overridable base URL (env var or config) for testing, so CI can test provider flows without hitting production endpoints.
5. Documentation
   - Update `docs/explanation/` with a short note about the provider’s model discovery behaviour and auth handling so future implementers don't reintroduce brittle assumptions.
6. Telemetry / Logging
   - Add logs (and optionally telemetry hooks) to count or surface repeated parsing failures and authentication failures so we can detect regressions.
7. Code comments / TODOs
   - Add an inline comment at the `COPILOT_MODELS_URL` constant reminding contributors to prefer discovery over static lists.
8. Validation
   - The PR must run and pass: `cargo fmt --all`, `cargo check --all-targets --all-features`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-features`.

Adhering to these guardrails will prevent reintroducing hardcoded model lists or brittle parsing and will ensure auth failures are recoverable and actionable.

**Testing Approach**:

- Use `testdata/models.json` with real API response for parsing tests
- Parse tests verify deserialization logic without API calls
- Async tests document need for mocking in integration tests
- Never hardcode expected model names in tests (API changes)

**Reference**: `docs/explanation/copilot_dynamic_model_fetching.md`

### Lesson 1: Ollama Default Model Selection

**Issue**: Initial default model (`qwen2.5-coder`) was not a standard Ollama model and unavailable in most installations.

**Impact**: Provider failed to initialize, users couldn't use Ollama without manual configuration.

**Resolution**: Changed default to `llama3.2:latest` (standard, widely available, supports tools).

**Critical Requirements**:

- Default model MUST be a standard Ollama model (comes with standard installation)
- Default model MUST support tool calling (FunctionCalling capability required for XZatoma)
- Never use `qwen*` models as defaults - not standard Ollama models
- Approved defaults: `llama3.2:latest` (recommended), `llama3.3:latest`, `mistral:latest`

**Reference**: `docs/explanation/ollama_default_model_fix.md`

### Lesson 2: Tool Support Detection and Validation

**Issue**: Initial implementation assumed ALL Ollama models support tool calling (function calling).

**Impact**: Users could switch to models like `llama3:latest` that don't support tools, causing runtime failures.

**Resolution**:

1. Implemented whitelist-based capability detection in `add_model_capabilities()`
2. Added validation in `set_model()` to reject models without FunctionCalling capability
3. Provided helpful error messages suggesting alternative models

**Critical Requirements**:

- DO NOT assume all models support tools - only specific models do
- Use whitelist approach (safer than blacklist - new models default to no support)
- Validate tool support BEFORE allowing model switch
- Provide descriptive errors with model suggestions when validation fails

**Known Models WITH Tool Support**:

- `llama3.2`, `llama3.3` - Meta Llama 3.2/3.3
- `mistral`, `mistral-nemo` - Mistral AI
- `firefunction` - Specialized function calling
- `command-r`, `command-r-plus` - Cohere Command R
- `granite3`, `granite4` - IBM Granite

**Known Models WITHOUT Tool Support**:

- `llama3` (original), `llama2` - Older versions
- `gemma` - Google Gemma
- `codellama` - Code-focused
- `llava` - Vision-focused
- Most other models unless verified

**Reference**: `docs/explanation/ollama_tool_support_validation.md`

### Lesson 3: Copilot Response Parsing Flexibility

**Issue**: Strict deserialization of Copilot responses failed when API returned tool calls without a `content` field.

**Impact**: Parser crashed with "missing field `content`" error even for valid tool call responses.

**Resolution**: Made `content` field optional with `#[serde(default)]` to default to empty string when missing.

**Critical Requirements**:

- Use `#[serde(default)]` for `content` field in Copilot message structures
- When model returns tool calls, `content` may be omitted or null
- This is standard OpenAI-compatible API behavior
- Empty content is correct for tool-only responses

**Response Structure Template**:

```rust
#[derive(Debug, Serialize, Deserialize)]
struct CopilotMessage {
    role: String,
    #[serde(default)]  // May be missing when tool calls present
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<CopilotToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}
```

**Reference**: `docs/explanation/copilot_response_parsing_fix.md`

### Lesson 4: Ollama Response Parsing Flexibility

**Issue**: Strict deserialization of Ollama responses failed with models like `granite4:latest` that omit optional fields (`type`, `id`, `arguments`, `content`).

**Impact**: Parser crashed with "missing field" errors even though essential data (function name) was present.

**Resolution**:

1. Made optional fields use `#[serde(default)]` or `#[serde(default = "function")]`
2. Generated unique IDs for tool calls when missing: `call_{timestamp_ms}_{index}`
3. Handled empty content gracefully for tool-only responses

**Critical Requirements**:

- Use flexible deserialization for Ollama responses (models have varying formats)
- Use `#[serde(default)]` for: `id`, `arguments`, `content`
- Use `#[serde(default = "default_tool_type")]` for `type` field
- Generate unique IDs when missing using timestamp + index pattern
- Still require essential fields (function name)

**Response Structure Template**:

```rust
struct OllamaMessage {
    role: String,
    #[serde(default)]  // May be empty for tool-only responses
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaToolCall>>,
}

struct OllamaToolCall {
    #[serde(default)]  // May be empty, generate if needed
    id: String,
    #[serde(default = "default_tool_type")]  // May be missing
    r#type: String,
    function: OllamaFunctionCall,
}

struct OllamaFunctionCall {
    name: String,  // Required - must have function name
    #[serde(default)]  // May be empty for parameterless functions
    arguments: serde_json::Value,
}

fn default_tool_type() -> String {
    "function".to_string()
}
```

**Reference**: `docs/explanation/ollama_response_parsing_fix.md`

### Lesson 5: Documentation Naming Conventions

**Issue**: Agents commonly use kebab-case (`manage-models.md`) or CamelCase for documentation filenames.

**Impact**: Inconsistent naming, hard to find files, breaks documentation standards.

**Resolution**: Enforce lowercase_with_underscores.md for ALL documentation files.

**Critical Requirements**:

- Use `lowercase_with_underscores.md` for ALL markdown files
- Exception: `README.md` is the ONLY uppercase filename allowed
- Never use kebab-case (`manage-models.md`) - use `manage_models.md`
- Never use CamelCase (`ManageModels.md`) - use `manage_models.md`
- Never use emojis anywhere in documentation
- All code blocks must specify language (`rust not `)

**Correct Examples**:

- `docs/how-to/manage_models.md` ✅
- `docs/how-to/switch_models.md` ✅
- `docs/reference/model_management.md` ✅

**Wrong Examples**:

- `docs/how-to/manage-models.md` ❌ (kebab-case)
- `docs/how-to/ManageModels.md` ❌ (CamelCase)
- `docs/how-to/manage_models_🚀.md` ❌ (emoji)

**Reference**: `AGENTS.md` - Rule 2: Markdown File Naming

### Lesson 6: Model Capability Detection Varies by Provider

**Copilot**: Use API metadata from `/models` endpoint

**Issue**: Ollama `/api/show` endpoint does not expose whether a model supports tool calling.

**Impact**: Cannot programmatically detect tool support from API metadata.

**Resolution**: Maintain curated whitelist of known models with tool support based on model family name.

**Critical Requirements**:

- Use model family name (from `name.split(':').next()`) to detect capabilities
- Cannot rely on Ollama API metadata for tool support detection
- Maintain whitelist in code based on: official docs, testing, user feedback
- Update whitelist as new models are released and verified
- Document which models are verified vs. assumed to have capabilities

**Detection Pattern**:

```rust
fn add_model_capabilities(model: &mut ModelInfo, family: &str) {
    match family.to_lowercase().as_str() {
        // Whitelist: verified to support tools
        "llama3.2" | "llama3.3" | "mistral" | "granite4" => {
            model.add_capability(ModelCapability::FunctionCalling);
        }
        _ => {
            // Default: assume NO tool support unless explicitly verified
        }
    }
}
```

**Reference**: `docs/explanation/ollama_tool_support_validation.md`

### Lesson 7: Error Messages Must Be Actionable

**Issue**: Generic error messages like "Model not found" don't help users recover.

**Impact**: Users don't know what to do next or which models will work.

**Resolution**: Provide specific, actionable error messages with model suggestions.

**Critical Requirements**:

- Always suggest specific alternative models that will work
- Include the reason for failure (tool support, not found, etc.)
- Format: "Model 'X' failed because Y. Try A, B, or C instead."
- For Ollama, suggest: `llama3.2:latest`, `llama3.3:latest`, `mistral:latest`

**Good Error Example**:

```
Error: Model 'llama3:latest' does not support tool calling. XZatoma requires
models with tool/function calling support. Try llama3.2:latest, llama3.3:latest,
or mistral:latest instead.
```

**Bad Error Example**:

```
Error: Model not supported
```

**Reference**: `docs/explanation/ollama_tool_support_validation.md`

### Implementation Checklist for Future Phases

When implementing model management features, verify:

**Copilot Provider**:

- [ ] Default Copilot model is `gpt-5-mini`
- [ ] NEVER use hardcoded model lists - ALWAYS fetch from `/models` API
- [ ] `fetch_copilot_models()` filters to `policy.state == "enabled"`
- [ ] Capabilities extracted from API metadata (`supports.tool_calls`, `supports.vision`)
- [ ] Context window extracted from `limits.max_context_window_tokens`
- [ ] `CopilotMessage.content` uses `#[serde(default)]` for missing field handling
- [ ] `set_model()` validates against dynamically fetched models
- [ ] `set_model()` checks tool calling support before allowing switch
- [ ] Error messages suggest actual available models from API

**Ollama Provider**:

- [ ] Default Ollama model is `llama3.2:latest` (NOT `llama3:latest` or `qwen*`)
- [ ] `add_model_capabilities()` uses whitelist approach (only verified models get FunctionCalling)
- [ ] `set_model()` validates tool support before allowing switch
- [ ] Ollama response structures use `#[serde(default)]` for optional fields
- [ ] Tool call ID generation implemented for missing/empty IDs
- [ ] Model capability detection uses family name whitelist, not API metadata
- [ ] Whitelist is documented and up-to-date

**General**:

- [ ] Error messages are actionable and suggest specific alternative models
- [ ] Documentation filenames use `lowercase_with_underscores.md`
- [ ] No emojis in documentation
- [ ] All code blocks specify language (e.g., `rust not `)
- [ ] All optional API fields use `#[serde(default)]` or `Option<T>`

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
