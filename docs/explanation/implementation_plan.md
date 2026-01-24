# Implementation Plan

Status: Canonical implementation plan (summary)

Overview
--------

This document is the canonical, human-readable implementation plan for XZatoma. It summarizes the refactored plan, highlights the critical changes made during the refactor, and provides a compact, phase-based roadmap for implementation work. For the complete, detailed refactoring audit and the full spec, see the archived refactor summary:

- Full refactor summary: `../archive/implementation_summaries/implementation_plan_refactoring_summary.md`

Executive (Refactor) Summary
---------------------------

Key outcomes of the refactor (short form):

- Security-first: added a dedicated security phase (Command validation, denylist/allowlist, path validation, execution modes).
- Robust conversation management: token counting, pruning, and summary generation to prevent context overflow.
- Agent safety: explicit iteration limits (max_turns) to prevent runaway/autonomous loops.
- Improved error model: expanded error types with clear, descriptive errors and conversions.
- Provider and tooling improvements: clarified provider abstraction, standardized tool call/result formats, and added testable provider traits.
- Clear quality gates: enforced `cargo fmt`, `cargo check`, `cargo clippy -D warnings`, and `cargo test` with >80% coverage as non-negotiable acceptance criteria.
- Phases reorganized to improve testability and reduce risk (agent core before providers; security early).

Refactor highlights (abridged)
- Security Model: Command validator with an explicit denylist and allowlist. Path validation rejects absolute paths, `~`, and directory traversal escapes beyond the working directory.
- Conversation: Store token counts per conversation, prune at configurable thresholds (e.g., 80%), and preserve recent turns + system messages.
- Iteration limits: Enforce `max_turns` to ensure the agent can never run indefinitely.
- Provider trait & formats: Converged on consistent message, tool call, and streaming formats across OpenAI/Copilot/Anthropic/Ollama, with environment variable configuration and local Ollama host support.
- Testing & coverage: Specify concrete tests (security patterns, token pruning, iteration enforcement, provider parsing and tool calls), and require >80% coverage.

Compact Examples
----------------

Command validator sketch:
```rust
pub struct CommandValidator {
    mode: ExecutionMode,
    working_dir: PathBuf,
    allowlist: Vec<String>,
    denylist: Vec<Regex>,
}
```

Conversation sketch:
```rust
pub struct Conversation {
    messages: Vec<Message>,
    token_count: usize,
    max_tokens: usize,
    min_retain_turns: usize,
}
```

Tool result sketch:
```rust
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub truncated: bool,
    pub metadata: HashMap<String, String>,
}
```

Phase-based Roadmap (summary)
-----------------------------

Phase 1 — Foundation & Core Infrastructure
- Deliverables: config, error types, CLI skeleton, testing utilities, initial module layout.
- Goals: Provide a stable scaffold that other phases can reliably build on.

Phase 2 — Agent Core & Token Management
- Deliverables: `Conversation` with token accounting and pruning, `Tool` and `ToolExecutor`, `Agent` with iteration and timeout handling, mock provider for tests.
- Goals: Allow comprehensive unit/integration testing of the agent core without real provider dependencies.

Phase 3 — Security & Terminal Validation (CRITICAL)
- Deliverables: `CommandValidator`, denylist/allowlist rules, path validation, terminal executor integration, detailed security test suite.
- Goals: Prevent destructive or unsafe operations; security tests must be exhaustive.

Phase 4 — AI Providers
- Deliverables: Provider trait, implementations for Copilot (OAuth device flow), Ollama (local host), OpenAI, Anthropic; streaming support and retry logic.
- Goals: Reliable provider abstraction with consistent tool-call handling and robust auth flows.

Phase 5 — File Tools & Plan Parsing
- Deliverables: File operations toolset (list/read/write/delete/diff), plan parser (YAML/JSON/Markdown) with validation.
- Goals: Safe and testable file tooling and plan execution.

Phase 6 — CLI Integration & Polish
- Deliverables: Command handlers, interactive & run commands, usage examples, docs finish, CI integration.
- Goals: Clean user experience, complete documentation, polished UX and error handling.

Quality Gates (non-negotiable)
------------------------------

Before merging feature work ensure:
1. `cargo fmt --all` (applied)
2. `cargo check --all-targets --all-features` (passes)
3. `cargo clippy --all-targets --all-features -- -D warnings` (zero warnings)
4. `cargo test --all-features` with >80% coverage

Documentation rules
-------------------
- Filenames: `lowercase_with_underscores.md`
- YAML: use `.yaml` extension (NOT `.yml`)
- No emojis anywhere in docs, comments, or commit messages
- All public items must have `///` doc comments with examples when applicable

Testing Requirements (high level)
---------------------------------
- Security tests: denylist patterns, allowlist behaviors, and path validation.
- Token & conversation tests: counting, pruning, and summary regeneration.
- Agent loop tests: iteration limit behavior, timeouts, and tool executions.
- Provider tests: request/response parsing, tool call extraction, streaming accumulation.
- Integration tests: end-to-end plan execution with mocked providers where needed.
- Coverage target: >80% across the workspace.

Deliverables & Acceptance Criteria
---------------------------------
Each phase must produce:
- Code changes with accompanying unit + integration tests.
- Documentation changes (docs/how-to, docs/reference or docs/explanation as appropriate).
- Passing quality gates and CI checks.
- A concise phase summary in `docs/explanation` (or archive for developer logs).

Conventions for PRs & Releases
------------------------------
- Use move-only PRs for large structural reorganizations (no unrelated edits).
- Use focused content PRs for merges and doc creation (small, reviewable).
- Follow conventional commit messages: `<type>(<scope>): <description>`.

References
----------
- Full refactor audit: `../archive/implementation_summaries/implementation_plan_refactoring_summary.md`
- Architecture reference: `../reference/architecture.md`
- Documentation conventions & agent rules: `../../AGENTS.md` (project root)
- Documentation index: `../README.md`

How to use this file
--------------------
- This `implementation_plan.md` is the canonical roadmap — use it when planning feature or phase work.
- For implementation detail, test matrices, and bigger code-pattern examples consult the full refactor summary in `docs/archive/implementation_summaries/`.
- Propose changes as small PRs that include tests and doc updates; run the quality gates locally before pushing.

Change log
----------
- This document is the canonical, condensed implementation plan and includes a short, high-level refactor summary. The full refactor audit is archived and should be referenced for detailed decisions and implementation notes.

---

If you need a more detailed per-task checklist (file-level), the refactored plan in the archive contains the expanded tasks and LOC estimates used to break the work into smaller PRs.
