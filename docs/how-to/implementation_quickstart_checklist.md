# XZatoma Implementation Quick Start Checklist

**Date**: 2025-01-15
**For**: Implementation Team
**Reference**: `docs/explanation/implementation_plan.md`

## Purpose

This quickstart checklist helps you get started implementing the refactored plan. It lists mandatory pre-work, phase-level tasks, quality gates, and common troubleshooting steps so you can begin work confidently and follow the project's standards.

Follow this checklist before creating a feature branch and submit work as small, reviewable PRs.

---

## Before You Start (MANDATORY)

### 1. Read these documents (in order)
- [ ] `AGENTS.md` - Development rules (CRITICAL)
- [ ] `PLAN.md` - Planning methodology
- [ ] `docs/reference/architecture.md` - Architecture reference
- [ ] `docs/explanation/implementation_plan.md` - Canonical implementation plan (source of truth)
- [ ] `docs/archive/implementation_summaries/implementation_plan_refactoring_summary.md` - What changed and why (refactor summary)

### 2. Understand Critical Requirements
- [ ] Security first: terminal validation prevents dangerous commands
- [ ] Iteration limits: agent must enforce `max_turns`
- [ ] Token management: conversations must track and prune tokens
- [ ] Test coverage: >80% required
- [ ] File extensions: use `.yaml` (NOT `.yml`)
- [ ] Markdown naming: `lowercase_with_underscores.md` (except `README.md`)
- [ ] No emojis anywhere in docs, comments, or commit messages

### 3. Set up your development environment
```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Required components
rustup component add clippy rustfmt

# Optional but recommended
cargo install cargo-audit
cargo install cargo-tarpaulin # coverage tool

# Verify
cargo --version
rustc --version
cargo clippy --version
cargo fmt --version
```

### 4. Clone & initialize
```bash
cd /path/to/your/workspace
git clone git@github.com:xbcsmith/xzatoma.git
cd xzatoma
git checkout -b pr-xzatoma-phase1-foundation
cargo check
```

---

## Phase 1: Foundation (Weeks 1-2) — Checklist
- [ ] Add dependencies from `implementation_plan.md` to `Cargo.toml`
- [ ] Create module layout: `src/agent/`, `src/providers/`, `src/tools/`
- [ ] Implement error type (`src/error.rs`) with all variants required by plan
- [ ] Implement configuration (`src/config.rs`) with precedence: CLI > ENV > File > Default
- [ ] Add `config.example.yaml` using `.yaml` extension
- [ ] Create testing infrastructure (`tests/`, utilities, fixtures)
- [ ] Implement CLI skeleton (`src/cli.rs`) — commands: `chat`, `run`, `auth`, `models`
- [ ] Deliver documentation: doc comments and examples for public items

Phase 1 Quality Gates (must pass before Phase 2):
```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

---

## Phase 2: Agent Core (Weeks 3-4) — Checklist
- [ ] Implement `Conversation` with token counting, pruning, and summaries
- [ ] Implement `Tool`, `ToolExecutor` trait, and `ToolResult` struct
- [ ] Implement `Agent` with iteration limits (`max_turns`) and timeout handling
- [ ] Add mocks and unit tests for Agent behavior
- [ ] Add integration tests using a `MockProvider` to validate loop & tool flow

Phase 2 Quality Gates:
```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

---

## Phase 3: Security (Weeks 5-6) — CRITICAL (MANDATORY)
- [ ] Implement `CommandValidator` with denylist (20+ dangerous patterns) and allowlist
- [ ] Implement path validation (reject absolute paths, `~`, `..` escapes beyond working dir)
- [ ] Integrate validator into terminal executor
- [ ] Add comprehensive tests for denylist, allowlist, and path validation
- [ ] Add tests demonstrating enforcement per execution mode (Interactive, RestrictedAutonomous, FullAutonomous)

Critical security test examples:
```text
- rm -rf /    -> BLOCKED
- :(){ :|:& };:  -> BLOCKED (fork bomb)
- curl ... | sh  -> BLOCKED
- /etc/passwd   -> REJECTED (absolute)
- ../../etc/passwd -> REJECTED (escape)
```

---

## Phase 4: Providers
- [ ] Implement `Provider` trait & base types
- [ ] Implement `Ollama` provider (local host support)
- [ ] Implement `GitHub Copilot` provider (OAuth device flow, token caching)
- [ ] Implement `OpenAI` & `Anthropic` providers (API keys, host config)
- [ ] Document provider configuration in `docs/how-to/configure_providers.md`

---

## Daily Development Workflow (Suggested)
### Morning
```bash
git fetch origin
git rebase origin/main
cargo fmt --all
cargo check --all-targets --all-features
```
### During Development
- Iterate with small, focused commits
- Run `cargo test` and `cargo clippy` often
- Update and run unit tests for any new public API
### End of Day
```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
git add -A
git commit -m "feat(scope): concise description (XZATOMA-<issue>)"
```

---

## Commit Message Format
Use conventional commits:
```
<type>(<scope>): <description> (XZATOMA-<issue>)
```
Example:
```
feat(config): add configuration precedence logic (XZATOMA-001)
fix(agent): enforce iteration limits in execution loop (XZATOMA-002)
docs(readme): update installation instructions (XZATOMA-003)
```

---

## Troubleshooting

### Clippy warnings
```bash
cargo clippy --fix --allow-dirty
cargo clippy --all-targets --all-features -- -D warnings
```
Fix warnings promptly; CI treats warnings as errors.

### Failing tests
```bash
cargo test -- --nocapture
RUST_BACKTRACE=1 cargo test
```
Run affected tests locally, add debug output if needed.

### Coverage below 80%
```bash
cargo install cargo-tarpaulin
cargo tarpaulin --out Html
```
Add tests to increase coverage; aim for >80%.

---

## Critical Reminders

NEVER:
- Use `.yml` instead of `.yaml`
- Use CamelCase or uppercase in markdown filenames
- Put emojis in docs, code comments, or commit messages
- Use `unwrap()` without careful justification
- Ignore Clippy warnings (CI treats them as errors)

ALWAYS:
- Run quality gates before merging
- Add tests for new public functions
- Add doc comments with runnable examples for public items
- Keep PRs small and focused (one logical change per PR)

---

## Resources
- Architecture: `docs/reference/architecture.md`
- Implementation Plan: `docs/explanation/implementation_plan.md`
- Refactor Summary: `docs/archive/implementation_summaries/implementation_plan_refactoring_summary.md`
- Dev guidelines: `AGENTS.md`
- Rust Book: https://doc.rust-lang.org/book/
- Async Book: https://rust-lang.github.io/async-book/

---

## Success Metrics
- Phase complete when all deliverables checked, quality gates pass, and tests show >80% coverage.
- Project v1.0.0: all phases complete, tests passing, documentation thorough, and CI stable.

---

Start by checking off the "Before You Start" items. If you'd like, I can help draft the PR description for your Phase 1 branch or prepare a focused test plan for Phase 2 components.
