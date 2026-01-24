## Plan: Documentation cleanup

TL;DR: Audit and reorganize `docs/` to follow the Diataxis framework. Archive developer-only implementation notes, reclassify misfiled content into `docs/how-to/` and `docs/reference/`, create missing user-facing docs (quickstart, provider how-to, CLI/config references), update indexes, and add lightweight docs CI checks. Work in phased, reviewable PRs (move-only PRs first, content PRs next).

**Steps (short):**

1. Archive implementation notes to `docs/archive/implementation_summaries/`.
2. Reclassify misfiled docs into `docs/how-to/` and `docs/reference/`.
3. Create missing docs: `docs/tutorials/quickstart.md`, `docs/how-to/configure_providers.md`, `docs/reference/cli.md`.
4. Update `docs/README.md` and add `docs/explanation/documentation_conventions.md`.
5. Add docs CI checks (filenames, links, code-fence languages).

**Open Questions:**

1. Archive policy: move-only / move-and-delete-after-signoff / keep in archive forever?
2. Which user doc to prioritize: `quickstart` or `configure_providers`?
3. Any explanation docs that must remain top-level (not archived)?

This is a draft for review. Please confirm scope or ask clarifying questions so I can refine the plan.

# Documentation Cleanup Implementation Plan

## Overview

Goal: Make `docs/` user-first and maintainable by:

- Organizing content according to Diataxis (tutorials, how-to, explanation, reference).
- Removing internal implementation noise from top-level documentation (archive instead).
- Filling missing user documentation (quickstart, provider configuration, CLI reference).
- Adding automated checks that prevent regressions (filenames, links, code-fence languages).

Approach: Phased, reversible changes. Start with conservative move-only PRs that only relocate files and update links, followed by focused content PRs that add or consolidate docs.

## Current State Analysis

### Existing Infrastructure

- `docs/` with subdirectories: `explanation/`, `how-to/`, `reference/`, `tutorials/` (empty).
- `docs/README.md` lists desired pages (many marked "coming soon").
- `AGENTS.md` (project root) prescribes doc naming conventions and validation gates.
- `implementations.md` is the canonical high-level implementation summary (should be preserved).

### Identified Issues

- Many `docs/explanation/` files are internal dev notes, phase logs, or implementation plans that clutter the user-facing index.
- Misclassified content (some how-to/reference content lives in `explanation/`).
- Missing high-value docs: quickstart, provider configuration how-to, CLI and configuration references, workflow format.
- Inconsistent filename patterns and a lack of automated docs checks.

## Implementation Phases

### Phase 1: Core Implementation (Archive historical content)

#### Task 1.1 Foundation Work

- Create `docs/archive/implementation_summaries/` to hold implementation logs and phase reports.
- Compile a reviewable list of archive-candidate files (phase reports, notes, deep implementation logs).

#### Task 1.2 Move-only PRs

- Open a move-only PR that relocates archive-candidate files into `docs/archive/implementation_summaries/`.
- Keep high-level summaries (`implementations.md`, `overview.md`) in `docs/explanation/`.

#### Task 1.3 Integration

- Update references in `docs/` to point to new archive paths where relevant and add a short archival note in `implementations.md` referencing the archive.

#### Task 1.4 Testing Requirements

- Run link checks and filename checks to ensure no broken links and that file names follow `lowercase_with_underscores.md`.

#### Task 1.5 Deliverables

- `docs/archive/implementation_summaries/` with moved files.
- Move-only PR(s) ready for review and merge.
- `docs/explanation/documentation_cleanup_summary.md` (audit of moved files and reasons).

#### Task 1.6 Success Criteria

- Move-only PR merged.
- No broken links introduced.
- Maintainers confirm archival policy and preserved content.

### Phase 2: Feature Implementation (Reclassify & Consolidate)

#### Task 2.1 Reclassify Misfiled Content

- Move short usage/how-to snippets to `docs/how-to/` (e.g., merge `chat_mode_provider_display.md` into `docs/how-to/use_chat_modes.md`).
- Move quick references to `docs/reference/` (e.g., `provider_abstraction_quick_reference.md` → `docs/reference/provider_abstraction.md`).

#### Task 2.2 Consolidate Provider Docs

- Extract user-facing provider configuration/auth instructions into `docs/how-to/configure_providers.md`.
- Consolidate provider internals (Copilot/Ollama deep notes) into archive.

#### Task 2.3 Canonicalization

- Rename `implementation_plan_refactored.md` → `implementation_plan.md` and merge small refactor summaries into it.

#### Task 2.4 Testing Requirements

- Validate cross-links and references between `how-to`, `reference`, and `explanation` docs.

#### Task 2.5 Deliverables

- Reclassified docs under `how-to/` and `reference/`.
- Merged canonical `implementation_plan.md`.

#### Task 2.6 Success Criteria

- Diataxis categories align with content.
- Search and top-level index surface user-facing content first.

### Phase 3: Feature Implementation (Create missing docs)

#### Task 3.1 Author high-impact docs

- Write `docs/tutorials/quickstart.md` (install + first run + example).
- Write `docs/how-to/configure_providers.md` (auth, provider caveats).
- Write `docs/how-to/create_workflows.md` and `docs/how-to/generate_documentation.md`.
- Write `docs/reference/cli.md`, `docs/reference/configuration.md`, `docs/reference/workflow_format.md`.

#### Task 3.2 Content Quality

- Ensure each new doc follows AGENTS.md rules: lowercase filenames, no emojis, code blocks with language tags, and includes examples where helpful.

#### Task 3.3 Testing Requirements

- Run docs link checks and a reviewer pass to validate clarity and completeness.

#### Task 3.4 Deliverables

- New tutorial and how-to pages live in `docs/`.
- `docs/README.md` updated to link to these pages.

#### Task 3.5 Success Criteria

- A new user can follow `quickstart` to install and run a basic workflow.
- Provider setup doc enables authentication/config in one session.

### Phase 4: Index update & navigation

#### Task 4.1 Update Index

- Update `docs/README.md` to reflect new structure; remove or mark "coming soon" items and link to issues for deferred items.

#### Task 4.2 Documentation conventions

- Add `docs/explanation/documentation_conventions.md` summarizing contributor rules (file naming, no emojis, Diataxis boundaries, AGENTS.md link).

#### Task 4.3 Success Criteria

- `docs/README.md` is accurate and navigable for new users and contributors.

### Phase 5: Validation & CI

#### Task 5.1 Add docs CI Checks

- Add lightweight CI checks to validate: filenames follow rules, internal links are valid, no emojis in docs, code fences include explicit language tags.

#### Task 5.2 PR Validation

- Ensure docs PRs run link checks and filename checks before merge; recommend "move-only" PR pattern for reorganizations.

#### Task 5.3 Success Criteria

- CI prevents merges that break links or violate naming conventions.
- PRs that change docs are small and focused.

## Implementation Checklist (actionable)

- [ ] Create `docs/archive/implementation_summaries/` (Phase 1).
- [ ] Open move-only PR(s) for archive candidates (Phase 1).
- [x] Reclassify `chat_mode_provider_display.md` → `docs/how-to/use_chat_modes.md` (Phase 2).
- [x] Move `provider_abstraction_quick_reference.md` → `docs/reference/provider_abstraction.md` (Phase 2).
- [x] Rename `implementation_plan_refactored.md` → `implementation_plan.md` (Phase 2).
- [ ] Create `docs/tutorials/quickstart.md` (Phase 3).
- [x] Create `docs/how-to/configure_providers.md` (Phase 2).
- [ ] Update `docs/README.md` and add `docs/explanation/documentation_conventions.md` (Phase 4).
- [ ] Add docs CI checks (Phase 5).
- [x] Prepare `docs/explanation/documentation_cleanup_summary.md` listing all moves and rationale.

## Success criteria (measurable outcomes)

- Top-level `docs/` surfaces user-facing docs first; implementation logs are archived.
- `docs/explanation/` implementation-note files reduced by ≥60%.
- `docs/archive/implementation_summaries/` contains the archived files (no deletions without sign-off).
- All internal links resolve; link-checks report zero failures.
- Filenames follow `lowercase_with_underscores.md` pattern; no emojis present.
- Docs CI checks pass on PRs.

## Time Estimates (rough)

- Phase 1 (archive): 1–3 hours
- Phase 2 (reclassify & consolidate): 2–6 hours
- Phase 3 (create missing docs): 8–16 hours
- Phase 4 (index update): 1–2 hours
- Phase 5 (validation & CI): 2–6 hours
  Estimated total: 1–3 days (single contributor, part-time), subject to review time.

## Risks & Decisions

- Risk: Important context lost when archiving. Mitigation: conservative approach—move-first to `docs/archive/` and record reasons in `documentation_cleanup_summary.md`. Git history preserves all data.
- Risk: Misclassification of explanation vs. how-to. Mitigation: consult maintainers on ambiguous cases; keep high-level explanation docs in `docs/explanation/`.
- Decision: Favor user clarity; developer logs belong in archive or in repository/issue history, not top-level docs.

## Next steps & ownership

Please review and indicate the scope you want to proceed with (reply with one option):

- "Phase 1: Archive now" — create move-only PR plan and change list.
- "Phase 1 + Phase 2" — archive + reclassifications and renames.
- "Full implementation" — all phases in sequence, PR-by-PR.

If you confirm a scope, I will refine that phase into a PR-ready, step-by-step change list and a pre-merge validation checklist (still planning — I will not start implementation without your instruction).

## Appendix A — Proposed move/rename summary

- Rename: `docs/explanation/implementation_plan_refactored.md` → `docs/explanation/implementation_plan.md`
- Move: `docs/explanation/provider_abstraction_quick_reference.md` → `docs/reference/provider_abstraction.md`
- Move: `docs/explanation/implementation_quickstart_checklist.md` → `docs/how-to/implementation_quickstart_checklist.md`
- Archive: All `docs/explanation/phase*` → `docs/archive/implementation_summaries/`
- Archive: `docs/explanation/notes*.md`, `copilot_*.md`, `ollama_*.md` (developer-focused) → `docs/archive/implementation_summaries/`
- Keep: `docs/explanation/overview.md`, `docs/reference/architecture.md`, `docs/reference/model_management.md`, and `docs/how-to/*` that are user-facing.

## Appendix B — Pre-merge validation checklist (for doc PRs)

- Move-only PRs: only file moves/renames (no deletions or large edits), links updated.
- Content PRs: small, focused changes (e.g., add `quickstart`), include reviewer checklist.
- CI checks: filename checker, link checker, emoji scan, ensure fenced code blocks have language tags.
- Final PR: update `docs/README.md` and include `docs/explanation/documentation_cleanup_summary.md` summarizing all changes.

## References

- Diataxis Framework: https://diataxis.fr/
- Project doc conventions: `AGENTS.md`
- Current docs index: `docs/README.md`

---

Draft ready for review — please tell me which scope you'd like me to refine into a PR-ready change list (Phase 1 / Phase 1+2 / Full), or ask clarifying questions so I can update this plan.
