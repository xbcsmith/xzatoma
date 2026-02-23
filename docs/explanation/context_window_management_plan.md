# Context Window Management Implementation Plan

## Overview

This plan implements intelligent context window management for XZatoma with different behaviors for chat mode (user-controlled) and run mode (automatic). The system will monitor token usage, warn users when approaching limits, provide summarization capabilities, and automatically manage context in autonomous execution.

Key features:

- Context window monitoring with configurable thresholds
- Warning system for chat mode when approaching token limits
- `/context summary` command for manual summarization
- Automatic summarization in run mode
- Configurable summarization model to avoid wasting premium tokens
- Model override support for summarization commands

## Current State Analysis

### Existing Infrastructure

**Conversation Management**

- `src/agent/conversation.rs` - Manages conversation history with token tracking
- `Conversation::get_context_info()` - Returns `ContextInfo` with token usage stats
- `Conversation::prune_if_needed()` - Automatic pruning at 80% threshold (creates summary)
- Token estimation via `estimate_tokens()` heuristic
- Support for provider-reported token usage via `update_from_provider_usage()`

**Command System**

- `src/commands/special_commands.rs` - Special command parsing
- `SpecialCommand` enum with existing commands (mode, safety, status, help, etc.)
- `ShowContextInfo` command already exists (displays token stats)
- Command parsing with error handling

**Configuration**

- `src/config.rs` - YAML-based configuration
- `ConversationConfig` - max_tokens, min_retain_turns, prune_threshold
- `AgentConfig` - chat, conversation, tools configuration
- Environment variable and CLI override support

**Chat vs Run Modes**

- Chat mode: `src/commands/mod.rs::run_chat()` - Interactive with special commands
- Run mode: `src/commands/mod.rs::run_plan()` - Autonomous plan execution
- Different execution patterns and user interaction models

### Identified Issues

1. **No Warning System**: Users hit token limits without advance warning
2. **No Manual Control**: Cannot trigger summarization on-demand in chat mode
3. **Fixed Summarization Model**: Uses same model as conversation (wastes premium tokens)
4. **Run Mode Lacks Auto-Summary**: No automatic recovery when context fills during plan execution
5. **No Threshold Configuration**: Warning and auto-summary thresholds are hardcoded
6. **Missing Context Commands**: No `/context` command namespace for context operations

## Implementation Phases

### Phase 1: Core Context Monitoring Infrastructure

**Goal**: Add context window monitoring with configurable thresholds and warning detection.

#### Task 1.1: Extend ConversationConfig

**File**: `src/config.rs`

Add new fields to `ConversationConfig`:

```rust
pub struct ConversationConfig {
    pub max_tokens: usize,
    pub min_retain_turns: usize,
    pub prune_threshold: f32,
    // NEW FIELDS:
    pub warning_threshold: f32,        // Default: 0.85 (warn at 85%)
    pub auto_summary_threshold: f32,    // Default: 0.90 (auto-summarize at 90%)
    pub summary_model: Option<String>,  // Model to use for summarization
}
```

Add default functions:

- `default_warning_threshold()` -> 0.85
- `default_auto_summary_threshold()` -> 0.90

Update `config/config.yaml` example with new fields.

#### Task 1.2: Add Context Warning Detection

**File**: `src/agent/conversation.rs`

Add new method to `Conversation`:

```rust
pub fn check_context_status(&self) -> ContextStatus {
    let percentage = self.token_count as f64 / self.max_tokens as f64;

    if percentage >= auto_summary_threshold {
        ContextStatus::Critical
    } else if percentage >= warning_threshold {
        ContextStatus::Warning
    } else {
        ContextStatus::Normal
    }
}

pub enum ContextStatus {
    Normal,
    Warning { percentage: f64, tokens_remaining: usize },
    Critical { percentage: f64, tokens_remaining: usize },
}
```

Add helper method:

```rust
pub fn should_warn(&self, warning_threshold: f64) -> bool
pub fn should_auto_summarize(&self, auto_threshold: f64) -> bool
```

#### Task 1.3: Conversation Summary API

**File**: `src/agent/conversation.rs`

Make existing `create_summary()` method public:

```rust
pub fn create_summary_message(&self, messages: &[Message]) -> String
```

Add new public method for explicit summarization:

```rust
pub fn summarize_and_reset(&mut self) -> Result<String, ConversationError> {
    // 1. Collect all non-system messages
    // 2. Create comprehensive summary
    // 3. Clear messages except system
    // 4. Add summary as system message
    // 5. Reset token count
    // 6. Return summary text for display
}
```

#### Task 1.4: Testing Requirements

**Tests** in `src/agent/conversation.rs`:

- `test_context_status_normal()` - Below warning threshold
- `test_context_status_warning()` - Between warning and critical
- `test_context_status_critical()` - Above auto-summary threshold
- `test_should_warn_thresholds()` - Boundary conditions
- `test_summarize_and_reset()` - Explicit summarization
- `test_summarize_preserves_system_messages()` - System messages retained

#### Task 1.5: Deliverables

- Extended `ConversationConfig` with thresholds
- `ContextStatus` enum and detection logic
- Public summarization API
- Configuration validation for threshold ranges (0.0-1.0)
- Unit tests achieving >80% coverage

#### Task 1.6: Success Criteria

- `cargo fmt --all` passes
- `cargo check --all-targets --all-features` passes
- `cargo clippy --all-targets --all-features -- -D warnings` shows zero warnings
- `cargo test --all-features` passes with new tests
- Configuration loads with new fields
- Context status correctly detects warning/critical states

### Phase 2: Chat Mode Warning System

**Goal**: Implement warning display in chat mode when approaching token limits.

#### Task 2.1: Add Warning Display Logic

**File**: `src/commands/mod.rs`

In `run_chat()` function, after each agent turn:

```rust
// Check context status after agent response
let context_status = conversation.check_context_status();

match context_status {
    ContextStatus::Warning { percentage, tokens_remaining } => {
        println!("\n⚠️  Warning: Context window is {}% full",
                 (percentage * 100.0) as u8);
        println!("   {} tokens remaining. Consider running '/context summary'",
                 tokens_remaining);
        println!();
    }
    ContextStatus::Critical { percentage, tokens_remaining } => {
        println!("\n🔴 Critical: Context window is {}% full!",
                 (percentage * 100.0) as u8);
        println!("   Only {} tokens remaining!", tokens_remaining);
        println!("   Run '/context summary' to free up space or risk losing context.");
        println!();
    }
    ContextStatus::Normal => {}
}
```

#### Task 2.2: Add Context Command Namespace

**File**: `src/commands/special_commands.rs`

Extend `SpecialCommand` enum:

```rust
pub enum SpecialCommand {
    // ... existing variants ...

    /// Display context window information
    ContextInfo,

    /// Summarize current context and start fresh
    /// Optional model parameter to override summarization model
    ContextSummary { model: Option<String> },
}
```

Update `parse_special_command()`:

```rust
fn parse_special_command(input: &str) -> Result<SpecialCommand, CommandError> {
    // ... existing parsing ...

    if input == "/context" || input == "/context info" {
        return Ok(SpecialCommand::ContextInfo);
    }

    if input.starts_with("/context summary") {
        let model = input.strip_prefix("/context summary")
            .and_then(|rest| {
                rest.trim()
                    .strip_prefix("--model")
                    .or_else(|| rest.trim().strip_prefix("-m"))
                    .map(|m| m.trim().to_string())
            });
        return Ok(SpecialCommand::ContextSummary { model });
    }
}
```

#### Task 2.3: Implement Context Command Handlers

**File**: `src/commands/mod.rs`

In `run_chat()`, handle new commands:

```rust
SpecialCommand::ContextInfo => {
    let info = conversation.get_context_info(model_context_window);
    println!("\nContext Window Status:");
    println!("  Total: {} tokens", info.max_tokens);
    println!("  Used: {} tokens ({}%)",
             info.used_tokens, info.percentage_used);
    println!("  Remaining: {} tokens\n", info.remaining_tokens);
    continue;
}

SpecialCommand::ContextSummary { model } => {
    // Determine which model to use for summarization
    let summary_model = model
        .or(config.agent.conversation.summary_model.clone())
        .unwrap_or_else(|| current_model.clone());

    // Create provider for summary if different model
    let summary_provider = if summary_model != current_model {
        create_provider_for_model(&config, &summary_model).await?
    } else {
        provider.clone()
    };

    // Perform summarization
    let summary_text = perform_context_summary(
        &mut conversation,
        summary_provider,
        &summary_model
    ).await?;

    println!("\n✅ Context summarized using model: {}", summary_model);
    println!("   Previous conversation: {} messages", previous_count);
    println!("   New conversation: {} messages\n", conversation.messages().len());

    continue;
}
```

#### Task 2.4: Summarization Provider Function

**File**: `src/commands/mod.rs`

Add helper function:

```rust
async fn perform_context_summary(
    conversation: &mut Conversation,
    provider: Arc<dyn Provider>,
    model_name: &str,
) -> Result<String, XzatomaError> {
    // 1. Get current messages
    let messages_to_summarize = conversation.messages().to_vec();

    // 2. Create summarization prompt
    let summary_prompt = create_summary_prompt(&messages_to_summarize);

    // 3. Call provider with summary prompt
    let (response, _usage) = provider.complete(
        &[Message::user(summary_prompt)],
        &[],
    ).await?;

    // 4. Extract summary from response
    let summary_text = response.content
        .unwrap_or_else(|| "Unable to generate summary".to_string());

    // 5. Reset conversation with summary
    conversation.clear();
    conversation.add_system_message(format!(
        "Previous conversation summary:\n\n{}",
        summary_text
    ));

    Ok(summary_text)
}

fn create_summary_prompt(messages: &[Message]) -> String {
    format!(
        "Please provide a concise summary of the following conversation, \
         focusing on key topics, decisions made, and important context. \
         Keep the summary under 500 words.\n\nConversation:\n{}",
        format_messages_for_summary(messages)
    )
}
```

#### Task 2.5: Testing Requirements

**Tests** in `src/commands/special_commands.rs`:

- `test_parse_context_info()` - Parse `/context` and `/context info`
- `test_parse_context_summary_no_model()` - Parse `/context summary`
- `test_parse_context_summary_with_model()` - Parse `/context summary --model gpt-5.3-codex`
- `test_parse_context_summary_short_flag()` - Parse `/context summary -m gpt-5.3-codex`
- `test_context_command_errors()` - Invalid subcommands

**Integration tests** (manual verification):

- Warning displays at 85% threshold
- Critical warning displays at 90% threshold
- `/context info` shows current status
- `/context summary` uses default/config model
- `/context summary --model X` uses specified model

#### Task 2.6: Deliverables

- Warning display after each agent turn in chat mode
- `/context info` command implementation
- `/context summary [--model X]` command implementation
- Model override functionality
- Comprehensive error handling
- Updated help text with new commands

#### Task 2.7: Success Criteria

- All quality checks pass (fmt, check, clippy, test)
- Warnings appear at correct thresholds
- Context commands work in interactive mode
- Model override correctly switches provider
- Help text documents new commands
- No emojis in code (only in user-facing output)

### Phase 3: Run Mode Automatic Summarization

**Goal**: Implement automatic context summarization in run mode to prevent execution failures.

#### Task 3.1: Add Auto-Summary to Agent Execution

**File**: `src/commands/mod.rs`

In `run_plan()` and `run_plan_with_options()`, add monitoring:

```rust
// Inside agent execution loop, after each turn:
if conversation.should_auto_summarize(config.agent.conversation.auto_summary_threshold) {
    tracing::warn!(
        "Context window critical (>{}%), triggering automatic summarization",
        (config.agent.conversation.auto_summary_threshold * 100.0) as u8
    );

    // Get summarization model
    let summary_model = config.agent.conversation.summary_model
        .clone()
        .unwrap_or_else(|| get_current_model_name(&provider));

    // Create summary provider if needed
    let summary_provider = create_summary_provider_if_needed(
        &config,
        &provider,
        &summary_model
    ).await?;

    // Perform summarization
    let summary_result = perform_context_summary(
        &mut conversation,
        summary_provider,
        &summary_model
    ).await;

    match summary_result {
        Ok(_) => {
            tracing::info!(
                "Automatic summarization complete using model: {}",
                summary_model
            );
        }
        Err(e) => {
            tracing::error!(
                "Automatic summarization failed: {}. Continuing with pruning.",
                e
            );
            // Fall back to existing prune_if_needed logic
            conversation.prune_if_needed();
        }
    }
}
```

#### Task 3.2: Summary Provider Helper

**File**: `src/commands/mod.rs`

Add helper to create provider for summary model:

```rust
async fn create_summary_provider_if_needed(
    config: &Config,
    current_provider: &Arc<dyn Provider>,
    summary_model: &str,
) -> Result<Arc<dyn Provider>, XzatomaError> {
    let current_model = current_provider.get_current_model();

    if summary_model == current_model {
        // Use existing provider
        Ok(current_provider.clone())
    } else {
        // Create new provider for summary model
        tracing::debug!(
            "Creating separate provider for summarization model: {}",
            summary_model
        );
        create_provider_for_model(config, summary_model).await
    }
}

async fn create_provider_for_model(
    config: &Config,
    model_name: &str,
) -> Result<Arc<dyn Provider>, XzatomaError> {
    // Implementation depends on provider type
    match config.provider.provider_type.as_str() {
        "copilot" => {
            let mut copilot_config = config.provider.copilot.clone();
            copilot_config.model = model_name.to_string();
            let provider = CopilotProvider::new(copilot_config).await?;
            Ok(Arc::new(provider))
        }
        "ollama" => {
            let mut ollama_config = config.provider.ollama.clone();
            ollama_config.model = model_name.to_string();
            let provider = OllamaProvider::new(ollama_config);
            Ok(Arc::new(provider))
        }
        _ => Err(XzatomaError::UnsupportedProvider(
            config.provider.provider_type.clone()
        ))
    }
}
```

#### Task 3.3: Configuration Validation

**File**: `src/config.rs`

In `Config::validate()`, add checks:

```rust
// Validate conversation thresholds
if config.agent.conversation.warning_threshold < 0.0
    || config.agent.conversation.warning_threshold > 1.0 {
    return Err("warning_threshold must be between 0.0 and 1.0".into());
}

if config.agent.conversation.auto_summary_threshold < 0.0
    || config.agent.conversation.auto_summary_threshold > 1.0 {
    return Err("auto_summary_threshold must be between 0.0 and 1.0".into());
}

if config.agent.conversation.auto_summary_threshold <= config.agent.conversation.warning_threshold {
    return Err("auto_summary_threshold must be greater than warning_threshold".into());
}

// Validate summary model if specified
if let Some(ref model) = config.agent.conversation.summary_model {
    // Check model exists in provider's model list (if provider supports it)
    tracing::debug!("Summary model configured: {}", model);
}
```

#### Task 3.4: Testing Requirements

**Tests** in `src/config.rs`:

- `test_conversation_config_with_thresholds()` - Valid threshold configuration
- `test_validation_invalid_warning_threshold()` - Out of range warning threshold
- `test_validation_invalid_auto_summary_threshold()` - Out of range auto threshold
- `test_validation_auto_summary_below_warning()` - auto_summary <= warning
- `test_conversation_config_summary_model()` - Summary model configuration

**Integration tests** (manual verification):

- Run mode triggers auto-summary at threshold
- Summary uses configured model
- Falls back to pruning on summary failure
- Logging shows summarization events

#### Task 3.5: Deliverables

- Automatic summarization in run mode
- Configurable summary model
- Provider creation for different models
- Configuration validation
- Fallback to pruning on failure
- Comprehensive logging

#### Task 3.6: Success Criteria

- All quality checks pass
- Auto-summarization triggers at correct threshold
- Summary model configuration works
- Fallback logic handles errors gracefully
- Run mode continues execution after summary
- Logging provides clear audit trail

### Phase 4: Documentation and Configuration Examples

**Goal**: Document the new features and provide configuration examples.

#### Task 4.1: Update Configuration Reference

**File**: `docs/reference/configuration.md`

Add section for context window management:

````markdown
### Conversation Context Window Management

Configure how XZatoma manages the conversation context window:

- `warning_threshold`: Token usage percentage to trigger warnings (0.0-1.0, default: 0.85)
- `auto_summary_threshold`: Token usage percentage to trigger automatic summarization (0.0-1.0, default: 0.90)
- `summary_model`: Model to use for generating summaries (optional, defaults to current model)

Example:

```yaml
agent:
  conversation:
    max_tokens: 100000
    min_retain_turns: 5
    prune_threshold: 0.8
    warning_threshold: 0.85
    auto_summary_threshold: 0.90
    summary_model: "gpt-5.1-codex-mini" # Use cheaper model for summaries
```
````

````

#### Task 4.2: Create How-To Guide

**File**: `docs/how-to/manage_context_window.md`

Create comprehensive guide:
```markdown
# How to Manage Context Window

## Understanding Context Window Limits

Every AI model has a context window limit...

## Monitoring Context Usage

Use `/context info` to check current usage...

## Manual Summarization in Chat Mode

When you see the warning...

## Automatic Summarization in Run Mode

In run mode, XZatoma automatically...

## Configuring Summary Models

To save costs, use a cheaper model for summaries...
````

#### Task 4.3: Update Example Configurations

**File**: `config/config.yaml`

Add commented examples:

```yaml
agent:
  conversation:
    max_tokens: 100000
    min_retain_turns: 5
    prune_threshold: 0.8

    # Context window management
    warning_threshold: 0.85 # Warn at 85% full
    auto_summary_threshold: 0.90 # Auto-summarize at 90% full


    # Use cheaper model for summaries (optional)
    # summary_model: "gpt-5.1-codex-mini"
```

#### Task 4.4: Update Command Help Text

**File**: `src/commands/special_commands.rs`

Update `print_help()`:

```rust
println!("  /context info              Show context window usage");
println!("  /context summary           Summarize and reset context");
println!("  /context summary -m MODEL  Summarize using specific model");
```

#### Task 4.5: Create Implementation Summary

**File**: `docs/explanation/context_window_management_implementation.md`

Document the implementation following the template in AGENTS.md.

#### Task 4.6: Deliverables

- Updated configuration reference documentation
- How-to guide for context window management
- Example configurations with comments
- Updated help text
- Implementation summary document

#### Task 4.7: Success Criteria

- Documentation follows Diataxis framework
- All files use lowercase_with_underscores.md naming
- No emojis in documentation
- Code examples specify language in blocks
- Configuration examples are valid YAML

## Configuration Schema

### New Fields in config.yaml

```yaml
agent:
  conversation:
    max_tokens: 100000 # Existing
    min_retain_turns: 5 # Existing
    prune_threshold: 0.8 # Existing
    warning_threshold: 0.85 # NEW: Warn user at 85%
    auto_summary_threshold: 0.90 # NEW: Auto-summarize at 90%
    summary_model: "gpt-5.1-codex-mini" # NEW: Model for summaries (optional)
```

### Environment Variable Overrides

- `XZATOMA_CONVERSATION_WARNING_THRESHOLD`
- `XZATOMA_CONVERSATION_AUTO_SUMMARY_THRESHOLD`
- `XZATOMA_CONVERSATION_SUMMARY_MODEL`

## Command Reference

### New Commands

```bash
# Show context window information
/context info
/context              # Alias for /context info

# Summarize current context
/context summary                    # Use default/config model
/context summary --model gpt-5.1-codex-mini  # Use specific model
/context summary -m gpt-5.1-codex-mini       # Short flag
```

## Flow Diagrams

### Chat Mode Flow

```text
User sends message
  ↓
Agent processes and responds
  ↓
Check context status
  ↓
├─ Normal (<85%) → Continue
├─ Warning (85-90%) → Display warning, suggest /context summary
└─ Critical (>90%) → Display critical warning, recommend immediate action
  ↓
User can run /context summary
  ↓
System creates summary using configured model
  ↓
Conversation reset with summary as context
```

### Run Mode Flow

```text
Agent executing plan
  ↓
After each turn, check context
  ↓
├─ Below 90% → Continue execution
└─ Above 90% → Trigger auto-summary
      ↓
  Create summary provider (if different model)
      ↓
  Generate summary via AI
      ↓
  ├─ Success → Reset conversation, continue execution
  └─ Failure → Fall back to pruning, continue execution
```

## Testing Strategy

### Unit Tests

- Configuration parsing and validation
- Threshold detection logic
- Command parsing for `/context` variants
- Context status enumeration
- Summary prompt generation

### Integration Tests

- Warning display in chat mode
- Manual summarization flow
- Automatic summarization in run mode
- Model override functionality
- Fallback to pruning on error

### Manual Testing Checklist

- [ ] Chat mode shows warning at 85% threshold
- [ ] Chat mode shows critical warning at 90% threshold
- [ ] `/context info` displays correct usage
- [ ] `/context summary` works with default model
- [ ] `/context summary --model X` uses specified model
- [ ] Run mode auto-summarizes at threshold
- [ ] Summary model configuration works
- [ ] Environment variables override config
- [ ] Help text includes new commands
- [ ] Error handling for invalid models

## Migration Guide

### For Users

No breaking changes. New features are opt-in via configuration.

To enable:

1. Add `warning_threshold` and `auto_summary_threshold` to config
2. Optionally configure `summary_model` for cost savings
3. Use `/context summary` when warned in chat mode

### For Developers

No API changes to public interfaces. Internal additions:

- `ConversationConfig` has new optional fields
- `SpecialCommand` enum has new variants
- `Conversation` has new public methods

## Open Questions

1. Should we persist summaries to conversation history database?

   - Option A: Yes, store summaries as metadata
   - Option B: No, summaries are transient
   - **Recommendation**: Option B for Phase 1, Option A for future enhancement

2. Should summary generation have a timeout?

   - Option A: Use standard provider timeout
   - Option B: Add separate summary timeout config
   - **Recommendation**: Option A for simplicity

3. Should we support multiple summary strategies (extractive vs abstractive)?
   - Option A: Single strategy (abstractive via LLM)
   - Option B: Configurable strategies
   - **Recommendation**: Option A for Phase 1

## Success Metrics

- Zero `cargo clippy` warnings
- Test coverage >80% for new code
- All documentation files use correct naming conventions
- Configuration validates correctly
- Chat mode warnings appear at correct thresholds
- Run mode completes long-running tasks without context overflow
- Users can override summary model via config or command

## References

- [Conversation Management](src/agent/conversation.rs)
- [Special Commands](src/commands/special_commands.rs)
- [Configuration](src/config.rs)
- [Chat Mode Architecture](docs/explanation/chat_modes_architecture.md)
- [AGENTS.md Rules](AGENTS.md)
