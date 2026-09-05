# Phase 4: Stale Reference Cleanup Implementation

This document summarizes the work completed for Phase 4 of the
[codebase cleanup plan](./codebase_cleanup_plan.md): removing stale "Phase N"
development-phase labels from `src/` and archiving completed-feature phase-named
implementation docs.

## Overview

Phase 4 is mechanical, low-risk hygiene. All changes are comment/doc-comment
rewordings in `src/` plus documentation relocation and link fixes. No code,
string literals, or test assertions were changed. All quality gates pass.

## Task 4.1: Stale "Phase N" Labels Removed From `src/`

An audit found 34 total mentions of "phase" in `src/`. Of these, 9 are
legitimate domain uses and were preserved; the remaining 25 were stale
development-phase labels (test-section headers, production-code section
comments, and doc comments) that were reworded to drop the "Phase N" label while
preserving all meaning.

### Legitimate domain uses preserved (9)

| Location                                      | Usage                                 |
| --------------------------------------------- | ------------------------------------- |
| `src/acp/streaming.rs`                        | streaming replay "live phase"         |
| `src/config.rs` (`LogRotation` doc)           | "lifecycle phases"                    |
| `src/agent/events.rs` (x2)                    | model "reasoning / thinking phase"    |
| `src/mcp/protocol.rs` (module doc)            | "two phases of an MCP client session" |
| `src/watcher/generic/result_producer.rs` (x4) | "drain phase" / "publish phase"       |

The grep gate `rg -ni "phase [0-9]" src` now returns no matches, and
`rg -ni "phase" src` returns only the nine reviewed lines above.

### Stale labels reworded

- `src/acp/stdio.rs` (13): four production-code section comments in
  `resolve_special_command_response` (`// Phase 2: Informational Commands.` ->
  `// Informational Commands.`, etc.), two inline test comments, and seven
  test-section divider headers (`// Phase 2: Context Window Display tests` ->
  `// Context Window Display tests`, etc.).
- `src/cli.rs` (3): test-section headers renamed to describe their tests
  (`// --- Phase 1 new tests ---` ->
  `// --- Common args and flag parsing tests ---`,
  `// --- Phase 3 new tests ---` -> `// --- Debug and trace flag tests ---`,
  `// --- Phase 4 new tests ---` -> `// --- Log format flag tests ---`).
- `src/config.rs` (4): three doc comments describing `WatcherPlanExecutionMode`
  reworded from "pre-Phase-1 behaviour" to "legacy single-prompt behaviour", and
  one test header `// --- Phase 3 LogConfig tests ---` ->
  `// --- LogConfig tests ---`.
- `src/agent/core.rs` (2): a mock-provider doc comment and a test-section
  header.
- `src/acp/available_commands.rs` (1): a test-section divider header.
- `src/acp/manifest.rs` (1): doc comment "later phases can expose it" -> "later
  revisions can expose it".
- `src/acp/session.rs` (1): doc comment "later ACP phases can extend" -> "later
  ACP revisions can extend".
- `src/mcp/protocol.rs` (1): a test comment "changing public API ... mid-phase"
  reworded (the legitimate module-doc protocol-phase reference was preserved).

Note: the plan listed `src/tools/ide_tools.rs` as a stale-label site, but the
file contains no "phase" references in the current tree, so no change was needed
there.

## Task 4.2: Phase-Named Implementation Docs Archived

`docs/archive/` already existed as the archival convention. Five completed
prior-feature phase-named implementation summaries were moved from
`docs/explanation/` into `docs/archive/` (relocated, not deleted, to preserve
AGENTS.md Rule 5 history):

- `chat_command_unification_phase1_implementation.md`
- `chat_command_unification_phase4_format_models_help_text_implementation.md`
- `chat_unification_phase4_acp_informational_commands_implementation.md`
- `chat_unification_phase5_acp_advertisement_updates_implementation.md`
- `chat_unification_phase6_documentation_and_demos_implementation.md`

The three cleanup-series summaries
(`phase1_security_hardening_implementation.md`,
`phase2_error_handling_standardization_implementation.md`, and
`phase3_dead_code_and_annotation_cleanup_implementation.md`) were intentionally
retained in `docs/explanation/` alongside the living `codebase_cleanup_plan.md`,
since they document the active cleanup effort. They can be archived once all six
cleanup phases are complete.

### Cross-link updates

- `docs/explanation/implementations.md`: the three links to the archived
  `chat_unification_phase{4,5,6}_*` docs were repointed to `../archive/`. (The
  two `chat_command_unification_*` files were not referenced anywhere.)
- `docs/reference/model_management.md`: two stale "See Also" links to
  nonexistent phase docs (`phase4_agent_integration_implementation.md` and
  `phase6_chat_mode_model_management_implementation.md`) were removed. Broader
  `model_management.md` reference accuracy is finalized in Phase 6.

## Task 4.3: Markdown Lint/Format

`markdownlint --fix --config .markdownlint.json` and
`prettier --write --parser markdown --prose-wrap always` were run on every
touched Markdown file (the five relocated docs, `implementations.md`,
`model_management.md`, `codebase_cleanup_plan.md`, and this summary).

## Task 4.4: Validation

- Grep gate: `rg -ni "phase [0-9]" src` returns no matches; `rg -ni "phase" src`
  returns only the nine reviewed legitimate domain uses.
- Doc links verified: the three repointed `../archive/` links resolve to the
  relocated files; the removed `model_management.md` links targeted files that
  never existed.
- `cargo fmt --all` clean.
- `cargo check --all-targets --all-features` clean.
- `cargo clippy --all-targets --all-features -- -D warnings` clean.
- `cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth` —
  2342 passed, 0 failed.
