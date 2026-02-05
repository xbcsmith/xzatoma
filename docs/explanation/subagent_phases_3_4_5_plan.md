# Subagent Support - Phases 3, 4, 5 Implementation Plan

**Status**: Ready for implementation (Phases 1-2 complete)
**Prerequisites**: Phase 1 and Phase 2 successfully completed and validated
**Document Version**: 1.0
**Last Updated**: 2024

---

## Overview

This document provides a comprehensive, phased approach for implementing advanced subagent features in XZatoma. Phases 1-2 delivered core subagent functionality with basic delegation and tool filtering. Phases 3-5 enhance the feature with production-ready configuration, conversation persistence for debugging, and advanced parallel execution patterns.

**Phase Summary**:

- **Phase 3**: Configuration and Observability - Move hardcoded constants to configuration files, add telemetry/logging, create end-to-end integration tests, and write user documentation
- **Phase 4**: Conversation Persistence and History - Enable subagent conversation storage, replay capabilities, and historical analysis for debugging and auditing
- **Phase 5**: Advanced Execution Patterns - Implement parallel subagent execution, performance profiling, resource management, and execution quotas

**Total Estimated Effort**: 8-12 days (3-4 days per phase)

---

## Current State Analysis

### Existing Infrastructure (Post Phase 2)

**Core Implementation** (`src/tools/subagent.rs`):
- `SubagentTool` with depth tracking and provider sharing
- Tool registry filtering (whitelist/blacklist support)
- Summary prompt handling with defaults
- Hardcoded constants: `MAX_SUBAGENT_DEPTH = 3`, `DEFAULT_SUBAGENT_MAX_TURNS = 10`, `SUBAGENT_OUTPUT_MAX_SIZE = 4096`
- 19 unit tests with >80% coverage
- Parent tool failure handling (ADR-006 compliant)

**Integration** (`src/commands/mod.rs`, `src/tools/mod.rs`):
- Module exported and types re-exported
- Registered in chat-mode tool registry
- Provider shared via `Arc<dyn Provider>`
- Agent constructor supports shared providers

**Configuration** (`src/config.rs`):
- `AgentConfig` with tools, terminal, conversation, and chat settings
- Environment variable and YAML file support
- Validation logic for config constraints
- No subagent-specific configuration yet (uses constants)

**Logging Infrastructure**:
- `tracing` crate with structured logging
- `tracing-subscriber` with env-filter, fmt, and json features
- Existing usage in `Conversation::prune_if_needed` and `Agent::execute`
- No subagent-specific telemetry yet

**Testing**:
- 19 unit tests in `src/tools/subagent.rs`
- 628 total project tests passing
- No end-to-end CLI integration tests for subagents
- No conversation persistence tests

### Identified Issues

**Configuration Inflexibility**:
- Subagent limits hardcoded, cannot be adjusted per environment
- Production vs development environments need different limits
- Users cannot tune performance vs resource constraints
- No runtime visibility into configured limits

**Observability Gaps**:
- No telemetry for subagent lifecycle events (spawn, complete, fail)
- Cannot track nested subagent execution depth in logs
- No metrics for subagent resource usage (turns, tokens)
- Difficult to debug complex delegation chains

**Testing Limitations**:
- Unit tests validate logic but not real CLI behavior
- No end-to-end tests with real providers and tools
- Cannot verify user experience with actual subagent invocations
- Integration scenarios not validated in CI/CD

**Conversation Ephemerality**:
- Subagent conversations discarded after completion
- Cannot replay/debug failed subagent executions
- No audit trail for compliance/security requirements
- Lost learning opportunities from subagent interactions

**Resource Management**:
- No limits on concurrent subagent executions
- Potential resource exhaustion with many parallel delegations
- No performance profiling data
- Sequential-only execution limits scalability

---

## Implementation Phases

---

## Phase 3: Configuration and Observability

**Estimated Effort**: 3-4 days
**Prerequisite**: Phase 2 complete and validated
**Blocks**: Phase 4, Phase 5

**Objective**: Make subagent feature production-ready with flexible configuration, comprehensive telemetry, end-to-end testing, and user documentation.

---

### Task 3.1: Configuration Schema and Migration

**Estimated Time**: 2-3 hours
**Files**: `src/config.rs`, `src/tools/subagent.rs`
**Lines**: ~150 lines added/modified

#### Implementation Details

**Step 1: Add SubagentConfig to configuration schema**

Add to `src/config.rs` after `ToolsConfig` (around line 343):

```rust
/// Subagent delegation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentConfig {
    /// Maximum recursion depth for nested subagents
    /// Root agent is depth 0, first subagent is depth 1
    #[serde(default = "default_subagent_max_depth")]
    pub max_depth: usize,

    /// Default maximum turns per subagent execution
    #[serde(default = "default_subagent_max_turns")]
    pub default_max_turns: usize,

    /// Maximum output size in bytes before truncation
    #[serde(default = "default_subagent_output_max_size")]
    pub output_max_size: usize,

    /// Enable subagent execution telemetry
    #[serde(default = "default_subagent_telemetry_enabled")]
    pub telemetry_enabled: bool,

    /// Enable conversation persistence for debugging
    #[serde(default = "default_subagent_persistence_enabled")]
    pub persistence_enabled: bool,
}

fn default_subagent_max_depth() -> usize { 3 }
fn default_subagent_max_turns() -> usize { 10 }
fn default_subagent_output_max_size() -> usize { 4096 }
fn default_subagent_telemetry_enabled() -> bool { true }
fn default_subagent_persistence_enabled() -> bool { false }

impl Default for SubagentConfig {
    fn default() -> Self {
        Self {
            max_depth: default_subagent_max_depth(),
            default_max_turns: default_subagent_max_turns(),
            output_max_size: default_subagent_output_max_size(),
            telemetry_enabled: default_subagent_telemetry_enabled(),
            persistence_enabled: default_subagent_persistence_enabled(),
        }
    }
}
```

**Step 2: Add subagent field to AgentConfig**

Modify `AgentConfig` struct (around line 126):

```rust
pub struct AgentConfig {
    // ... existing fields ...
    pub chat: ChatConfig,

    /// Subagent delegation settings
    #[serde(default)]
    pub subagent: SubagentConfig,
}
```

Update `AgentConfig::default()` implementation:

```rust
impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            // ... existing fields ...
            chat: ChatConfig::default(),
            subagent: SubagentConfig::default(),
        }
    }
}
```

**Step 3: Add configuration validation**

Add to `Config::validate()` method (around line 777):

```rust
// Validate subagent configuration
if self.agent.subagent.max_depth == 0 {
    errors.push("agent.subagent.max_depth must be greater than 0".to_string());
}
if self.agent.subagent.max_depth > 10 {
    errors.push("agent.subagent.max_depth cannot exceed 10 (stack overflow risk)".to_string());
}
if self.agent.subagent.default_max_turns == 0 {
    errors.push("agent.subagent.default_max_turns must be greater than 0".to_string());
}
if self.agent.subagent.output_max_size < 1024 {
    errors.push("agent.subagent.output_max_size must be at least 1024 bytes".to_string());
}
```

**Step 4: Update SubagentTool to use configuration**

Modify `src/tools/subagent.rs` (remove constants, update struct):

```rust
// Remove these constants:
// const MAX_SUBAGENT_DEPTH: usize = 3;
// const DEFAULT_SUBAGENT_MAX_TURNS: usize = 10;
// const SUBAGENT_OUTPUT_MAX_SIZE: usize = 4096;

pub struct SubagentTool {
    provider: Arc<dyn Provider>,
    config: AgentConfig,
    parent_registry: Arc<ToolRegistry>,
    current_depth: usize,
    subagent_config: SubagentConfig,  // Add this field
}

impl SubagentTool {
    pub fn new(
        provider: Arc<dyn Provider>,
        config: AgentConfig,
        parent_registry: Arc<ToolRegistry>,
        current_depth: usize,
    ) -> Self {
        let subagent_config = config.subagent.clone();
        Self {
            provider,
            config,
            parent_registry,
            current_depth,
            subagent_config,
        }
    }

    // Update execute method to use self.subagent_config.max_depth
    // instead of MAX_SUBAGENT_DEPTH
}
```

**Step 5: Update CLI registration**

Modify `src/commands/mod.rs` to pass full config (no changes needed, already passing `config.clone()`).

#### Deliverables

- `src/config.rs` modified (~100 lines added)
- `src/tools/subagent.rs` modified (~20 lines changed)
- Configuration validation logic added
- Backward compatibility maintained (defaults match hardcoded values)

#### Validation

```bash
# Configuration schema compiles
cargo check

# Configuration defaults work
cargo test config::tests::test_subagent_config_defaults

# YAML parsing works
echo "agent:
  subagent:
    max_depth: 5
    default_max_turns: 20" > test_config.yaml
cargo run -- chat --config test_config.yaml --help

# Validation catches errors
echo "agent:
  subagent:
    max_depth: 0" > invalid_config.yaml
cargo run -- chat --config invalid_config.yaml --help 2>&1 | grep "must be greater than 0"

# Cleanup
rm test_config.yaml invalid_config.yaml
```

#### Success Criteria

- [ ] `SubagentConfig` struct defined with all fields
- [ ] Configuration defaults match Phase 2 hardcoded values
- [ ] Validation prevents invalid configurations
- [ ] YAML configuration loads successfully
- [ ] SubagentTool uses configuration instead of constants
- [ ] All existing tests pass (no regressions)

---

### Task 3.2: Telemetry and Logging Implementation

**Estimated Time**: 3-4 hours
**Files**: `src/tools/subagent.rs`
**Lines**: ~80 lines added

#### Implementation Details

**Step 1: Add telemetry helper module**

Add to `src/tools/subagent.rs` at module level:

```rust
mod telemetry {
    use tracing::{debug, info, warn, error};

    pub fn log_subagent_spawn(label: &str, depth: usize, max_turns: usize, allowed_tools: &[String]) {
        info!(
            subagent.event = "spawn",
            subagent.label = label,
            subagent.depth = depth,
            subagent.max_turns = max_turns,
            subagent.allowed_tools = ?allowed_tools,
            "Spawning subagent"
        );
    }

    pub fn log_subagent_complete(label: &str, depth: usize, turns_used: usize, tokens_used: usize, status: &str) {
        info!(
            subagent.event = "complete",
            subagent.label = label,
            subagent.depth = depth,
            subagent.turns_used = turns_used,
            subagent.tokens_consumed = tokens_used,
            subagent.status = status,
            "Subagent completed"
        );
    }

    pub fn log_subagent_error(label: &str, depth: usize, error: &str) {
        error!(
            subagent.event = "error",
            subagent.label = label,
            subagent.depth = depth,
            subagent.error = error,
            "Subagent execution failed"
        );
    }

    pub fn log_output_truncation(label: &str, original_size: usize, truncated_size: usize) {
        warn!(
            subagent.event = "truncation",
            subagent.label = label,
            subagent.original_size = original_size,
            subagent.truncated_size = truncated_size,
            "Subagent output truncated"
        );
    }

    pub fn log_max_turns_exceeded(label: &str, depth: usize, max_turns: usize) {
        warn!(
            subagent.event = "max_turns_exceeded",
            subagent.label = label,
            subagent.depth = depth,
            subagent.max_turns = max_turns,
            "Subagent exceeded max turns"
        );
    }

    pub fn log_depth_limit_reached(label: &str, current_depth: usize, max_depth: usize) {
        debug!(
            subagent.event = "depth_limit",
            subagent.label = label,
            subagent.current_depth = current_depth,
            subagent.max_depth = max_depth,
            "Subagent recursion depth limit enforced"
        );
    }
}
```

**Step 2: Integrate telemetry into SubagentTool::execute**

Modify `impl ToolExecutor for SubagentTool` (in `execute` method):

```rust
async fn execute(&self, args: serde_json::Value) -> ToolResult {
    let input: SubagentToolInput = serde_json::from_value(args)
        .map_err(|e| ToolResult::error(&format!("Invalid input: {}", e)))?;

    // Check telemetry enabled
    let telemetry_enabled = self.subagent_config.telemetry_enabled;

    // Depth limit check
    if self.current_depth >= self.subagent_config.max_depth {
        if telemetry_enabled {
            telemetry::log_depth_limit_reached(
                &input.label,
                self.current_depth,
                self.subagent_config.max_depth
            );
        }
        return ToolResult::error(&format!(
            "Maximum subagent depth {} reached",
            self.subagent_config.max_depth
        ));
    }

    // Log spawn
    if telemetry_enabled {
        telemetry::log_subagent_spawn(
            &input.label,
            self.current_depth + 1,
            input.max_turns.unwrap_or(self.subagent_config.default_max_turns),
            &input.allowed_tools.clone().unwrap_or_default(),
        );
    }

    // ... existing execution logic ...

    // Before returning success
    let status = if turns_used >= max_turns {
        if telemetry_enabled {
            telemetry::log_max_turns_exceeded(&input.label, self.current_depth + 1, max_turns);
        }
        "incomplete"
    } else {
        "complete"
    };

    if telemetry_enabled {
        telemetry::log_subagent_complete(
            &input.label,
            self.current_depth + 1,
            turns_used,
            tokens_consumed,
            status,
        );
    }

    // Check truncation
    if final_output.len() > self.subagent_config.output_max_size {
        if telemetry_enabled {
            telemetry::log_output_truncation(
                &input.label,
                final_output.len(),
                self.subagent_config.output_max_size,
            );
        }
        final_output.truncate(self.subagent_config.output_max_size);
        final_output.push_str("\n[Output truncated]");
    }

    // ... return ToolResult ...
}
```

**Step 3: Add error logging**

Wrap error paths with telemetry:

```rust
.map_err(|e| {
    if telemetry_enabled {
        telemetry::log_subagent_error(&input.label, self.current_depth + 1, &e.to_string());
    }
    ToolResult::error(&format!("Subagent execution failed: {}", e))
})?;
```

#### Deliverables

- Telemetry helper module in `subagent.rs`
- Structured logging with `tracing` macros
- Six telemetry events: spawn, complete, error, truncation, max_turns_exceeded, depth_limit
- Configuration flag to enable/disable telemetry
- Consistent log field naming (`subagent.*`)

#### Validation

```bash
# Enable tracing
export RUST_LOG=info

# Run with telemetry
cargo run -- chat
# In chat, invoke subagent and observe logs

# Check log output contains structured fields
cargo run -- chat 2>&1 | grep "subagent.event=spawn"

# Disable telemetry via config
echo "agent:
  subagent:
    telemetry_enabled: false" > config.yaml
cargo run -- chat --config config.yaml
# Verify no subagent.* logs appear
rm config.yaml
```

#### Success Criteria

- [ ] All six telemetry events implemented
- [ ] Logs contain structured fields (label, depth, turns, tokens)
- [ ] Telemetry respects `telemetry_enabled` configuration flag
- [ ] Log levels appropriate (info for normal, warn for issues, error for failures)
- [ ] No performance degradation (async logging)
- [ ] Logs parseable as JSON when using json formatter

---

### Task 3.3: End-to-End Integration Tests

**Estimated Time**: 4-5 hours
**Files**: `tests/integration_subagent.rs` (new file)
**Lines**: ~400 lines

#### Implementation Details

**Step 1: Create integration test file**

Create `tests/integration_subagent.rs`:

```rust
//! End-to-end integration tests for subagent functionality
//!
//! These tests validate subagent behavior with real CLI invocation
//! and actual tool execution (not mocks).

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Test 1: Basic subagent invocation via chat mode
#[test]
fn test_basic_subagent_invocation() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    // Create minimal config
    fs::write(&config_path, "provider:\n  provider_type: copilot\n").unwrap();

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("chat")
        .arg("--config")
        .arg(config_path)
        .write_stdin("Use the subagent tool to list files in current directory\n/quit\n");

    // Should not panic or error
    cmd.assert().success();
}

/// Test 2: Subagent with tool filtering
#[test]
fn test_subagent_tool_filtering() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    fs::write(&config_path, "provider:\n  provider_type: copilot\n").unwrap();

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("chat")
        .arg("--config")
        .arg(config_path)
        .write_stdin(
            r#"{"label": "test", "task_prompt": "hello", "allowed_tools": ["terminal"]}"#
        );

    // Tool registry should only contain terminal
    cmd.assert().success();
}

/// Test 3: Recursion depth limit enforcement
#[test]
fn test_recursion_depth_limit() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    fs::write(
        &config_path,
        "provider:\n  provider_type: copilot\nagent:\n  subagent:\n    max_depth: 2\n"
    ).unwrap();

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("chat")
        .arg("--config")
        .arg(config_path);

    // Attempt to spawn nested subagents beyond limit
    // Should return error before executing
    cmd.assert().success(); // Chat mode itself succeeds
}

/// Test 4: Configuration override validation
#[test]
fn test_config_override() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    fs::write(
        &config_path,
        "agent:\n  subagent:\n    max_depth: 5\n    default_max_turns: 20\n"
    ).unwrap();

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("chat")
        .arg("--config")
        .arg(config_path)
        .arg("--help");

    cmd.assert().success();
}

/// Test 5: Invalid configuration detection
#[test]
fn test_invalid_config() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    fs::write(
        &config_path,
        "agent:\n  subagent:\n    max_depth: 0\n"
    ).unwrap();

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("chat")
        .arg("--config")
        .arg(config_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("must be greater than 0"));
}

/// Test 6: Telemetry output validation
#[test]
fn test_telemetry_output() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");

    fs::write(
        &config_path,
        "agent:\n  subagent:\n    telemetry_enabled: true\n"
    ).unwrap();

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("chat")
        .arg("--config")
        .arg(config_path)
        .env("RUST_LOG", "info");

    // Should contain telemetry logs
    // Note: Capturing stderr requires additional setup
}
```

**Step 2: Add Cargo.toml test dependencies**

Add to `[dev-dependencies]` in `Cargo.toml`:

```toml
assert_cmd = "2.0"
predicates = "3.0"
tempfile = "3.8"
```

**Step 3: Create integration test documentation**

Create `docs/explanation/subagent_integration_tests.md` (see Task 3.5 for content).

#### Deliverables

- `tests/integration_subagent.rs` with 6 integration tests
- Test dependencies added to `Cargo.toml`
- Tests validate: basic invocation, tool filtering, depth limits, config overrides, invalid config, telemetry
- Tests use `assert_cmd` for real CLI execution
- Tests use `tempfile` for isolated test environments

#### Validation

```bash
# Run integration tests
cargo test --test integration_subagent

# Verify all 6 tests pass
cargo test --test integration_subagent -- --nocapture

# Run in CI/CD pipeline
cargo test --all-features
```

#### Success Criteria

- [ ] 6 integration tests implemented and passing
- [ ] Tests run real CLI commands (not unit tests)
- [ ] Tests isolated with temporary directories
- [ ] Tests validate configuration, telemetry, and depth limits
- [ ] Tests executable in CI/CD pipeline
- [ ] Test execution time reasonable (<30 seconds total)

---

### Task 3.4: User Documentation

**Estimated Time**: 3-4 hours
**Files**: `docs/tutorials/subagent_usage.md`, `docs/reference/subagent_api.md`
**Lines**: ~600 lines total

#### Implementation Details

**Step 1: Create tutorial document**

Create `docs/tutorials/subagent_usage.md`:

```markdown
# Using Subagents for Task Delegation

## Overview

This tutorial teaches you how to use XZatoma's subagent feature to delegate
complex tasks to autonomous sub-agents with controlled tool access.

## Prerequisites

- XZatoma installed and configured
- Basic familiarity with chat mode
- Understanding of tool execution

## What You'll Learn

- How to invoke subagents from chat mode
- How to filter tools for security
- How to configure subagent limits
- How to debug subagent executions

## Step 1: Basic Subagent Invocation

Start a chat session:

```bash
xzatoma chat
```

Invoke a subagent with a simple task:

```
Please use the subagent tool with this input:
{
  "label": "file_analyzer",
  "task_prompt": "Analyze all .rs files in src/ and summarize their purpose"
}
```

The subagent will:
1. Receive the task prompt
2. Execute autonomously with all available tools
3. Summarize findings and return results

## Step 2: Tool Filtering for Security

Restrict subagent to specific tools:

```
{
  "label": "safe_reader",
  "task_prompt": "Read README.md and summarize",
  "allowed_tools": ["read_file"]
}
```

This prevents the subagent from:
- Executing terminal commands
- Writing files
- Fetching external URLs
- Spawning nested subagents

## Step 3: Controlling Execution Length

Set maximum turns:

```
{
  "label": "quick_scan",
  "task_prompt": "Find all TODO comments",
  "max_turns": 5
}
```

If the subagent doesn't complete in 5 turns:
- Returns partial results
- Includes metadata: `"completion_status": "incomplete"`
- Parent agent can retry with higher limit

## Step 4: Custom Summary Prompts

Guide how subagents summarize:

```
{
  "label": "detailed_analyzer",
  "task_prompt": "Analyze error handling patterns in src/",
  "summary_prompt": "Provide a detailed report with code examples and recommendations"
}
```

## Step 5: Configuration

Adjust limits in `config.yaml`:

```yaml
agent:
  subagent:
    max_depth: 5          # Allow deeper nesting
    default_max_turns: 20 # Longer execution
    output_max_size: 8192 # More output before truncation
    telemetry_enabled: true
```

## Step 6: Monitoring and Debugging

Enable telemetry logs:

```bash
RUST_LOG=info xzatoma chat
```

Look for structured logs:

```
INFO subagent.event=spawn subagent.label=analyzer subagent.depth=1
INFO subagent.event=complete subagent.turns_used=7 subagent.status=complete
```

## Common Patterns

### Pattern 1: Parallel Analysis

Spawn multiple subagents for independent tasks:

```
Task 1: Analyze Rust code (subagent A with allowed_tools: [grep, read_file])
Task 2: Check dependencies (subagent B with allowed_tools: [read_file, terminal])
```

### Pattern 2: Iterative Refinement

Use partial results to refine:

```
1. Spawn subagent with max_turns: 5
2. Review partial results
3. Spawn new subagent with refined task_prompt
```

### Pattern 3: Security Isolation

Delegate untrusted tasks with minimal tools:

```
{
  "label": "external_data",
  "task_prompt": "Process user-provided data",
  "allowed_tools": []  # No tools, only reasoning
}
```

## Troubleshooting

### "Maximum subagent depth reached"

Reduce nesting or increase `max_depth` in config.

### "Subagent output truncated"

Increase `output_max_size` or ask subagent for more concise summaries.

### Subagent not completing

Increase `max_turns` or simplify `task_prompt`.

## Next Steps

- Read API reference: `docs/reference/subagent_api.md`
- Explore advanced patterns: Conversation persistence (Phase 4)
- Learn parallel execution: Phase 5 features
```

**Step 2: Create API reference document**

Create `docs/reference/subagent_api.md`:

```markdown
# Subagent API Reference

## Tool Definition

**Tool Name**: `subagent`

**Purpose**: Delegate tasks to autonomous sub-agents with controlled tool access and execution limits.

## Input Schema

```json
{
  "label": "string (required)",
  "task_prompt": "string (required)",
  "summary_prompt": "string (optional)",
  "allowed_tools": ["string"] (optional),
  "max_turns": number (optional)
}
```

### Field Specifications

#### `label` (required)

- **Type**: String
- **Purpose**: Human-readable identifier for the subagent
- **Constraints**: Non-empty
- **Example**: `"file_analyzer"`, `"security_scanner"`

#### `task_prompt` (required)

- **Type**: String
- **Purpose**: The task to delegate to the subagent
- **Constraints**: Non-empty
- **Example**: `"Analyze all error handling in src/ directory"`

#### `summary_prompt` (optional)

- **Type**: String
- **Purpose**: Guide how the subagent summarizes its findings
- **Default**: `"Summarize your findings concisely"`
- **Example**: `"Provide detailed report with recommendations"`

#### `allowed_tools` (optional)

- **Type**: Array of strings
- **Purpose**: Whitelist of tools the subagent can use
- **Default**: All parent tools except "subagent"
- **Constraints**: Cannot include "subagent" (prevents recursion)
- **Example**: `["read_file", "grep", "terminal"]`

#### `max_turns` (optional)

- **Type**: Number (positive integer)
- **Purpose**: Maximum execution turns before stopping
- **Default**: Value from `agent.subagent.default_max_turns` config (10)
- **Example**: `15`

## Output Schema

```json
{
  "content": "string",
  "metadata": {
    "turns_used": number,
    "tokens_consumed": number,
    "recursion_depth": number,
    "completion_status": "complete" | "incomplete",
    "max_turns_reached": "true" | "false"
  }
}
```

### Output Fields

#### `content`

The subagent's final output (task results + summary).

#### `metadata.turns_used`

Number of conversation turns executed.

#### `metadata.tokens_consumed`

Total tokens used by the subagent.

#### `metadata.recursion_depth`

Nesting level (1 for first subagent, 2 for nested, etc.).

#### `metadata.completion_status`

- `"complete"`: Task finished within max_turns
- `"incomplete"`: Stopped due to max_turns limit

#### `metadata.max_turns_reached`

- `"true"`: Execution stopped by max_turns limit
- `"false"`: Completed before limit

## Configuration Reference

### `agent.subagent.max_depth`

- **Type**: Number
- **Default**: 3
- **Range**: 1-10
- **Purpose**: Maximum recursion depth for nested subagents
- **Example**: Root agent (0) → Subagent (1) → Nested (2) → Nested (3) → ERROR

### `agent.subagent.default_max_turns`

- **Type**: Number
- **Default**: 10
- **Range**: 1-1000
- **Purpose**: Default maximum turns when not specified in input

### `agent.subagent.output_max_size`

- **Type**: Number (bytes)
- **Default**: 4096
- **Range**: 1024+
- **Purpose**: Maximum output size before truncation

### `agent.subagent.telemetry_enabled`

- **Type**: Boolean
- **Default**: true
- **Purpose**: Enable structured logging for subagent lifecycle

### `agent.subagent.persistence_enabled`

- **Type**: Boolean
- **Default**: false
- **Purpose**: Enable conversation persistence (Phase 4 feature)

## Error Handling

### `ToolResult::error`

All operational failures return structured errors:

```json
{
  "error": "Error message describing failure"
}
```

### Common Errors

#### "Maximum subagent depth N reached"

- **Cause**: Attempted to spawn subagent beyond `max_depth`
- **Solution**: Reduce nesting or increase config value

#### "Invalid input: missing field `label`"

- **Cause**: Required field omitted
- **Solution**: Provide all required fields

#### "Tool 'subagent' cannot be used by subagents"

- **Cause**: `allowed_tools` includes "subagent"
- **Solution**: Remove "subagent" from whitelist

#### "Unknown tool: X"

- **Cause**: `allowed_tools` includes non-existent tool
- **Solution**: Use valid tool names from parent registry

## Telemetry Events

When `telemetry_enabled: true`, structured logs emitted:

### Event: spawn

```
INFO subagent.event=spawn subagent.label=X subagent.depth=N subagent.max_turns=M
```

### Event: complete

```
INFO subagent.event=complete subagent.label=X subagent.turns_used=N subagent.status=complete
```

### Event: error

```
ERROR subagent.event=error subagent.label=X subagent.error=MESSAGE
```

### Event: max_turns_exceeded

```
WARN subagent.event=max_turns_exceeded subagent.label=X subagent.max_turns=N
```

### Event: truncation

```
WARN subagent.event=truncation subagent.original_size=X subagent.truncated_size=Y
```

### Event: depth_limit

```
DEBUG subagent.event=depth_limit subagent.current_depth=X subagent.max_depth=Y
```

## Examples

See `docs/tutorials/subagent_usage.md` for practical examples.
```

#### Deliverables

- `docs/tutorials/subagent_usage.md` (~400 lines)
- `docs/reference/subagent_api.md` (~300 lines)
- Comprehensive coverage: tutorials for learning, reference for lookup
- Follows Diataxis framework (tutorials vs reference separation)

#### Validation

```bash
# Verify Markdown syntax
markdownlint docs/tutorials/subagent_usage.md
markdownlint docs/reference/subagent_api.md

# Check all code examples are valid JSON
grep -A 10 '```json' docs/reference/subagent_api.md

# Verify all referenced config keys exist
grep 'agent.subagent' docs/reference/subagent_api.md
```

#### Success Criteria

- [ ] Tutorial covers all basic and intermediate use cases
- [ ] API reference documents all fields, configs, errors, and events
- [ ] Code examples are complete and runnable
- [ ] No emojis anywhere in documentation (AGENTS.md Rule 3)
- [ ] Filenames use lowercase_with_underscores.md pattern
- [ ] Documents placed in correct Diataxis categories

---

### Task 3.5: Phase 3 Deliverables Summary

**Estimated Time**: 1 hour
**File**: `docs/explanation/subagent_phase3_implementation.md`

#### Implementation Details

Create comprehensive summary document following the pattern from Phase 1/2 documentation.

#### Deliverables

- Configuration migration complete (`SubagentConfig` in `config.rs`)
- Telemetry implementation with 6 structured events
- 6 end-to-end integration tests
- User documentation (tutorial + API reference)
- Phase 3 implementation summary document

#### Validation

Run complete Phase 3 validation:

```bash
# Format and check
cargo fmt --all
cargo check --all-targets --all-features

# Lint with zero warnings
cargo clippy --all-targets --all-features -- -D warnings

# All tests pass (unit + integration)
cargo test --all-features

# Integration tests specifically
cargo test --test integration_subagent

# Configuration validation
echo "agent:
  subagent:
    max_depth: 5" > test.yaml
cargo run -- chat --config test.yaml --help
rm test.yaml

# Documentation exists
ls docs/tutorials/subagent_usage.md
ls docs/reference/subagent_api.md
ls docs/explanation/subagent_phase3_implementation.md
```

#### Success Criteria

- [ ] All cargo quality gates pass (fmt, check, clippy, test)
- [ ] Integration tests pass (6/6)
- [ ] Configuration validation prevents invalid configs
- [ ] Telemetry respects enabled/disabled flag
- [ ] Documentation complete and follows naming conventions
- [ ] No regressions in Phase 1/2 functionality
- [ ] Test coverage remains >80%

---

## Phase 4: Conversation Persistence and History

**Estimated Effort**: 3-4 days
**Prerequisite**: Phase 3 complete
**Blocks**: None (Phase 5 independent)

**Objective**: Enable subagent conversation storage for debugging, auditing, and replay capabilities.

---

### Task 4.1: Persistence Schema and Storage

**Estimated Time**: 3-4 hours
**Files**: `src/agent/persistence.rs` (new), `Cargo.toml`
**Lines**: ~250 lines

#### Implementation Details

**Step 1: Add storage dependencies**

Add to `Cargo.toml`:

```toml
[dependencies]
# Existing dependencies...
sled = "0.34"           # Embedded key-value store
ulid = "1.1"            # Sortable unique IDs (preferred over UUID per PLAN.md)
chrono = "0.4"          # RFC-3339 timestamps
```

**Step 2: Create persistence module**

Create `src/agent/persistence.rs`:

```rust
//! Conversation persistence for debugging and auditing
//!
//! Stores subagent conversation history in embedded database
//! with support for replay and historical analysis.

use crate::error::{Result, XzatomaError};
use crate::providers::Message;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sled::Db;
use std::path::Path;
use ulid::Ulid;

/// Persisted conversation record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationRecord {
    /// Unique conversation identifier (ULID)
    pub id: String,

    /// Parent conversation ID (if subagent)
    pub parent_id: Option<String>,

    /// Subagent label (from input)
    pub label: String,

    /// Recursion depth (0=root, 1=first subagent, etc.)
    pub depth: usize,

    /// Conversation messages
    pub messages: Vec<Message>,

    /// Start timestamp (RFC-3339)
    pub started_at: String,

    /// End timestamp (RFC-3339)
    pub completed_at: Option<String>,

    /// Execution metadata
    pub metadata: ConversationMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMetadata {
    pub turns_used: usize,
    pub tokens_consumed: usize,
    pub completion_status: String,
    pub max_turns_reached: bool,
    pub task_prompt: String,
    pub summary_prompt: Option<String>,
    pub allowed_tools: Vec<String>,
}

/// Conversation persistence manager
pub struct ConversationStore {
    db: Db,
}

impl ConversationStore {
    /// Open or create conversation store
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let db = sled::open(path)
            .map_err(|e| XzatomaError::Storage(format!("Failed to open database: {}", e)))?;
        Ok(Self { db })
    }

    /// Save conversation record
    pub fn save(&self, record: &ConversationRecord) -> Result<()> {
        let key = record.id.as_bytes();
        let value = serde_json::to_vec(record)
            .map_err(|e| XzatomaError::Storage(format!("Serialization failed: {}", e)))?;

        self.db.insert(key, value)
            .map_err(|e| XzatomaError::Storage(format!("Insert failed: {}", e)))?;

        self.db.flush()
            .map_err(|e| XzatomaError::Storage(format!("Flush failed: {}", e)))?;

        Ok(())
    }

    /// Retrieve conversation by ID
    pub fn get(&self, id: &str) -> Result<Option<ConversationRecord>> {
        let key = id.as_bytes();
        match self.db.get(key)
            .map_err(|e| XzatomaError::Storage(format!("Get failed: {}", e)))?
        {
            Some(bytes) => {
                let record = serde_json::from_slice(&bytes)
                    .map_err(|e| XzatomaError::Storage(format!("Deserialization failed: {}", e)))?;
                Ok(Some(record))
            }
            None => Ok(None),
        }
    }

    /// List all conversations (paginated)
    pub fn list(&self, limit: usize, offset: usize) -> Result<Vec<ConversationRecord>> {
        let mut records = Vec::new();
        for (i, result) in self.db.iter().enumerate() {
            if i < offset {
                continue;
            }
            if records.len() >= limit {
                break;
            }

            let (_, value) = result
                .map_err(|e| XzatomaError::Storage(format!("Iteration failed: {}", e)))?;

            let record: ConversationRecord = serde_json::from_slice(&value)
                .map_err(|e| XzatomaError::Storage(format!("Deserialization failed: {}", e)))?;

            records.push(record);
        }

        Ok(records)
    }

    /// Find conversations by parent ID
    pub fn find_by_parent(&self, parent_id: &str) -> Result<Vec<ConversationRecord>> {
        let mut records = Vec::new();
        for result in self.db.iter() {
            let (_, value) = result
                .map_err(|e| XzatomaError::Storage(format!("Iteration failed: {}", e)))?;

            let record: ConversationRecord = serde_json::from_slice(&value)
                .map_err(|e| XzatomaError::Storage(format!("Deserialization failed: {}", e)))?;

            if record.parent_id.as_deref() == Some(parent_id) {
                records.push(record);
            }
        }

        Ok(records)
    }
}

/// Generate new ULID for conversation
pub fn new_conversation_id() -> String {
    Ulid::new().to_string()
}

/// Get current timestamp in RFC-3339 format
pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}
```

**Step 3: Add storage error variant**

Add to `src/error.rs`:

```rust
#[derive(Error, Debug)]
pub enum XzatomaError {
    // ... existing variants ...

    #[error("Storage error: {0}")]
    Storage(String),
}
```

**Step 4: Export persistence module**

Add to `src/agent/mod.rs`:

```rust
pub mod persistence;
pub use persistence::{ConversationRecord, ConversationStore, ConversationMetadata};
```

#### Deliverables

- `src/agent/persistence.rs` (~250 lines)
- Embedded database with `sled`
- ULID-based IDs (sortable, timestamp-embedded)
- RFC-3339 timestamps
- CRUD operations: save, get, list, find_by_parent
- Storage error handling

#### Validation

```bash
# Compiles
cargo check

# Storage tests
cargo test persistence::

# Database created on first use
cargo run -- chat
ls -la ~/.xzatoma/conversations.db
```

#### Success Criteria

- [ ] Persistence module compiles and tests pass
- [ ] Database operations (save, get, list) work correctly
- [ ] ULIDs generated correctly (sortable by time)
- [ ] Timestamps in RFC-3339 format (per PLAN.md)
- [ ] Parent-child conversation linking supported
- [ ] Error handling for all database operations

---

### Task 4.2: Subagent Persistence Integration

**Estimated Time**: 2-3 hours
**Files**: `src/tools/subagent.rs`, `src/config.rs`
**Lines**: ~100 lines modified

#### Implementation Details

**Step 1: Add persistence path to configuration**

Modify `SubagentConfig` in `src/config.rs`:

```rust
pub struct SubagentConfig {
    // ... existing fields ...

    /// Path to conversation database (when persistence enabled)
    #[serde(default = "default_persistence_path")]
    pub persistence_path: String,
}

fn default_persistence_path() -> String {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".xzatoma")
        .join("conversations.db")
        .to_string_lossy()
        .to_string()
}
```

**Step 2: Add ConversationStore to SubagentTool**

Modify `SubagentTool` struct:

```rust
pub struct SubagentTool {
    // ... existing fields ...
    conversation_store: Option<Arc<ConversationStore>>,
    parent_conversation_id: Option<String>,
}

impl SubagentTool {
    pub fn new(
        provider: Arc<dyn Provider>,
        config: AgentConfig,
        parent_registry: Arc<ToolRegistry>,
        current_depth: usize,
    ) -> Self {
        let subagent_config = config.subagent.clone();

        // Initialize store if persistence enabled
        let conversation_store = if subagent_config.persistence_enabled {
            match ConversationStore::new(&subagent_config.persistence_path) {
                Ok(store) => Some(Arc::new(store)),
                Err(e) => {
                    tracing::warn!("Failed to initialize conversation store: {}", e);
                    None
                }
            }
        } else {
            None
        };

        Self {
            provider,
            config,
            parent_registry,
            current_depth,
            subagent_config,
            conversation_store,
            parent_conversation_id: None,
        }
    }

    pub fn with_parent_conversation_id(mut self, id: String) -> Self {
        self.parent_conversation_id = Some(id);
        self
    }
}
```

**Step 3: Persist conversations in execute method**

Modify `SubagentTool::execute`:

```rust
async fn execute(&self, args: serde_json::Value) -> ToolResult {
    // ... existing input parsing and validation ...

    // Generate conversation ID
    let conversation_id = new_conversation_id();
    let started_at = now_rfc3339();

    // ... existing subagent execution ...

    // After execution completes
    if let Some(store) = &self.conversation_store {
        let record = ConversationRecord {
            id: conversation_id.clone(),
            parent_id: self.parent_conversation_id.clone(),
            label: input.label.clone(),
            depth: self.current_depth + 1,
            messages: subagent.conversation().messages().to_vec(),
            started_at,
            completed_at: Some(now_rfc3339()),
            metadata: ConversationMetadata {
                turns_used,
                tokens_consumed,
                completion_status: status.to_string(),
                max_turns_reached: turns_used >= max_turns,
                task_prompt: input.task_prompt.clone(),
                summary_prompt: input.summary_prompt.clone(),
                allowed_tools: input.allowed_tools.clone().unwrap_or_default(),
            },
        };

        if let Err(e) = store.save(&record) {
            tracing::warn!("Failed to persist conversation {}: {}", conversation_id, e);
        } else {
            tracing::info!(
                subagent.event = "persisted",
                subagent.conversation_id = conversation_id,
                "Conversation persisted"
            );
        }
    }

    // ... return ToolResult ...
}
```

#### Deliverables

- Persistence integrated into `SubagentTool`
- Conversations saved when `persistence_enabled: true`
- Parent-child conversation linking
- Graceful degradation if persistence fails
- Telemetry event for successful persistence

#### Validation

```bash
# Enable persistence
echo "agent:
  subagent:
    persistence_enabled: true" > config.yaml

# Run subagent
cargo run -- chat --config config.yaml
# Invoke subagent via chat

# Check database created
ls ~/.xzatoma/conversations.db

# Verify telemetry
cargo run -- chat --config config.yaml 2>&1 | grep "subagent.event=persisted"

rm config.yaml
```

#### Success Criteria

- [ ] Conversations persisted when enabled
- [ ] Database path configurable
- [ ] Parent-child relationships tracked
- [ ] Graceful error handling (no crashes if persistence fails)
- [ ] Telemetry logged for persisted conversations
- [ ] No performance degradation (async storage)

---

### Task 4.3: Conversation Replay CLI Command

**Estimated Time**: 3-4 hours
**Files**: `src/commands/replay.rs` (new), `src/commands/mod.rs`
**Lines**: ~200 lines

#### Implementation Details

**Step 1: Create replay command module**

Create `src/commands/replay.rs`:

```rust
//! Replay subagent conversations for debugging

use crate::agent::ConversationStore;
use crate::error::Result;
use clap::Args;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct ReplayArgs {
    /// Conversation ID to replay
    #[arg(long, short = 'i')]
    id: Option<String>,

    /// List all conversations
    #[arg(long, short = 'l')]
    list: bool,

    /// Path to conversation database
    #[arg(long, default_value = "~/.xzatoma/conversations.db")]
    db_path: PathBuf,

    /// Limit for list results
    #[arg(long, default_value = "10")]
    limit: usize,

    /// Offset for pagination
    #[arg(long, default_value = "0")]
    offset: usize,

    /// Show conversation tree (with nested subagents)
    #[arg(long, short = 't')]
    tree: bool,
}

pub async fn run_replay(args: ReplayArgs) -> Result<()> {
    let store = ConversationStore::new(&args.db_path)?;

    if args.list {
        list_conversations(&store, args.limit, args.offset)?;
    } else if let Some(id) = args.id {
        if args.tree {
            show_conversation_tree(&store, &id)?;
        } else {
            replay_conversation(&store, &id)?;
        }
    } else {
        eprintln!("Error: Must specify --list or --id");
        std::process::exit(1);
    }

    Ok(())
}

fn list_conversations(store: &ConversationStore, limit: usize, offset: usize) -> Result<()> {
    let records = store.list(limit, offset)?;

    println!("Conversations (showing {} starting at {}):", limit, offset);
    println!();

    for record in records {
        println!("ID:     {}", record.id);
        println!("Label:  {}", record.label);
        println!("Depth:  {}", record.depth);
        println!("Status: {}", record.metadata.completion_status);
        println!("Turns:  {}", record.metadata.turns_used);
        println!("Start:  {}", record.started_at);
        if let Some(parent_id) = &record.parent_id {
            println!("Parent: {}", parent_id);
        }
        println!();
    }

    Ok(())
}

fn replay_conversation(store: &ConversationStore, id: &str) -> Result<()> {
    match store.get(id)? {
        Some(record) => {
            println!("=== Conversation {} ===", record.id);
            println!("Label: {}", record.label);
            println!("Depth: {}", record.depth);
            println!("Started: {}", record.started_at);
            if let Some(completed_at) = &record.completed_at {
                println!("Completed: {}", completed_at);
            }
            println!();
            println!("Task: {}", record.metadata.task_prompt);
            println!();
            println!("=== Messages ===");
            println!();

            for (i, message) in record.messages.iter().enumerate() {
                println!("--- Message {} ({}) ---", i + 1, message.role);
                println!("{}", message.content);
                println!();
            }

            println!("=== Metadata ===");
            println!("Turns Used: {}", record.metadata.turns_used);
            println!("Tokens Consumed: {}", record.metadata.tokens_consumed);
            println!("Status: {}", record.metadata.completion_status);
            println!("Max Turns Reached: {}", record.metadata.max_turns_reached);
            println!("Allowed Tools: {:?}", record.metadata.allowed_tools);
        }
        None => {
            eprintln!("Error: Conversation {} not found", id);
            std::process::exit(1);
        }
    }

    Ok(())
}

fn show_conversation_tree(store: &ConversationStore, id: &str) -> Result<()> {
    fn print_tree(store: &ConversationStore, id: &str, indent: usize) -> Result<()> {
        let record = store.get(id)?
            .ok_or_else(|| crate::error::XzatomaError::Storage(format!("Conversation {} not found", id)))?;

        let prefix = "  ".repeat(indent);
        println!("{}├─ {} [{}] (depth={}, turns={})",
            prefix, record.id, record.label, record.depth, record.metadata.turns_used);

        let children = store.find_by_parent(id)?;
        for child in children {
            print_tree(store, &child.id, indent + 1)?;
        }

        Ok(())
    }

    println!("Conversation tree:");
    print_tree(store, id, 0)?;
    Ok(())
}
```

**Step 2: Register replay command**

Modify `src/commands/mod.rs`:

```rust
pub mod replay;

#[derive(Debug, Subcommand)]
pub enum Commands {
    // ... existing commands ...

    /// Replay subagent conversations
    Replay(replay::ReplayArgs),
}

// In run function:
match cli.command {
    // ... existing matches ...
    Commands::Replay(args) => replay::run_replay(args).await,
}
```

#### Deliverables

- `src/commands/replay.rs` (~200 lines)
- Subcommands: `replay --list`, `replay --id <ID>`, `replay --tree --id <ID>`
- Pagination support for list
- Conversation tree visualization (parent-child relationships)
- Message replay with formatting

#### Validation

```bash
# List conversations
cargo run -- replay --list

# Replay specific conversation
cargo run -- replay --id <ULID>

# Show conversation tree
cargo run -- replay --tree --id <ULID>

# Custom database path
cargo run -- replay --list --db-path ./test.db
```

#### Success Criteria

- [ ] Replay command compiles and runs
- [ ] List shows all conversations with metadata
- [ ] Replay displays full conversation history
- [ ] Tree visualization shows parent-child structure
- [ ] Pagination works correctly
- [ ] Custom database path supported

---

### Task 4.4: Phase 4 Testing and Documentation

**Estimated Time**: 2-3 hours
**Files**: `tests/integration_persistence.rs`, `docs/how-to/debug_subagents.md`

#### Implementation Details

**Step 1: Create integration tests for persistence**

Create `tests/integration_persistence.rs`:

```rust
//! Integration tests for conversation persistence

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;
use std::fs;

#[test]
fn test_persistence_enabled() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.yaml");
    let db_path = temp_dir.path().join("conversations.db");

    fs::write(
        &config_path,
        format!(
            "agent:\n  subagent:\n    persistence_enabled: true\n    persistence_path: {}\n",
            db_path.display()
        )
    ).unwrap();

    // Run chat with persistence
    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("chat")
        .arg("--config")
        .arg(config_path);

    // Database should be created
    // (Actual test would invoke subagent and verify)
}

#[test]
fn test_replay_list() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("conversations.db");

    // Create mock database with test data
    // ... (implementation would populate test data)

    let mut cmd = Command::cargo_bin("xzatoma").unwrap();
    cmd.arg("replay")
        .arg("--list")
        .arg("--db-path")
        .arg(db_path);

    cmd.assert().success();
}

// Additional tests for:
// - Replay by ID
// - Tree visualization
// - Pagination
// - Error handling
```

**Step 2: Create debugging how-to guide**

Create `docs/how-to/debug_subagents.md`:

```markdown
# How to Debug Subagent Executions

## Enable Conversation Persistence

Edit `config.yaml`:

```yaml
agent:
  subagent:
    persistence_enabled: true
    persistence_path: ~/.xzatoma/conversations.db
```

## Run Your Workflow

Execute tasks that use subagents:

```bash
xzatoma chat
# Invoke subagents...
```

## List All Conversations

```bash
xzatoma replay --list
```

Output:

```
ID:     01HQXXX...
Label:  file_analyzer
Depth:  1
Status: complete
Turns:  7
Start:  2025-11-07T18:12:07.982682Z
```

## Replay Specific Conversation

```bash
xzatoma replay --id 01HQXXX...
```

Shows full message history and metadata.

## View Conversation Tree

For nested subagents:

```bash
xzatoma replay --tree --id 01HQXXX...
```

Output:

```
├─ 01HQXXX... [root_task] (depth=1, turns=7)
  ├─ 01HQYYY... [subtask_1] (depth=2, turns=3)
  ├─ 01HQZZZ... [subtask_2] (depth=2, turns=5)
```

## Common Debugging Scenarios

### Why did subagent fail?

1. Replay conversation
2. Look for error messages in final messages
3. Check metadata for `max_turns_reached`

### Why was output truncated?

Check `output_max_size` in config and increase if needed.

### How many tokens did subagent consume?

Look at `metadata.tokens_consumed` in replay output.
```

#### Deliverables

- Integration tests for persistence and replay
- How-to guide for debugging
- Test coverage for persistence features

#### Validation

```bash
cargo test --test integration_persistence
markdownlint docs/how-to/debug_subagents.md
```

#### Success Criteria

- [ ] Integration tests pass
- [ ] How-to guide complete with examples
- [ ] Documentation follows Diataxis framework (how-to category)
- [ ] All persistence features covered in tests

---

### Task 4.5: Phase 4 Deliverables Summary

**Estimated Time**: 1 hour
**File**: `docs/explanation/subagent_phase4_implementation.md`

#### Validation

```bash
# All quality gates
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features

# Persistence-specific tests
cargo test persistence::
cargo test --test integration_persistence

# Replay command works
cargo run -- replay --list
```

#### Success Criteria

- [ ] All cargo quality gates pass
- [ ] Persistence module complete with CRUD operations
- [ ] Replay command functional (list, replay, tree)
- [ ] Configuration controls persistence behavior
- [ ] Integration tests pass
- [ ] Documentation complete (how-to guide)
- [ ] No regressions in existing functionality

---

## Phase 5: Advanced Execution Patterns

**Estimated Effort**: 4-5 days
**Prerequisite**: Phase 3 complete (Phase 4 optional but recommended)
**Blocks**: None (final phase)

**Objective**: Implement parallel subagent execution, performance profiling, resource management, and execution quotas for production-scale deployments.

---

### Task 5.1: Parallel Execution Infrastructure

**Estimated Time**: 4-5 hours
**Files**: `src/tools/parallel_subagent.rs` (new), `Cargo.toml`
**Lines**: ~300 lines

#### Implementation Details

**Step 1: Add concurrency dependencies**

Add to `Cargo.toml`:

```toml
[dependencies]
# ... existing ...
tokio = { version = "1.35", features = ["full", "sync"] }
futures = "0.3"
```

**Step 2: Create parallel execution tool**

Create `src/tools/parallel_subagent.rs`:

```rust
//! Parallel subagent execution for independent tasks

use crate::agent::Agent;
use crate::config::AgentConfig;
use crate::error::Result;
use crate::providers::Provider;
use crate::tools::{ToolExecutor, ToolRegistry, ToolResult};
use async_trait::async_trait;
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

#[derive(Debug, Deserialize)]
pub struct ParallelSubagentInput {
    /// Tasks to execute in parallel
    pub tasks: Vec<ParallelTask>,

    /// Maximum concurrent executions (default: 5)
    pub max_concurrent: Option<usize>,

    /// Fail fast on first error (default: false)
    pub fail_fast: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ParallelTask {
    pub label: String,
    pub task_prompt: String,
    pub summary_prompt: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub max_turns: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct ParallelSubagentOutput {
    pub results: Vec<TaskResult>,
    pub total_duration_ms: u64,
    pub successful: usize,
    pub failed: usize,
}

#[derive(Debug, Serialize)]
pub struct TaskResult {
    pub label: String,
    pub success: bool,
    pub output: String,
    pub duration_ms: u64,
    pub error: Option<String>,
}

pub struct ParallelSubagentTool {
    provider: Arc<dyn Provider>,
    config: AgentConfig,
    parent_registry: Arc<ToolRegistry>,
    current_depth: usize,
}

impl ParallelSubagentTool {
    pub fn new(
        provider: Arc<dyn Provider>,
        config: AgentConfig,
        parent_registry: Arc<ToolRegistry>,
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

#[async_trait]
impl ToolExecutor for ParallelSubagentTool {
    fn tool_definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "parallel_subagent",
                "description": "Execute multiple independent tasks in parallel using subagents",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "tasks": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "label": { "type": "string" },
                                    "task_prompt": { "type": "string" },
                                    "summary_prompt": { "type": "string" },
                                    "allowed_tools": { "type": "array", "items": { "type": "string" } },
                                    "max_turns": { "type": "number" }
                                },
                                "required": ["label", "task_prompt"]
                            }
                        },
                        "max_concurrent": { "type": "number" },
                        "fail_fast": { "type": "boolean" }
                    },
                    "required": ["tasks"]
                }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> ToolResult {
        let input: ParallelSubagentInput = match serde_json::from_value(args) {
            Ok(input) => input,
            Err(e) => return ToolResult::error(&format!("Invalid input: {}", e)),
        };

        if input.tasks.is_empty() {
            return ToolResult::error("At least one task required");
        }

        let max_concurrent = input.max_concurrent.unwrap_or(5);
        let fail_fast = input.fail_fast.unwrap_or(false);

        info!(
            parallel_subagent.event = "start",
            parallel_subagent.task_count = input.tasks.len(),
            parallel_subagent.max_concurrent = max_concurrent,
            "Starting parallel subagent execution"
        );

        let start = std::time::Instant::now();
        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent));

        let tasks: Vec<_> = input.tasks.into_iter().map(|task| {
            let provider = Arc::clone(&self.provider);
            let config = self.config.clone();
            let parent_registry = Arc::clone(&self.parent_registry);
            let current_depth = self.current_depth;
            let sem = Arc::clone(&semaphore);

            tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                execute_task(task, provider, config, parent_registry, current_depth).await
            })
        }).collect();

        let mut results = Vec::new();
        let mut successful = 0;
        let mut failed = 0;

        for task_result in join_all(tasks).await {
            match task_result {
                Ok(result) => {
                    if result.success {
                        successful += 1;
                    } else {
                        failed += 1;
                        if fail_fast {
                            warn!("Parallel execution failed fast on error");
                            break;
                        }
                    }
                    results.push(result);
                }
                Err(e) => {
                    warn!("Task panicked: {}", e);
                    failed += 1;
                }
            }
        }

        let total_duration_ms = start.elapsed().as_millis() as u64;

        info!(
            parallel_subagent.event = "complete",
            parallel_subagent.successful = successful,
            parallel_subagent.failed = failed,
            parallel_subagent.duration_ms = total_duration_ms,
            "Parallel execution complete"
        );

        let output = ParallelSubagentOutput {
            results,
            total_duration_ms,
            successful,
            failed,
        };

        ToolResult::success(&serde_json::to_string(&output).unwrap())
    }
}

async fn execute_task(
    task: ParallelTask,
    provider: Arc<dyn Provider>,
    config: AgentConfig,
    parent_registry: Arc<ToolRegistry>,
    current_depth: usize,
) -> TaskResult {
    let start = std::time::Instant::now();

    // Create filtered registry
    let registry = if let Some(allowed_tools) = &task.allowed_tools {
        create_filtered_registry(&parent_registry, allowed_tools)
    } else {
        Arc::new(parent_registry.clone_without("subagent"))
    };

    // Create subagent
    let mut agent = Agent::new_from_shared_provider(
        Arc::clone(&provider),
        config.clone(),
        Arc::new(registry),
    );

    // Execute task
    let result = match agent.execute(&task.task_prompt).await {
        Ok(output) => {
            // Get summary if requested
            let final_output = if let Some(summary_prompt) = task.summary_prompt {
                agent.execute(&summary_prompt).await.unwrap_or(output)
            } else {
                output
            };

            TaskResult {
                label: task.label,
                success: true,
                output: final_output,
                duration_ms: start.elapsed().as_millis() as u64,
                error: None,
            }
        }
        Err(e) => TaskResult {
            label: task.label,
            success: false,
            output: String::new(),
            duration_ms: start.elapsed().as_millis() as u64,
            error: Some(e.to_string()),
        },
    };

    result
}

fn create_filtered_registry(parent: &ToolRegistry, allowed: &[String]) -> ToolRegistry {
    // Implementation same as SubagentTool::create_filtered_registry
    // (Reuse or refactor into shared utility)
    parent.clone_with_filter(allowed)
}
```

**Step 3: Register parallel tool**

Add to `src/tools/mod.rs`:

```rust
pub mod parallel_subagent;
pub use parallel_subagent::ParallelSubagentTool;
```

Add to `src/commands/mod.rs`:

```rust
let parallel_tool = ParallelSubagentTool::new(
    Arc::clone(&provider),
    config.clone(),
    registry.clone(),
    0,
);
registry.register("parallel_subagent", Arc::new(parallel_tool));
```

#### Deliverables

- `src/tools/parallel_subagent.rs` (~300 lines)
- Parallel execution with configurable concurrency limit
- Semaphore-based resource control
- Fail-fast option
- Individual task results with timing
- Aggregate statistics (successful, failed, total duration)

#### Validation

```bash
# Compiles
cargo check

# Unit tests
cargo test parallel_subagent::

# Manual test
cargo run -- chat
# Invoke: parallel_subagent with tasks array
```

#### Success Criteria

- [ ] Parallel tool compiles and registers
- [ ] Tasks execute concurrently (not sequentially)
- [ ] Concurrency limit enforced (semaphore)
- [ ] Fail-fast works when enabled
- [ ] Individual task timing tracked
- [ ] No resource leaks or deadlocks

---

### Task 5.2: Resource Management and Quotas

**Estimated Time**: 3-4 hours
**Files**: `src/agent/quota.rs` (new), `src/config.rs`
**Lines**: ~200 lines

#### Implementation Details

**Step 1: Add quota configuration**

Add to `SubagentConfig` in `src/config.rs`:

```rust
pub struct SubagentConfig {
    // ... existing fields ...

    /// Maximum total subagent executions per session
    #[serde(default = "default_max_executions")]
    pub max_executions: Option<usize>,

    /// Maximum total tokens consumable by subagents
    #[serde(default = "default_max_total_tokens")]
    pub max_total_tokens: Option<usize>,

    /// Maximum wall-clock time for all subagents (seconds)
    #[serde(default = "default_max_total_time")]
    pub max_total_time: Option<u64>,
}

fn default_max_executions() -> Option<usize> { None }
fn default_max_total_tokens() -> Option<usize> { None }
fn default_max_total_time() -> Option<u64> { None }
```

**Step 2: Create quota tracking module**

Create `src/agent/quota.rs`:

```rust
//! Resource quota tracking and enforcement

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use crate::error::{Result, XzatomaError};

#[derive(Debug, Clone)]
pub struct QuotaLimits {
    pub max_executions: Option<usize>,
    pub max_total_tokens: Option<usize>,
    pub max_total_time: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct QuotaUsage {
    pub executions: usize,
    pub total_tokens: usize,
    pub start_time: Instant,
}

pub struct QuotaTracker {
    limits: QuotaLimits,
    usage: Arc<Mutex<QuotaUsage>>,
}

impl QuotaTracker {
    pub fn new(limits: QuotaLimits) -> Self {
        Self {
            limits,
            usage: Arc::new(Mutex::new(QuotaUsage {
                executions: 0,
                total_tokens: 0,
                start_time: Instant::now(),
            })),
        }
    }

    pub fn check_and_reserve(&self) -> Result<()> {
        let usage = self.usage.lock().unwrap();

        // Check execution limit
        if let Some(max) = self.limits.max_executions {
            if usage.executions >= max {
                return Err(XzatomaError::QuotaExceeded(format!(
                    "Execution limit reached: {}/{}",
                    usage.executions, max
                )).into());
            }
        }

        // Check time limit
        if let Some(max_time) = self.limits.max_total_time {
            if usage.start_time.elapsed() >= max_time {
                return Err(XzatomaError::QuotaExceeded(format!(
                    "Time limit exceeded: {:?}",
                    usage.start_time.elapsed()
                )).into());
            }
        }

        Ok(())
    }

    pub fn record_execution(&self, tokens: usize) -> Result<()> {
        let mut usage = self.usage.lock().unwrap();
        usage.executions += 1;
        usage.total_tokens += tokens;

        // Check token limit
        if let Some(max) = self.limits.max_total_tokens {
            if usage.total_tokens > max {
                return Err(XzatomaError::QuotaExceeded(format!(
                    "Token limit exceeded: {}/{}",
                    usage.total_tokens, max
                )).into());
            }
        }

        Ok(())
    }

    pub fn get_usage(&self) -> QuotaUsage {
        self.usage.lock().unwrap().clone()
    }
}
```

**Step 3: Add quota error variant**

Add to `src/error.rs`:

```rust
#[error("Resource quota exceeded: {0}")]
QuotaExceeded(String),
```

**Step 4: Integrate quota tracking in SubagentTool**

Modify `SubagentTool` to accept and use `QuotaTracker`.

#### Deliverables

- `src/agent/quota.rs` (~200 lines)
- Quota limits: executions, tokens, wall-clock time
- Thread-safe tracking with `Arc<Mutex<>>`
- Pre-execution quota checks
- Post-execution usage recording
- Quota exceeded error handling

#### Validation

```bash
# Configuration with quotas
echo "agent:
  subagent:
    max_executions: 5
    max_total_tokens: 10000" > config.yaml

# Test quota enforcement
cargo run -- chat --config config.yaml
# Spawn 6 subagents, 6th should fail

rm config.yaml
```

#### Success Criteria

- [ ] Quota tracking thread-safe
- [ ] Execution limit enforced
- [ ] Token limit enforced
- [ ] Time limit enforced
- [ ] Clear error messages when quota exceeded
- [ ] Configuration controls quota behavior

---

### Task 5.3: Performance Profiling and Metrics

**Estimated Time**: 3-4 hours
**Files**: `src/agent/metrics.rs` (new)
**Lines**: ~250 lines

#### Implementation Details

**Step 1: Add metrics dependencies**

Add to `Cargo.toml`:

```toml
[dependencies]
# ... existing ...
metrics = "0.21"
metrics-exporter-prometheus = { version = "0.13", optional = true }

[features]
prometheus = ["metrics-exporter-prometheus"]
```

**Step 2: Create metrics module**

Create `src/agent/metrics.rs`:

```rust
//! Performance metrics for subagent execution

use metrics::{counter, histogram, gauge};
use std::time::Instant;

pub struct SubagentMetrics {
    label: String,
    depth: usize,
    start: Instant,
}

impl SubagentMetrics {
    pub fn new(label: String, depth: usize) -> Self {
        counter!("subagent_executions_total", "depth" => depth.to_string()).increment(1);
        gauge!("subagent_active_count", "depth" => depth.to_string()).increment(1.0);

        Self {
            label,
            depth,
            start: Instant::now(),
        }
    }

    pub fn record_completion(&self, turns: usize, tokens: usize, status: &str) {
        let duration = self.start.elapsed();

        histogram!(
            "subagent_duration_seconds",
            "depth" => self.depth.to_string(),
            "status" => status.to_string()
        ).record(duration.as_secs_f64());

        histogram!(
            "subagent_turns_used",
            "depth" => self.depth.to_string()
        ).record(turns as f64);

        histogram!(
            "subagent_tokens_consumed",
            "depth" => self.depth.to_string()
        ).record(tokens as f64);

        counter!(
            "subagent_completions_total",
            "depth" => self.depth.to_string(),
            "status" => status.to_string()
        ).increment(1);

        gauge!("subagent_active_count", "depth" => self.depth.to_string()).decrement(1.0);
    }

    pub fn record_error(&self, error: &str) {
        counter!(
            "subagent_errors_total",
            "depth" => self.depth.to_string(),
            "error_type" => error.to_string()
        ).increment(1);

        gauge!("subagent_active_count", "depth" => self.depth.to_string()).decrement(1.0);
    }
}

impl Drop for SubagentMetrics {
    fn drop(&mut self) {
        // Ensure active count decremented even on panic
        gauge!("subagent_active_count", "depth" => self.depth.to_string()).decrement(1.0);
    }
}

pub fn init_metrics_exporter() {
    #[cfg(feature = "prometheus")]
    {
        use metrics_exporter_prometheus::PrometheusBuilder;
        PrometheusBuilder::new()
            .install()
            .expect("Failed to install Prometheus exporter");
    }
}
```

**Step 3: Integrate metrics in SubagentTool**

Modify `SubagentTool::execute`:

```rust
async fn execute(&self, args: serde_json::Value) -> ToolResult {
    // ... input parsing ...

    let metrics = SubagentMetrics::new(input.label.clone(), self.current_depth + 1);

    // ... execution ...

    metrics.record_completion(turns_used, tokens_consumed, status);

    // On error path:
    // metrics.record_error("depth_limit");
}
```

#### Deliverables

- `src/agent/metrics.rs` (~250 lines)
- Metrics: executions, duration, turns, tokens, errors, active count
- Prometheus exporter (optional feature)
- Metrics recorded per-depth for analysis
- Automatic cleanup on drop (panic-safe)

#### Validation

```bash
# With Prometheus feature
cargo build --features prometheus

# Metrics recorded
cargo run -- chat
# Check metrics output (if exporter enabled)
```

#### Success Criteria

- [ ] Metrics module compiles
- [ ] All key metrics tracked (duration, turns, tokens, errors)
- [ ] Metrics labeled by depth for analysis
- [ ] Prometheus export optional (feature flag)
- [ ] No performance overhead when metrics disabled
- [ ] Panic-safe (Drop trait ensures cleanup)

---

### Task 5.4: Phase 5 Testing and Documentation

**Estimated Time**: 3-4 hours
**Files**: `tests/integration_parallel.rs`, `docs/explanation/subagent_performance.md`

#### Implementation Details

**Step 1: Create parallel execution tests**

Create `tests/integration_parallel.rs`:

```rust
//! Integration tests for parallel subagent execution

use assert_cmd::Command;

#[test]
fn test_parallel_execution_basic() {
    // Test basic parallel execution with 3 tasks
}

#[test]
fn test_parallel_concurrency_limit() {
    // Verify max_concurrent enforced
}

#[test]
fn test_parallel_fail_fast() {
    // Verify fail_fast stops on first error
}

#[test]
fn test_quota_enforcement() {
    // Verify execution/token/time limits
}

#[test]
fn test_metrics_recording() {
    // Verify metrics recorded correctly
}
```

**Step 2: Create performance explanation document**

Create `docs/explanation/subagent_performance.md`:

```markdown
# Subagent Performance and Scalability

## Parallel Execution

XZatoma supports parallel subagent execution for independent tasks...

## Resource Management

Quotas prevent resource exhaustion:
- Execution limits
- Token limits
- Time limits

## Performance Metrics

Available metrics:
- `subagent_executions_total`
- `subagent_duration_seconds`
- `subagent_turns_used`
- `subagent_tokens_consumed`
- `subagent_active_count`
- `subagent_errors_total`

## Tuning Guidelines

### Concurrency

Default: 5 concurrent executions
Increase for I/O-bound tasks
Decrease for memory-constrained environments

### Quotas

Set based on:
- Provider rate limits
- Cost constraints
- Execution time budgets

## Benchmarks

(Include benchmark results)
```

#### Deliverables

- Integration tests for parallel execution and quotas
- Performance explanation document
- Tuning guidelines

#### Validation

```bash
cargo test --test integration_parallel
markdownlint docs/explanation/subagent_performance.md
```

#### Success Criteria

- [ ] All integration tests pass
- [ ] Performance documentation complete
- [ ] Tuning guidelines provided
- [ ] Benchmarks included (if available)

---

### Task 5.5: Phase 5 Deliverables Summary

**Estimated Time**: 1 hour
**File**: `docs/explanation/subagent_phase5_implementation.md`

#### Validation

```bash
# All quality gates
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features

# Parallel-specific tests
cargo test parallel_subagent::
cargo test quota::
cargo test metrics::
cargo test --test integration_parallel
```

#### Success Criteria

- [ ] All cargo quality gates pass
- [ ] Parallel execution tool functional
- [ ] Quotas enforced correctly
- [ ] Metrics recorded (with optional Prometheus export)
- [ ] Integration tests pass
- [ ] Performance documentation complete
- [ ] No regressions in existing functionality

---

## Final Validation (All Phases Complete)

### Comprehensive Quality Gates

```bash
# Code quality
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings

# All tests (unit + integration)
cargo test --all-features

# Specific test suites
cargo test subagent::                     # Phase 1-2 unit tests
cargo test --test integration_subagent    # Phase 3
cargo test --test integration_persistence # Phase 4
cargo test --test integration_parallel    # Phase 5

# Documentation validation
find docs/ -name "*.md" -exec markdownlint {} \;

# Configuration validation
cargo run -- chat --help
cargo run -- replay --help
```

### Feature Checklist

**Phase 3: Configuration and Observability**
- [ ] Configuration schema (SubagentConfig)
- [ ] Telemetry logging (6 events)
- [ ] Integration tests (6 tests)
- [ ] User documentation (tutorial + API reference)

**Phase 4: Conversation Persistence**
- [ ] Persistence module (sled database)
- [ ] Replay CLI command (list, replay, tree)
- [ ] Integration with SubagentTool
- [ ] Debugging how-to guide

**Phase 5: Advanced Execution**
- [ ] Parallel execution tool
- [ ] Resource quotas (executions, tokens, time)
- [ ] Performance metrics (Prometheus optional)
- [ ] Performance documentation

### Success Criteria

**Code Quality**:
- All cargo commands pass (fmt, check, clippy, test)
- Test coverage >80%
- Zero clippy warnings
- Zero compiler warnings

**Functionality**:
- All features work as documented
- Configuration controls all configurable behaviors
- Telemetry provides actionable insights
- Persistence enables debugging workflows
- Parallel execution improves performance
- Quotas prevent resource exhaustion

**Documentation**:
- All features documented
- Tutorials for learning
- References for lookup
- How-to guides for tasks
- Explanations for concepts
- Follows Diataxis framework

**Testing**:
- Unit tests for all modules
- Integration tests for all features
- End-to-end CLI tests
- Performance benchmarks

---

## Appendix: Architecture Decisions (Phases 3-5)

### Phase 3 Decisions

**D3.1: Configuration Strategy**
- Use YAML with serde defaults
- Environment variables override config file
- Validation in Config::validate()
- Backward compatible defaults

**D3.2: Telemetry Events**
- Six events: spawn, complete, error, truncation, max_turns_exceeded, depth_limit
- Structured logging with tracing crate
- Consistent field naming (subagent.*)
- Configuration flag to enable/disable

**D3.3: Integration Testing**
- Use assert_cmd for CLI testing
- Temporary directories for isolation
- Real provider interactions (not mocks)
- Part of CI/CD pipeline

### Phase 4 Decisions

**D4.1: Persistence Storage**
- Embedded database (sled) for simplicity
- ULID for sortable IDs (per PLAN.md)
- RFC-3339 timestamps (per PLAN.md)
- Parent-child relationship tracking

**D4.2: Replay Interface**
- CLI subcommand (not separate binary)
- List, replay by ID, tree visualization
- Pagination for scalability
- Human-readable output

### Phase 5 Decisions

**D5.1: Parallel Execution**
- Tokio for async runtime
- Semaphore for concurrency control
- Fail-fast option for error handling
- Per-task timing and results

**D5.2: Resource Quotas**
- Three limits: executions, tokens, time
- Session-scoped (not global)
- Optional (None = unlimited)
- Clear error messages when exceeded

**D5.3: Metrics Strategy**
- Prometheus-compatible metrics
- Optional feature (no runtime cost if disabled)
- Per-depth labeling for analysis
- Panic-safe cleanup (Drop trait)

---

## Changelog

**2024-01-XX**: Initial plan created for Phases 3-5
- Phase 3: Configuration and Observability
- Phase 4: Conversation Persistence
- Phase 5: Advanced Execution Patterns

---

**End of Implementation Plan**
