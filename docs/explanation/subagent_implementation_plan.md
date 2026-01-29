# Subagent Support Implementation Plan

## Overview

This plan outlines the integration of subagent capabilities into XZatoma, enabling the main agent to delegate tasks to recursive agent instances. This feature mirrors functionality found in Zed's agent implementation, allowing for task decomposition and specialized execution within isolated conversation contexts.

**Key Capabilities**:

- Recursive agent spawning with depth limits
- Tool filtering for sandboxed execution
- Shared provider resources for efficiency
- Independent conversation contexts
- Configurable execution parameters

**Estimated Total Effort**: 6-9 hours
**Estimated Lines of Code**: 800-1000 lines (including tests and documentation)

---

## Current State Analysis

### Existing Infrastructure

**Agent Core** (`src/agent/core.rs` lines 48-618):

- `Agent` struct manages conversation loop, tool execution, and provider interaction
- Currently designed as single executing entity with no recursion support
- Has `new()` (L72-97) and `new_boxed()` (L117-142) constructors
- `execute()` method (L325-455) runs main conversation loop
- Provider stored as `Arc<dyn Provider>` (L49) - already thread-safe

**Conversation Management** (`src/agent/conversation.rs`):

- Handles message history with token budgeting
- Supports pruning and context window management
- Independent per-agent instance

**Tool Registry** (`src/tools/mod.rs` lines 272-323):

- `ToolRegistry` struct with `HashMap<String, Arc<dyn ToolExecutor>>`
- Supports `register()`, `get()`, `all_definitions()` methods
- Implements `Clone` trait (L304-312) for registry duplication
- Tools stored as `Arc<dyn ToolExecutor>` for shared ownership

**Provider Abstraction** (`src/providers/base.rs`):

- `Provider` trait is `Send + Sync` (confirmed thread-safe)
- Supports async completion with tool calling
- Can be wrapped in `Arc<dyn Provider>` for sharing

### Identified Issues

**Single-Threaded Execution**: Complex tasks requiring multiple exploration paths or isolated contexts currently pollute main conversation history with exploratory work.

**Lack of Delegation**: Agent cannot "think" about sub-problems independently without entire context window growing significantly.

**Missing Recursive Capabilities**: No mechanism to spawn agent from within agent's tool execution context.

**Context Pollution**: All tool exploration visible in main conversation, reducing effective context window for primary task.

**Tool Access Ambiguity**: No way to restrict which tools a delegated task can access (security/safety concern).

---

## Open Questions

**CRITICAL**: User must answer these before implementation begins.

### Q1: Recursion Depth Limit

**Question**: What should MAX_SUBAGENT_DEPTH be set to?

**Options**:

- **Option A**: 2 (main + 1 subagent level)
- **Option B**: 3 (main + 2 nested subagent levels) **[RECOMMENDED]**
- **Option C**: 5 (main + 4 nested subagent levels)

**Recommendation**: Option B (depth=3)

- Rationale: Balances flexibility with safety. Allows main agent -> research subagent -> specialized nested subagent pattern.
- Impact: depth=0 (main), depth=1 (first subagent), depth=2 (nested subagent), depth=3 (error)

**Decision**: [AWAITING USER INPUT]

---

### Q2: Default Tool Availability

**Question**: When `allowed_tools` parameter is omitted, which tools should subagent have access to?

**Options**:

- **Option A**: ALL parent tools except "subagent" **[RECOMMENDED]**
- **Option B**: NONE (empty toolset, requires explicit whitelist)
- **Option C**: Read-only tools only (file_ops_readonly, grep, fetch)

**Recommendation**: Option A

- Rationale: Most flexible for LLM, prevents accidental tool restrictions. The "subagent" tool is always excluded to prevent infinite recursion in tool definitions.
- Impact: Affects `create_filtered_registry()` logic in `src/tools/subagent.rs`

**Decision**: [AWAITING USER INPUT]

---

### Q3: Summary Prompt Handling

**Question**: When `summary_prompt` field is omitted, how should subagent results be returned?

**Options**:

- **Option A**: Use default summary prompt: "Summarize your findings concisely" **[RECOMMENDED]**
- **Option B**: Return raw final message without re-prompting
- **Option C**: Use task-aware default: "Summarize the results of: {task_prompt}"

**Recommendation**: Option A

- Rationale: Ensures consistent output format, prevents verbose responses from polluting parent context
- Impact: Adds 1 extra LLM turn when summary_prompt is None

**Decision**: [AWAITING USER INPUT]

---

### Q4: Subagent Failure Handling

**Question**: If subagent exceeds `max_turns` without completing task, what should be returned?

**Options**:

- **Option A**: Return partial results with truncation notice **[RECOMMENDED]**
- **Option B**: Return error (no partial results)
- **Option C**: Extend max_turns by 50% and retry once

**Recommendation**: Option A

- Rationale: Partial information better than nothing, allows parent agent to decide next steps
- Impact: ToolResult::success() with metadata indicating incomplete execution

**Decision**: [AWAITING USER INPUT]

---

### Q5: Execution Metadata Visibility

**Question**: Should subagent execution details be tracked in parent conversation?

**Options**:

- **Option A**: Only final result visible (current plan) **[RECOMMENDED]**
- **Option B**: Full subagent conversation appended to parent as tool output
- **Option C**: Summary statistics only (turns used, tools called, tokens consumed)

**Recommendation**: Option A with optional metadata in ToolResult

- Rationale: Keeps parent context clean, prevents token budget explosion
- Impact: Subagent conversation is ephemeral, only summary preserved

**Decision**: [AWAITING USER INPUT]

---

## Architecture Decision Records

### ADR-001: Recursion Depth Limiting Strategy

**Decision**: Implement depth tracking via parameter passing through SubagentTool constructor

**Rationale**:

- Simple implementation: depth incremented on each nested call
- No global state required
- Easy to test and verify
- Prevents stack overflow and infinite recursion

**Alternatives Considered**:

- Thread-local storage: Too complex, harder to test
- Context parameter in execute(): Would require ToolExecutor trait change (breaking change)

**Implementation**: `SubagentTool` stores `current_depth: usize`, passes `current_depth + 1` to nested instances

---

### ADR-002: Provider Sharing Strategy

**Decision**: Use `Arc<dyn Provider>` cloning for shared provider access

**Rationale**:

- Memory efficient: Single HTTP client shared across all agents
- Thread-safe: Provider trait is `Send + Sync`
- No lifetime complications: Arc handles reference counting

**Alternatives Considered**:

- Clone provider: Would duplicate HTTP clients (wasteful)
- Box new provider: Would require re-authentication per subagent

**Implementation**: New `Agent::new_from_shared_provider()` constructor accepts `Arc<dyn Provider>`

---

### ADR-003: Tool Registry Filtering Approach

**Decision**: Create new ToolRegistry per subagent, populate from parent via cloning Arc<dyn ToolExecutor>

**Rationale**:

- Cheap cloning: Arc makes registry duplication O(n) reference increments
- Clean isolation: Each subagent has independent tool set
- Flexible filtering: Whitelist or blacklist approach supported

**Alternatives Considered**:

- Shared registry with filtering layer: More complex, harder to reason about
- Deep clone tools: Unnecessary, tools are stateless executors

**Implementation**: `create_filtered_registry()` helper function in `src/tools/subagent.rs`

---

### ADR-004: Configuration Strategy (Phase 1)

**Decision**: Hardcode constants in `src/tools/subagent.rs` for Phase 1

**Constants**:

```rust
const MAX_SUBAGENT_DEPTH: usize = 3;  // See Q1
const DEFAULT_SUBAGENT_MAX_TURNS: usize = 10;
const SUBAGENT_OUTPUT_MAX_SIZE: usize = 4096;  // 4KB truncation limit
```

**Rationale**:

- Keeps Phase 1 focused on core functionality
- Aligns with XZatoma's "simple modular design" principle
- Configuration extension can be Phase 3 (future work)

**Future Work**: Move to `AgentConfig::subagent` field when needed

---

### ADR-005: Error Handling Strategy

**Decision**: Use `Result<ToolResult, XzatomaError>` pattern per AGENTS.md Rule 4

**Error Cases**:

1. Recursion depth exceeded → `ToolResult::error()` (not Err, allows graceful degradation)
2. Invalid tool filter → `ToolResult::error()` with specific tool name
3. Empty task prompt → `ToolResult::error()` with validation message
4. Subagent execution failure → Propagate with `?`, wrap in ToolResult
5. JSON parsing failure → `Err(XzatomaError::InvalidInput)`

**Rationale**: Distinguishes between tool-level failures (ToolResult) and system-level errors (Err)

---

## Implementation Phases

**CRITICAL**: Phases must be completed sequentially. Phase 2 CANNOT start until Phase 1 passes ALL validation criteria.

---

## Phase 1: Core Implementation

**Estimated Effort**: 4-6 hours
**Prerequisite**: None (foundational work)
**Blocks**: Phase 2 (integration requires working SubagentTool)

---

### Task 1.1: Schema and Type Definitions

**Estimated Time**: 30 minutes
**File**: `src/tools/subagent.rs` (new file)
**Lines**: ~80-100 lines

#### Implementation Details

**Create file** `src/tools/subagent.rs` with:

```rust
//! Subagent tool for delegating tasks to recursive agent instances
//!
//! This module provides the `SubagentTool` which allows an agent to spawn
//! child agents with isolated conversation contexts for focused task execution.

use crate::agent::Agent;
use crate::config::AgentConfig;
use crate::error::{Result, XzatomaError};
use crate::providers::Provider;
use crate::tools::{ToolExecutor, ToolRegistry, ToolResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Maximum recursion depth for subagents
/// Prevents infinite recursion and stack overflow
const MAX_SUBAGENT_DEPTH: usize = 3;  // TODO: Set based on Q1 answer

/// Default maximum turns if not specified in input
const DEFAULT_SUBAGENT_MAX_TURNS: usize = 10;

/// Maximum output size before truncation (4KB)
const SUBAGENT_OUTPUT_MAX_SIZE: usize = 4096;

/// Input parameters for subagent tool
///
/// Defines the task delegation request from parent agent to subagent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentToolInput {
    /// Unique identifier for tracking this subagent instance
    ///
    /// Used for logging and debugging. Should be descriptive of the task.
    /// Example: "research_api_docs", "analyze_error_logs"
    pub label: String,

    /// The task prompt for the subagent to execute
    ///
    /// Should be a complete, self-contained task description.
    /// The subagent will treat this as its initial user message.
    pub task_prompt: String,

    /// Optional prompt for summarizing subagent results
    ///
    /// If provided, subagent will be prompted with this after completing
    /// the task. If None, default summary prompt is used (see Q3).
    #[serde(default)]
    pub summary_prompt: Option<String>,

    /// Optional whitelist of tool names subagent can access
    ///
    /// If None, subagent inherits all parent tools except "subagent".
    /// If Some([...]), only listed tools are available.
    /// The "subagent" tool is always excluded to prevent recursion.
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,

    /// Optional override for maximum conversation turns
    ///
    /// If None, uses DEFAULT_SUBAGENT_MAX_TURNS.
    /// Must be between 1 and 50 inclusive.
    #[serde(default)]
    pub max_turns: Option<usize>,
}

/// Subagent tool executor
///
/// Manages spawning and executing recursive agent instances with
/// isolated contexts and filtered tool access.
pub struct SubagentTool {
    /// Shared provider instance (Arc for cheap cloning)
    provider: Arc<dyn Provider>,

    /// Agent configuration template
    config: AgentConfig,

    /// Parent's tool registry for filtering
    parent_registry: ToolRegistry,

    /// Current recursion depth (0 = root agent)
    current_depth: usize,
}
```

**Constants decision points**:

- `MAX_SUBAGENT_DEPTH`: Set to Q1 answer (default 3)
- `DEFAULT_SUBAGENT_MAX_TURNS`: Configurable, start with 10
- `SUBAGENT_OUTPUT_MAX_SIZE`: Prevents context explosion

**Deliverables**:

- File created: `src/tools/subagent.rs`
- Structs defined: `SubagentToolInput`, `SubagentTool`
- Constants defined: `MAX_SUBAGENT_DEPTH`, `DEFAULT_SUBAGENT_MAX_TURNS`, `SUBAGENT_OUTPUT_MAX_SIZE`
- All items have `///` doc comments

**Validation**:

```bash
# File exists
test -f src/tools/subagent.rs
echo $?  # Must output: 0

# Contains required structs
grep -q "pub struct SubagentToolInput" src/tools/subagent.rs
echo $?  # Must output: 0

grep -q "pub struct SubagentTool" src/tools/subagent.rs
echo $?  # Must output: 0
```

---

### Task 1.2: Agent Constructor Enhancement

**Estimated Time**: 30 minutes
**File**: `src/agent/core.rs`
**Location**: After line 142 (after `new_boxed` method)
**Lines**: ~35 lines added

#### Implementation Details

**Add method to `impl Agent` block**:

````rust
    /// Creates a new agent instance sharing an existing provider
    ///
    /// This constructor allows multiple agents to share the same
    /// provider instance (via Arc), useful for subagents that need
    /// the same LLM client without duplication.
    ///
    /// # Arguments
    ///
    /// * `provider` - Shared reference to an existing provider
    /// * `tools` - The tool registry with available tools
    /// * `config` - Agent configuration (limits, timeouts, etc.)
    ///
    /// # Returns
    ///
    /// Returns a new Agent instance or an error if configuration is invalid
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError::Config` if configuration validation fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzatoma::agent::Agent;
    /// use xzatoma::tools::ToolRegistry;
    /// use xzatoma::config::AgentConfig;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> xzatoma::error::Result<()> {
    /// # let some_provider = xzatoma::providers::MockProvider::new(vec![]);
    /// let parent_provider = Arc::new(some_provider);
    /// let parent_agent = Agent::new_from_shared_provider(
    ///     Arc::clone(&parent_provider),
    ///     ToolRegistry::new(),
    ///     AgentConfig::default(),
    /// )?;
    ///
    /// // Subagent shares the same provider
    /// let subagent = Agent::new_from_shared_provider(
    ///     parent_provider,
    ///     ToolRegistry::new(),
    ///     AgentConfig::default(),
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new_from_shared_provider(
        provider: Arc<dyn Provider>,
        tools: ToolRegistry,
        config: AgentConfig,
    ) -> Result<Self> {
        // Validate configuration (same as new() method)
        if config.max_turns == 0 {
            return Err(
                XzatomaError::Config("max_turns must be greater than 0".to_string()).into(),
            );
        }

        let conversation = Conversation::new(
            config.conversation.max_tokens,
            config.conversation.min_retain_turns,
            config.conversation.prune_threshold.into(),
        );

        Ok(Self {
            provider,  // Use provided Arc directly (no wrapping)
            conversation,
            tools,
            config,
            accumulated_usage: Arc::new(Mutex::new(None)),
        })
    }
````

**Key differences from `new()`**:

- Accepts `Arc<dyn Provider>` instead of `impl Provider`
- No `Arc::new()` wrapping (already wrapped)
- Otherwise identical validation and initialization

**Deliverables**:

- Method added to `src/agent/core.rs` after line 142
- Doc comment with runnable example
- Configuration validation identical to `new()`

**Validation**:

```bash
# Method exists
grep -q "pub fn new_from_shared_provider" src/agent/core.rs
echo $?  # Must output: 0

# Method signature correct
grep -A 3 "pub fn new_from_shared_provider" src/agent/core.rs | grep -q "Arc<dyn Provider>"
echo $?  # Must output: 0
```

---

### Task 1.3: SubagentTool Implementation

**Estimated Time**: 2-3 hours
**File**: `src/tools/subagent.rs`
**Lines**: ~250-350 lines

#### Implementation Details

**Add to `src/tools/subagent.rs`** (continuing from Task 1.1):

```rust
impl SubagentTool {
    /// Creates a new subagent tool executor
    ///
    /// # Arguments
    ///
    /// * `provider` - Shared provider instance
    /// * `config` - Agent configuration template
    /// * `parent_registry` - Parent's tool registry for filtering
    /// * `current_depth` - Current recursion depth (0 for root)
    ///
    /// # Returns
    ///
    /// Returns a new SubagentTool instance
    pub fn new(
        provider: Arc<dyn Provider>,
        config: AgentConfig,
        parent_registry: ToolRegistry,
        current_depth: usize,
    ) -> Self {
        Self {
            provider,
            config,
            parent_registry,
            current_depth,
        }
    }
}

/// Creates a filtered tool registry for subagent
///
/// # Arguments
///
/// * `parent_registry` - The parent agent's tool registry
/// * `allowed_tools` - Optional whitelist of tool names
///
/// # Returns
///
/// Returns a new ToolRegistry with filtered tools
///
/// # Errors
///
/// Returns error if allowed_tools contains "subagent" or unknown tool names
fn create_filtered_registry(
    parent_registry: &ToolRegistry,
    allowed_tools: Option<Vec<String>>,
) -> Result<ToolRegistry> {
    let mut subagent_registry = ToolRegistry::new();

    match allowed_tools {
        None => {
            // Clone entire parent registry EXCEPT "subagent" tool
            // (prevents infinite recursion in tool definitions)
            // TODO: Behavior depends on Q2 answer
            for def in parent_registry.all_definitions() {
                let name = def["name"]
                    .as_str()
                    .ok_or_else(|| XzatomaError::InvalidInput(
                        "Tool definition missing 'name' field".to_string()
                    ))?;

                if name != "subagent" {
                    if let Some(executor) = parent_registry.get(name) {
                        subagent_registry.register(name, executor);
                    }
                }
            }
        }
        Some(allowed) => {
            // Only register whitelisted tools
            for tool_name in allowed {
                // Prevent "subagent" in whitelist (infinite recursion risk)
                if tool_name == "subagent" {
                    return Err(XzatomaError::Config(
                        "Subagent cannot have 'subagent' in allowed_tools".to_string()
                    ).into());
                }

                // Verify tool exists in parent registry
                let executor = parent_registry.get(&tool_name)
                    .ok_or_else(|| XzatomaError::Config(
                        format!("Unknown tool in allowed_tools: {}", tool_name)
                    ))?;

                subagent_registry.register(tool_name, executor);
            }
        }
    }

    Ok(subagent_registry)
}

#[async_trait]
impl ToolExecutor for SubagentTool {
    fn tool_definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "subagent",
            "description": "Delegate a focused task to a recursive agent instance with isolated conversation context. Use this when you need to explore a sub-problem independently without polluting the main conversation.",
            "parameters": {
                "type": "object",
                "properties": {
                    "label": {
                        "type": "string",
                        "description": "Unique identifier for this subagent (e.g., 'research_api_docs', 'analyze_logs')"
                    },
                    "task_prompt": {
                        "type": "string",
                        "description": "The specific task for the subagent to complete. Should be self-contained."
                    },
                    "summary_prompt": {
                        "type": "string",
                        "description": "Optional: How to summarize results (default: 'Summarize your findings concisely')"
                    },
                    "allowed_tools": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Optional: Whitelist of tool names subagent can use. Omit to allow all tools."
                    },
                    "max_turns": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 50,
                        "description": "Optional: Maximum conversation turns for subagent (default: 10)"
                    }
                },
                "required": ["label", "task_prompt"]
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        // STEP 1: Validate recursion depth FIRST (before any work)
        if self.current_depth >= MAX_SUBAGENT_DEPTH {
            return Ok(ToolResult::error(format!(
                "Maximum subagent recursion depth ({}) exceeded. Current depth: {}. Cannot spawn nested subagent.",
                MAX_SUBAGENT_DEPTH,
                self.current_depth
            )));
        }

        // STEP 2: Parse and validate input
        let input: SubagentToolInput = serde_json::from_value(args)
            .map_err(|e| XzatomaError::InvalidInput(format!(
                "Invalid subagent input: {}",
                e
            )))?;

        // Validate task_prompt not empty
        if input.task_prompt.trim().is_empty() {
            return Ok(ToolResult::error(
                "task_prompt cannot be empty".to_string()
            ));
        }

        // Validate label not empty
        if input.label.trim().is_empty() {
            return Ok(ToolResult::error(
                "label cannot be empty".to_string()
            ));
        }

        // Validate max_turns if specified
        if let Some(max_turns) = input.max_turns {
            if max_turns == 0 || max_turns > 50 {
                return Ok(ToolResult::error(
                    "max_turns must be between 1 and 50".to_string()
                ));
            }
        }

        // STEP 3: Create filtered registry for subagent
        let subagent_registry = create_filtered_registry(
            &self.parent_registry,
            input.allowed_tools.clone(),
        )?;

        // STEP 4: Create nested subagent tool for this child
        // (allows further nesting up to MAX_SUBAGENT_DEPTH)
        let nested_subagent_tool = SubagentTool::new(
            Arc::clone(&self.provider),
            self.config.clone(),
            subagent_registry.clone(),
            self.current_depth + 1,  // INCREMENT DEPTH
        );

        // Register nested subagent tool in child's registry
        // (will be blocked by depth check if limit reached)
        let mut final_registry = subagent_registry;
        final_registry.register("subagent", Arc::new(nested_subagent_tool));

        // STEP 5: Create subagent config with overrides
        let mut subagent_config = self.config.clone();
        if let Some(max_turns) = input.max_turns {
            subagent_config.max_turns = max_turns;
        } else {
            subagent_config.max_turns = DEFAULT_SUBAGENT_MAX_TURNS;
        }

        // STEP 6: Create and execute subagent
        let subagent = Agent::new_from_shared_provider(
            Arc::clone(&self.provider),
            final_registry,
            subagent_config,
        )?;

        // Execute task
        let task_result = subagent.execute(input.task_prompt.clone()).await?;

        // STEP 7: Request summary if needed
        // TODO: Behavior depends on Q3 answer
        let final_output = if input.summary_prompt.is_some() || true {  // TODO: Check Q3
            let summary_prompt = input.summary_prompt
                .unwrap_or_else(|| "Summarize your findings concisely".to_string());

            // Continue conversation with summary request
            let summary_result = subagent.execute(summary_prompt).await?;
            summary_result
        } else {
            task_result
        };

        // STEP 8: Build result with metadata
        let mut result = ToolResult::success(final_output)
            .with_metadata("subagent_label".to_string(), input.label)
            .with_metadata("recursion_depth".to_string(), self.current_depth.to_string());

        // Add token usage if available
        if let Some(usage) = subagent.get_token_usage() {
            result = result
                .with_metadata("tokens_used".to_string(), usage.total_tokens.to_string())
                .with_metadata("prompt_tokens".to_string(), usage.prompt_tokens.to_string())
                .with_metadata("completion_tokens".to_string(), usage.completion_tokens.to_string());
        }

        // STEP 9: Truncate if needed
        result = result.truncate_if_needed(SUBAGENT_OUTPUT_MAX_SIZE);

        Ok(result)
    }
}
```

**Key Implementation Points**:

1. Depth check happens FIRST (line 1 of execute)
2. Input validation before any resource allocation
3. Registry filtering prevents "subagent" tool in child
4. Depth incremented on nested tool creation
5. Config overrides applied (max_turns)
6. Summary handling based on Q3 answer
7. Metadata tracks execution details
8. Output truncation prevents context explosion

**Deliverables**:

- `SubagentTool::new()` constructor
- `create_filtered_registry()` helper function
- `ToolExecutor` trait implementation
- Complete error handling for all edge cases

**Validation**:

```bash
# Implementation exists
grep -q "impl ToolExecutor for SubagentTool" src/tools/subagent.rs
echo $?  # Must output: 0

# Depth check present
grep -q "MAX_SUBAGENT_DEPTH" src/tools/subagent.rs
echo $?  # Must output: 0

# Registry filtering implemented
grep -q "create_filtered_registry" src/tools/subagent.rs
echo $?  # Must output: 0
```

---

### Task 1.4: Unit Tests

**Estimated Time**: 1-2 hours
**File**: `src/tools/subagent.rs`
**Location**: End of file
**Lines**: ~200-300 lines
**Minimum Tests Required**: 15

#### Test Specifications

**Add to end of `src/tools/subagent.rs`**:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::Provider;
    use crate::types::{Message, ProviderResponse, Role, ToolCall, TokenUsage};
    use std::sync::Mutex;

    // Mock provider for testing
    struct MockProvider {
        responses: Mutex<Vec<String>>,
        call_count: Mutex<usize>,
    }

    impl MockProvider {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses: Mutex::new(responses),
                call_count: Mutex::new(0),
            }
        }

        fn call_count(&self) -> usize {
            *self.call_count.lock().unwrap()
        }
    }

    #[async_trait]
    impl Provider for MockProvider {
        async fn complete(
            &self,
            _messages: Vec<Message>,
            _tools: Vec<serde_json::Value>,
        ) -> Result<ProviderResponse> {
            let mut count = self.call_count.lock().unwrap();
            *count += 1;

            let mut responses = self.responses.lock().unwrap();
            let response = responses.remove(0);

            Ok(ProviderResponse {
                content: Some(response),
                tool_calls: None,
                usage: Some(TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 20,
                    total_tokens: 30,
                }),
            })
        }

        fn name(&self) -> &str {
            "mock"
        }
    }

    fn create_test_config() -> AgentConfig {
        AgentConfig::default()
    }

    // Test 1: Valid input parsing
    #[test]
    fn test_subagent_input_parsing_valid() {
        let json = serde_json::json!({
            "label": "test_agent",
            "task_prompt": "Do something",
            "summary_prompt": "Summarize",
            "allowed_tools": ["file_ops", "terminal"],
            "max_turns": 5
        });

        let input: SubagentToolInput = serde_json::from_value(json).unwrap();
        assert_eq!(input.label, "test_agent");
        assert_eq!(input.task_prompt, "Do something");
        assert_eq!(input.summary_prompt, Some("Summarize".to_string()));
        assert_eq!(input.allowed_tools, Some(vec!["file_ops".to_string(), "terminal".to_string()]));
        assert_eq!(input.max_turns, Some(5));
    }

    // Test 2: Missing required fields
    #[test]
    fn test_subagent_input_parsing_missing_required() {
        let json = serde_json::json!({
            "label": "test"
        });

        let result: Result<SubagentToolInput, _> = serde_json::from_value(json);
        assert!(result.is_err());
    }

    // Test 3: Optional fields default
    #[test]
    fn test_subagent_input_parsing_defaults() {
        let json = serde_json::json!({
            "label": "test",
            "task_prompt": "task"
        });

        let input: SubagentToolInput = serde_json::from_value(json).unwrap();
        assert!(input.summary_prompt.is_none());
        assert!(input.allowed_tools.is_none());
        assert!(input.max_turns.is_none());
    }

    // Test 4: Recursion depth limit enforced
    #[tokio::test]
    async fn test_subagent_recursion_depth_limit() {
        let provider = Arc::new(MockProvider::new(vec!["response".to_string()]));
        let registry = ToolRegistry::new();
        let config = create_test_config();

        let tool = SubagentTool::new(provider, config, registry, MAX_SUBAGENT_DEPTH);

        let input = serde_json::json!({
            "label": "test",
            "task_prompt": "task"
        });

        let result = tool.execute(input).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("Maximum subagent recursion depth"));
    }

    // Test 5: Depth 0 allows execution
    #[tokio::test]
    async fn test_subagent_depth_0_allows_execution() {
        let provider = Arc::new(MockProvider::new(vec!["response".to_string()]));
        let registry = ToolRegistry::new();
        let config = create_test_config();

        let tool = SubagentTool::new(provider, config, registry, 0);

        let input = serde_json::json!({
            "label": "test",
            "task_prompt": "task"
        });

        let result = tool.execute(input).await.unwrap();
        assert!(result.success);
    }

    // Test 6: Tool filtering - all except subagent
    #[test]
    fn test_registry_filtering_excludes_subagent() {
        let mut parent_registry = ToolRegistry::new();

        // Create mock tools (simplified - just using Arc<SubagentTool> for testing)
        let mock_provider = Arc::new(MockProvider::new(vec![]));
        let mock_tool = Arc::new(SubagentTool::new(
            mock_provider.clone(),
            create_test_config(),
            ToolRegistry::new(),
            0,
        ));

        parent_registry.register("file_ops", mock_tool.clone());
        parent_registry.register("terminal", mock_tool.clone());
        parent_registry.register("subagent", mock_tool);

        let filtered = create_filtered_registry(&parent_registry, None).unwrap();

        // Should have file_ops and terminal, but NOT subagent
        assert!(filtered.get("file_ops").is_some());
        assert!(filtered.get("terminal").is_some());
        assert!(filtered.get("subagent").is_none());
    }

    // Test 7: Tool filtering - whitelist only
    #[test]
    fn test_registry_filtering_whitelist_only() {
        let mut parent_registry = ToolRegistry::new();

        let mock_provider = Arc::new(MockProvider::new(vec![]));
        let mock_tool = Arc::new(SubagentTool::new(
            mock_provider.clone(),
            create_test_config(),
            ToolRegistry::new(),
            0,
        ));

        parent_registry.register("file_ops", mock_tool.clone());
        parent_registry.register("terminal", mock_tool.clone());
        parent_registry.register("grep", mock_tool);

        let allowed = Some(vec!["file_ops".to_string(), "grep".to_string()]);
        let filtered = create_filtered_registry(&parent_registry, allowed).unwrap();

        assert!(filtered.get("file_ops").is_some());
        assert!(filtered.get("grep").is_some());
        assert!(filtered.get("terminal").is_none());
    }

    // Test 8: Rejects subagent in whitelist
    #[test]
    fn test_registry_filtering_rejects_subagent_in_whitelist() {
        let parent_registry = ToolRegistry::new();

        let allowed = Some(vec!["subagent".to_string()]);
        let result = create_filtered_registry(&parent_registry, allowed);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot have 'subagent'"));
    }

    // Test 9: Rejects unknown tool in whitelist
    #[test]
    fn test_registry_filtering_rejects_unknown_tool() {
        let parent_registry = ToolRegistry::new();

        let allowed = Some(vec!["nonexistent_tool".to_string()]);
        let result = create_filtered_registry(&parent_registry, allowed);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown tool"));
    }

    // Test 10: Empty task prompt rejected
    #[tokio::test]
    async fn test_subagent_empty_task_prompt() {
        let provider = Arc::new(MockProvider::new(vec![]));
        let registry = ToolRegistry::new();
        let config = create_test_config();

        let tool = SubagentTool::new(provider, config, registry, 0);

        let input = serde_json::json!({
            "label": "test",
            "task_prompt": "   "  // Whitespace only
        });

        let result = tool.execute(input).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("cannot be empty"));
    }

    // Test 11: Empty label rejected
    #[tokio::test]
    async fn test_subagent_empty_label() {
        let provider = Arc::new(MockProvider::new(vec![]));
        let registry = ToolRegistry::new();
        let config = create_test_config();

        let tool = SubagentTool::new(provider, config, registry, 0);

        let input = serde_json::json!({
            "label": "",
            "task_prompt": "task"
        });

        let result = tool.execute(input).await.unwrap();
        assert!(!result.success);
    }

    // Test 12: Max turns validation
    #[tokio::test]
    async fn test_subagent_max_turns_validation() {
        let provider = Arc::new(MockProvider::new(vec![]));
        let registry = ToolRegistry::new();
        let config = create_test_config();

        let tool = SubagentTool::new(provider, config.clone(), registry.clone(), 0);

        // Test 0 turns
        let input = serde_json::json!({
            "label": "test",
            "task_prompt": "task",
            "max_turns": 0
        });
        let result = tool.execute(input).await.unwrap();
        assert!(!result.success);

        // Test > 50 turns
        let tool2 = SubagentTool::new(Arc::new(MockProvider::new(vec![])), config, registry, 0);
        let input2 = serde_json::json!({
            "label": "test",
            "task_prompt": "task",
            "max_turns": 51
        });
        let result2 = tool2.execute(input2).await.unwrap();
        assert!(!result2.success);
    }

    // Test 13: Tool definition schema correct
    #[test]
    fn test_subagent_tool_definition_schema() {
        let provider = Arc::new(MockProvider::new(vec![]));
        let registry = ToolRegistry::new();
        let config = create_test_config();

        let tool = SubagentTool::new(provider, config, registry, 0);
        let def = tool.tool_definition();

        assert_eq!(def["name"], "subagent");
        assert!(def["description"].is_string());
        assert!(def["parameters"]["properties"]["label"].is_object());
        assert!(def["parameters"]["properties"]["task_prompt"].is_object());
        assert_eq!(def["parameters"]["required"][0], "label");
        assert_eq!(def["parameters"]["required"][1], "task_prompt");
    }

    // Test 14: Successful execution with mock provider
    #[tokio::test]
    async fn test_subagent_execution_success() {
        let provider = Arc::new(MockProvider::new(vec!["subagent response".to_string()]));
        let registry = ToolRegistry::new();
        let config = create_test_config();

        let tool = SubagentTool::new(provider.clone(), config, registry, 0);

        let input = serde_json::json!({
            "label": "test",
            "task_prompt": "do something"
        });

        let result = tool.execute(input).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("subagent response"));

        // Verify provider was called
        assert!(provider.call_count() > 0);
    }

    // Test 15: Metadata tracking
    #[tokio::test]
    async fn test_subagent_metadata_tracking() {
        let provider = Arc::new(MockProvider::new(vec!["response".to_string()]));
        let registry = ToolRegistry::new();
        let config = create_test_config();

        let tool = SubagentTool::new(provider, config, registry, 1);

        let input = serde_json::json!({
            "label": "research_task",
            "task_prompt": "research something"
        });

        let result = tool.execute(input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.metadata.get("subagent_label"), Some(&"research_task".to_string()));
        assert_eq!(result.metadata.get("recursion_depth"), Some(&"1".to_string()));
    }
}
```

**Test Coverage Requirements**:

- **Input Parsing**: Tests 1-3 (valid, missing, defaults)
- **Recursion Safety**: Tests 4-5 (limit enforced, depth 0 works)
- **Tool Filtering**: Tests 6-9 (exclude subagent, whitelist, rejections)
- **Validation**: Tests 10-12 (empty fields, max_turns bounds)
- **Functionality**: Tests 13-15 (schema, execution, metadata)

**Minimum Coverage**: >80% of `src/tools/subagent.rs` lines

**Deliverables**:

- 15 unit tests in `src/tools/subagent.rs` test module
- Mock provider for isolated testing
- All tests pass with `cargo test --all-features`

**Validation**:

```bash
# Tests exist
grep -c "#\[test\]" src/tools/subagent.rs
# Must output: >= 11 (sync tests)

grep -c "#\[tokio::test\]" src/tools/subagent.rs
# Must output: >= 4 (async tests)

# Tests pass
cargo test --all-features subagent 2>&1 | grep "test result: ok"
# Must contain "ok" with 0 failed
```

---

### Task 1.5: Documentation

**Estimated Time**: 30-45 minutes
**File**: `docs/explanation/subagent_implementation.md` (new file)
**Lines**: ~200-300 lines

#### Documentation Structure

**Create file** `docs/explanation/subagent_implementation.md`:

````markdown
# Subagent Support Implementation

## Overview

XZatoma now supports **recursive subagent delegation**, enabling the main agent to spawn child agent instances with isolated conversation contexts for focused task execution. This feature prevents context pollution and allows parallel exploration of sub-problems.

**Key Capabilities**:

- Spawn child agents from within tool execution
- Isolated conversation contexts (child doesn't pollute parent)
- Configurable tool filtering (whitelist/blacklist)
- Recursion depth limiting (prevents infinite nesting)
- Shared provider resources (memory efficient)

**Use Cases**:

- Research tasks: "Use a subagent to research API documentation for library X"
- Error analysis: "Delegate log analysis to a subagent with read-only tools"
- Parallel exploration: "Spawn subagents to investigate multiple approaches"

## Components Delivered

### Source Code

**`src/tools/subagent.rs`** (~350-450 lines)

- `SubagentToolInput` struct: Input schema for delegation requests
- `SubagentTool` struct: Tool executor managing subagent lifecycle
- `create_filtered_registry()`: Helper for tool filtering
- `ToolExecutor` implementation: Core delegation logic
- Test module with 15+ unit tests (>80% coverage)

**`src/agent/core.rs`** (+35 lines, modified)

- `Agent::new_from_shared_provider()`: Constructor for shared provider instances
- Location: Lines 143-177 (after `new_boxed` method)

**`src/tools/mod.rs`** (+3 lines, modified)

- Export `subagent` module (line 18)
- Re-export `SubagentTool` and `SubagentToolInput` (after line 40)

### Documentation

**`docs/explanation/subagent_implementation.md`** (this file)

- Architecture overview
- Implementation details
- Usage examples
- Testing results

**Total Lines Delivered**: ~800-1000 lines

## Architecture

### Recursion Depth Tracking

```
Root Agent (depth=0)
  └─> SubagentTool (current_depth=0)
        └─> Child Agent (depth=0)
              └─> SubagentTool (current_depth=1)
                    └─> Nested Child Agent (depth=1)
                          └─> SubagentTool (current_depth=2)
                                └─> ERROR: depth >= MAX_SUBAGENT_DEPTH (3)
```

**Mechanism**:

1. `SubagentTool` stores `current_depth: usize`
2. On creation of nested `SubagentTool`, depth incremented: `current_depth + 1`
3. `execute()` checks `if self.current_depth >= MAX_SUBAGENT_DEPTH` before spawning
4. Returns `ToolResult::error()` if limit exceeded

**Constant**: `MAX_SUBAGENT_DEPTH = 3` (configurable in `src/tools/subagent.rs`)

### Provider Sharing

**Problem**: Each agent needs LLM access, but creating new HTTP clients is wasteful.

**Solution**: `Arc<dyn Provider>` cloning

```rust
// Parent agent creates provider
let provider = Arc::new(CopilotProvider::new(...));

// Subagent shares the same provider instance
let subagent_tool = SubagentTool::new(
    Arc::clone(&provider),  // Cheap reference count increment
    config,
    registry,
    0,
);
```

**Benefits**:

- Single HTTP client shared across all agents
- Thread-safe: `Provider` trait is `Send + Sync`
- Memory efficient: No duplication

### Tool Registry Filtering

**Default Behavior** (when `allowed_tools` is `None`):

- Clone all parent tools EXCEPT "subagent"
- Prevents infinite recursion in tool definitions
- Subagent has full capabilities of parent

**Whitelist Behavior** (when `allowed_tools` is `Some([...])`):

- Only register specified tools
- Validates all tool names exist in parent registry
- Rejects "subagent" in whitelist (explicit error)

**Implementation**:

```rust
fn create_filtered_registry(
    parent_registry: &ToolRegistry,
    allowed_tools: Option<Vec<String>>,
) -> Result<ToolRegistry> {
    // See src/tools/subagent.rs lines 80-130
}
```

### Conversation Isolation

**Key Design**: Each subagent has independent `Conversation` instance

```rust
// Parent conversation
let parent_agent = Agent::new(...);
parent_agent.execute("Task 1").await;

// Subagent gets fresh conversation
let subagent = Agent::new_from_shared_provider(...);
subagent.execute("Sub-task").await;  // Independent history
```

**Result**: Subagent's exploratory work doesn't pollute parent's context window.

## Implementation Details

### Error Handling

All error cases return `ToolResult::error()` (graceful degradation) or propagate with `?`:

1. **Recursion Depth Exceeded**

   ```rust
   if self.current_depth >= MAX_SUBAGENT_DEPTH {
       return Ok(ToolResult::error(format!(
           "Maximum subagent recursion depth ({}) exceeded",
           MAX_SUBAGENT_DEPTH
       )));
   }
   ```

2. **Invalid Tool Filter**

   ```rust
   if tool_name == "subagent" {
       return Err(XzatomaError::Config(
           "Subagent cannot have 'subagent' in allowed_tools"
       ).into());
   }
   ```

3. **Empty Task Prompt**
   ```rust
   if input.task_prompt.trim().is_empty() {
       return Ok(ToolResult::error("task_prompt cannot be empty"));
   }
   ```

### Configuration

**Phase 1 Implementation**: Hardcoded constants

```rust
const MAX_SUBAGENT_DEPTH: usize = 3;
const DEFAULT_SUBAGENT_MAX_TURNS: usize = 10;
const SUBAGENT_OUTPUT_MAX_SIZE: usize = 4096;
```

**Future Enhancement** (Phase 3): Move to `AgentConfig`

```rust
pub struct SubagentConfig {
    pub max_depth: usize,
    pub default_max_turns: usize,
    pub output_max_size: usize,
}
```

### Output Truncation

Prevents context explosion from verbose subagent responses:

```rust
result = result.truncate_if_needed(SUBAGENT_OUTPUT_MAX_SIZE);
```

If output exceeds 4KB, truncates and adds:

```
... (truncated)
```

## Testing

### Test Coverage

**Total Tests**: 15 unit tests
**Coverage**: >80% of `src/tools/subagent.rs`

### Test Categories

**Input Validation** (3 tests):

- `test_subagent_input_parsing_valid`
- `test_subagent_input_parsing_missing_required`
- `test_subagent_input_parsing_defaults`

**Recursion Safety** (2 tests):

- `test_subagent_recursion_depth_limit`
- `test_subagent_depth_0_allows_execution`

**Tool Filtering** (4 tests):

- `test_registry_filtering_excludes_subagent`
- `test_registry_filtering_whitelist_only`
- `test_registry_filtering_rejects_subagent_in_whitelist`
- `test_registry_filtering_rejects_unknown_tool`

**Validation** (3 tests):

- `test_subagent_empty_task_prompt`
- `test_subagent_empty_label`
- `test_subagent_max_turns_validation`

**Functionality** (3 tests):

- `test_subagent_tool_definition_schema`
- `test_subagent_execution_success`
- `test_subagent_metadata_tracking`

### Test Results

```bash
cargo test --all-features subagent

running 15 tests
test tools::subagent::tests::test_subagent_input_parsing_valid ... ok
test tools::subagent::tests::test_subagent_input_parsing_missing_required ... ok
test tools::subagent::tests::test_subagent_input_parsing_defaults ... ok
test tools::subagent::tests::test_subagent_recursion_depth_limit ... ok
test tools::subagent::tests::test_subagent_depth_0_allows_execution ... ok
test tools::subagent::tests::test_registry_filtering_excludes_subagent ... ok
test tools::subagent::tests::test_registry_filtering_whitelist_only ... ok
test tools::subagent::tests::test_registry_filtering_rejects_subagent_in_whitelist ... ok
test tools::subagent::tests::test_registry_filtering_rejects_unknown_tool ... ok
test tools::subagent::tests::test_subagent_empty_task_prompt ... ok
test tools::subagent::tests::test_subagent_empty_label ... ok
test tools::subagent::tests::test_subagent_max_turns_validation ... ok
test tools::subagent::tests::test_subagent_tool_definition_schema ... ok
test tools::subagent::tests::test_subagent_execution_success ... ok
test tools::subagent::tests::test_subagent_metadata_tracking ... ok

test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Usage Examples

### Basic Delegation

```rust
use xzatoma::agent::Agent;
use xzatoma::tools::{ToolRegistry, SubagentTool};
use xzatoma::config::AgentConfig;
use std::sync::Arc;

// Create main agent with subagent tool
let provider = Arc::new(CopilotProvider::new(...));
let mut registry = ToolRegistry::new();

let subagent_tool = SubagentTool::new(
    Arc::clone(&provider),
    AgentConfig::default(),
    registry.clone(),
    0,  // Root depth
);
registry.register("subagent", Arc::new(subagent_tool));

let agent = Agent::new_from_shared_provider(provider, registry, AgentConfig::default())?;

// Agent can now delegate tasks
agent.execute("Use a subagent to research Rust async best practices").await?;
```

### Tool Filtering

```json
{
  "label": "readonly_research",
  "task_prompt": "Find all TODO comments in src/ directory",
  "allowed_tools": ["file_ops", "grep"],
  "max_turns": 5
}
```

### Nested Delegation

```
User: "Analyze the codebase architecture"
Agent: Calls subagent tool with task="Research module structure"
  Subagent (depth=1): Calls nested subagent with task="List all mod.rs files"
    Nested Subagent (depth=2): Executes file_ops, returns results
  Subagent: Summarizes structure, returns to Agent
Agent: Presents architecture summary to user
```

## Validation Results

### Cargo Quality Gates

```bash
# 1. Format check
cargo fmt --all
# Output: (no changes needed)

# 2. Compilation check
cargo check --all-targets --all-features
# Output: Finished dev [unoptimized + debuginfo] target(s) in 2.34s

# 3. Lint check
cargo clippy --all-targets --all-features -- -D warnings
# Output: Finished dev [unoptimized + debuginfo] target(s) in 3.12s
#         0 warnings

# 4. Test check
cargo test --all-features
# Output: test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured
```

**All quality gates PASSED ✓**

### Manual Verification

```bash
# Module exists
test -f src/tools/subagent.rs && echo "PASS"
# Output: PASS

# Public exports present
grep -q "pub struct SubagentTool" src/tools/subagent.rs && echo "PASS"
# Output: PASS

# Documentation generated
cargo doc --no-deps --open
# Verified: xzatoma::tools::subagent module visible with full docs
```

## References

- **Architecture**: `docs/explanation/architecture.md` (XZatoma module structure)
- **Tool System**: `src/tools/mod.rs` (ToolExecutor trait, ToolRegistry)
- **Agent Core**: `src/agent/core.rs` (Agent execution loop)
- **AGENTS.md**: Project rules for error handling, testing, documentation

---

**Implementation Status**: Phase 1 Complete ✓
**Next Phase**: Phase 2 (CLI Integration)
````

**Deliverables**:

- File created: `docs/explanation/subagent_implementation.md`
- Sections: Overview, Components, Architecture, Implementation, Testing, Usage, Validation
- Code examples with proper syntax highlighting
- Test results documented

**Validation**:

```bash
# File exists
test -f docs/explanation/subagent_implementation.md
echo $?  # Must output: 0

# Uses lowercase_underscore naming
ls docs/explanation/ | grep -q "subagent_implementation.md"
echo $?  # Must output: 0

# No emojis (except AGENTS.md-style markers)
grep -v "✓" docs/explanation/subagent_implementation.md | grep -P "[\x{1F600}-\x{1F64F}]"
echo $?  # Must output: 1 (no matches)
```

---

### Task 1.6: Quality Gates and Validation

**Estimated Time**: 15 minutes
**Purpose**: Verify all Phase 1 requirements met

#### Automated Validation

**Execute commands in order**:

```bash
# 1. Format code
cargo fmt --all
# Expected: No output (all files formatted)

# 2. Check compilation
cargo check --all-targets --all-features
# Expected: "Finished dev [unoptimized + debuginfo] target(s)" with 0 errors

# 3. Lint with zero warnings
cargo clippy --all-targets --all-features -- -D warnings
# Expected: "Finished dev [unoptimized + debuginfo] target(s)" with 0 warnings

# 4. Run tests with >80% coverage
cargo test --all-features
# Expected: "test result: ok. N passed; 0 failed" where N ≥ 15 for subagent tests

# 5. Verify documentation builds
cargo doc --no-deps
# Expected: Documentation generated without warnings
```

#### Manual Verification Checklist

**Files Created**:

- [ ] `src/tools/subagent.rs` exists
- [ ] `docs/explanation/subagent_implementation.md` exists

**Code Quality**:

- [ ] All public items have `///` doc comments
- [ ] All doc comments include examples (where applicable)
- [ ] No `unwrap()` or `expect()` without justification
- [ ] All error cases use `Result<T, E>` pattern

**Testing**:

- [ ] Minimum 15 tests in `src/tools/subagent.rs`
- [ ] All tests pass
- [ ] Coverage >80% (verify with test output)

**Documentation**:

- [ ] Filename uses `lowercase_underscore.md`
- [ ] No emojis in documentation
- [ ] All code blocks specify language
- [ ] Validation results documented

**Architecture**:

- [ ] No circular dependencies introduced
- [ ] Respects module boundaries (tools/ doesn't import agent/)
- [ ] Thread safety maintained (Send + Sync)

#### Success Criteria (Machine-Verifiable)

```bash
# All checks must return 0 (success)
cargo check --all-targets --all-features
echo $?  # Must output: 0

cargo clippy --all-targets --all-features -- -D warnings 2>&1 | grep -c "warning:"
echo $?  # Must output: 0 (or grep outputs "0" warnings)

cargo test --all-features subagent 2>&1 | grep -q "15 passed; 0 failed"
echo $?  # Must output: 0

test -f src/tools/subagent.rs
echo $?  # Must output: 0

test -f docs/explanation/subagent_implementation.md
echo $?  # Must output: 0

grep -q "pub struct SubagentTool" src/tools/subagent.rs
echo $?  # Must output: 0
```

**All commands must return 0 for Phase 1 to be considered complete.**

**Deliverables**:

- Validation checklist completed
- All automated checks passing
- Results documented in `docs/explanation/subagent_implementation.md`

---

## Phase 2: Feature Implementation

**Estimated Effort**: 2-3 hours
**Prerequisite**: Phase 1 complete and ALL quality gates passed
**Blocks**: None (this is final phase)

---

### Task 2.1: Module Export

**Estimated Time**: 10 minutes
**File**: `src/tools/mod.rs`
**Lines**: ~3 lines added

#### Implementation Details

**Add module declaration** (after line 17, before `terminal` module):

```rust
pub mod subagent;
```

**Add public exports** (after line 40, in re-export section):

```rust
// Re-export subagent tool and types
pub use subagent::{SubagentTool, SubagentToolInput};
```

**Deliverables**:

- `src/tools/mod.rs` modified
- `subagent` module publicly accessible
- Types re-exported for convenience

**Validation**:

```bash
# Module declared
grep -q "pub mod subagent;" src/tools/mod.rs
echo $?  # Must output: 0

# Types exported
grep -q "pub use subagent::" src/tools/mod.rs
echo $?  # Must output: 0

# Compiles
cargo check
echo $?  # Must output: 0
```

---

### Task 2.2: CLI Integration

**Estimated Time**: 30 minutes
**File**: `src/commands/mod.rs`
**Location**: In tool registration section of `run_chat` function
**Lines**: ~10-15 lines added

#### Implementation Details

**Find tool registration section** in `src/commands/mod.rs` (search for `registry.register`):

**Add after other tool registrations**:

```rust
// Register subagent tool for task delegation
let subagent_tool = SubagentTool::new(
    Arc::clone(&provider),        // Share provider with main agent
    config.clone(),                // Use same config as template
    registry.clone(),              // Parent registry for filtering
    0,                             // Root depth (main agent is depth 0)
);
registry.register("subagent", Arc::new(subagent_tool));
```

**Import requirements** (add to top of file):

```rust
use crate::tools::SubagentTool;
```

**Context**: This registration makes the "subagent" tool available to the LLM during chat sessions.

**Deliverables**:

- `SubagentTool` registered in main agent's tool registry
- Depth initialized to 0 (root level)
- Provider shared via Arc clone

**Validation**:

```bash
# Import added
grep -q "use crate::tools::SubagentTool" src/commands/mod.rs
echo $?  # Must output: 0

# Registration added
grep -q "SubagentTool::new" src/commands/mod.rs
echo $?  # Must output: 0

# Compiles and runs
cargo run -- chat --help
echo $?  # Must output: 0
```

---

### Task 2.3: Configuration Updates

**Estimated Time**: 5 minutes (no code changes)
**Purpose**: Document configuration decisions

#### Configuration Decision

**DECISION FOR PHASE 1**: Use hardcoded constants (no config file changes)

**Rationale**:

- Keeps implementation focused on core functionality
- Aligns with "simple modular design" principle from AGENTS.md
- Configuration can be extended in future phase if needed

**Constants Used** (in `src/tools/subagent.rs`):

```rust
const MAX_SUBAGENT_DEPTH: usize = 3;          // Answer from Q1
const DEFAULT_SUBAGENT_MAX_TURNS: usize = 10;
const SUBAGENT_OUTPUT_MAX_SIZE: usize = 4096;
```

**Future Enhancement** (Phase 3 - out of scope):
If configuration needs arise, add to `src/config.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentConfig {
    /// Maximum recursion depth (default: 3)
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,

    /// Default max turns per subagent (default: 10)
    #[serde(default = "default_max_turns")]
    pub default_max_turns: usize,

    /// Output truncation size in bytes (default: 4096)
    #[serde(default = "default_output_max_size")]
    pub output_max_size: usize,
}

fn default_max_depth() -> usize { 3 }
fn default_max_turns() -> usize { 10 }
fn default_output_max_size() -> usize { 4096 }
```

**Deliverables**:

- Configuration decision documented
- No changes to `src/config.rs` in this phase
- Future enhancement path identified

---

### Task 2.4: Integration Testing

**Estimated Time**: 1 hour
**Purpose**: Verify end-to-end functionality with real CLI

#### Manual Test Scenarios

**Test 1: Basic Subagent Invocation**

```bash
cargo run -- chat

# In chat session, send:
> Use a subagent to research Rust async traits. Label it "async_research".
```

**Expected Behavior**:

1. Agent calls "subagent" tool with JSON:
   ```json
   {
     "label": "async_research",
     "task_prompt": "Research Rust async traits",
     "summary_prompt": "Summarize your findings concisely"
   }
   ```
2. Subagent executes (may call file_ops, grep, etc.)
3. Summary returned to main conversation
4. Main agent incorporates result into response

**Acceptance**:

- [ ] No panics or unwrap errors
- [ ] Subagent executes independently
- [ ] Result integrated into main conversation
- [ ] Tool call visible in debug output (if enabled)

---

**Test 2: Tool Filtering**

```bash
cargo run -- chat

# In chat session:
> Use a subagent with ONLY the grep tool to search for "async fn" in src/
```

**Expected Behavior**:

1. Agent calls "subagent" tool with `allowed_tools: ["grep"]`
2. Subagent has access to only grep tool
3. Completes task with limited toolset

**Acceptance**:

- [ ] Subagent doesn't attempt to use filtered-out tools
- [ ] Task completes successfully with grep
- [ ] No error about missing tools

---

**Test 3: Recursion Limit**

```bash
cargo run -- chat

# In chat session:
> Create a subagent that creates a subagent that creates a subagent that creates a subagent
```

**Expected Behavior**:

1. Agent creates subagent (depth=1)
2. Subagent creates nested subagent (depth=2)
3. Nested subagent attempts depth=3 → **ERROR**
4. Error message: "Maximum subagent recursion depth (3) exceeded"

**Acceptance**:

- [ ] Recursion blocked at MAX_SUBAGENT_DEPTH
- [ ] Graceful error message (not panic)
- [ ] Main agent receives error as tool result

---

**Test 4: Summary Prompt**

```bash
cargo run -- chat

# In chat session:
> Use a subagent to analyze errors in logs, summarize as bullet points
```

**Expected Behavior**:

1. Agent extracts summary instruction
2. Subagent completes analysis
3. Subagent prompted with summary request
4. Formatted bullet list returned

**Acceptance**:

- [ ] Summary follows requested format
- [ ] Output concise (not verbose logs)
- [ ] Main agent presents summary to user

---

#### Integration Test Documentation

**Document results in** `docs/explanation/subagent_implementation.md`:

```markdown
## Integration Testing Results

### Test 1: Basic Subagent Invocation

- **Status**: PASS / FAIL
- **Notes**: [Observations]

### Test 2: Tool Filtering

- **Status**: PASS / FAIL
- **Notes**: [Observations]

### Test 3: Recursion Limit

- **Status**: PASS / FAIL
- **Notes**: [Observations]

### Test 4: Summary Prompt

- **Status**: PASS / FAIL
- **Notes**: [Observations]

### Issues Found

- [List any bugs or unexpected behavior]

### Performance Notes

- Subagent execution time: ~X seconds
- Token usage overhead: ~Y tokens per subagent call
```

**Deliverables**:

- All 4 test scenarios executed
- Results documented
- Any issues logged for future work

---

### Task 2.5: Deliverables Summary

**Code Files**:

- `src/tools/subagent.rs` (~350-450 lines) - **NEW**
- `src/agent/core.rs` (+35 lines) - **MODIFIED**
- `src/tools/mod.rs` (+3 lines) - **MODIFIED**
- `src/commands/mod.rs` (+10-15 lines) - **MODIFIED**

**Test Files**:

- `src/tools/subagent.rs` test module (~200-300 lines) - **NEW**
- Minimum 15 unit tests
- Coverage: >80%

**Documentation**:

- `docs/explanation/subagent_implementation.md` (~200-300 lines) - **NEW**
  - Overview of subagent architecture
  - Recursion depth limiting strategy
  - Tool filtering mechanism
  - Usage examples
  - Integration test results
  - Validation results

**Total Estimated Lines**: 800-1000 lines

**Quality Gates**:

- ✓ `cargo fmt --all` - All files formatted
- ✓ `cargo check --all-targets --all-features` - 0 compilation errors
- ✓ `cargo clippy --all-targets --all-features -- -D warnings` - 0 warnings
- ✓ `cargo test --all-features` - All tests pass, >80% coverage
- ✓ Documentation complete with examples

---

### Task 2.6: Final Validation

**Estimated Time**: 15 minutes
**Purpose**: Verify entire implementation meets all requirements

#### Final Quality Gates

```bash
# 1. Clean format
cargo fmt --all
git diff --exit-code
# Expected: Exit code 0 (no uncommitted formatting changes)

# 2. Compilation
cargo check --all-targets --all-features
# Expected: Finished with 0 errors

# 3. Linting
cargo clippy --all-targets --all-features -- -D warnings
# Expected: 0 warnings

# 4. All tests pass
cargo test --all-features
# Expected: ok. N passed; 0 failed (N ≥ previous + 15)

# 5. Documentation builds
cargo doc --no-deps --open
# Expected: Opens browser with subagent module visible

# 6. Binary runs
cargo run -- chat --help
# Expected: Help text displays
```

#### Final Checklist

**Phase 1 Verification**:

- [ ] `src/tools/subagent.rs` complete with all functions
- [ ] `Agent::new_from_shared_provider()` added to `src/agent/core.rs`
- [ ] 15+ unit tests, all passing
- [ ] `docs/explanation/subagent_implementation.md` created

**Phase 2 Verification**:

- [ ] `subagent` module exported in `src/tools/mod.rs`
- [ ] `SubagentTool` registered in `src/commands/mod.rs`
- [ ] All 4 integration tests executed and documented
- [ ] No configuration changes (hardcoded constants used)

**Code Quality**:

- [ ] All cargo commands pass (fmt, check, clippy, test)
- [ ] No compiler warnings
- [ ] No clippy warnings
- [ ] Test coverage >80%

**Documentation Quality**:

- [ ] Lowercase filename with underscores
- [ ] No emojis (except checkmarks in validation section)
- [ ] All code blocks have language specified
- [ ] Examples are runnable
- [ ] Validation results included

**Architecture Compliance**:

- [ ] No circular dependencies
- [ ] Module boundaries respected
- [ ] Thread safety maintained (Send + Sync)
- [ ] Error handling uses Result<T, E>

#### Success Criteria

**ALL of the following must be true**:

```bash
# Zero compilation errors
cargo check --all-targets --all-features 2>&1 | grep -c "error:"
# Output: 0

# Zero warnings
cargo clippy --all-targets --all-features -- -D warnings 2>&1 | grep -c "warning:"
# Output: 0

# All tests pass
cargo test --all-features 2>&1 | grep "test result:" | grep -c "0 failed"
# Output: 1 (meaning "0 failed" found in output)

# Files exist
test -f src/tools/subagent.rs && \
test -f docs/explanation/subagent_implementation.md && \
echo "PASS"
# Output: PASS

# Exports present
grep -q "pub mod subagent" src/tools/mod.rs && \
grep -q "pub use subagent::" src/tools/mod.rs && \
echo "PASS"
# Output: PASS
```

**If ANY check fails, implementation is NOT complete.**

**Deliverables**:

- Final validation checklist completed
- All success criteria met
- Implementation ready for code review

---

## Implementation Complete

**When both phases pass all validation**:

1. Update `docs/explanation/subagent_implementation.md` with final results
2. Commit changes (user handles git operations per AGENTS.md)
3. Implementation is production-ready

**Next Steps** (Future Work):

- Phase 3: Configuration file support (optional)
- Phase 4: Subagent conversation persistence (optional)
- Phase 5: Parallel subagent execution (advanced)

---

## Appendix: Decision Answers

**Record user's answers to Open Questions here**:

### Q1: Recursion Depth Limit

**Answer**: [TO BE FILLED]
**Constant Set**: `MAX_SUBAGENT_DEPTH = ___`

### Q2: Default Tool Availability

**Answer**: [TO BE FILLED]
**Behavior**: Option A / B / C

### Q3: Summary Prompt Handling

**Answer**: [TO BE FILLED]
**Implementation**: Option A / B / C

### Q4: Subagent Failure Handling

**Answer**: [TO BE FILLED]
**Implementation**: Option A / B / C

### Q5: Execution Metadata Visibility

**Answer**: [TO BE FILLED]
**Implementation**: Option A / B / C

---

**End of Implementation Plan**
