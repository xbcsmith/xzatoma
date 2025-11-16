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

# Fix clippy warnings
cargo clippy --fix

# Generate documentation
cargo doc --open

# Run with logging
RUST_LOG=debug cargo run
```

### Quality Gates

```bash
# All checks must pass before commit
cargo fmt --check
cargo clippy -- -D warnings
cargo test --all-features
cargo build --release
```

## Project Structure

```
xzatoma/
├── src/
│   ├── main.rs              # Binary entry point
│   ├── lib.rs               # Library root
│   ├── cli.rs               # CLI parser
│   ├── config.rs            # Configuration
│   ├── error.rs             # Error types
│   ├── agent/               # Agent core
│   ├── providers/           # AI providers
│   ├── workflow/            # Workflow engine
│   ├── repository/          # Repository ops
│   ├── docgen/              # Doc generation
│   └── tools/               # Agent tools
├── tests/
│   ├── unit/                # Unit tests
│   ├── integration/         # Integration tests
│   └── fixtures/            # Test fixtures
├── docs/
│   ├── tutorials/           # Learning-oriented
│   ├── how_to/              # Task-oriented
│   ├── explanation/         # Understanding-oriented
│   └── reference/           # Information-oriented
└── examples/                # Usage examples
```

## Module Organization

### Core Modules

- `cli` - Command-line interface
- `config` - Configuration management
- `error` - Error types and handling
- `agent` - Autonomous agent logic
- `providers` - AI provider abstraction
- `workflow` - Workflow execution
- `repository` - Repository analysis
- `docgen` - Documentation generation
- `tools` - Agent tools

### Module Dependencies

```
main → cli → agent → provider
       ↓      ↓       ↓
    config  workflow tools
              ↓       ↓
          repository docgen
```

## Key Traits

### Provider Trait

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn metadata() -> ProviderMetadata;
    fn get_name(&self) -> &str;
    async fn complete(
        &self,
        messages: Vec<Message>,
        tools: Vec<Tool>,
    ) -> Result<Message, ProviderError>;
}
```

### Tool Executor Trait

```rust
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, params: Value) -> Result<ToolResult>;
}
```

## Configuration

### Configuration File

```yaml
# ~/.config/xzatoma/config.yaml

provider:
  type: copilot  # or 'ollama'
  copilot:
    model: gpt-4o
  ollama:
    host: localhost:11434
    model: qwen3

agent:
  max_turns: 50
  timeout_seconds: 600
  retry_attempts: 3

repository:
  clone_depth: 1
  ignore_patterns:
    - node_modules
    - target
    - .git

documentation:
  output_dir: docs
  categories:
    - tutorials
    - how_to
    - explanation
    - reference
```

### Environment Variables

```bash
# Provider settings
export XZATOMA_PROVIDER=copilot
export COPILOT_MODEL=gpt-4o
export OLLAMA_HOST=localhost:11434
export OLLAMA_MODEL=qwen3

# Agent settings
export XZATOMA_MAX_TURNS=50
export XZATOMA_TIMEOUT=600

# Logging
export RUST_LOG=info
export RUST_LOG=xzatoma=debug
```

## CLI Commands

### Authentication

```bash
# Authenticate with GitHub Copilot
xzatoma auth --provider copilot

# Authenticate with Ollama (if needed)
xzatoma auth --provider ollama
```

### Workflow Execution

```bash
# Run workflow from file
xzatoma run --plan workflow.yaml

# Run with specific provider
xzatoma run --plan workflow.yaml --provider ollama

# Run with verbose logging
RUST_LOG=debug xzatoma run --plan workflow.yaml
```

### Repository Analysis

```bash
# Scan repository
xzatoma scan --repository /path/to/repo

# Scan and output JSON
xzatoma scan --repository /path/to/repo --format json
```

### Documentation Generation

```bash
# Generate all documentation
xzatoma generate --repository /path/to/repo

# Generate specific category
xzatoma generate --repository /path/to/repo --category tutorial

# Generate with custom output
xzatoma generate --repository /path/to/repo --output docs/
```

## Workflow File Format

### YAML Format

```yaml
name: Generate Documentation
description: Analyze and document repository

repository: git@github.com:user/project.git

steps:
  - id: scan
    description: Scan repository
    action: scan_repository
    
  - id: analyze
    description: Analyze code
    action: analyze_code
    dependencies: [scan]
    
  - id: generate
    description: Generate docs
    action: generate_documentation
    category: reference
    dependencies: [analyze]

deliverables:
  - docs/reference/api.md
  - docs/explanation/implementation.md
```

### JSON Format

```json
{
  "name": "Generate Documentation",
  "description": "Analyze and document repository",
  "repository": "git@github.com:user/project.git",
  "steps": [
    {
      "id": "scan",
      "description": "Scan repository",
      "action": "scan_repository"
    },
    {
      "id": "analyze",
      "description": "Analyze code",
      "action": "analyze_code",
      "dependencies": ["scan"]
    }
  ],
  "deliverables": [
    "docs/reference/api.md"
  ]
}
```

## Error Handling

### Error Types

```rust
XzatomaError
├── Config(ConfigError)
├── Provider(ProviderError)
├── Workflow(WorkflowError)
├── Repository(RepositoryError)
└── Io(std::io::Error)
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

## Testing

### Unit Test Example

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_loading() {
        let config = Config::default();
        assert_eq!(config.agent.max_turns, 50);
    }

    #[tokio::test]
    async fn test_provider_completion() {
        let provider = MockProvider::new();
        let result = provider.complete(vec![], vec![]).await;
        assert!(result.is_ok());
    }
}
```

### Integration Test Example

```rust
#[tokio::test]
async fn test_workflow_execution() {
    let plan = Plan::from_yaml(EXAMPLE_PLAN)?;
    let executor = WorkflowExecutor::new(plan, agent);
    
    let result = executor.execute().await?;
    
    assert_eq!(result.steps_completed, 3);
    assert!(result.deliverables.contains(&"docs/api.md"));
}
```

## Documentation Categories (Diataxis)

### Tutorial
- **Purpose**: Learning-oriented
- **Location**: `docs/tutorials/`
- **Example**: `quickstart.md`
- **Content**: Step-by-step lessons

### How-To Guide
- **Purpose**: Task-oriented
- **Location**: `docs/how_to/`
- **Example**: `configure_providers.md`
- **Content**: Problem-solving guides

### Explanation
- **Purpose**: Understanding-oriented
- **Location**: `docs/explanation/`
- **Example**: `implementation_plan.md`
- **Content**: Conceptual discussion

### Reference
- **Purpose**: Information-oriented
- **Location**: `docs/reference/`
- **Example**: `architecture.md`
- **Content**: Technical specifications

## Common Patterns

### Provider Implementation

```rust
pub struct MyProvider {
    client: Client,
    model: ModelConfig,
    name: String,
}

#[async_trait]
impl Provider for MyProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            name: "my-provider".to_string(),
            display_name: "My Provider".to_string(),
            // ...
        }
    }
    
    async fn complete(
        &self,
        messages: Vec<Message>,
        tools: Vec<Tool>,
    ) -> Result<Message, ProviderError> {
        // Implementation
    }
}
```

### Tool Implementation

```rust
pub struct MyTool;

#[async_trait]
impl ToolExecutor for MyTool {
    async fn execute(&self, params: Value) -> Result<ToolResult> {
        // Parse parameters
        let args: MyArgs = serde_json::from_value(params)?;
        
        // Execute logic
        let result = self.do_work(&args).await?;
        
        // Return result
        Ok(ToolResult {
            success: true,
            data: serde_json::to_value(result)?,
        })
    }
}
```

## Debugging

### Enable Debug Logging

```bash
# All debug logs
RUST_LOG=debug cargo run

# Specific module
RUST_LOG=xzatoma::agent=debug cargo run

# Multiple modules
RUST_LOG=xzatoma::agent=debug,xzatoma::provider=trace cargo run
```

### Common Issues

1. **Build Errors**: Run `cargo clean && cargo build`
2. **Test Failures**: Check with `cargo test -- --nocapture`
3. **Clippy Warnings**: Fix with `cargo clippy --fix`
4. **Format Issues**: Run `cargo fmt`

## Performance Optimization

### Release Build

```bash
# Optimized release build
cargo build --release

# Profile-guided optimization
cargo build --release --profile pgo
```

### Profiling

```bash
# Install flamegraph
cargo install flamegraph

# Profile application
cargo flamegraph --bin xzatoma
```

## Dependencies

### Core Dependencies

- `clap` - CLI parsing
- `tokio` - Async runtime
- `serde` - Serialization
- `anyhow` - Error handling
- `thiserror` - Error derives
- `tracing` - Logging

### Provider Dependencies

- `reqwest` - HTTP client
- `async-trait` - Async traits
- `keyring` - Credential storage

### Repository Dependencies

- `git2` - Git operations
- `ignore` - Gitignore support
- `walkdir` - Directory traversal

## Best Practices

### Code Style

1. Use `Result<T>` for fallible operations
2. Prefer `anyhow::Result` for applications
3. Use `thiserror` for library errors
4. Always handle errors explicitly
5. Add context to errors
6. Document public APIs
7. Write tests for all logic
8. Use meaningful variable names

### Testing Strategy

1. Unit tests for individual functions
2. Integration tests for workflows
3. Mock external dependencies
4. Test error paths
5. Aim for >80% coverage
6. Use fixtures for test data

### Documentation

1. Follow Diataxis framework
2. Use `.md` extension (not `.txt`)
3. Use lowercase with underscores
4. Document all public APIs
5. Include examples
6. Keep docs up to date

## Release Checklist

- [ ] All tests pass
- [ ] Clippy warnings resolved
- [ ] Code formatted
- [ ] Documentation updated
- [ ] CHANGELOG updated
- [ ] Version bumped
- [ ] Git tag created
- [ ] Binary built for all platforms
- [ ] Release notes written

## Useful Links

- [Rust Book](https://doc.rust-lang.org/book/)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [Diataxis Framework](https://diataxis.fr/)
- [Cargo Guide](https://doc.rust-lang.org/cargo/)

---

**Version**: 0.1.0  
**Last Updated**: 2025-01-07  
**Maintained By**: XZatoma Development Team
