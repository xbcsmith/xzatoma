# Design Decisions

## Overview

This document captures high-level design decisions and their rationale for the XZatoma project. It is a living reference intended to:

- Make architectural and product choices explicit and discoverable for contributors.
- Provide a consistent place to record alternatives considered and the reasons for decisions.
- Aid future maintenance by documenting the impacts and follow-up actions for each decision.

Status: placeholder — add decision entries below using the template in this document.

## Purpose & audience

This page is for maintainers, contributors, and reviewers who need to understand why the project is structured the way it is. It complements implementation notes in `docs/archive/implementation_summaries/` and guidelines in `AGENTS.md` (see References).

## How to use this file

- Add an entry for every non-trivial decision (architecture, UX, repo conventions, major public APIs).
- Keep each decision compact and focused: Summary → Rationale → Alternatives → Impact → References.
- When proposing a decision, open an issue and discuss before marking it as Accepted. Link the issue/PR from the decision entry.

## Accepted decisions (summary)

These represent project-level decisions that are already in effect or were intentionally chosen:

- CLI framework: Use `clap` (derive API) for argument parsing.
 - Rationale: concise derivation of flags and subcommands; good ergonomics for small CLIs.
 - Implications: consistent CLI structure in `src/cli.rs`; add tests for parsing.

- Provider abstraction: Keep a provider trait and provider-specific adapters (e.g., Copilot, Ollama).
 - Rationale: separates provider-specific logic from agent control flow and tools.
 - Implications: `providers/` contains implementations and is independent from `agent/`.

- Keep the system intentionally simple and avoid premature abstraction.
 - Rationale: the agent should remain small, focused, and easy to reason about.
 - Implications: prefer pragmatic, incremental changes over heavy architectural rewrites.

- Documentation organization: follow the Diataxis framework (tutorials, how-to, explanation, reference).
 - Rationale: provides a predictable structure for both readers and contributors.
 - Implications: place user-facing content in `docs/tutorials/`, `docs/how-to/`, and `docs/reference/`; keep implementation notes in `docs/archive/`.

- Documentation conventions:
 - Filenames: `lowercase_with_underscores.md`
 - No emojis in docs or code comments
 - Code fences must include a language tag
 - Rationale: consistency, tooling compatibility, and readability
 - References: `AGENTS.md`

- Plan/workflow format: keep a minimal, easy-to-parse model (`name`, `description`, `steps` with `name`, `action`, `context`) with support for YAML/JSON/Markdown inputs.
 - Rationale: low friction for authors and deterministic parsing.
 - Implications: `PlanParser` enforces basic validation (name non-empty, at least one step, step names/actions required).

- Documentation-first PR strategy for reorganization:
 - When moving/renaming docs, prefer move-only PRs followed by focused content PRs.
 - Rationale: preserves history and simplifies review.
 - See: `docs/explanation/documentation_cleanup_implementation_plan.md`

## Decisions template

Copy and paste this template when adding a new decision:

---
### Decision: <short title>
- Status: {Proposed | Accepted | Rejected | Deferred}
- Date: YYYY-MM-DD
- Summary:
 - One-line description of the decision.
- Rationale:
 - Why this option was chosen (benefits).
- Alternatives considered:
 - Short list of alternatives and why they were rejected.
- Impact:
 - Files/modules affected, migration or compatibility notes, testing requirements.
- Implementation notes:
 - Implementation steps, owner, timeline.
- References:
 - Links to related issues, PRs, docs.
---

Example (filled):

---
### Decision: Use `clap` for CLI parsing
- Status: Accepted
- Date: 2025-01-07
- Summary: Use the `clap` crate with derive macros for CLI parsing.
- Rationale: Small API surface, idiomatic Rust, testable parsing behavior.
- Alternatives considered: hand-written parser (rejected: more code to maintain).
- Impact: `src/cli.rs` is the canonical definition. Add tests for parsing and examples in docs.
- Implementation notes: maintain `Cli::parse_args()` and add tests for each subcommand.
- References: `src/cli.rs`, `AGENTS.md`
---

## Open questions (to be resolved)

- Archive retention policy for implementation notes: move-only / move-and-delete-after-signoff / keep forever? (Action: decide in maintainer meeting and document in `docs/archive/README.md`.)
- Priority of user docs: Quickstart vs. Provider configuration — which should be prioritized for discoverability improvements?
- Are there any explanation docs that must remain top-level (not archived) for discoverability?

Add items here as they are raised; track discussion links and decisions.

## How to propose changes to decisions

1. Open a GitHub issue summarizing the proposed change and motivation.
2. Discuss alternatives in the issue until there is rough consensus.
3. Create a PR that:
  - Adds or updates the decision entry in this file (use the template above).
  - Includes tests and docs if code changes are required.
  - References the issue and any relevant PRs.
4. Request review from maintainers. Merely editing the decision document without a linked issue/PR context is discouraged.

## Maintenance and governance

- Maintain decisions in this file as a living record; mark the status (Proposed / Accepted / Rejected / Deferred).
- For significant changes, prefer to keep a short changelog entry (date + brief summary).
- Decisions that change public behavior (API, CLI, config formats) should include migration guidance and test coverage.

## References

- Project guidelines and mandatory doc conventions: `../../AGENTS.md`
- Documentation cleanup plan: `document_cleanup_implementation_plan.md`
- Implementation notes archive: `../archive/implementation_summaries/`
- Workflow format: `../reference/workflow_format.md`

---

Last updated: 2026-01-24
