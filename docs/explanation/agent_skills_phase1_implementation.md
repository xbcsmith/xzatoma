# Agent Skills Phase 1 Implementation

## Overview

This document summarizes the intended Phase 1 implementation for agent skills:
skill discovery, `SKILL.md` parsing, validation, diagnostics, and the initial
catalog foundation.

Phase 1 is intentionally limited in scope. It does not include activation,
prompt injection, trust lifecycle management, or CLI skill commands. Instead, it
establishes the core runtime model that later phases depend on:

- deterministic discovery across supported roots
- strict separation between valid loaded skills and invalid discovered skills
- parsing of `SKILL.md` frontmatter and Markdown body
- validation of required fields and naming rules
- diagnostics for invalid and shadowed skills
- a valid-only catalog structure

## Scope Implemented

Phase 1 covers the following deliverables from the implementation plan:

- creation of the `src/skills/` module tree
- crate wiring through `src/lib.rs`
- discovery support for project, user, and custom roots
- parsing support for `SKILL.md`
- validation rules for required fields and naming
- diagnostic modeling for invalid and shadowed skills
- catalog support for valid loaded skills only
- parser and discovery test coverage

## Module Structure

Phase 1 introduces the following files:

- `src/skills/mod.rs`
- `src/skills/types.rs`
- `src/skills/discovery.rs`
- `src/skills/parser.rs`
- `src/skills/validation.rs`
- `src/skills/catalog.rs`

The crate root must export the module with:

- `pub mod skills;`

## Design Goals

The implementation follows several rules from the plan:

1. valid skills and invalid discovered candidates must be represented
   separately
2. startup metadata must be separated from full activation content
3. absolute filesystem paths must be preserved explicitly
4. source scope must be preserved explicitly
5. precedence and collision handling must be deterministic
6. invalid skills must never be promoted into the loaded catalog

## Core Data Model

### `SkillSourceScope`

`SkillSourceScope` models where a skill came from. The discovery order and
precedence rules are:

1. project client-specific: `<working_dir>/.xzatoma/skills/`
2. project shared convention: `<working_dir>/.agents/skills/`
3. user client-specific: `~/.xzatoma/skills/`
4. user shared convention: `~/.agents/skills/`
5. custom configured paths from `skills.additional_paths`

The enum should expose a stable precedence rank so collision handling remains
predictable.

### `SkillMetadata`

`SkillMetadata` contains startup-visible skill information only:

- `name`
- `description`
- `license`
- `compatibility`
- `metadata`
- raw `allowed-tools`
- normalized `allowed-tools`

This structure excludes the Markdown body so startup disclosure remains light.

### `SkillRecord`

`SkillRecord` represents a fully loaded valid skill. It combines:

- validated `SkillMetadata`
- absolute skill directory path
- absolute `SKILL.md` path
- source scope
- configured source ordering
- Markdown body

This is the record stored in the valid catalog after precedence resolution.

### `SkillDiagnostic`

`SkillDiagnostic` models non-fatal and fatal problems discovered during scan,
parse, and validation. It should contain enough information to explain:

- what went wrong
- where it happened
- which skill was involved if known
- which scope produced the issue
- which winning skill shadowed another skill when applicable

Phase 1 diagnostics must cover both invalid candidates and shadowed valid
skills.

### `SkillCatalog`

`SkillCatalog` stores only valid winning skills. It must never contain invalid
candidates and must not include shadowed entries.

## Configuration Foundation

Phase 1 extends `Config` with:

- `#[serde(default)] pub skills: SkillsConfig`

`SkillsConfig` must support the schema required by the plan:

- `enabled`
- `project_enabled`
- `user_enabled`
- `additional_paths`
- `max_discovered_skills`
- `max_scan_directories`
- `max_scan_depth`
- `catalog_max_entries`
- `activation_tool_enabled`
- `project_trust_required`
- `trust_store_path`
- `allow_custom_paths_without_trust`
- `strict_frontmatter`

Default values required by the plan:

- `enabled: true`
- `project_enabled: true`
- `user_enabled: true`
- `additional_paths: []`
- `max_discovered_skills: 256`
- `max_scan_directories: 2000`
- `max_scan_depth: 6`
- `catalog_max_entries: 128`
- `activation_tool_enabled: true`
- `project_trust_required: true`
- `trust_store_path: None`
- `allow_custom_paths_without_trust: false`
- `strict_frontmatter: true`

Phase 1 also includes environment variable overrides for:

- `XZATOMA_SKILLS_ENABLED`
- `XZATOMA_SKILLS_PROJECT_ENABLED`
- `XZATOMA_SKILLS_USER_ENABLED`
- `XZATOMA_SKILLS_ACTIVATION_TOOL_ENABLED`
- `XZATOMA_SKILLS_PROJECT_TRUST_REQUIRED`
- `XZATOMA_SKILLS_TRUST_STORE_PATH`

## Discovery Implementation

### Discovery Rules

Discovery must:

- scan only configured and supported roots
- visit at most `max_scan_directories`
- recurse at most `max_scan_depth`
- ignore directories without `SKILL.md`
- stop loading additional valid skills after `max_discovered_skills`
- continue collecting invalid diagnostics after the valid cap only when this is
  still cheap and bounded

### Root Order

Discovery walks roots in the precedence order defined earlier. This means higher
precedence roots are encountered first, but collision resolution must still be
explicit and deterministic rather than relying only on traversal order.

### Candidate Handling

For each candidate directory containing `SKILL.md`, Phase 1 does the following:

1. parse `SKILL.md`
2. validate the parsed result
3. collect diagnostics if invalid
4. add the skill to the valid candidate set if valid
5. resolve collisions after candidate collection
6. move only winning records into `SkillCatalog`

### Collision Resolution

When multiple valid skills share the same `name`, the winner is selected using:

1. lower precedence rank number wins
2. if same rank, earlier configured path wins
3. if same directory scope, lexicographically smaller absolute `SKILL.md` path
   wins

Every non-winning valid skill must generate a shadowed-skill diagnostic. Only
the winning skill is inserted into the catalog.

## Parser Implementation

### Input Format

Phase 1 parses `SKILL.md` files containing YAML frontmatter followed by Markdown
body.

Required parse fields:

- `name`
- `description`
- `license`
- `compatibility`
- `metadata`
- `allowed-tools`
- Markdown body

### Parser Responsibilities

The parser is responsible for:

- splitting frontmatter from body
- parsing YAML frontmatter
- preserving the Markdown body exactly
- preserving raw `allowed-tools`
- preserving raw metadata values in a form validation can normalize
- reporting malformed frontmatter distinctly from validation failures

### Missing Frontmatter

If frontmatter is missing, the candidate must be diagnosed as invalid and must
not be loaded into the catalog.

### Malformed Frontmatter

If frontmatter exists but is malformed YAML, the candidate must be diagnosed as
invalid and must not be loaded into the catalog.

## Validation Implementation

Validation applies the Phase 1 acceptance rules.

### Required Rules

A skill must be rejected when:

- `description` is missing
- `description` is empty
- `name` does not match the parent directory name
- `name` violates the supported format

### Name Format

Phase 1 uses a strict spec-compatible naming rule. The implementation should
accept lowercase snake_case names with digits permitted after the first
character. A practical validation pattern is:

- `^[a-z][a-z0-9_]*$`

### `allowed-tools`

The validator must preserve:

- the raw original string form
- a normalized vector form

Normalization should trim whitespace, discard empty entries, and support common
delimiter handling such as commas and newlines.

### Invalid Skill Handling

Invalid skills must remain visible only through diagnostics and validation
surfaces. They must never enter the valid catalog.

## Catalog Behavior

`SkillCatalog` is a deterministic valid-only map from skill name to
`SkillRecord`.

Expected behavior:

- insert only valid winning skills
- expose deterministic iteration order
- expose lookup by skill name
- reject or avoid duplicate unresolved entries
- remain separate from invalid diagnostic storage

This separation is important because later phases disclose only valid skills at
startup and via runtime activation surfaces.

## Testing Summary

Phase 1 requires tests for both parser behavior and discovery behavior.

### `tests/skills_parser.rs`

Required cases:

- invalid `name`
- invalid `description`
- missing frontmatter
- malformed frontmatter

Additional useful checks:

- valid `allowed-tools` normalization
- name and parent directory mismatch
- metadata preservation
- body extraction

### `tests/skills_discovery.rs`

Required cases:

- valid project skill discovery
- valid user skill discovery
- custom path discovery
- collision precedence
- shadowed valid skill diagnostics

Additional useful checks:

- max scan depth enforcement
- max scan directory enforcement
- max discovered skills cap
- invalid skills excluded from catalog
- lexicographic tie-breaking for same-rank collisions

## Runtime Behavior Achieved in Phase 1

After Phase 1, the runtime should be able to:

- discover skills from supported project, user, and custom locations
- parse and validate `SKILL.md`
- exclude invalid candidates from the loaded set
- keep diagnostics for invalid and shadowed candidates
- build a deterministic valid-only catalog

What Phase 1 does not yet do:

- disclose skills to the model at startup
- register an activation tool
- activate skills into session state
- inject active skills into prompts
- enforce project trust state
- expose CLI `skills` commands

Those responsibilities are deferred to later phases.

## Deliverables Checklist

Phase 1 is complete only when all of the following exist and pass verification:

- `src/skills/` module tree exists
- `src/lib.rs` exports `skills`
- valid skill discovery is implemented
- invalid skills are excluded from the loaded catalog
- diagnostics model is implemented
- discovery and parser tests pass
- `cargo test --all-features` passes

## Quality Gates

Per project rules, the following commands must pass before Phase 1 is considered
complete:

```/dev/null/commands.sh#L1-4
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

This documentation file must also pass Markdown linting and formatting checks.

## Notes on Public API Documentation

All public modules, structs, enums, and functions introduced for Phase 1 should
include `///` doc comments with runnable examples where practical. This is
required by project policy and is especially important here because the skills
subsystem introduces a new public surface area in the crate.

## Summary

Phase 1 establishes the parsing and discovery foundation for agent skills
without taking on activation or prompt integration yet.

The key architectural result is a clean split between:

- parsed but invalid discovered candidates
- valid loaded skill metadata and records
- diagnostics explaining invalid or shadowed states
- a deterministic catalog of valid winners only

That split is the core dependency for all later skill phases.
