# Chat Modes Implementation Plan

## Overview

This plan adds two distinct chat modes to XZatoma's interactive chat: **Planning Mode** (read-only, creates plans) and **Write Mode** (read/write, executes plans). Users can switch between modes during a session while preserving conversation history. A safety toggle allows switching between "Always Confirm" and "YOLO" (never confirm) modes for dangerous operations.

## Current State Analysis

### Existing Infrastructure

- **Interactive Chat**: Implemented in `src/commands/mod.rs` using `rustyline` for readline-based interaction
- **Tool Registry**: `ToolRegistry` in `src/tools/mod.rs` manages available tools
- **Tool Executors**: `FileOpsTool` and `TerminalTool` provide file and terminal operations
- **Command Validator**: `CommandValidator` in `src/tools/terminal.rs` validates terminal commands with allowlist/denylist
- **CLI Structure**: `Commands::Chat` enum variant in `src/cli.rs` accepts provider override

### Identified Issues

- No distinction between planning and execution modes
- All tools available regardless of user intent
- No mid-session mode switching capability
- No visual indicators of current mode
- Safety mode is configuration-only, not runtime-toggleable
- No plan format validation or output structure

## Implementation Phases

### Phase 1: Core Mode Infrastructure

#### Task 1.1: Create Mode Enums and Types

**Files to Create:**
- `src/commands/chat_mode.rs` - Mode definitions and display logic

**Implementation:**
```rust
// ChatMode enum with Planning and Write variants
pub enum ChatMode {
    Planning,  // Read-only, creates plans
    Write,     // Read/write, executes plans
}

// SafetyMode enum for confirmation behavior
pub enum SafetyMode {
    AlwaysConfirm,  // Confirm dangerous operations
    NeverConfirm,   // YOLO mode - never confirm
}

// ChatModeState to track current session state
pub struct ChatModeState {
    chat_mode: ChatMode,
    safety_mode: SafetyMode,
}
```

**Display formatting:**
- Planning mode prompt: `[PLANNING] >>`
- Write mode prompt: `[WRITE] >>`
- Safety mode indicator: `[SAFE]` or `[YOLO]`

#### Task 1.2: Update CLI Structure

**Files to Modify:**
- `src/cli.rs` - Add mode and safety flags to `Commands::Chat`

**Changes:**
```rust
Chat {
    provider: Option<String>,
    #[arg(short, long, default_value = "planning")]
    mode: Option<String>,  // "planning" or "write"
    #[arg(short = 's', long)]
    safe: bool,  // If true, use AlwaysConfirm; else NeverConfirm
}
```

**Tests to Add:**
- `test_cli_parse_chat_with_mode_planning`
- `test_cli_parse_chat_with_mode_write`
- `test_cli_parse_chat_with_safety_flag`
- `test_cli_parse_chat_mode_default`

#### Task 1.3: Update Configuration

**Files to Modify:**
- `config/config.yaml` - Add default chat mode settings
- `src/config.rs` - Add `ChatConfig` struct

**Configuration Schema:**
```yaml
agent:
  chat:
    default_mode: "planning"  # planning or write
    default_safety: "confirm" # confirm or yolo
    allow_mode_switching: true
```

**Rust Structure:**
```rust
pub struct ChatConfig {
    pub default_mode: String,
    pub default_safety: String,
    pub allow_mode_switching: bool,
}
```

#### Task 1.4: Testing Requirements

- Unit tests for `ChatMode` and `SafetyMode` enum conversions
- Unit tests for `ChatModeState` transitions
- CLI parsing tests for all new flags
- Configuration loading tests for chat settings

#### Task 1.5: Deliverables

- `src/commands/chat_mode.rs` (~200 lines)
- Updated `src/cli.rs` (+30 lines)
- Updated `config/config.yaml` (+5 lines)
- Updated `src/config.rs` (+40 lines)
- Test coverage >80%

#### Task 1.6: Success Criteria

- `cargo fmt --all` passes
- `cargo check --all-targets --all-features` passes
- `cargo clippy --all-targets --all-features -- -D warnings` shows zero warnings
- `cargo test --all-features` passes with >80% coverage
- CLI accepts `--mode` and `--safe` flags without errors

### Phase 2: Tool Filtering and Registration

#### Task 2.1: Implement Mode-Aware Tool Registry

**Files to Create:**
- `src/tools/registry_builder.rs` - Builder for mode-specific tool registration

**Implementation:**
```rust
pub struct ToolRegistryBuilder {
    mode: ChatMode,
    safety_mode: SafetyMode,
    working_dir: PathBuf,
    config: ToolsConfig,
}

impl ToolRegistryBuilder {
    pub fn new(mode: ChatMode, safety_mode: SafetyMode) -> Self { ... }

    pub fn build_for_planning(&self) -> ToolRegistry {
        // Register only read-only tools
        // - file_ops: read_file, list_files, search_files
        // - NO write_file, NO terminal execution
    }

    pub fn build_for_write(&self) -> ToolRegistry {
        // Register all tools
        // - file_ops: all operations
        // - terminal: with safety_mode consideration
    }
}
```

#### Task 2.2: Create Read-Only FileOps Tool

**Files to Modify:**
- `src/tools/file_ops.rs` - Add `FileOpsReadOnlyTool` variant

**Implementation:**
- Clone existing `FileOpsTool`
- Create `FileOpsReadOnlyTool` that only exposes:
  - `read_file(path: string) -> string`
  - `list_files(directory: string) -> array`
  - `search_files(pattern: string, directory: string) -> array`
- Remove write operations from tool definition

**Alternative Approach:**
- Add `read_only: bool` flag to `FileOpsTool`
- Conditionally include write operations in `tool_definition()`
- Return error if write operation called in read-only mode

#### Task 2.3: Integrate Mode-Aware Tools in Chat Command

**Files to Modify:**
- `src/commands/mod.rs` - Update `run_chat()` function

**Changes:**
```rust
pub async fn run_chat(
    config: Config,
    provider_name: Option<String>,
    initial_mode: ChatMode,
    initial_safety: SafetyMode,
) -> Result<()> {
    let mut mode_state = ChatModeState::new(initial_mode, initial_safety);

    // Build tools based on current mode
    let tools = build_tools_for_mode(&mode_state, &config);

    // Create agent with initial tools
    let mut agent = create_agent(provider_name, tools, &config)?;

    // Interactive loop with mode switching support
    ...
}

fn build_tools_for_mode(
    mode_state: &ChatModeState,
    config: &Config,
) -> ToolRegistry {
    let builder = ToolRegistryBuilder::new(
        mode_state.chat_mode.clone(),
        mode_state.safety_mode.clone(),
    );

    match mode_state.chat_mode {
        ChatMode::Planning => builder.build_for_planning(),
        ChatMode::Write => builder.build_for_write(),
    }
}
```

#### Task 2.4: Testing Requirements

- Test tool registration for Planning mode (only read tools)
- Test tool registration for Write mode (all tools)
- Test SafetyMode affects terminal tool behavior
- Test tool execution errors in wrong mode
- Integration test for mode-specific tool availability

#### Task 2.5: Deliverables

- `src/tools/registry_builder.rs` (~150 lines)
- Updated `src/tools/file_ops.rs` (+80 lines for read-only variant)
- Updated `src/commands/mod.rs` (+60 lines)
- Test coverage >80%

#### Task 2.6: Success Criteria

- Planning mode agent has only read-only tools
- Write mode agent has all tools including write operations
- SafetyMode correctly passed to terminal validator
- All cargo checks pass with zero warnings
- Test coverage >80%

### Phase 3: Interactive Mode Switching

#### Task 3.1: Implement Special Commands Parser

**Files to Create:**
- `src/commands/special_commands.rs` - Parse and handle special commands

**Implementation:**
```rust
pub enum SpecialCommand {
    SwitchMode(ChatMode),
    SwitchSafety(SafetyMode),
    ShowStatus,
    Help,
    Exit,
    None,  // Not a special command
}

pub fn parse_special_command(input: &str) -> SpecialCommand {
    match input.trim() {
        "/mode planning" | "/planning" => SpecialCommand::SwitchMode(ChatMode::Planning),
        "/mode write" | "/write" => SpecialCommand::SwitchMode(ChatMode::Write),
        "/safe" | "/safety on" => SpecialCommand::SwitchSafety(SafetyMode::AlwaysConfirm),
        "/yolo" | "/safety off" => SpecialCommand::SwitchSafety(SafetyMode::NeverConfirm),
        "/status" => SpecialCommand::ShowStatus,
        "/help" => SpecialCommand::Help,
        "exit" | "quit" => SpecialCommand::Exit,
        _ => SpecialCommand::None,
    }
}
```

#### Task 3.2: Update Interactive Loop

**Files to Modify:**
- `src/commands/mod.rs` - Update readline loop in `run_chat()`

**Changes:**
```rust
loop {
    // Build dynamic prompt based on current mode
    let prompt = format_prompt(&mode_state);  // e.g., "[PLANNING][SAFE] >> "

    match rl.readline(&prompt) {
        Ok(line) => {
            let trimmed = line.trim();

            // Check for special commands first
            match parse_special_command(trimmed) {
                SpecialCommand::SwitchMode(new_mode) => {
                    handle_mode_switch(&mut agent, &mut mode_state, new_mode, &config)?;
                    continue;
                }
                SpecialCommand::SwitchSafety(new_safety) => {
                    handle_safety_switch(&mut mode_state, new_safety);
                    continue;
                }
                SpecialCommand::ShowStatus => {
                    print_status(&mode_state, &agent);
                    continue;
                }
                SpecialCommand::Help => {
                    print_help();
                    continue;
                }
                SpecialCommand::Exit => break,
                SpecialCommand::None => {
                    // Regular agent execution
                    agent.execute(trimmed.to_string()).await?;
                }
            }
        }
        ...
    }
}
```

#### Task 3.3: Implement Mode Switching with Conversation Preservation

**Implementation:**
```rust
fn handle_mode_switch(
    agent: &mut Agent,
    mode_state: &mut ChatModeState,
    new_mode: ChatMode,
    config: &Config,
) -> Result<()> {
    // Show warning when switching to Write mode
    if matches!(new_mode, ChatMode::Write) {
        println!("⚠️  Switching to WRITE mode - agent can now modify files and execute commands!");
        println!("   Type '/safe' to enable confirmations, or '/yolo' to disable.");
    }

    // Update mode state
    let old_mode = mode_state.chat_mode.clone();
    mode_state.chat_mode = new_mode;

    // Rebuild tools for new mode
    let new_tools = build_tools_for_mode(mode_state, config);

    // Preserve conversation history
    let conversation = agent.conversation().clone();

    // Create new agent with new tools but same conversation
    let provider = create_provider(&config)?;
    let mut new_agent = Agent::new(provider, new_tools, config.agent.clone())?;

    // Restore conversation history
    new_agent.restore_conversation(conversation)?;

    // Replace agent
    *agent = new_agent;

    println!("✓ Switched from {:?} to {:?} mode", old_mode, mode_state.chat_mode);
    Ok(())
}
```

#### Task 3.4: Testing Requirements

- Test special command parsing (all variants)
- Test mode switching preserves conversation
- Test tool availability changes after mode switch
- Test safety mode switching updates terminal validator
- Test warning displays when switching to Write mode
- Integration test for complete mode switch workflow

#### Task 3.5: Deliverables

- `src/commands/special_commands.rs` (~120 lines)
- Updated `src/commands/mod.rs` (+150 lines)
- Updated `src/agent/core.rs` (+30 lines for `restore_conversation()`)
- Test coverage >80%

#### Task 3.6: Success Criteria

- Special commands parsed correctly
- Mode switching updates tool registry
- Conversation history preserved across switches
- Safety mode changes affect terminal execution
- Warning displayed when entering Write mode
- All cargo checks pass with zero warnings

### Phase 4: System Prompts and Plan Format Support

#### Task 4.1: Create Mode-Specific System Prompts

**Files to Create:**
- `src/prompts/mod.rs` - System prompts for each mode
- `src/prompts/planning_prompt.rs` - Planning mode prompt
- `src/prompts/write_prompt.rs` - Write mode prompt

**Planning Mode System Prompt:**
```text
You are in PLANNING mode. Your role is to analyze the request and create a detailed plan.

AVAILABLE ACTIONS:
- Read files to understand current state
- List directory contents
- Search for patterns in files

OUTPUT FORMAT:
You should output a plan in one of these formats:
1. YAML format (following the structure in PLAN.md)
2. Markdown with sections: Overview, Steps, Open Questions
3. Markdown with YAML frontmatter

You CANNOT:
- Modify files
- Execute commands
- Make any changes to the system

Focus on creating a thorough, actionable plan that another agent or human can execute.
```

**Write Mode System Prompt:**
```text
You are in WRITE mode. You can read files, modify files, and execute terminal commands.

AVAILABLE TOOLS:
- Read/write files
- Execute terminal commands (subject to safety validation)
- Search and list files

SAFETY:
- Current safety mode: {safety_mode}
- Always validate commands before execution
- Prefer safe, reversible operations
- Ask for confirmation when in doubt

Your goal is to execute the plan or task using the available tools effectively.
```

#### Task 4.2: Integrate System Prompts into Agent

**Files to Modify:**
- `src/agent/core.rs` - Add system message to conversation
- `src/agent/conversation.rs` - Support system role messages

**Changes:**
```rust
impl Agent {
    pub fn new_with_mode(
        provider: impl Provider + 'static,
        tools: ToolRegistry,
        config: AgentConfig,
        mode: ChatMode,
        safety: SafetyMode,
    ) -> Result<Self> {
        let mut agent = Self::new(provider, tools, config)?;

        // Add mode-specific system prompt
        let system_prompt = build_system_prompt(mode, safety);
        agent.conversation.add_system_message(system_prompt);

        Ok(agent)
    }
}
```

#### Task 4.3: Implement Plan Format Validators

**Files to Create:**
- `src/tools/plan_format.rs` - Validate and parse plan outputs

**Implementation:**
```rust
pub enum PlanFormat {
    Yaml,
    Markdown,
    MarkdownWithFrontmatter,
}

pub fn detect_plan_format(content: &str) -> PlanFormat {
    if content.trim_start().starts_with("---") {
        PlanFormat::MarkdownWithFrontmatter
    } else if content.trim_start().starts_with("# ") || content.contains("## ") {
        PlanFormat::Markdown
    } else {
        PlanFormat::Yaml
    }
}

pub fn validate_plan(content: &str) -> Result<ValidatedPlan> {
    let format = detect_plan_format(content);
    match format {
        PlanFormat::Yaml => validate_yaml_plan(content),
        PlanFormat::Markdown => validate_markdown_plan(content),
        PlanFormat::MarkdownWithFrontmatter => validate_frontmatter_plan(content),
    }
}
```

#### Task 4.4: Testing Requirements

- Test system prompt generation for each mode
- Test plan format detection (YAML, Markdown, frontmatter)
- Test plan validation for each format
- Test system prompt injection into conversation
- Test mode-specific behavior differences

#### Task 4.5: Deliverables

- `src/prompts/mod.rs` (~80 lines)
- `src/prompts/planning_prompt.rs` (~60 lines)
- `src/prompts/write_prompt.rs` (~60 lines)
- `src/tools/plan_format.rs` (~200 lines)
- Updated `src/agent/core.rs` (+40 lines)
- Updated `src/agent/conversation.rs` (+20 lines)
- Test coverage >80%

#### Task 4.6: Success Criteria

- Planning mode agent receives appropriate system prompt
- Write mode agent receives appropriate system prompt
- Plan format detection works for all three formats
- System prompts correctly influence agent behavior
- All cargo checks pass with zero warnings

### Phase 5: UI/UX Polish and Documentation

#### Task 5.1: Implement Status Display

**Files to Modify:**
- `src/commands/mod.rs` - Add status display functions

**Implementation:**
```rust
fn print_status(mode_state: &ChatModeState, agent: &Agent) {
    println!("\n=== XZatoma Status ===");
    println!("Mode: {:?}", mode_state.chat_mode);
    println!("Safety: {:?}", mode_state.safety_mode);
    println!("Tools: {}", agent.num_tools());
    println!("Conversation turns: {}", agent.conversation().len());
    println!("====================\n");
}

fn print_help() {
    println!("\n=== XZatoma Help ===");
    println!("Special Commands:");
    println!("  /mode planning, /planning  - Switch to Planning mode (read-only)");
    println!("  /mode write, /write        - Switch to Write mode (read/write)");
    println!("  /safe, /safety on          - Enable safety confirmations");
    println!("  /yolo, /safety off         - Disable safety confirmations (YOLO mode)");
    println!("  /status                    - Show current status");
    println!("  /help                      - Show this help");
    println!("  exit, quit                 - Exit chat");
    println!("\nModes:");
    println!("  Planning - Read files and create plans (safe, no modifications)");
    println!("  Write    - Execute plans, modify files, run commands");
    println!("====================\n");
}

fn format_prompt(mode_state: &ChatModeState) -> String {
    let mode_indicator = match mode_state.chat_mode {
        ChatMode::Planning => "[PLANNING]",
        ChatMode::Write => "[WRITE]",
    };
    let safety_indicator = match mode_state.safety_mode {
        SafetyMode::AlwaysConfirm => "[SAFE]",
        SafetyMode::NeverConfirm => "[YOLO]",
    };
    format!("{}{} >> ", mode_indicator, safety_indicator)
}
```

#### Task 5.2: Add Welcome Banner

**Implementation:**
```rust
fn print_welcome_banner(mode: &ChatMode, safety: &SafetyMode) {
    println!("\n╔════════════════════════════════════════╗");
    println!("║     XZatoma Interactive Chat Mode      ║");
    println!("╚════════════════════════════════════════╝");
    println!();
    println!("Starting in {:?} mode with {:?} safety", mode, safety);
    println!();
    println!("Type '/help' for commands, 'exit' to quit");
    println!();
}
```

#### Task 5.3: Update Documentation

**Files to Create/Update:**
- `docs/how-to/use_chat_modes.md` - User guide for chat modes
- `docs/explanation/chat_modes_architecture.md` - Architecture explanation
- Update `README.md` with chat mode examples

**Documentation Structure:**

**`docs/how-to/use_chat_modes.md`:**
- What are Planning and Write modes
- When to use each mode
- How to switch between modes
- Safety mode settings
- Example workflows

**`docs/explanation/chat_modes_architecture.md`:**
- Design decisions
- Mode switching implementation
- Tool filtering strategy
- Conversation preservation approach

#### Task 5.4: Testing Requirements

- Test status display shows correct information
- Test help text displays all commands
- Test prompt formatting for all mode combinations
- Test welcome banner displays correctly

#### Task 5.5: Deliverables

- Updated `src/commands/mod.rs` (+80 lines for UI functions)
- `docs/how-to/use_chat_modes.md` (~400 lines)
- `docs/explanation/chat_modes_architecture.md` (~300 lines)
- Updated `README.md` (+50 lines)

#### Task 5.6: Success Criteria

- Status command shows accurate information
- Help text is clear and complete
- Prompt clearly indicates current mode
- Documentation is comprehensive and follows Diataxis
- All cargo checks pass with zero warnings

## Testing Strategy

### Unit Tests

- `ChatMode` and `SafetyMode` enum operations
- Special command parsing
- Plan format detection and validation
- Tool registry builder for each mode
- Prompt formatting functions

### Integration Tests

- Complete chat session with mode switching
- Tool availability changes after mode switch
- Conversation preservation across switches
- Safety mode affecting terminal execution
- Plan creation in Planning mode
- File modification in Write mode

### Manual Testing Checklist

- [ ] Start chat in Planning mode
- [ ] Create a plan using read-only tools
- [ ] Switch to Write mode
- [ ] Execute plan with write tools
- [ ] Toggle safety mode on/off
- [ ] Verify conversation preserved across mode switches
- [ ] Test all special commands
- [ ] Verify prompt indicators update correctly
- [ ] Test plan output in all three formats (YAML, Markdown, frontmatter)

## Migration Path

### Backward Compatibility

- Default behavior: Start in Planning mode with SafetyMode::AlwaysConfirm
- Existing `xzatoma chat` command works without changes
- Configuration file additions are optional with sensible defaults

### Configuration Migration

**Before:**
```yaml
agent:
  max_iterations: 10
```

**After:**
```yaml
agent:
  max_iterations: 10
  chat:
    default_mode: "planning"
    default_safety: "confirm"
    allow_mode_switching: true
```

## Dependencies

### New Dependencies

None required - all functionality uses existing dependencies:
- `rustyline` (already in use)
- `serde_yaml` (already in use)
- Standard library for string parsing

### Modified Dependencies

None

## Rollout Plan

### Phase 1 Rollout

- Release with Planning mode as default
- Write mode available via `--mode write` flag
- Documentation emphasizes safety-first approach

### Phase 2 Rollout

- Collect user feedback on mode switching UX
- Refine special commands based on usage patterns
- Add telemetry for mode usage (if applicable)

### Phase 3 Rollout

- Add advanced features (plan templates, mode-specific history)
- Enhance plan format validation
- Add export/import for chat sessions

## Success Metrics

- All quality checks pass (fmt, check, clippy, test)
- Test coverage >80% for all new code
- Documentation complete in all Diataxis categories
- Zero breaking changes to existing API
- Users can successfully create plans in Planning mode
- Users can successfully execute plans in Write mode
- Mode switching preserves conversation context

## References

- Architecture: `docs/explanation/architecture_validation.md`
- Plan Format: `PLAN.md`
- Agent Rules: `AGENTS.md`
- Terminal Security: `docs/explanation/architecture_fixes_applied.md`
