# Agent Skills Phase 2 Implementation

## Overview

This document describes the Phase 2 implementation for agent skills: catalog
disclosure and prompt integration.

Phase 2 builds directly on the Phase 1 skills foundation. Phase 1 established
discovery, parsing, validation, diagnostics, and a valid-only catalog model.
Phase 2 adds controlled startup disclosure of visible skills so the model can
see which skills are available without exposing full `SKILL.md` instruction
bodies.

The primary goals of this phase are:

- render a visible skill catalog for startup prompt use
- enforce visibility and trust gating before disclosure
- integrate catalog disclosure into both chat and run startup flows
- keep invalid skills and full skill bodies out of startup prompt context
- preserve deterministic ordering and bounded disclosure size

## Scope

Phase 2 covers the following planned responsibilities:

- create `src/skills/disclosure.rs`
- implement:
  - `render_skill_catalog`
  - `build_skill_disclosure_section`
- integrate discovery and disclosure into:
  - `commands::chat::run_chat`
  - `commands::run::run_plan_with_options`
- enforce visibility rules for disclosure time
- add disclosure-focused tests
- document the design and implementation

Phase 2 does not yet implement:

- skill activation
- active-skill registry
- prompt injection of full skill bodies
- activation tool registration
- trust store persistence lifecycle
- skills CLI command group

Those are deferred to later phases.

## Design Principles

Phase 2 follows several non-negotiable rules from the implementation plan.

### Only valid skills may be disclosed

Disclosure must be built from the valid catalog only. Invalid discovered
candidates remain visible only through diagnostics and validation surfaces.

### Startup disclosure is metadata-only

The startup catalog must include enough information for the model to decide
whether a skill may be useful, but it must not include the full `SKILL.md` body.

Allowed startup disclosure content includes:

- skill name
- skill description
- source location or file location if useful for runtime messaging

Disallowed startup disclosure content includes:

- full skill body
- hidden invalid skills
- malformed or untrusted entries

### Visibility is distinct from discovery

A discovered skill is not automatically visible.

Phase 1 determines whether a skill is valid. Phase 2 determines whether a valid
skill is visible at disclosure time.

That distinction is important because trust rules can suppress a valid skill
from the startup catalog without reclassifying it as invalid.

### Deterministic order is required

Disclosed entries must be deterministic. This ensures:

- stable prompt behavior
- stable tests
- predictable collision outcomes
- easier debugging

### Prompt impact must be bounded

Disclosure size is capped by `skills.catalog_max_entries`.

This keeps startup prompts from growing unbounded as more valid skills are
installed.

## Files Added and Updated

### New file

Phase 2 introduces:

- `src/skills/disclosure.rs`

### Existing files updated

Phase 2 requires updates to:

- `src/commands/mod.rs`
- `src/config.rs`
- `src/skills/mod.rs`
- `docs/explanation/implementations.md`
- `config/config.yaml`

### Tests added

Phase 2 adds:

- `tests/skills_disclosure.rs`

## Runtime Model

Phase 2 introduces a disclosure-specific runtime layer between skill discovery
and provider completion.

The effective runtime flow becomes:

1. start session
2. discover valid skills using Phase 1 discovery
3. apply visibility and trust gating
4. cap visible entries to `catalog_max_entries`
5. render disclosure text
6. inject disclosure into startup prompt context
7. continue normal execution

This model keeps the skill system simple:

- discovery finds valid skills
- disclosure selects visible skills
- prompt integration exposes only catalog metadata
- activation remains a later concern

## Disclosure Visibility Rules

Phase 2 implements disclosure gating based on both source scope and
configuration.

### Project skills

Project skills require trust when:

- `skills.project_trust_required == true`

This applies to project-scoped roots:

- `<working_dir>/.xzatoma/skills/`
- `<working_dir>/.agents/skills/`

If trust is required and the project root is not trusted, project skills are
omitted from disclosure.

### User skills

User skills do not require project trust.

These roots remain visible by default when valid:

- `~/.xzatoma/skills/`
- `~/.agents/skills/`

### Custom paths

Custom paths require trust unless:

- `skills.allow_custom_paths_without_trust == true`

This rule is intended to prevent arbitrary configured paths from silently
becoming visible to the model when trust has not been granted.

### Invalid skills

Invalid skills are never visible.

This remains true even if a path is trusted.

### Entry count limit

Visible disclosed entries must not exceed:

- `skills.catalog_max_entries`

When more valid visible skills exist than the configured cap, the disclosure
layer selects only the first entries in deterministic order.

## Disclosure Data Shape

The disclosure section is intentionally compact and metadata-only.

Each visible entry should contain:

- `name`
- `description`
- optionally a path reference such as the containing skill directory or
  `SKILL.md` path when useful for runtime messaging

The section must not include:

- the full instruction body
- activation content
- invalid candidates
- shadowed losing skills
- hidden untrusted skills

## Disclosure Renderer

Phase 2 adds a dedicated renderer module so the startup disclosure format is
owned by the skills subsystem rather than being embedded inside command logic.

### `render_skill_catalog`

This function is responsible for rendering a deterministic textual catalog from
visible `SkillRecord` entries.

Its responsibilities include:

- consume only valid visible entries
- preserve deterministic ordering
- omit invalid and hidden entries
- keep output concise
- avoid including full instruction bodies

A representative disclosure shape is:

```/dev/null/example.txt#L1-8
Available skills:
- name: git_review
  description: Review git status, diffs, and likely next actions
- name: rust_refactor
  description: Assist with Rust-safe refactors and module cleanup
```

The exact wording can vary, but the output must remain:

- deterministic
- compact
- metadata-only

### `build_skill_disclosure_section`

This function is responsible for producing the complete prompt-ready section or
omitting it entirely when no visible skills exist.

Its responsibilities include:

- call the renderer only when needed
- return no disclosure block for an empty visible catalog
- provide stable section headings and framing
- ensure the catalog is ready for inclusion in the startup prompt

Representative behavior:

- if visible list is empty: return `None`
- if visible list is non-empty: return a fully formatted disclosure section

## Prompt Integration

Phase 2 integrates catalog disclosure into both major startup paths.

### Chat startup

`commands::chat::run_chat` must:

1. discover skills at session startup
2. filter by trust and config
3. build disclosure text
4. inject the disclosure before the first provider completion
5. omit disclosure entirely when no visible skills exist

This integration must happen before the model receives the first user message.

### Run startup

`commands::run::run_plan_with_options` must perform the same steps:

1. discover skills at startup
2. apply visibility and trust rules
3. build disclosure text
4. inject it before the first provider call
5. omit it if nothing visible exists

This keeps behavior aligned between interactive and non-interactive execution
modes.

## Prompt Construction Strategy

The implementation should avoid changing provider contracts. The provider API
already accepts conversation messages, so the simplest and most consistent
approach is to inject disclosure as a system-context addition before the first
completion.

There are two practical strategies:

1. append disclosure to the existing mode-specific system prompt
2. insert an additional system message before the user task

Either strategy is acceptable if it satisfies the required behavior.

The preferred implementation is usually:

- build the normal system prompt
- append the skill disclosure section
- place the combined result into conversation startup context

This keeps skills catalog visibility close to other execution instructions while
avoiding any provider-specific branching.

## No Full Body Loading at Startup

One of the Phase 2 success criteria is that no full `SKILL.md` bodies are loaded
at startup.

In practice, this means the startup disclosure layer should rely on
metadata-only records prepared during discovery and catalog construction.

If the Phase 1 implementation currently stores body content in full records, the
disclosure layer must not use it.

The renderer should read only:

- `name`
- `description`
- source location metadata if needed

This preserves a clean boundary between:

- startup awareness
- later activation-time body loading

## Trust Gating Model

Phase 2 introduces enforcement of trust rules at disclosure time even if full
trust store lifecycle management is still deferred to Phase 4.

The implementation can use a lightweight trust predicate abstraction for now,
such as:

- trusted project path
- trusted custom path
- implicit trust for user paths

The key requirement is not the final trust storage mechanism but the disclosure
behavior:

- trusted visible skills appear
- untrusted gated skills do not appear

This means Phase 2 may use:

- temporary helpers
- in-memory checks
- placeholder trust evaluation wiring

as long as the runtime behavior matches the plan and later phases can extend it
cleanly.

## Catalog Ordering

The disclosure output must be deterministic.

The effective ordering should remain consistent with the valid catalog ordering.
A safe ordering policy is:

1. valid catalog order
2. stable lexical skill name ordering if needed
3. stable precedence-resolved winner set only

This avoids unstable prompt differences across runs.

## Integration Responsibilities in `commands/mod.rs`

### `chat::run_chat`

This function must gain startup logic to:

- determine working directory
- discover skills using `config.skills`
- filter them for visibility
- build disclosure text
- inject the disclosure into the conversation before first execution

It must also preserve all existing behavior unrelated to skills:

- provider selection
- mode handling
- MCP registration
- storage initialization
- resume behavior

The skills integration should be a narrow additive step, not a rewrite of chat
flow structure.

### `run::run_plan_with_options`

This function must gain the same startup integration before task execution
begins.

It should preserve all existing behavior for:

- plan loading
- prompt construction
- tool registry setup
- provider creation
- MCP tool registration

Again, the Phase 2 addition should be narrow and predictable.

## Testing Requirements

Phase 2 requires a dedicated test file:

- `tests/skills_disclosure.rs`

### Required tests

The implementation plan requires the following tests.

#### Disclosure contains only valid visible skills

This verifies that:

- valid visible skills are rendered
- invalid skills are excluded
- hidden entries do not appear

#### Untrusted project skills are omitted

This verifies that:

- project skills are hidden when trust is required and missing
- disclosure output contains no entries from untrusted project roots

#### Invalid skills are omitted

This verifies that malformed, missing-frontmatter, bad-name, and bad-description
skills never appear in disclosure output.

#### Empty catalog yields no disclosure block

This verifies that startup prompt content is not polluted by empty headings or
empty sections.

#### Disclosed catalog ordering is deterministic

This verifies that repeated runs produce stable ordering.

#### Catalog entry count obeys `catalog_max_entries`

This verifies that disclosure remains bounded even when more valid visible
skills exist.

### Recommended additional tests

Although not strictly required, Phase 2 benefits from a few extra checks:

- user skills remain visible without project trust
- custom paths are hidden unless explicitly allowed or trusted
- full skill body text never appears in disclosure output
- prompt integration tests confirm disclosure presence before first provider
  call
- prompt integration tests confirm omission when no visible skills exist

## Example Disclosure Output

A reasonable disclosure section might look like this:

```/dev/null/example.txt#L1-10
Visible skills catalog:
- git_review: Review repository state and summarize likely next actions
- rust_refactor: Help structure safe Rust refactors and module cleanup
- test_debugging: Assist with failure triage and targeted test isolation

Use a skill only when it is relevant. Full skill instructions are not active
yet and must be explicitly activated in later phases.
```

Important properties of this example:

- only valid visible skills are listed
- only name and description are shown
- no full skill body content is included
- ordering is stable
- the section is concise

## Documentation and Configuration Updates

Phase 2 must update documentation and examples so the new behavior is visible to
future maintainers.

Required outputs include:

- this implementation explanation document
- implementation index update
- config example updates if disclosure-related comments are added or clarified

If trust behavior is partially staged pending later phases, the documentation
should state that clearly.

## Deliverables Checklist

Phase 2 is complete only when all of the following are true:

- `src/skills/disclosure.rs` exists
- disclosure renderer functions are implemented
- chat startup integrates skill catalog disclosure
- run startup integrates skill catalog disclosure
- visibility rules enforce trust-gated omission
- invalid skills never appear in disclosure output
- disclosed entry count respects `catalog_max_entries`
- tests for disclosure behavior pass
- documentation is added in `docs/explanation/`

## Quality Gates

Per project rules, all quality gates must pass in this order:

```/dev/null/commands.sh#L1-4
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Markdown outputs for this phase must also be linted and formatted:

```/dev/null/commands.sh#L1-2
markdownlint --fix --config .markdownlint.json "docs/explanation/agent_skills_phase2_implementation.md"
prettier --write --parser markdown --prose-wrap always "docs/explanation/agent_skills_phase2_implementation.md"
```

## Risks and Mitigations

### Risk: leaking too much startup context

If disclosure includes too much path or metadata detail, startup prompts become
noisy.

Mitigation:

- keep output compact
- include only fields explicitly required
- cap entries with `catalog_max_entries`

### Risk: accidental disclosure of invalid or untrusted skills

If discovery and visibility logic are mixed carelessly, hidden entries may leak
into prompt construction.

Mitigation:

- perform trust and visibility filtering after discovery but before rendering
- render only from visible records
- test invalid and untrusted omission directly

### Risk: accidental inclusion of full skill bodies

If the disclosure layer reuses activation-oriented structures incorrectly, it
may include body text.

Mitigation:

- render only metadata fields
- explicitly test that body text is not present
- keep renderer API metadata-oriented

### Risk: chat and run flows diverge

If disclosure integration is implemented separately in inconsistent ways,
interactive and non-interactive behavior may differ.

Mitigation:

- share disclosure-building helpers
- test both flows
- centralize rendering in the skills module

## Summary

Phase 2 adds startup visibility for skills without activating them.

The essential result of this phase is:

- the system can discover valid skills
- filter them by trust and visibility rules
- disclose a bounded metadata-only catalog
- inject that disclosure into startup prompt context for both chat and run flows

This keeps the design intentionally simple:

- Phase 1 established valid skill discovery
- Phase 2 makes visible skills known to the model
- later phases will handle activation, resource access, trust persistence, and
  lifecycle management

That separation preserves clarity, keeps prompt context bounded, and ensures the
model sees only valid visible skills at startup.
