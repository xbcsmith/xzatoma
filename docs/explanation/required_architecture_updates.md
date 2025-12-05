# Required Architecture Updates

## Overview

This document specifies the exact changes needed to docs/reference/architecture.md before proceeding with implementation. These updates address critical issues identified in the architecture validation review.

## Critical Updates (MUST Address Before Phase 1)

### Update 1: Add Iteration Limits to Agent Example

**Location**: Core Components → Agent Core → Architecture Pattern

**Current Code**:
```rust
impl Agent {
    pub async fn execute(&mut self, instruction: String) -> Result<String> {
        self.conversation.add_user_message(instruction);

        loop {
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

**Required Change**: Replace with:

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

**Also Add to Error Handling Section**:

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

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

### Update 2: Add Terminal Execution Security Section

**Location**: After Security Considerations → Terminal Execution

**Current Content**:
```markdown
### Terminal Execution

- Require confirmation for all commands (optional flag)
- Log all executed commands
- Timeout for long-running commands
```

**Required Change**: Replace with comprehensive section:

```markdown
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
   - Other commands require confirmation

3. **Full Autonomous Mode** (requires `--allow-dangerous` flag)
   - All commands allowed without confirmation
   - Denylist applied for catastrophic commands
   - User must explicitly opt-in with flag

#### Command Validation

**Denylist** (rejected in all modes):
- `rm -rf /` or `rm -rf /*`
- `dd if=/dev/zero`
- `mkfs.*`
- `:(){:|:&};:` (fork bomb)
- Commands with `curl | sh`, `wget | sh`, or similar piping to shell
- Commands containing `eval` or `exec` with untrusted input

**Path Validation**:
- All file paths must be within current working directory or subdirectories
- Reject absolute paths starting with `/` unless explicitly allowed
- Reject `..` path traversal beyond working directory root
- Symlinks are followed but final target must be within allowed directory

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

        // 3. Check mode-specific rules
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
- Default timeout: 30 seconds
- Configurable via `agent.command_timeout_seconds` in config
- Kill process tree on timeout

**Output Limits**:
- Maximum stdout: 10 MB (configurable)
- Maximum stderr: 1 MB (configurable)
- Truncate with warning if exceeded

**Audit Trail**:
- All commands logged to `~/.xzatoma/audit.log`
- Format: ISO 8601 timestamp, working directory, command, exit code, duration
- Example: `2025-01-15T10:30:45Z | /home/user/project | ls -la | exit:0 | 0.023s`

**Process Isolation**:
- Commands run in separate process group
- No shell expansion (exec directly, not via `/bin/sh -c`)
- Environment variables sanitized (only safe vars passed)

#### Configuration

```yaml
agent:
  terminal:
    default_mode: restricted_autonomous  # interactive | restricted_autonomous | full_autonomous
    timeout_seconds: 30
    max_stdout_bytes: 10485760  # 10 MB
    max_stderr_bytes: 1048576   # 1 MB
    allowlist:
      - ls
      - cat
      - grep
      - find
    custom_denylist:
      - "rm -rf"
      - "sudo"
```
```

### Update 3: Add Conversation Management Section

**Location**: After Agent Core section, before Provider Abstraction

**Add New Section**:

```markdown
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

| Provider | Model | Context Window | Safe Limit (80%) |
|----------|-------|----------------|------------------|
| Copilot  | gpt-4o | 128,000 tokens | 102,400 tokens |
| Copilot  | gpt-4o-mini | 128,000 tokens | 102,400 tokens |
| Ollama   | qwen3 | 32,768 tokens | 26,214 tokens |
| Ollama   | llama3 | 8,192 tokens | 6,553 tokens |

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
  conversation:
    max_tokens: 100000  # Provider-specific limit
    min_retain_turns: 5  # Always keep last N turns
    prune_threshold: 0.8  # Prune when 80% of max_tokens
```
```

## Medium Priority Updates (Before Phase 2)

### Update 4: Expand Provider Trait

**Location**: Provider Abstraction → Provider Trait

**Add After Current Trait**:

```markdown
**Extended Provider Trait** (for future phases):

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    /// Non-streaming completion (required)
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<Response>;

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

**Note**: Phase 1 implementation can use simplified trait. Streaming support added in Phase 3.
```

### Update 5: Define Structured Tool Results

**Location**: Basic Tools → Tool Definition

**Add After ToolExecutor Trait**:

```markdown
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
            self.output.truncate(max_size);
            self.output.push_str("\n\n... (output truncated)");
            self.truncated = true;
            self.metadata.insert(
                "original_size".to_string(),
                self.output.len().to_string()
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

**Tool Execution Size Limits**:

```yaml
agent:
  tools:
    max_output_size: 1048576  # 1 MB per tool result
    max_file_read_size: 10485760  # 10 MB for read_file
```
```

### Update 6: Add Plan Execution Strategy

**Location**: After Plan File Format section

**Add New Section**:

```markdown
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

| Aspect | Plan Mode | Interactive Mode |
|--------|-----------|------------------|
| Input | Structured file | Natural language prompt |
| Guidance | Explicit instructions | Open-ended |
| Tracking | Can track instruction progress | Free-form conversation |
| Use Case | Repeatable tasks | Exploratory tasks |

### Plan Instruction Tracking

**Optional Enhancement** (not in Phase 1):

```rust
pub struct PlanExecution {
    plan: Plan,
    completed_instructions: HashSet<usize>,
}

impl PlanExecution {
    pub fn mark_instruction_complete(&mut self, index: usize) {
        self.completed_instructions.insert(index);
    }

    pub fn progress_summary(&self) -> String {
        format!(
            "Completed {}/{} instructions",
            self.completed_instructions.len(),
            self.plan.instructions.len()
        )
    }
}
```

This allows the agent to report progress like:
```
Completed 2/4 instructions:
✓ List all API endpoint files
✓ Extract function signatures
⧗ Create OpenAPI spec (in progress)
○ Generate documentation website
```
```

## Low Priority Updates (Can Address During Implementation)

### Update 7: Document Configuration Precedence

**Location**: Configuration → After Environment Variables

**Add Section**:

```markdown
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
xzatoma --provider copilot  # Uses copilot (CLI wins)
```
```

### Update 8: Clarify Agent Module Responsibilities

**Location**: Module Structure → Add subsection after directory tree

**Add Section**:

```markdown
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
```

### Update 9: Add File Operations Details

**Location**: Basic Tools → File Operations

**Expand Current List**:

```markdown
**File Operations** (detailed):

- `list_files` - List files in directory
  - Parameters: `path` (string), `pattern` (optional glob), `recursive` (bool)
  - Pattern syntax: Standard glob (`*.rs`, `**/*.md`, `src/**/*.{rs,toml}`)
  - Returns: JSON array of file paths
  - Example: `["src/main.rs", "src/lib.rs"]`

- `read_file` - Read file content
  - Parameters: `path` (string)
  - Returns: File contents as string
  - Limit: 10 MB (configurable)
  - Large files truncated with warning

- `write_file` - Create or overwrite file
  - Parameters: `path` (string), `content` (string)
  - Creates parent directories if needed
  - Returns: Success message with file size
  - Safety: Requires confirmation in interactive mode

- `create_directory` - Create directory
  - Parameters: `path` (string)
  - Creates parent directories (like `mkdir -p`)
  - Returns: Success message

- `delete_path` - Delete file or directory
  - Parameters: `path` (string), `recursive` (bool)
  - Safety: Always requires confirmation in interactive mode
  - Safety: Rejected if path outside working directory
  - Returns: Success message with deleted item count

- `diff_files` - Show diff between two files
  - Parameters: `path1` (string), `path2` (string), `context_lines` (int, default 3)
  - Returns: Unified diff format
  - Uses `similar` crate for diff generation
```

### Update 10: Add Credential Storage Details

**Location**: Security Considerations → Credentials

**Expand Section**:

```markdown
### Credentials

**Storage Backends by Platform**:

| Platform | Backend | Keyring Implementation |
|----------|---------|------------------------|
| macOS | Keychain | System Keychain |
| Linux | Secret Service | gnome-keyring, kwallet, keepassxc |
| Windows | Credential Manager | Windows Credential Manager |

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

**Credential Retrieval**:

```rust
pub async fn get_provider_credentials(provider: &str) -> Result<Credentials> {
    // 1. Check environment variables
    if let Ok(token) = env::var("GITHUB_TOKEN") {
        return Ok(Credentials::new(token));
    }

    // 2. Try keyring
    match keyring::Entry::new("xzatoma", provider) {
        Ok(entry) => {
            if let Ok(password) = entry.get_password() {
                return Ok(Credentials::new(password));
            }
        }
        Err(e) => {
            tracing::warn!("Keyring unavailable: {}", e);
        }
    }

    // 3. Prompt user (interactive mode only)
    if is_interactive() {
        let token = prompt_for_token(provider)?;

        // Try to save for next time
        if let Ok(entry) = keyring::Entry::new("xzatoma", provider) {
            let _ = entry.set_password(&token); // Ignore errors
        }

        return Ok(Credentials::new(token));
    }

    Err(XzatomaError::MissingCredentials(provider.to_string()))
}
```
```

## Summary of Required Changes

### Critical (Must Have Before Phase 1)
1. ✅ Add iteration limits to Agent example
2. ✅ Add comprehensive terminal security section
3. ✅ Add conversation management section

### Important (Should Have Before Phase 2)
4. ✅ Expand Provider trait for future features
5. ✅ Define structured ToolResult format
6. ✅ Add plan execution strategy section

### Nice to Have (During Implementation)
7. ✅ Document configuration precedence
8. ✅ Clarify agent module responsibilities
9. ✅ Expand file operations details
10. ✅ Add credential storage details

## Implementation Note

These updates should be incorporated into docs/reference/architecture.md before creating the phased implementation plan. The critical updates (1-3) are required for a safe and functional implementation.
