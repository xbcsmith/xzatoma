# XZatoma Project Overview

## Executive Summary

XZatoma is a simple autonomous AI agent CLI written in Rust that executes tasks through conversation with AI providers (GitHub Copilot or Ollama). Think of it as a command-line version of Zed's agent chat - you give it a goal (via interactive prompt or structured plan), and it uses basic file and terminal tools to accomplish it.

## Vision

To provide a simple, powerful AI agent that can:

- Execute tasks autonomously using basic tools
- Work with multiple AI providers (Copilot, Ollama)
- Run interactively or from structured plan files
- Accomplish complex goals through simple building blocks

## Key Features

### Core Capabilities

1. **Multi-Provider AI Integration**

  - GitHub Copilot support with OAuth authentication
  - Ollama support for local models
  - Simple provider abstraction

2. **Autonomous Agent**

  - Conversation-based execution loop
  - Multi-turn tool calling
  - Handles errors and retries

3. **Basic Tools**

  - File operations: list, read, write, delete, diff
  - Terminal execution: run shell commands
  - Plan parsing: JSON, YAML, Markdown

4. **Flexible Input**
  - Interactive chat mode
  - Structured plan files
  - One-shot prompts

## Technical Architecture

### High-Level Design

```
User Input → CLI → Agent Core → AI Provider
           ↓       ↓
          Tools ← ─ ─ ─ ─ ─ ┘
          (File ops, Terminal)
```

### Core Components

1. **CLI Layer**: Command parsing, interactive mode, plan loading
2. **Agent Core**: Conversation management, execution loop
3. **Provider Layer**: Copilot and Ollama integration
4. **Tools**: File operations, terminal execution, plan parsing

### Technology Stack

- **Language**: Rust (stable)
- **Async Runtime**: Tokio
- **CLI Framework**: Clap
- **Serialization**: Serde (JSON, YAML)
- **HTTP Client**: Reqwest
- **Error Handling**: anyhow, thiserror
- **Logging**: tracing

## Implementation Strategy

The project follows a 5-phase implementation plan:

### Phase 1: Foundation (2-3 weeks)

- Rust project structure
- Configuration management
- Error handling system
- Testing infrastructure

### Phase 2: AI Provider Integration (2-3 weeks)

- Provider trait definition
- Ollama provider implementation
- GitHub Copilot provider with OAuth
- Provider authentication

### Phase 3: Agent Core and Basic Tools (2-3 weeks)

- Conversation management
- Agent execution loop
- File operation tools
- Terminal execution tool

### Phase 4: Plan Parsing and CLI (2-3 weeks)

- Plan file parsing (JSON/YAML/Markdown)
- CLI commands (chat, run, auth)
- Interactive mode
- Plan execution mode

### Phase 5: Production Ready (2 weeks)

- Logging and error messages
- User documentation
- CI/CD pipeline
- Release preparation

**Total Timeline**: 10-14 weeks to v1.0.0

## Example Use Cases

### Interactive Mode

```bash
$ xzatoma chat
> Find all TODO comments in Rust files and create tasks.md

Agent: I'll search for TODO comments and create a summary.
[Using tool: list_files with pattern "*.rs"]
[Using tool: read_file for main.rs]
[Using tool: read_file for lib.rs]
[Using tool: write_file for tasks.md]

Done! Created tasks.md with 5 TODO items found.

> Great! Now create GitHub issues for each one

[Agent continues using tools to accomplish the task...]
```

### Plan Execution Mode

```yaml
# refactor.yaml
goal: "Refactor function names to follow snake_case convention"

context:
 directory: "src/"

instructions:
 - List all Python files in src/
 - Read each file
 - Identify functions with camelCase names
 - Rename to snake_case
 - Update all references
 - Write updated files
 - Show summary of changes
```

```bash
$ xzatoma run --plan refactor.yaml

Executing plan: Refactor function names...

[Using tool: list_files]
[Using tool: read_file for utils.py]
[Using tool: read_file for models.py]
[Analyzing function names...]
[Using tool: write_file for utils.py]
[Using tool: write_file for models.py]

Complete! Refactored 8 functions across 2 files:
- utils.py: 5 functions renamed
- models.py: 3 functions renamed
```

### One-Shot Mode

```bash
$ xzatoma run --prompt "Count lines of code in all Rust files"

[Using tool: list_files]
[Using tool: read_file for each .rs file]

Total lines of code: 2,347
- src/main.rs: 150 lines
- src/lib.rs: 200 lines
- src/agent/agent.rs: 450 lines
... (full breakdown)
```

## Design Principles

### 1. Simplicity

- Keep the agent generic
- Provide basic tools only
- Let AI figure out how to accomplish tasks

### 2. Flexibility

- Work with multiple providers
- Accept structured plans or free-form chat
- No specialized domain features

### 3. Reliability

- Comprehensive error handling
- Retry mechanisms
- Safe file operations

### 4. Extensibility

- Easy to add new tools
- Plugin-ready architecture (future)
- Provider abstraction

## Philosophy: Generic vs Specialized

XZatoma intentionally does NOT include:

- Git operations (agent uses terminal)
- Documentation generators (agent uses file tools)
- Code analysis tools (agent reads files)
- Repository scanners (agent uses list_files)
- Database clients (agent uses terminal)
- API clients (agent uses terminal with curl)

Instead, the agent uses basic building blocks creatively to accomplish complex tasks. This keeps XZatoma simple while maintaining maximum flexibility.

## Quality Standards

### Code Quality

- Test coverage >80%
- Zero clippy warnings
- Formatted with rustfmt
- Documented public APIs

### Testing

- Unit tests for all modules
- Integration tests for workflows
- End-to-end tests with mock provider
- Cross-platform testing

### Documentation

- Diataxis-compliant docs
- API reference
- Usage examples
- Troubleshooting guides

### Security

- Secure credential storage (keyring)
- Safe file operations
- Command confirmation (optional)
- Input validation

## Success Criteria

### Technical

- [ ] All phases completed
- [ ] Test coverage >80%
- [ ] CI/CD pipeline operational
- [ ] Documentation complete
- [ ] Zero critical bugs

### Functional

- [ ] Both providers working
- [ ] Interactive mode functional
- [ ] Plans execute successfully
- [ ] Tools work correctly
- [ ] Error handling robust

### Operational

- [ ] Binary distributions available
- [ ] Installation documented
- [ ] Examples provided
- [ ] v1.0.0 released

## Comparison to Zed Agent

XZatoma is similar to Zed's agent chat but:

- **CLI-based** instead of editor-integrated
- **Supports plan files** for structured tasks
- **Runs autonomously** without human in loop
- **Focused on file/terminal** operations
- **Simpler tool set** - basic building blocks only

## Future Roadmap

### v1.1.0 (Post-Launch)

- Additional providers (OpenAI, Anthropic, Claude)
- More tools (HTTP requests, etc.)
- Improved error messages
- Performance optimizations

### v1.2.0

- Plugin system for custom tools
- Web interface for monitoring
- Team/collaborative features
- Workflow templates

### v2.0.0

- Advanced tool orchestration
- Multi-agent collaboration
- Cloud integration
- Enterprise features

## Getting Started

### For Users

1. Read `docs/tutorials/quickstart.md` 
2. Configure your AI provider
3. Try interactive chat mode
4. Create your first plan file

### For Developers

1. Read `docs/reference/architecture.md`
2. Review `docs/explanation/implementation_plan.md`
3. Follow `AGENTS.md` coding guidelines
4. Start with Phase 1 tasks

### For Contributors

1. Check open issues
2. Review `CONTRIBUTING.md`
3. Follow coding standards
4. Ensure tests pass

## Project Status

**Current Phase**: Planning Complete
**Next Milestone**: Phase 1 - Foundation
**Target Release**: v1.0.0 (10-14 weeks)

## Resources

### Documentation

- [Architecture](../reference/architecture.md) - Technical design
- [Implementation Plan](implementation_plan.md) - Development roadmap
- [Quick Reference](../reference/quick_reference.md) - Commands and patterns
- [README](../../README.md) - Project overview

### External References

- [Zed Editor](https://github.com/zed-industries/zed) - Agent and provider patterns
- [Goose Project](https://github.com/block/goose) - Agent architecture
- [Diataxis Framework](https://diataxis.fr/) - Documentation organization
- [Rust Book](https://doc.rust-lang.org/book/) - Rust language

## Contact & Support

- **Issues**: GitHub Issues
- **Discussions**: GitHub Discussions
- **Contributing**: See CONTRIBUTING.md
- **License**: Apache 2.0

## Acknowledgments

This project draws inspiration from:

- **Zed**: Agent chat and tool integration patterns
- **Goose**: Agent architecture and provider abstraction
- **Diataxis**: Documentation framework

## Conclusion

XZatoma is intentionally simple:

- Agent with conversation loop
- Two AI providers (Copilot, Ollama)
- Basic file and terminal tools
- Plan files for structured tasks
- That's it

The power comes from the AI's ability to use these simple tools creatively to accomplish complex tasks, not from building specialized features into XZatoma itself. This keeps the codebase maintainable, the tool flexible, and the possibilities unlimited.

---

**Status**: Planning Complete
**Version**: 0.1.0-planning
**Last Updated**: 2025-01-07
**Maintained By**: XZatoma Development Team
