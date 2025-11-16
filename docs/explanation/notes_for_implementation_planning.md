# Notes for Implementation Planning Phase

## Context for Next Session

This document provides critical context for the AI agent who will create the phased implementation plan for XZatoma. Read this BEFORE starting the implementation plan.

## What Has Been Completed

### 1. Architecture Design ✅

**File**: `docs/reference/architecture.md` (1,114 lines)

**Status**: APPROVED FOR IMPLEMENTATION

**Key Sections**:
- High-level architecture (CLI → Agent → Providers → Tools)
- Core components detailed (CLI, Agent, Conversation Management, Providers, Tools)
- Module structure with responsibilities
- Security model (terminal execution, file operations, credentials)
- Configuration with precedence rules
- Error handling with all error types defined
- Plan file formats (YAML, JSON, Markdown)
- Testing strategy

**Critical Fixes Applied**:
1. ✅ Iteration limits enforced in Agent execution loop
2. ✅ Comprehensive terminal security model (execution modes, denylist, validation)
3. ✅ Complete conversation token management with pruning strategy

### 2. Architecture Validation ✅

**Files Created**:
- `docs/explanation/architecture_validation.md` (685 lines) - Full validation analysis
- `docs/explanation/required_architecture_updates.md` (866 lines) - Update specifications
- `docs/explanation/architecture_fixes_applied.md` (347 lines) - Summary of fixes
- `docs/explanation/architecture_validation_status.md` (353 lines) - Final approval

**Validation Score**: 9/10 (improved from 6/10)

**All Critical Issues Resolved**:
- Infinite loop risk → Fixed with max_iterations enforcement
- Terminal security gaps → Comprehensive validation model
- Token management missing → Complete pruning strategy

### 3. Competitive Analysis ✅

**File**: `docs/explanation/competitive_analysis.md` (480 lines)

**Key Findings**:
- Goose: Feature-rich, MCP extensions, ~9k lines, production
- Zed Agent: Editor-integrated, ~9.6k lines, production
- XZatoma: Intentionally simpler, CLI-focused, planned ~3-5k lines

**Positioning**: "The simplest autonomous AI agent for CLI automation"

**Niche**: DevOps, CI/CD, server environments, local LLM users, privacy-conscious

### 4. Project Guidelines ✅

**Updated Files**:
- `AGENTS.md` - Updated with XZatoma architecture (was XZepr-MCP)
- `PLAN.md` - Already correct, contains planning methodology

**Key Rules to Follow**:
- File extensions: `.yaml` (NOT `.yml`), `.md` for markdown
- Markdown naming: `lowercase_with_underscores.md` (except README.md)
- No emojis in documentation (except AGENTS.md itself)
- Test coverage: >80% required
- Documentation: Diataxis framework (tutorials, how-to, explanation, reference)
- Git commits: Conventional commits format
- Code quality: cargo fmt, clippy with zero warnings, all tests pass

## Critical Architecture Decisions

### Module Structure

```
src/
├── main.rs              # Entry point
├── lib.rs               # Library root
├── cli.rs               # CLI parser (clap)
├── config.rs            # Configuration management
├── error.rs             # Error types (thiserror)
│
├── agent/               # Agent core
│   ├── mod.rs
│   ├── agent.rs         # Main agent logic, execution loop
│   ├── conversation.rs  # Message history, token management
│   └── executor.rs      # Tool execution, registry
│
├── providers/           # AI providers
│   ├── mod.rs
│   ├── base.rs          # Provider trait
│   ├── copilot.rs       # GitHub Copilot
│   └── ollama.rs        # Ollama
│
└── tools/               # Basic tools
    ├── mod.rs
    ├── file_ops.rs      # File operations
    ├── terminal.rs      # Terminal execution
    └── plan.rs          # Plan parsing
```

### Key Architectural Patterns

**Agent Execution Loop**:
```rust
pub struct Agent {
    provider: Arc<dyn Provider>,
    conversation: Conversation,
    tools: Vec<Tool>,
    max_iterations: usize,  // CRITICAL: Must enforce
}

impl Agent {
    pub async fn execute(&mut self, instruction: String) -> Result<String> {
        let mut iterations = 0;
        loop {
            if iterations >= self.max_iterations {
                return Err(XzatomaError::MaxIterationsExceeded { ... });
            }
            iterations += 1;
            // ... provider.complete() and tool execution
        }
    }
}
```

**Tool Result Format**:
```rust
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub truncated: bool,
    pub metadata: HashMap<String, String>,
}
```

**Conversation Management**:
```rust
pub struct Conversation {
    messages: Vec<Message>,
    token_count: usize,
    max_tokens: usize,
    min_retain_turns: usize,  // Always keep last N turns
}
// MUST implement automatic pruning when approaching token limit
```

### Security Model (CRITICAL)

**Terminal Execution Modes**:
1. Interactive: User confirms each command
2. Restricted Autonomous: Only allowlist commands (ls, cat, grep, find, etc.)
3. Full Autonomous: All commands allowed (requires `--allow-dangerous` flag)

**Command Denylist** (MUST reject):
- `rm -rf /`, `rm -rf /*`
- `dd if=/dev/zero`
- `mkfs.*`
- `:(){:|:&};:` (fork bomb)
- `curl | sh`, `wget | sh`
- `sudo`, `su`
- Commands with `eval` or `exec`

**Path Validation** (MUST enforce):
- All paths within working directory only
- Reject absolute paths starting with `/`
- Reject `..` traversal beyond root
- Validate symlink targets

### Configuration Structure

```yaml
provider:
  type: copilot  # or 'ollama'
  copilot:
    model: gpt-4o
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
    max_output_size: 1048576
    max_file_read_size: 10485760
  terminal:
    default_mode: restricted_autonomous
    timeout_seconds: 30
    max_stdout_bytes: 10485760
    max_stderr_bytes: 1048576
```

**Precedence**: CLI args > Env vars > Config file > Defaults

## Implementation Strategy

### Phase Breakdown Recommendation

**Phase 1: Foundation** (Weeks 1-2)
- Error types (`error.rs`)
- Configuration loading with precedence (`config.rs`)
- Basic module structure (empty impls)
- CLI parser skeleton (`cli.rs`)
- Unit test framework setup
- Cargo.toml with dependencies

**Phase 2: Core Agent** (Weeks 3-4)
- Agent struct and execution loop with iteration limits
- Conversation management with token tracking
- Tool executor framework and registry
- Command validator for terminal security
- Integration tests with mock provider

**Phase 3: Providers** (Weeks 5-6)
- Provider trait implementation
- GitHub Copilot client
- Ollama client
- Authentication and credential storage
- Provider-specific tests

**Phase 4: Tools** (Weeks 7-8)
- File operations (list, read, write, create_directory, delete, diff)
- Terminal execution with validation
- Plan parser (YAML, JSON, Markdown)
- Tool result formatting and truncation
- Tool-specific tests

**Phase 5: Integration** (Weeks 9-10)
- CLI command implementation
- End-to-end testing
- Documentation completion
- Examples and tutorials
- Performance optimization

**Phase 6: Polish** (Weeks 11-12)
- Security audit
- Error message improvements
- Logging and observability
- Release preparation
- Docker image

### Target Metrics

**Code Size**: 3,000-5,000 lines total
- `error.rs`: ~100 lines
- `config.rs`: ~200 lines
- `cli.rs`: ~150 lines
- `agent/`: ~800 lines
- `providers/`: ~600 lines
- `tools/`: ~700 lines
- Tests: ~1,500 lines
- Other: ~500 lines

**Test Coverage**: >80% (MANDATORY)

**Performance**:
- Agent loop iteration: <100ms overhead
- Tool execution: Depends on tool, but framework adds <10ms
- Conversation pruning: <50ms
- Configuration loading: <100ms

**Dependencies** (Estimated):
```toml
[dependencies]
clap = { version = "4.5", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
anyhow = "1"
thiserror = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
reqwest = { version = "0.11", features = ["json"] }
async-trait = "0.1"
keyring = "2"
walkdir = "2"
similar = "2"
```

## Key Design Principles (DO NOT VIOLATE)

1. **Keep It Simple**: No unnecessary abstractions, <5k lines total
2. **Security First**: Command validation, path restrictions, iteration limits
3. **Generic Tools**: No specialized features, let AI figure it out
4. **CLI Native**: Works anywhere, no GUI dependencies
5. **Local LLM Friendly**: Ollama support is first-class
6. **Plan-Based**: YAML/JSON/Markdown plans for repeatable tasks
7. **Autonomous Ready**: Designed for unattended operation

## Common Pitfalls to Avoid

1. **Scope Creep**: Don't add features "because Goose has them"
2. **Over-Engineering**: Simple is better than clever
3. **Skipping Security**: Terminal execution is dangerous, validate everything
4. **Ignoring Token Limits**: Conversations grow, implement pruning from start
5. **Poor Error Messages**: Users need clear actionable errors
6. **Missing Tests**: >80% coverage is mandatory, not optional
7. **Documentation Debt**: Write docs as you implement, not after

## Files Ready for Reference

All documentation is in Diataxis structure:

**Reference** (specifications):
- `docs/reference/architecture.md` - Complete architecture (USE THIS)

**Explanation** (understanding):
- `docs/explanation/architecture_validation.md` - Validation analysis
- `docs/explanation/required_architecture_updates.md` - Update details
- `docs/explanation/architecture_fixes_applied.md` - What was fixed
- `docs/explanation/architecture_validation_status.md` - Approval status
- `docs/explanation/competitive_analysis.md` - Comparison to Goose/Zed
- `docs/explanation/agents_md_update.md` - AGENTS.md changes
- `docs/explanation/implementation_plan.md` - High-level plan (may exist)
- `docs/explanation/overview.md` - Project overview (may exist)

**Project Rules**:
- `AGENTS.md` - Development guidelines (FOLLOW STRICTLY)
- `PLAN.md` - Planning methodology (USE FOR STRUCTURE)

## Implementation Plan Requirements

When creating the phased implementation plan, it MUST include:

### For Each Phase:
1. **Overview** - What this phase accomplishes
2. **Tasks** - Specific implementable tasks (not vague)
3. **Files to Create/Modify** - Exact file paths
4. **Dependencies** - What must be done first
5. **Testing Requirements** - Specific tests needed
6. **Deliverables** - What's working at phase end
7. **Success Criteria** - How to know phase is complete
8. **Estimated Lines of Code** - Keep running total

### Format (from PLAN.md):
```markdown
# XZatoma Implementation Plan

## Overview
Overview of all phases

## Current State Analysis
Current state (architecture designed, no code)

### Existing Infrastructure
What exists now (docs, architecture)

### Identified Issues
None yet, but note security as critical

## Implementation Phases

### Phase 1: Foundation

#### Task 1.1: Error Types and Result Patterns
- Create src/error.rs
- Define XzatomaError enum with all variants
- Implement Display and Error traits
- Lines: ~100

#### Task 1.2: Configuration System
- Create src/config.rs
- Implement Settings struct
- Load from file, env vars, CLI args
- Test precedence rules
- Lines: ~200

[... continue for each task ...]

#### Task 1.6: Success Criteria
- [ ] All error types compile
- [ ] Configuration loads correctly
- [ ] Tests pass with >80% coverage
- [ ] Clippy shows zero warnings
- [ ] Documentation complete

### Phase 2: Core Agent
[... similar structure ...]
```

## Questions to Answer in Implementation Plan

1. What is the exact dependency list with versions?
2. What is the minimum viable product (MVP) scope?
3. What can be deferred to post-MVP?
4. What is the testing strategy for security-critical code?
5. How will we validate autonomous operation safety?
6. What examples will demonstrate the value?
7. How will we measure success?

## Success Metrics for Implementation Plan

The implementation plan is successful if:
- ✅ Every phase has clear, actionable tasks
- ✅ Dependencies between phases are explicit
- ✅ Testing is defined at every step
- ✅ Code size estimates keep total under 5k lines
- ✅ Security concerns are addressed in relevant phases
- ✅ MVP can be achieved in Phases 1-3
- ✅ Each phase produces working, testable deliverables
- ✅ Documentation requirements are specified

## Final Notes

**Philosophy**: Build the simplest thing that works, then stop.

**Goal**: CLI-native autonomous agent in <5k lines that's secure, simple, and effective.

**Not Goal**: Feature parity with Goose, MCP support, desktop app, or complex workflows.

**Timeline**: Realistic estimate is 10-12 weeks for full implementation to v1.0.

**Next Steps**:
1. Review this document thoroughly
2. Review `docs/reference/architecture.md` completely
3. Review `AGENTS.md` and `PLAN.md` for requirements
4. Create phased implementation plan following PLAN.md format
5. Place plan in `docs/explanation/implementation_plan.md`

**Remember**: You're not just writing code - you're proving that autonomous AI agents can be simple, safe, and useful. The architecture is solid. Now make it real.

---

**Document Version**: 1.0  
**Created**: 2025-01-15  
**For**: Implementation Planning Phase  
**Status**: Ready for next session
