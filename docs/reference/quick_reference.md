# XZatoma Quick Reference

## Project Commands

### Development

```bash
# Build project
cargo build

# Build release
cargo build --release

# Run tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_name

# Check without building
cargo check

# Format code
cargo fmt

# Lint code
cargo clippy

# Generate documentation
cargo doc --open

# Run with logging
RUST_LOG=debug cargo run
```

### Quality Gates

All checks must pass before claiming any task complete. Run in this order:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

All Markdown files must also pass linting and formatting:

```bash
markdownlint --fix --config .markdownlint.json "${FILE}"
prettier --write --parser markdown --prose-wrap always "${FILE}"
```

## Project Structure

```text
src/
├── main.rs              # Entry point
├── lib.rs               # Library root
├── cli.rs               # CLI parsing and user interface
├── config.rs            # Configuration management
├── error.rs             # Error types and conversions
├── chat_mode.rs         # Chat mode logic
├── mention_parser.rs    # Context mention (@file:, @search:, @grep:, @url:) parsing
├── test_utils.rs        # Test utilities
├── prompts/             # Prompt templates
├── agent/               # Autonomous execution loop
│   ├── mod.rs
│   ├── agent.rs         # Main agent logic
│   ├── conversation.rs  # Message history
│   └── executor.rs      # Tool execution
├── providers/           # AI provider abstraction
│   ├── mod.rs
│   ├── base.rs          # Provider trait
│   ├── copilot.rs       # GitHub Copilot
│   └── ollama.rs        # Ollama
├── tools/               # Agent tools
│   ├── mod.rs           # ToolExecutor trait and ToolRegistry
│   ├── file_ops.rs      # File operations
│   ├── terminal.rs      # Terminal execution
│   ├── plan.rs          # Plan parsing
│   ├── grep.rs          # Grep / search
│   └── fetch.rs         # HTTP fetch
├── commands/            # Command handlers
│   ├── chat.rs          # Interactive chat
│   ├── run.rs           # Plan / prompt execution
│   ├── watch.rs         # Kafka event watcher
│   ├── auth.rs          # Provider authentication
│   ├── models.rs        # Model management
│   ├── history.rs       # Conversation history
│   ├── replay.rs        # Subagent replay
│   ├── mcp.rs           # MCP server listing
│   ├── acp.rs           # ACP server management
│   └── skills.rs        # Skills management
├── mcp/                 # Model Context Protocol
│   ├── client.rs        # MCP client
│   ├── server.rs        # MCP server
│   ├── transport.rs     # Transport layer
│   ├── auth.rs          # MCP authentication
│   ├── tool_bridge.rs   # Tool bridging
│   ├── sampling.rs      # Sampling support
│   ├── elicitation.rs   # Elicitation support
│   ├── protocol.rs      # Protocol definitions
│   └── task_manager.rs  # Task management
├── acp/                 # Agent Communication Protocol
│   ├── server.rs        # ACP server
│   ├── runtime.rs       # ACP runtime
│   ├── handlers.rs      # Request handlers
│   ├── routes.rs        # HTTP routes
│   ├── events.rs        # Event system
│   ├── session.rs       # Session management
│   ├── streaming.rs     # Streaming support
│   └── manifest.rs      # Agent manifest
├── skills/              # Skill system
│   ├── discovery.rs     # Skill discovery
│   ├── parsing.rs       # Skill parsing
│   ├── activation.rs    # Skill activation
│   ├── trust.rs         # Trust management
│   ├── validation.rs    # Skill validation
│   └── catalog.rs       # Skill catalog
├── storage/             # Persistence layer
│   └── types.rs         # Storage types
├── watcher/             # Kafka-backed event watcher
│   ├── logging.rs       # Watcher logging
│   ├── generic/         # Generic plan-event watcher
│   └── xzepr/           # XZepr CloudEvents watcher
└── xzepr/               # Backward-compatible shim
```

## Module Organization

### Core Modules

| Module           | Responsibility                                    |
| ---------------- | ------------------------------------------------- |
| `cli`            | Command-line interface (clap)                     |
| `config`         | Configuration management (serde)                  |
| `error`          | Error types and conversions (thiserror)           |
| `chat_mode`      | Interactive chat logic                            |
| `mention_parser` | Context mention parsing (@file:, @search:, @url:) |
| `agent`          | Autonomous agent execution loop                   |
| `providers`      | AI provider abstraction (Copilot, Ollama)         |
| `tools`          | File ops, grep, terminal, plan, fetch             |
| `commands`       | CLI command handlers                              |
| `mcp`            | Model Context Protocol client and server          |
| `acp`            | Agent Communication Protocol server               |
| `skills`         | Skill discovery, trust, and activation            |
| `storage`        | Persistence layer                                 |
| `watcher`        | Kafka-backed event watcher                        |

### Module Dependencies

Permitted dependencies:

- `agent/` may call `providers/`, `tools/`, and `config`
- `providers/` may call `config`
- `commands/` may call `agent/`, `providers/`, `tools/`, `mcp/`, `acp/`,
  `skills/`, `storage/`, and `watcher/`
- `tools/` are independent (no cross-dependencies)
- All modules may use `error`

Forbidden dependencies:

- `providers/` must never import from `agent/` or `tools/`
- `tools/` must never import from `agent/` or `providers/`
- `config` must never import from `agent/`, `providers/`, or `tools/`
- No circular dependencies between modules

```text
main -> cli -> commands -> agent -> providers
                  |          |
                  |          +----> tools
                  |
                  +------> mcp, acp, skills, storage, watcher
                  |
                  +------> config (shared by all)
```

## Key Traits

### Provider Trait

Defined in `src/providers/trait_mod.rs` with shared provider types in
`src/providers/types.rs`:

```rust
pub trait Provider: Send + Sync {
    /// Complete a conversation with the given messages and available tools.
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[serde_json::Value],
    ) -> Result<CompletionResponse>;

    /// List available models for this provider.
    async fn list_models(&self) -> Result<Vec<ModelInfo>>;

    /// Get detailed information about a specific model.
    async fn get_model_info(&self, model_name: &str) -> Result<ModelInfo>;

    /// Get the name of the currently active model.
    fn get_current_model(&self) -> Result<String>;

    /// Get the capabilities of this provider.
    fn get_provider_capabilities(&self) -> ProviderCapabilities;

    /// Change the active model (if supported).
    async fn set_model(&mut self, model_name: String) -> Result<()>;
}
```

### ToolExecutor Trait

Defined in `src/tools/mod.rs`:

```rust
pub trait ToolExecutor: Send + Sync {
    /// Returns the tool definition as a JSON value (OpenAI function calling format).
    fn tool_definition(&self) -> serde_json::Value;

    /// Executes the tool with the given arguments.
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult>;
}
```

## Configuration

Configuration file location: `~/.config/xzatoma/config.yaml`

```yaml
provider:
  type: copilot # or 'ollama'
  copilot:
    model: gpt-5.3-codex
  ollama:
    host: http://localhost:11434
    model: llama3.2:3b

agent:
  max_turns: 50
  timeout_seconds: 300

mcp:
  auto_connect: true
  request_timeout_seconds: 30

skills:
  enabled: true

watcher:
  watcher_type: xzepr
  kafka:
    brokers: localhost:9092
    topic: xzepr.events
```

## Environment Variables

```bash
# Provider settings
XZATOMA_PROVIDER=copilot
XZATOMA_COPILOT_MODEL=gpt-5.3-codex
XZATOMA_OLLAMA_HOST=http://localhost:11434
XZATOMA_OLLAMA_MODEL=llama3.2:3b

# Agent settings
XZATOMA_MAX_TURNS=50
XZATOMA_TIMEOUT_SECONDS=300
XZATOMA_EXECUTION_MODE=interactive

# MCP settings
XZATOMA_MCP_AUTO_CONNECT=true
XZATOMA_MCP_REQUEST_TIMEOUT=30

# Logging
RUST_LOG=info
```

## CLI Commands

### Authentication

```bash
# Authenticate with GitHub Copilot
xzatoma auth --provider copilot

# Authenticate with Ollama
xzatoma auth --provider ollama
```

### Interactive Chat

```bash
# Start interactive chat
xzatoma chat

# Chat with a specific provider
xzatoma chat --provider ollama

# Chat in planning mode (no writes)
xzatoma chat --mode planning

# Chat in write mode
xzatoma chat --mode write

# Chat in safe mode (dangerous commands blocked)
xzatoma chat --safe

# Resume a previous conversation
xzatoma chat --resume
```

### Plan and Prompt Execution

```bash
# Execute a plan file
xzatoma run --plan plan.yaml

# Execute a prompt directly
xzatoma run --prompt "Refactor the error module"

# Allow dangerous commands during execution
xzatoma run --plan plan.yaml --allow-dangerous
```

### Event Watching

```bash
# Watch Kafka for events
xzatoma watch --topic xzepr.events

# Watch with specific watcher type
xzatoma watch --watcher-type xzepr --topic xzepr.events

# Watch with custom brokers
xzatoma watch --brokers kafka1:9092,kafka2:9092 --topic xzepr.events

# Dry run (no execution)
xzatoma watch --topic xzepr.events --dry-run

# Watch with action and name filters
xzatoma watch --topic xzepr.events --action build --name my-project

# Watch with version matching
xzatoma watch --topic xzepr.events --match-version "1.0.*"
```

### Model Management

```bash
# List available models
xzatoma models list

# Get information about a specific model
xzatoma models info

# Show current model
xzatoma models current
```

### Conversation History

```bash
# List conversation history
xzatoma history list

# Show a specific conversation
xzatoma history show

# Delete a conversation
xzatoma history delete
```

### Replay

```bash
# Replay a subagent conversation
xzatoma replay
```

### MCP Server Management

```bash
# List configured MCP servers
xzatoma mcp list
```

### ACP Server Management

```bash
# Start ACP server
xzatoma acp serve

# Show ACP configuration
xzatoma acp config

# List ACP runs
xzatoma acp runs

# Validate ACP manifest
xzatoma acp validate
```

### Skills Management

```bash
# List available skills
xzatoma skills list

# Validate a skill definition
xzatoma skills validate

# Show skill details
xzatoma skills show

# Show skill search paths
xzatoma skills paths

# Manage skill trust
xzatoma skills trust
```

## Error Handling

### Error Types

All errors are variants of `XzatomaError` defined in `src/error.rs`:

```text
XzatomaError
├── Config(String)                  # Configuration errors
├── Provider(String)                # Provider / API errors
├── Tool(String)                    # Tool execution errors
├── Watcher(String)                 # Kafka watcher errors
├── Command(String)                 # Command execution errors
├── Fetch(String)                   # HTTP fetch errors
├── MentionParse(String)            # Mention syntax errors
├── FileLoad(String)                # File read errors
├── Search(String)                  # Grep / search errors
├── RateLimitExceeded { limit, message }
├── MaxIterationsExceeded { limit, message }
├── DangerousCommand(String)        # Blocked dangerous command
├── CommandRequiresConfirmation(String)
├── PathOutsideWorkingDirectory(String)
├── StreamingNotSupported(String)
├── MissingCredentials(String)
├── Authentication(String)
├── Io(std::io::Error)
├── Serialization(serde_json::Error)
├── Yaml(serde_yaml::Error)
├── Http(reqwest::Error)
├── Regex(regex::Error)
├── Keyring(String)
├── Storage(String)                 # Persistence errors
├── QuotaExceeded(String)
├── Internal(String)
├── Mcp(String)                     # MCP protocol errors
├── McpTransport(String)
├── McpServerNotFound(String)
├── McpToolNotFound { server, tool }
├── McpProtocolVersion { expected, got }
├── McpTimeout { server, method }
├── McpAuth(String)
├── McpElicitation(String)
├── McpTask(String)
└── Acp(String)                     # ACP errors
```

### Error Handling Pattern

```rust
use anyhow::{Context, Result};

pub fn load_config(path: &Path) -> Result<Config> {
    let content = fs::read_to_string(path)
        .context("Failed to read config file")?;

    let config: Config = serde_yaml::from_str(&content)
        .context("Failed to parse config")?;

    config.validate()
        .context("Config validation failed")?;

    Ok(config)
}
```

Rules:

- Use `Result<T, E>` for all recoverable errors
- Use `?` for error propagation
- Use `thiserror` for custom error types
- Never use `unwrap()` or `expect()` without a justification comment
- Never ignore errors with `let _ =`
- Never use `panic!` for recoverable errors

## Testing

### Unit Test Example

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config_with_valid_yaml() {
        let result = parse_config("key: value");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().key, "value");
    }

    #[test]
    fn test_parse_config_with_invalid_yaml() {
        let result = parse_config("invalid: : yaml");
        assert!(result.is_err());
    }
}
```

### Async Test Example

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_provider_completion() {
        let provider = MockProvider::new();
        let result = provider.complete(&[], &[]).await;
        assert!(result.is_ok());
    }
}
```

### Test Naming Convention

Use descriptive names: `test_<function>_<condition>_<expected>`

```rust
#[test]
fn test_load_config_with_missing_file_returns_error() { /* ... */ }

#[test]
fn test_load_config_with_empty_yaml_returns_default() { /* ... */ }

#[test]
fn test_tool_execute_with_invalid_args_returns_tool_error() { /* ... */ }
```

### Running Tests

```bash
# All tests
cargo test --all-features

# Specific module
cargo test --all-features agent::

# With output
cargo test --all-features -- --nocapture

# Single test
cargo test --all-features test_parse_config_with_valid_yaml
```

## Documentation Categories (Diataxis)

| Directory           | Purpose                                        | Examples                  |
| ------------------- | ---------------------------------------------- | ------------------------- |
| `docs/tutorials/`   | Learning-oriented, step-by-step lessons        | `getting_started.md`      |
| `docs/how-to/`      | Task-oriented, problem-solving recipes         | `setup_monitoring.md`     |
| `docs/explanation/` | Understanding-oriented, conceptual discussion  | `agent_implementation.md` |
| `docs/reference/`   | Information-oriented, technical specifications | `quick_reference.md`      |

Implementation summaries created by AI agents belong in `docs/explanation/`.

## Debugging

### Enable Debug Logging

```bash
# All debug logs
RUST_LOG=debug cargo run

# Specific module
RUST_LOG=xzatoma::agent=debug cargo run

# Multiple modules
RUST_LOG=xzatoma::agent=debug,xzatoma::providers=trace cargo run

# Trace-level logging for MCP
RUST_LOG=xzatoma::mcp=trace cargo run

# Info for everything, debug for agent
RUST_LOG=info,xzatoma::agent=debug cargo run
```

### Common Issues

| Problem          | Solution                                          |
| ---------------- | ------------------------------------------------- |
| Build errors     | `cargo clean && cargo build`                      |
| Test failures    | `cargo test --all-features -- --nocapture`        |
| Clippy warnings  | `cargo clippy --fix --all-targets --all-features` |
| Format issues    | `cargo fmt --all`                                 |
| Auth failures    | `xzatoma auth --provider copilot`                 |
| Kafka connection | Check `--brokers` and topic configuration         |

## Dependencies

### Core

| Crate                | Purpose                         |
| -------------------- | ------------------------------- |
| `clap`               | CLI parsing (derive mode)       |
| `tokio`              | Async runtime (full)            |
| `serde`              | Serialization / deserialization |
| `serde_json`         | JSON support                    |
| `serde_yaml`         | YAML configuration parsing      |
| `anyhow`             | Application error handling      |
| `thiserror`          | Custom error derives            |
| `tracing`            | Structured logging              |
| `tracing-subscriber` | Log output and filtering        |

### Networking

| Crate         | Purpose                  |
| ------------- | ------------------------ |
| `reqwest`     | HTTP client (rustls-tls) |
| `axum`        | HTTP server (ACP)        |
| `tower`       | Middleware utilities     |
| `async-trait` | Async trait support      |

### Security

| Crate     | Purpose               |
| --------- | --------------------- |
| `keyring` | OS credential storage |
| `sha2`    | SHA-256 hashing       |

### File Operations

| Crate     | Purpose               |
| --------- | --------------------- |
| `walkdir` | Directory traversal   |
| `ignore`  | Gitignore support     |
| `similar` | Diff computation      |
| `strsim`  | Fuzzy string matching |
| `glob`    | Glob pattern matching |

### Infrastructure

| Crate       | Purpose                    |
| ----------- | -------------------------- |
| `rdkafka`   | Kafka client (watcher)     |
| `rusqlite`  | SQLite persistence         |
| `sled`      | Embedded key-value store   |
| `chrono`    | Date and time handling     |
| `uuid`      | UUID generation            |
| `ulid`      | Sortable unique IDs        |
| `regex`     | Regular expression support |
| `rustyline` | Interactive line editor    |
| `colored`   | Terminal color output      |
| `image`     | Image decoding             |
| `base64`    | Base64 encoding            |
| `url`       | URL parsing                |

### Dev Dependencies

| Crate         | Purpose               |
| ------------- | --------------------- |
| `mockall`     | Mock generation       |
| `tempfile`    | Temporary files       |
| `tokio-test`  | Async test utilities  |
| `wiremock`    | HTTP mocking          |
| `assert_cmd`  | CLI integration tests |
| `predicates`  | Test assertions       |
| `serial_test` | Serial test execution |

---

**Version**: 0.2.0 **Last Updated**: 2026-03-30 **Maintained By**: XZatoma
Development Team
