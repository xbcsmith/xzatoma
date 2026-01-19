# Chat Modes Architecture

This document explains the design and implementation of XZatoma's chat mode system, which provides two complementary operating modes (Planning and Write) with safety controls.

## Overview

The chat modes system is built on a simple but powerful principle: **give the user fine-grained control over what the AI agent can do**. This is achieved through two independent control dimensions:

1. **Chat Mode** - What tools are available (read-only vs read/write)
2. **Safety Mode** - Whether operations require confirmation

Together, these create four distinct configurations:
- **Planning + Safe** - Analyze with confirmations
- **Planning + YOLO** - Analyze without interruptions
- **Write + Safe** - Execute with safety net
- **Write + YOLO** - Execute at full speed

## Architecture Components

### 1. Chat Mode System (Read-Only vs Read/Write)

#### ChatMode Enum

```rust
pub enum ChatMode {
    Planning,  // Read-only access to files
    Write,     // Read and write access
}
```

**Planning Mode Characteristics:**
- Can read files and directories
- Can create analysis documents and plans
- Cannot modify, delete, or create files
- No terminal access
- No command execution

**Write Mode Characteristics:**
- Can read, create, update, and delete files
- Can execute terminal commands
- Can run arbitrary programs
- Can modify system state

#### ChatModeState Structure

```rust
pub struct ChatModeState {
    pub chat_mode: ChatMode,
    pub safety_mode: SafetyMode,
}
```

Tracks the current mode configuration during a session. Provides methods for:
- Switching between modes
- Formatting prompts with mode indicators
- Generating status displays

### 2. Safety Mode System (Confirmations)

#### SafetyMode Enum

```rust
pub enum SafetyMode {
    AlwaysConfirm,  // Ask before dangerous operations
    NeverConfirm,   // Execute without asking
}
```

**Safe Mode (AlwaysConfirm):**
- Dangerous operations require explicit confirmation
- Prevents accidental destructive actions
- Slows down execution but provides safety net
- Recommended for beginners and important files

**YOLO Mode (NeverConfirm):**
- Operations execute immediately
- Faster execution, no interruptions
- Higher risk of unintended consequences
- Use only with full understanding of agent's plan

### 3. Tool Registry Filtering

The core mechanism for enforcing modes is the **mode-aware tool registry**. Different tools are available based on the current chat mode.

#### ToolRegistryBuilder

```rust
pub struct ToolRegistryBuilder {
    mode: ChatMode,
    safety_mode: SafetyMode,
    working_dir: PathBuf,
    config: AgentConfig,
}

impl ToolRegistryBuilder {
    pub fn build_for_planning(&self) -> ToolRegistry {
        // Read-only tools only
        // - List files: allowed
        // - Read files: allowed
        // - Write files: BLOCKED
        // - Delete files: BLOCKED
        // - Terminal: BLOCKED
    }

    pub fn build_for_write(&self) -> ToolRegistry {
        // All tools available
        // - List files: allowed
        // - Read files: allowed
        // - Write files: allowed
        // - Delete files: allowed
        // - Terminal: allowed
    }
}
```

#### Tool Availability Matrix

| Tool | Planning | Write |
|------|----------|-------|
| List Files | ✓ | ✓ |
| Read Files | ✓ | ✓ |
| Write Files | ✗ | ✓ |
| Delete Files | ✗ | ✓ |
| Terminal | ✗ | ✓ |
| Plan Parser | ✓ | ✓ |

### 4. Mode-Specific System Prompts

Each chat mode receives a different system prompt that guides the AI's behavior.

#### Planning Mode Prompt

The planning prompt instructs the agent to:
- Focus on analysis and understanding
- Create comprehensive plans
- Output in YAML or Markdown format
- Not attempt write operations
- Recommend safe execution approaches

Key instruction: *"You are in PLANNING mode and can only read files and create plans. Never attempt to modify files or execute commands."*

#### Write Mode Prompt

The write prompt instructs the agent to:
- Execute plans confidently
- Use all available tools
- Follow safety mode instructions for confirmations
- Verify changes with tests
- Report progress clearly

Key instruction: *"You are in WRITE mode and can modify files and execute commands. Follow the Safety Mode guidelines for confirmations."*

### 5. Interactive Mode Switching

Users can switch between modes at any time during a chat session. The system preserves conversation history across mode switches.

#### Mode Switch Flow

```
Current Agent State:
├── Conversation History (preserved)
├── Tool Registry (rebuilt)
├── System Prompt (changed)
└── ChatModeState (updated)

Mode Switch:
1. Capture current conversation
2. Update ChatModeState
3. Rebuild ToolRegistry for new mode
4. Inject new system prompt
5. Create new Agent with same conversation
6. Display confirmation message
```

#### Conversation Preservation

When switching modes:
- All messages in the conversation are retained
- Tool results from previous operations are remembered
- Context from earlier discussions influences future responses
- Agent can refer to previous decisions and analysis

This allows natural workflows:
1. **Planning Phase**: Explore and analyze in Planning mode
2. **Implementation Phase**: Execute changes in Write mode
3. **Review Phase**: Switch back to Planning mode for verification

### 6. Special Commands Parser

Interactive mode provides special commands for controlling the session, implemented in `special_commands.rs`.

```rust
pub enum SpecialCommand {
    SwitchMode(ChatMode),      // /mode planning, /write
    SwitchSafety(SafetyMode),  // /safe, /yolo
    ShowStatus,                // /status
    Help,                      // /help
    Exit,                      // exit, quit
    None,                      // Regular agent prompt
}
```

Commands are:
- Case-insensitive
- Prefixed with `/`
- Have multiple aliases for convenience
- Integrated into the interactive readline loop

### 7. UI/UX Components

#### Welcome Banner

Displayed when starting interactive chat:

```
╔══════════════════════════════════════════════════════════════╗
║         XZatoma Interactive Chat Mode - Welcome!             ║
╚══════════════════════════════════════════════════════════════╝

Mode:   PLANNING (Read-only mode for creating plans)
Safety: SAFE (Confirm dangerous operations)

Type '/help' for available commands, 'exit' to quit
```

#### Status Display

Shown when user types `/status`:

```
╔══════════════════════════════════════════════════════════════╗
║                     XZatoma Session Status                   ║
╚══════════════════════════════════════════════════════════════╝

Chat Mode:         WRITE (Read/write mode for executing tasks)
Safety Mode:       SAFE (Confirm dangerous operations)
Available Tools:   5
Conversation Size: 12 messages
Prompt Format:     [WRITE][SAFE] >>
```

#### Mode Indicator Prompt

Every prompt line shows the current configuration:

```
[PLANNING][SAFE] >>
[WRITE][YOLO] >>
```

This provides constant visibility into what the agent can and cannot do.

## Data Flow

### Session Initialization

```
User Request
    ↓
Parse CLI Arguments (mode, safety)
    ↓
Initialize ChatModeState
    ↓
Build ToolRegistry for mode
    ↓
Create Provider
    ↓
Create Agent with system prompt
    ↓
Display Welcome Banner
    ↓
Enter Interactive Loop
```

### User Input Processing

```
User Types Input
    ↓
Check if Special Command
    ├─ Yes: Execute command (switch mode, show status, etc.)
    └─ No: Continue to Agent
    ↓
Send to Agent with Tools
    ↓
Agent Uses Available Tools
    ↓
Add Results to Conversation
    ↓
Display Response
    ↓
Loop
```

### Mode Switch Flow

```
User Types: /mode write
    ↓
Parse Special Command
    ↓
Create new ToolRegistry for Write mode
    ↓
Create new Agent with Write system prompt
    ↓
Preserve conversation history
    ↓
Update ChatModeState
    ↓
Display switch confirmation
    ↓
Continue with new mode
```

## Design Decisions

### Decision 1: Tool-Based Access Control (Not Permission-Based)

**Choice**: Use tool availability to control access rather than runtime permission checks.

**Rationale**:
- Simple to understand and maintain
- Prevents the agent from even attempting forbidden operations
- Clear boundaries prevent confusion
- Aligns with AI safety principles

**Alternative Considered**: Runtime permission checks that allow the agent to attempt operations but block them at execution time. This would be more complex and could lead to confusing error messages.

### Decision 2: Conversation Preservation Across Mode Switches

**Choice**: Preserve entire conversation history when switching modes.

**Rationale**:
- Natural workflow: explore in Planning, implement in Write
- Agent retains context from earlier analysis
- No information loss during mode switches
- Enables iterative refinement

**Alternative Considered**: Clear conversation on mode switch. This would be simpler but would force users to re-explain context.

### Decision 3: Orthogonal Chat Mode and Safety Mode

**Choice**: Keep chat mode (Planning/Write) and safety mode (Safe/YOLO) as independent dimensions.

**Rationale**:
- Provides fine-grained control
- Four configurations cover different use cases
- Safety mode applies consistently across both chat modes
- Simpler than hierarchical or nested mode systems

**Alternative Considered**: Single unified mode system (e.g., "ReadSafe", "ReadYOLO", "WriteSafe", "WriteYOLO"). This would be inflexible and harder to extend.

### Decision 4: Special Commands for Mode Switching

**Choice**: Use `/command` syntax for mode switching instead of menu-driven selection.

**Rationale**:
- Faster for keyboard-driven users
- Consistent with common CLI conventions
- Easily learned and remembered
- Allows multiple aliases for convenience

**Alternative Considered**: Menu system with numbered options. This would be slower and less convenient for power users.

### Decision 5: Mode-Specific System Prompts

**Choice**: Provide different system prompts for each mode to guide agent behavior.

**Rationale**:
- Agent understands its constraints and capabilities
- Guides agent toward appropriate tool usage
- Prevents agent from attempting forbidden operations
- Sets appropriate expectations for output format

**Alternative Considered**: No special prompts, rely only on tool availability. This could lead to agent attempts to use unavailable tools and confusing errors.

## Safety Considerations

### Planning Mode Safety

Planning mode is designed to be inherently safe because:

1. **No file modifications**: Agent cannot create, modify, or delete files
2. **No command execution**: Agent cannot run arbitrary commands
3. **Read-only analysis**: Agent can only read and analyze existing files
4. **Safe to explore**: Users can confidently run in Planning mode

**Safety Level**: HIGH - Very difficult for agent to cause harm

### Write Mode Safety

Write mode provides safety through:

1. **Safety confirmations**: AlwaysConfirm mode requires confirmation before dangerous ops
2. **Conversation history**: Users can see the agent's reasoning and plans
3. **Incremental execution**: Users can stop the agent between steps
4. **Clear warnings**: Mode switches include warnings about risks

**Safety Level**: MEDIUM-HIGH (with Safe mode) or MEDIUM (with YOLO mode)

### Best Practices for Safe Operation

1. **Start in Planning mode**: Explore and understand before making changes
2. **Create a plan**: Have the agent create a detailed plan before executing
3. **Use Safe mode**: Enable confirmations when working with important files
4. **Incremental changes**: Make small changes and verify each one
5. **Review before committing**: Use version control and review changes before committing

## Future Enhancements

### Planned Improvements

1. **Custom Tool Sets**: Allow users to create custom mode definitions with specific tool combinations
2. **Session Persistence**: Save and restore conversation state across sessions
3. **Audit Logging**: Detailed logs of all tool usage and mode switches
4. **Confirmation Workflows**: Customizable confirmation dialogs with operation details
5. **Mode Profiles**: Saved mode configurations for different project types

### Potential Extensions

1. **Time-Based Mode**: Automatically switch modes after analysis period
2. **Conditional Modes**: Switch modes based on AI confidence levels
3. **Collaborative Modes**: Multiple users with different mode permissions
4. **Sandboxed Write Mode**: Write mode that operates in isolated directory

## Integration with Other Systems

### Relationship to System Prompts

Chat modes work together with system prompts:
- System prompts tell the agent what it can do
- Chat mode tools enforce what it actually can do
- Together they create reliable constraints

### Relationship to Plan Format Detection

The plan format system works with chat modes:
- Planning mode recommends YAML or Markdown output
- Plan parser validates detected formats
- Write mode can execute parsed plans

### Relationship to Provider Abstraction

Chat modes are provider-agnostic:
- Same tool restrictions apply to all providers
- Each provider gets the same mode-specific system prompt
- Mode switching works identically across providers

## Implementation Details

### Code Organization

```
src/
├── chat_mode.rs           # ChatMode, SafetyMode, ChatModeState enums
├── commands/
│   ├── mod.rs             # print_welcome_banner, print_status_display
│   ├── chat.rs (mod)      # run_chat, build_tools_for_mode
│   └── special_commands.rs # parse_special_command, print_help
├── tools/
│   ├── registry_builder.rs # ToolRegistryBuilder
│   └── mod.rs             # Tool implementations
└── prompts/               # Mode-specific system prompts
    ├── planning_prompt.rs
    └── write_prompt.rs
```

### Tool Registry Rebuilding

When mode switches occur, the tool registry is rebuilt:

1. Previous registry is discarded
2. New registry is built for new mode
3. Same provider and configuration used
4. Conversation history preserved in Agent

This ensures clean tool state and prevents tool leakage between modes.

### System Prompt Injection

When creating an agent, the mode-specific system prompt is injected as the first system message in the conversation. This:

1. Guides the agent's understanding of available tools
2. Sets expectations for output format
3. Is preserved across the conversation
4. Influences all subsequent agent responses

## Testing Strategy

### Unit Tests

- Parse special commands correctly
- ChatModeState state transitions
- Tool registry building for each mode
- System prompt generation

### Integration Tests

- Complete mode switch flow
- Conversation preservation
- Tool availability in each mode
- Error handling for invalid operations

### Manual Testing Checklist

- Start in Planning mode, verify read-only behavior
- Switch to Write mode, verify full access
- Switch safety modes, verify confirmation behavior
- Verify mode indicator changes in prompt
- Test all special commands
- Verify conversation preserved across switches
- Test /status command displays correct info
- Verify welcome banner displays correctly

## Conclusion

The chat modes system provides a clean, user-friendly way to give users fine-grained control over what their AI agent can do. By combining orthogonal mode systems (chat mode + safety mode) with tool-based access control and conversation preservation, we create a safe yet powerful system that supports diverse workflows and use cases.

The design emphasizes:
- **Safety**: Multiple layers of protection
- **Simplicity**: Easy to understand and use
- **Flexibility**: Supports different working styles
- **Clarity**: Clear visibility into current state
