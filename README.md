# XZatoma

**Autonomous AI Agent for Repository Documentation Generation**

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)
[![Status](https://img.shields.io/badge/status-planning-yellow.svg)](docs/explanation/implementation_plan.md)

## Overview

XZatoma is a simple autonomous AI agent CLI written in Rust that executes tasks through conversation with AI providers (GitHub Copilot or Ollama). Think of it as a command-line version of Zed's agent chat - you give it a goal (via interactive prompt or structured plan), and it uses basic file and terminal tools to accomplish it.

### Key Features

- **Multi-Provider AI Integration**: GitHub Copilot and Ollama support
- **Autonomous Agent**: Conversation-based execution with multi-turn tool calling
- **Basic Tools**: File operations (list, read, write, delete, diff) and terminal execution
- **Flexible Input**: Interactive chat mode, structured plan files, or one-shot prompts
- **Generic Design**: No specialized features - agent uses basic tools creatively

## Quick Start

### Installation (Coming Soon)

```bash
# From source
cargo install --git https://github.com/xbcsmith/xzatoma

# Or download binary
# See releases page
```

### Usage Example

```bash
# Authenticate with provider
xzatoma auth --provider copilot

# Run a plan file
xzatoma run --plan task.yaml

# Interactive chat mode
xzatoma chat

# One-shot prompt
xzatoma run --prompt "Find all TODO comments and create tasks.md"
```

### Example Plan File

```yaml
goal: "Find all TODO comments and create a summary file"

context:
  directory: "src/"

instructions:
  - List all Rust files in src/
  - Read each file and find TODO comments
  - Create tasks.md with all TODO items
  - Include file name and line number for each
```

### Example Interactive Session

```bash
$ xzatoma chat
> Refactor all snake_case function names to camelCase in Python files

Agent: I'll search for Python files and refactor the function names.
[Using tool: list_files]
[Using tool: read_file for each .py file]
[Using tool: write_file with updated content]

Done! Refactored 12 functions across 5 files.
```

## How It Works

XZatoma is intentionally simple:

1. **You provide a goal** - via interactive chat or plan file
2. **Agent talks to AI provider** - sends conversation with available tools
3. **AI decides what to do** - calls tools (list files, read file, write file, run command)
4. **Agent executes tools** - runs the requested operations
5. **Repeat until done** - agent adds results to conversation, AI continues

The agent has no specialized features - it accomplishes complex tasks by using basic file and terminal tools creatively.

## Project Documentation

### For Users

- [Quick Start Tutorial](docs/tutorials/quickstart.md) _(coming soon)_
- [Configuration Guide](docs/how_to/configure_providers.md) _(coming soon)_
- [CLI Reference](docs/reference/cli.md) _(coming soon)_

### For Developers

- [Architecture Overview](docs/reference/architecture.md)
- [Implementation Plan](docs/explanation/implementation_plan.md)
- [Project Overview](docs/explanation/overview.md)
- [Quick Reference](docs/reference/quick_reference.md)

### Project Guidelines

- [Planning Guidelines](PLAN.md)
- [Agent Development Guidelines](AGENTS.md)

## Project Status

**Current Phase**: Planning Complete
**Next Milestone**: Phase 1 - Foundation
**Target Release**: v1.0.0 (14-19 weeks)

See [Implementation Plan](docs/explanation/implementation_plan.md) for details.

## Implementation Phases

1. **Phase 1: Foundation** (2-3 weeks) - Core infrastructure, config, error handling
2. **Phase 2: AI Providers** (2-3 weeks) - GitHub Copilot and Ollama integration
3. **Phase 3: Agent Core** (2-3 weeks) - Agent execution loop and basic tools
4. **Phase 4: Plans & CLI** (2-3 weeks) - Plan parsing and CLI commands
5. **Phase 5: Production** (2 weeks) - Polish, documentation, and release

## Architecture

XZatoma follows a modular architecture with clear separation of concerns:

```
User Input → CLI → Agent Core → AI Provider
                      ↓              ↓
                   Tools ← ─ ─ ─ ─ ─ ┘
                   (File ops, Terminal)
```

See [Architecture Document](docs/reference/architecture.md) for complete details.

## Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) _(coming soon)_ and follow the guidelines in [AGENTS.md](AGENTS.md).

### Development Setup

```bash
# Clone repository
git clone https://github.com/xbcsmith/xzatoma.git
cd xzatoma

# Build project
cargo build

# Run tests
cargo test

# Check code quality
cargo fmt --check
cargo clippy -- -D warnings
```

## Technology Stack

- **Language**: Rust (stable)
- **Async Runtime**: Tokio
- **CLI Framework**: Clap
- **AI Providers**: GitHub Copilot, Ollama
- **Tools**: File ops, terminal execution
- **Testing**: >80% coverage target

## License

Apache License 2.0 - See [LICENSE](LICENSE) for details.

## Acknowledgments

This project draws inspiration from:

- [Zed](https://github.com/zed-industries/zed) - Agent chat and tool integration patterns
- [Goose](https://github.com/block/goose) - Agent architecture and provider abstraction
- [Diataxis](https://diataxis.fr/) - Documentation organization

## Contact

- **Issues**: [GitHub Issues](https://github.com/xbcsmith/xzatoma/issues)
- **Discussions**: [GitHub Discussions](https://github.com/xbcsmith/xzatoma/discussions)

---

**Status**: Planning Complete | **Version**: 0.1.0-planning | **Last Updated**: 2025-01-07
