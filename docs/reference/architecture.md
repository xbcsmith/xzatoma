# XZatoma Architecture

## Overview

XZatoma is a simple autonomous AI agent CLI written in Rust that can execute tasks through conversation with AI providers (GitHub Copilot or Ollama). It provides basic file system and terminal tools, allowing the AI to accomplish tasks by using these generic capabilities rather than specialized features.

Think of it as a command-line version of Zed's agent chat - you give it a goal (via interactive prompt or structured plan), and it uses basic tools to accomplish it.

## System Architecture

### High-Level Architecture

```
┌─────────────────────────────────────────────────────────┐
│           XZatoma CLI             │
├─────────────────────────────────────────────────────────┤
│                             │
│ ┌──────────────┐   ┌─────────────┐        │
│ │  CLI Layer │────▶ │ Agent Core │        │
│ │  (clap)   │   │       │        │
│ └──────────────┘   └─────────────┘        │
│                │             │
│                ▼             │
│ ┌──────────────────────────────────────────────────┐ │
│ │     Provider Abstraction Layer        │ │
│ │ ┌─────────────────┐  ┌─────────────────┐   │ │
│ │ │ Copilot Provider│  │ Ollama Provider │   │ │
│ │ └─────────────────┘  └─────────────────┘   │ │
│ └──────────────────────────────────────────────────┘ │
│                │             │
│                ▼             │
│ ┌──────────────────────────────────────────────────┐ │
│ │       Basic Tools             │ │
│ │ ┌──────────┐ ┌──────────┐ ┌──────────┐   │ │
│ │ │File Ops │ │Terminal │ │ Plan   │   │ │
│ │ │(CRUD)  │ │Executor │ │ Parser  │   │ │
│ │ └──────────┘ └──────────┘ └──────────┘   │ │
│ └──────────────────────────────────────────────────┘ │
│                             │
└─────────────────────────────────────────────────────────┘
```

## Core Components

### 1. CLI Layer

**Purpose**: User interface and input handling

**Responsibilities**:

- Parse command-line arguments
- Load plan files (JSON, YAML, Markdown)
- Start interactive chat mode
- Display agent responses and tool usage
- Handle configuration

**Key Modules**:

- `cli.rs` - CLI parser using `clap`
- `config.rs` - Configuration management

**Usage Modes**:

```bash
# Interactive mode
xzatoma chat

# Execute a plan
xzatoma run --plan task.yaml

# One-shot prompt
xzatoma run --prompt "List all Rust files and count lines"
```

### 2. Agent Core

**Purpose**: Autonomous execution loop with AI provider

**Responsibilities**:

- Manage conversation with AI provider
- Execute tool calls returned by AI
- Add tool results back to conversation
- Continue until task complete
- Handle errors and retries

**Key Modules**:

- `agent/mod.rs` - Main agent
- `agent/conversation.rs` - Message history
- `agent/executor.rs` - Tool execution

**Execution Loop**:

```
1. User provides goal/instruction
2. Agent sends conversation to AI provider
3. AI responds with:
  - Tool call → Execute tool → Add result → Loop to step 2
  - Final answer → Return to user
4. Done
```

**Architecture Pattern**:

```rust
pub struct Agent {
  provider: Arc<dyn Provider>,
  conversation: Conversation,
  tools: Vec<Tool>,
  max_iterations: usize,
}

impl Agent {
  pub async fn execute(&mut self, instruction: String) -> Result<String> {
    self.conversation.add_user_message(instruction);

    let mut iterations = 0;

    loop {
      // Enforce iteration limit to prevent infinite loops
      if iterations >= self.max_iterations {
        return Err(XzatomaError::MaxIterationsExceeded {
          limit: self.max_iterations,
          message: "Agent exceeded maximum iteration limit".to_string(),
        });
      }
      iterations += 1;

      let response = self.provider.complete(
        &self.conversation.messages(),
        &self.tools
      ).await?;

      if let Some(tool_calls) = response.tool_calls {
        for call in tool_calls {
          let result = self.execute_tool(&call).await?;
          self.conversation.add_tool_result(result);
        }
      } else {
        return Ok(response.content);
      }
    }
  }
}
```

**Iteration Limits**:

The agent enforces a maximum iteration limit (configured via `agent.max_turns`, default: 100) to prevent infinite loops where the AI continuously calls tools without reaching a conclusion. When the limit is exceeded, the agent returns an error to the user.

### 2.5. Conversation Management

**Purpose**: Manage conversation history and token limits

**Responsibilities**:

- Track conversation messages
- Count approximate tokens
- Prune old messages when approaching limits
- Preserve essential context

**Key Modules**:

- `agent/conversation.rs` - Conversation history and token management

**Token Limits by Provider**:

| Provider | Model    | Context Window | Safe Limit (80%) |
| -------- | ----------- | -------------- | ---------------- |
| Copilot | gpt-5-mini | 128,000 tokens | 102,400 tokens  |
| Copilot | gpt-4o-mini | 128,000 tokens | 102,400 tokens  |
| Ollama  | qwen3    | 32,768 tokens | 26,214 tokens  |
| Ollama  | llama3   | 8,192 tokens  | 6,553 tokens   |

**Pruning Strategy**:

When conversation approaches token limit:

1. **Always Retain**:

  - System message (tool definitions)
  - Original user instruction
  - Last 5 turns of conversation

2. **Prune in Order**:

  - Oldest tool call/result pairs first
  - Keep most recent tool results (more relevant)
  - Summarize pruned content in special message

3. **Pruning Example**:
  ```
  [PRUNED: 15 tool calls between turn 3-18. Summary: Listed files, read configuration, searched for TODO comments]
  ```

**Implementation Pattern**:

```rust
pub struct Conversation {
  messages: Vec<Message>,
  token_count: usize,
  max_tokens: usize,
  min_retain_turns: usize,
}

impl Conversation {
  pub fn add_user_message(&mut self, content: String) {
    let message = Message::user(content);
    self.messages.push(message);
    self.update_token_count();
    self.prune_if_needed();
  }

  pub fn add_assistant_message(&mut self, content: String, tool_calls: Option<Vec<ToolCall>>) {
    let message = Message::assistant(content, tool_calls);
    self.messages.push(message);
    self.update_token_count();
    self.prune_if_needed();
  }

  pub fn add_tool_result(&mut self, tool_call_id: String, result: String) {
    let message = Message::tool_result(tool_call_id, result);
    self.messages.push(message);
    self.update_token_count();
    self.prune_if_needed();
  }

  fn update_token_count(&mut self) {
    // Approximate: 1 token ≈ 4 characters
    self.token_count = self.messages.iter()
      .map(|m| m.content.len() / 4)
      .sum();
  }

  fn prune_if_needed(&mut self) {
    if self.token_count <= self.max_tokens {
      return;
    }

    // Find pruneable range (between system message and last N turns)
    let system_end = 1; // First message is system
    let recent_start = self.messages.len().saturating_sub(self.min_retain_turns * 2);

    if recent_start <= system_end {
      // Can't prune enough - return error to user
      return;
    }

    // Create summary of pruned section
    let pruned = &self.messages[system_end..recent_start];
    let summary = self.create_summary(pruned);

    // Remove pruned messages and add summary
    self.messages.drain(system_end..recent_start);
    self.messages.insert(system_end, Message::system(format!(
      "[CONTEXT PRUNED: {}]", summary
    )));

    self.update_token_count();
  }

  fn create_summary(&self, messages: &[Message]) -> String {
    let tool_calls: Vec<_> = messages.iter()
      .filter_map(|m| m.tool_call.as_ref())
      .map(|tc| tc.function.name.as_str())
      .collect();

    format!(
      "{} turns with {} tool calls: {}",
      messages.len(),
      tool_calls.len(),
      tool_calls.join(", ")
    )
  }

  pub fn messages(&self) -> &[Message] {
    &self.messages
  }

  pub fn token_count(&self) -> usize {
    self.token_count
  }
}
```

**Configuration**:

```yaml
agent:
 max_turns: 100
 conversation:
  max_tokens: 100000 # Provider-specific limit
  min_retain_turns: 5 # Always keep last N turns
  prune_threshold: 0.8 # Prune when 80% of max_tokens
```

### 3. Provider Abstraction

**Purpose**: Unified interface to AI providers

**Responsibilities**:

- Abstract Copilot and Ollama APIs
- Handle authentication
- Support streaming responses
- Retry failed requests

**Key Modules**:

- `providers/mod.rs` - Provider trait
- `providers/copilot.rs` - GitHub Copilot
- `providers/ollama.rs` - Ollama

**Provider Trait**:

```rust
#[async_trait]
pub trait Provider: Send + Sync {
  async fn complete(
    &self,
    messages: &[Message],
    tools: &[Tool],
  ) -> Result<Response>;
}

pub struct Response {
  pub content: String,
  pub tool_calls: Option<Vec<ToolCall>>,
}
```

**Extended Provider Trait** (for future phases):

```rust
#[async_trait]
pub trait ProviderExtended: Provider {
  /// Streaming completion (optional - not all providers support)
  async fn complete_stream(
    &self,
    messages: &[Message],
    tools: &[Tool],
  ) -> Result<Pin<Box<dyn Stream<Item = Result<ResponseChunk>> + Send>>> {
    Err(XzatomaError::StreamingNotSupported)
  }

  /// Get provider capabilities
  fn capabilities(&self) -> ProviderCapabilities;

  /// Authenticate with provider
  async fn authenticate(&mut self) -> Result<()>;

  /// Check if authenticated
  fn is_authenticated(&self) -> bool;
}

pub struct ProviderCapabilities {
  pub max_tokens: usize,
  pub supports_tool_calls: bool,
  pub supports_streaming: bool,
  pub model_name: String,
}

pub struct ResponseChunk {
  pub delta: String,
  pub tool_call_delta: Option<ToolCallDelta>,
  pub finish_reason: Option<String>,
}
```

**Note**: Phase 1 implementation uses simplified `Provider` trait. Streaming support and advanced features added in later phases.

### 4. Basic Tools

**Purpose**: Generic file and terminal operations

**File Operations**:

- `list_files` - List files in directory
- `read_file` - Read file content
- `write_file` - Create or overwrite file
- `create_directory` - Create directory
- `delete_path` - Delete file or directory
- `diff_files` - Show diff between two files

**Terminal Operations**:

- `execute_command` - Run shell command

**Plan Operations**:

- `parse_plan` - Parse JSON/YAML/Markdown plan

**Tool Definition**:

```rust
pub struct Tool {
  pub name: String,
  pub description: String,
  pub parameters: serde_json::Value, // JSON Schema
}

#[async_trait]
pub trait ToolExecutor {
  async fn execute(&self, params: serde_json::Value) -> Result<ToolResult>;
}
```

**Tool Result Format**:

```rust
pub struct ToolResult {
  /// Whether tool executed successfully
  pub success: bool,

  /// Tool output (stdout or return value)
  pub output: String,

  /// Error message if success=false
  pub error: Option<String>,

  /// Whether output was truncated due to size
  pub truncated: bool,

  /// Additional metadata (execution time, file size, etc.)
  pub metadata: HashMap<String, String>,
}

impl ToolResult {
  pub fn success(output: String) -> Self {
    Self {
      success: true,
      output,
      error: None,
      truncated: false,
      metadata: HashMap::new(),
    }
  }

  pub fn error(error: String) -> Self {
    Self {
      success: false,
      output: String::new(),
      error: Some(error),
      truncated: false,
      metadata: HashMap::new(),
    }
  }

  pub fn truncate_if_needed(&mut self, max_size: usize) {
    if self.output.len() > max_size {
      let original_size = self.output.len();
      self.output.truncate(max_size);
      self.output.push_str("\n\n... (output truncated)");
      self.truncated = true;
      self.metadata.insert(
        "original_size".to_string(),
        original_size.to_string()
      );
    }
  }

  /// Format for AI consumption
  pub fn to_message(&self) -> String {
    if self.success {
      let mut msg = self.output.clone();
      if self.truncated {
        msg.push_str(&format!(
          "\n[Note: Output truncated at {} bytes]",
          self.metadata.get("original_size").unwrap_or(&"unknown".to_string())
        ));
      }
      msg
    } else {
      format!("Error: {}", self.error.as_ref().unwrap())
    }
  }
}
```

## Module Structure

```
xzatoma/
├── src/
│  ├── main.rs       # Entry point
│  ├── lib.rs        # Library root
│  ├── cli.rs        # CLI parser
│  ├── config.rs      # Configuration
│  ├── error.rs       # Error types
│  │
│  ├── agent/        # Agent core
│  │  ├── mod.rs
│  │  ├── agent.rs     # Main agent logic
│  │  ├── conversation.rs # Message history and token management
│  │  └── executor.rs   # Tool execution and registry
│  │
│  ├── providers/      # AI providers
│  │  ├── mod.rs
│  │  ├── base.rs     # Provider trait
│  │  ├── copilot.rs    # GitHub Copilot
│  │  └── ollama.rs    # Ollama
│  │
│  └── tools/        # Basic tools
│    ├── mod.rs
│    ├── file_ops.rs   # File operations
│    ├── terminal.rs   # Terminal execution
│    └── plan.rs     # Plan parsing
│
├── tests/          # Tests
└── docs/          # Documentation
```

### Agent Module Details

The `agent/` directory contains three focused modules:

**agent/agent.rs**:

- `Agent` struct definition
- `execute()` method - main execution loop
- Provider interaction logic
- Iteration limit enforcement
- High-level error handling

**agent/conversation.rs**:

- `Conversation` struct - message history
- Token counting and management
- Message pruning when approaching token limit
- Conversation serialization for debugging

**agent/executor.rs**:

- `ToolExecutor` trait implementations
- Tool registry - maps tool names to implementations
- Tool call validation (parameter schema checking)
- Tool execution dispatch
- Result formatting and size limiting
- Tool execution timeout handling

**Separation Rationale**:

- `agent.rs`: Orchestrates the conversation flow
- `conversation.rs`: Manages message history state
- `executor.rs`: Handles tool-specific concerns

This keeps each module under 300 lines and focused on a single responsibility.

## Data Flow

### Interactive Mode

```
User Input → Agent → AI Provider (with tools) → Tool Call
        ↑                  ↓
        └────────── Tool Result ─────────────┘
             (loop until done)
```

### Plan Execution Mode

```
Plan File → Parse Plan → Extract Goal → Agent → AI Provider
                      ↓
                  Tool Execution Loop
                      ↓
                    Results
```

## Configuration

### Configuration Structure

```yaml
# ~/.config/xzatoma/config.yaml

provider:
 type: copilot # or 'ollama'

 copilot:
  model: gpt-5-mini

 ollama:
  host: localhost:11434
  model: qwen3

agent:
 max_turns: 100
 timeout_seconds: 600
 conversation:
  max_tokens: 100000
  min_retain_turns: 5
  prune_threshold: 0.8
 tools:
  max_output_size: 1048576 # 1 MB per tool result
  max_file_read_size: 10485760 # 10 MB for read_file
 terminal:
  default_mode: restricted_autonomous
  timeout_seconds: 30
  max_stdout_bytes: 10485760
  max_stderr_bytes: 1048576
```

### Configuration Precedence

Configuration is merged from multiple sources with the following precedence (highest to lowest):

1. **Command-line arguments** - Highest priority
  - Example: `xzatoma --provider ollama`
2. **Environment variables** - Override config file
  - Example: `XZATOMA_PROVIDER=ollama`
3. **Configuration file** - `~/.config/xzatoma/config.yaml`
  - Loaded if present
4. **Default values** - Built-in defaults
  - Example: `provider: copilot`, `max_turns: 100`

**Example Priority Resolution**:

```yaml
# config.yaml
provider:
 type: copilot
```

```bash
# Environment variable overrides config.yaml
export XZATOMA_PROVIDER=ollama

# Command-line overrides both
xzatoma --provider copilot # Uses copilot (CLI wins)
```

### Environment Variables

```bash
XZATOMA_PROVIDER=copilot
COPILOT_MODEL=gpt-5-mini
OLLAMA_HOST=localhost:11434
OLLAMA_MODEL=qwen3
```

## Plan File Format

Plans are just structured instructions for the agent. The agent decides how to accomplish them using available tools.

### YAML Example

```yaml
goal: "Scan the repository and generate a README"

context:
 repository: /path/to/repo

instructions:
 - List all source files in the repository
 - Identify the main components
 - Read key files to understand functionality
 - Create a comprehensive README.md with sections for overview, installation, and usage
```

### JSON Example

```json
{
 "goal": "Refactor function names in all Python files",
 "context": {
  "directory": "src/"
 },
 "instructions": [
  "Find all .py files",
  "Identify functions with camelCase naming",
  "Rename to snake_case",
  "Update all references"
 ]
}
```

### Markdown Example

```markdown
# Task: Generate Project Documentation

## Goal

Create comprehensive documentation for the project

## Context

- Repository: /path/to/project
- Focus on: Architecture and API reference

## Steps

1. Scan repository structure
2. Read main source files
3. Generate docs/architecture.md
4. Generate docs/api.md
```

The agent interprets these instructions and uses its tools to accomplish the goal.

## Plan Execution Strategy

Plans provide structured guidance to the agent but don't strictly constrain its behavior.

### Plan Processing Flow

```
1. Parse plan file (JSON/YAML/Markdown)
2. Extract: goal, context, instructions
3. Format as initial system prompt
4. Begin agent execution loop
5. AI uses instructions as guidance
```

### Plan to Prompt Translation

**YAML Plan**:

```yaml
goal: "Generate API documentation"
context:
 directory: "src/api/"
instructions:
 - List all API endpoint files
 - Extract function signatures
 - Create OpenAPI spec
```

**Translated to Agent Prompt**:

```
You are assisting with the following task:

GOAL: Generate API documentation

CONTEXT:
- Directory: src/api/

INSTRUCTIONS (follow as guidance):
1. List all API endpoint files
2. Extract function signatures
3. Create OpenAPI spec

Use the available tools to accomplish this goal. You may adapt your approach as needed, but try to follow the instructions provided.
```

### Plan vs Interactive Mode

| Aspect  | Plan Mode           | Interactive Mode    |
| -------- | ------------------------------ | ----------------------- |
| Input  | Structured file        | Natural language prompt |
| Guidance | Explicit instructions     | Open-ended       |
| Tracking | Can track instruction progress | Free-form conversation |
| Use Case | Repeatable tasks        | Exploratory tasks    |

## Error Handling

```rust
#[derive(Debug, thiserror::Error)]
pub enum XzatomaError {
  #[error("Configuration error: {0}")]
  Config(String),

  #[error("Provider error: {0}")]
  Provider(String),

  #[error("Tool execution error: {0}")]
  Tool(String),

  #[error("Agent exceeded maximum iterations: {limit} (reason: {message})")]
  MaxIterationsExceeded { limit: usize, message: String },

  #[error("Dangerous command blocked: {0}")]
  DangerousCommand(String),

  #[error("Command requires confirmation: {0}")]
  CommandRequiresConfirmation(String),

  #[error("Path outside working directory: {0}")]
  PathOutsideWorkingDirectory(String),

  #[error("Streaming not supported by this provider")]
  StreamingNotSupported,

  #[error("Missing credentials for provider: {0}")]
  MissingCredentials(String),

  #[error("IO error: {0}")]
  Io(#[from] std::io::Error),
}
```

## Security Considerations

### File Operations

**Path Restrictions**:

- All file paths must be within current working directory or subdirectories
- Reject absolute paths starting with `/` unless explicitly allowed
- Reject `..` path traversal beyond working directory root
- Symlinks are followed but final target must be within allowed directory

**Destructive Operations**:

- `write_file`: Requires confirmation in interactive mode when overwriting existing file
- `delete_path`: Always requires confirmation in interactive mode
- Both operations log to audit trail

**Size Limits**:

- `read_file`: Maximum 10 MB (configurable via `agent.tools.max_file_read_size`)
- Tool output: Maximum 1 MB (configurable via `agent.tools.max_output_size`)
- Large content automatically truncated with warning

### Terminal Execution

#### Security Model

**Execution Modes**:

1. **Interactive Mode** (default for `xzatoma chat`)

  - Requires user confirmation before executing each command
  - Shows full command to user
  - User can approve, modify, or reject

2. **Restricted Autonomous Mode** (default for `xzatoma run`)

  - Only safe read-only commands allowed without confirmation
  - Allowlist: `ls`, `cat`, `head`, `tail`, `grep`, `find`, `echo`, `pwd`, `which`, `type`
  - Other commands require confirmation or are blocked

3. **Full Autonomous Mode** (requires `--allow-dangerous` flag)
  - All commands allowed without confirmation
  - Denylist applied for catastrophic commands
  - User must explicitly opt-in with flag

#### Command Validation

**Denylist** (rejected in all modes):

- `rm -rf /` or `rm -rf /*` (system wipe)
- `dd if=/dev/zero` (disk overwrite)
- `mkfs.*` (filesystem formatting)
- `:(){:|:&};:` (fork bomb)
- Commands with `curl | sh`, `wget | sh`, or similar piping to shell
- Commands containing `eval` or `exec` with untrusted input
- `chmod -R 777` (dangerous permissions)
- Commands with `sudo` or `su` (privilege escalation)

**Path Validation**:

- All file paths validated before command execution
- Reject paths outside working directory
- Check both command program and arguments

**Command Parsing**:

```rust
pub struct CommandValidator {
  mode: ExecutionMode,
  working_dir: PathBuf,
  allowlist: HashSet<String>,
  denylist: Vec<Regex>,
}

impl CommandValidator {
  pub fn validate(&self, command: &str) -> Result<ValidatedCommand> {
    // 1. Check against denylist patterns
    for pattern in &self.denylist {
      if pattern.is_match(command) {
        return Err(XzatomaError::DangerousCommand(command.to_string()));
      }
    }

    // 2. Parse command and arguments
    let parsed = self.parse_command(command)?;

    // 3. Validate all paths in command
    self.validate_paths(&parsed)?;

    // 4. Check mode-specific rules
    match self.mode {
      ExecutionMode::Interactive => Ok(parsed), // Always requires confirmation
      ExecutionMode::RestrictedAutonomous => {
        if self.allowlist.contains(&parsed.program) {
          Ok(parsed)
        } else {
          Err(XzatomaError::CommandRequiresConfirmation(command.to_string()))
        }
      }
      ExecutionMode::FullAutonomous => Ok(parsed),
    }
  }

  fn validate_paths(&self, command: &ParsedCommand) -> Result<()> {
    for path in &command.paths {
      let canonical = path.canonicalize()?;
      if !canonical.starts_with(&self.working_dir) {
        return Err(XzatomaError::PathOutsideWorkingDirectory(
          path.to_string_lossy().to_string()
        ));
      }
    }
    Ok(())
  }
}
```

#### Safety Mechanisms

**Timeouts**:

- Default timeout: 30 seconds (configurable via `agent.terminal.timeout_seconds`)
- Kill process tree on timeout
- Prevents hung processes

**Output Limits**:

- Maximum stdout: 10 MB (configurable via `agent.terminal.max_stdout_bytes`)
- Maximum stderr: 1 MB (configurable via `agent.terminal.max_stderr_bytes`)
- Truncate with warning if exceeded

**Audit Trail**:

- All commands logged to `~/.xzatoma/audit.log`
- Format: RFC 3339 timestamp, working directory, command, exit code, duration
- Example: `2025-01-15T10:30:45Z | /home/user/project | ls -la | exit:0 | 0.023s`
- Never truncated, size-limited by log rotation

**Process Isolation**:

- Commands run in separate process group
- No shell expansion (exec directly, not via `/bin/sh -c`)
- Environment variables sanitized (only safe vars passed)
- Working directory enforced

### Credentials

**Storage Backends by Platform**:

| Platform | Backend      | Keyring Implementation      |
| -------- | ------------------ | --------------------------------- |
| macOS  | Keychain      | System Keychain          |
| Linux  | Secret Service   | gnome-keyring, kwallet, keepassxc |
| Windows | Credential Manager | Windows Credential Manager    |

**Storage Strategy**:

1. **First run**: Prompt for credentials
2. **Store**: Attempt to store in system keyring
3. **Fallback**: If keyring unavailable, store in memory for session only
4. **Warning**: If fallback, warn user credentials won't persist

**Environment Variable Override**:

Credentials can be provided via environment variables (bypasses keyring):

```bash
# GitHub Copilot
export GITHUB_TOKEN="ghp_..."

# Ollama (if authentication enabled)
export OLLAMA_API_KEY="..."
```

**Security Best Practices**:

- Never log credentials (sanitize logs)
- Never include credentials in error messages
- Never write credentials to config file
- Clear credentials from memory on exit
- Use secure string types where available

## Dependencies

### Core

- `clap` - CLI parsing
- `tokio` - Async runtime
- `serde`, `serde_json`, `serde_yaml` - Serialization
- `anyhow`, `thiserror` - Error handling
- `tracing` - Logging

### Providers

- `reqwest` - HTTP client
- `async-trait` - Async traits
- `keyring` - Credential storage

### Tools

- `walkdir` - Directory traversal
- `similar` - Diff generation

## Example: How a Task is Executed

User request: "Find all TODO comments in Rust files and create a tasks.md file"

```
1. User: "Find all TODO comments..."

2. Agent → AI: [conversation context + available tools]

3. AI → Agent: Call tool: list_files { path: ".", pattern: "*.rs" }

4. Agent: Executes list_files, gets: ["main.rs", "lib.rs", "agent.rs"]

5. Agent → AI: [tool result]

6. AI → Agent: Call tool: read_file { path: "main.rs" }

7. Agent: Executes read_file, gets file content

8. Agent → AI: [tool result]

9. AI → Agent: Call tool: read_file { path: "lib.rs" }

  ... (repeats for each file)

10. Agent → AI: [all tool results]

11. AI → Agent: Call tool: write_file {
    path: "tasks.md",
    content: "# TODO Items\n\n- main.rs:42: Fix error handling\n..."
  }

12. Agent: Executes write_file

13. Agent → AI: [tool result: "File written successfully"]

14. AI → Agent: "I found 5 TODO comments and created tasks.md"

15. Agent → User: "I found 5 TODO comments and created tasks.md"
```

## Testing Strategy

### Unit Tests

- Tool execution logic
- Provider message formatting
- Configuration loading
- Plan parsing

### Integration Tests

- Agent execution loop with mock provider
- Tool chaining
- Error handling

### Mock Provider

```rust
pub struct MockProvider {
  responses: VecDeque<Response>,
}

impl Provider for MockProvider {
  async fn complete(&self, messages: &[Message], tools: &[Tool]) -> Result<Response> {
    Ok(self.responses.pop_front().unwrap())
  }
}
```

## Deployment

- Single static binary
- No external dependencies (except AI provider)
- Configuration via file or environment
- Works offline with local Ollama

## Future Considerations

### Possible Extensions

- Additional tools (HTTP requests, database queries)
- Plugin system for custom tools
- Web interface for monitoring
- Team/collaborative features
- Workflow templates

### Not Core Features

- Specialized domain tools (Git, Docker, etc.) - agent uses terminal
- Documentation generators - agent uses file tools
- Code analysis - agent reads files
- Repository scanning - agent uses list_files

The philosophy is: keep the agent generic, let the AI figure out how to accomplish tasks using basic building blocks.

## Comparison to Zed Agent

XZatoma is similar to Zed's agent chat but:

- CLI-based instead of editor-integrated
- Supports structured plans (JSON/YAML/Markdown)
- Can run autonomously without human in loop
- Focused on file system and terminal operations

## Conclusion

XZatoma is intentionally simple:

- Agent with conversation loop
- Two AI providers (Copilot, Ollama)
- Basic file and terminal tools
- Plan files for structured tasks
- That's it

The power comes from the AI's ability to use these simple tools creatively to accomplish complex tasks, not from building specialized features into XZatoma itself.
