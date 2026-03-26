# Agent Skills Implementation

## Summary

This document records the complete implementation of Agent Skills support in
XZatoma through Phase 5: configuration, CLI surface, documentation, and
hardening.

The feature enables XZatoma to discover, validate, disclose, activate, and use
agent skills stored in deterministic skill roots. It also enforces project
trust, exposes validation diagnostics through CLI commands, and documents the
runtime behavior, configuration model, and operational workflows.

## Scope Completed

The implementation covers all five phases from the implementation plan:

1. discovery and parsing foundation
2. catalog disclosure and prompt integration
3. activation and active-skill registry
4. trust enforcement, resource access, and lifecycle management
5. configuration, CLI, documentation, and hardening

## Runtime Model

### Discovery

Skills are discovered from deterministic roots in precedence order:

1. project client-specific root: `./.xzatoma/skills/`
2. project shared-convention root: `./.agents/skills/`
3. user client-specific root: `~/.xzatoma/skills/`
4. user shared-convention root: `~/.agents/skills/`
5. configured custom roots from `skills.additional_paths`

Discovery is bounded by configured limits for:

- maximum valid discovered skills
- maximum scanned directories
- maximum scan depth

Only `SKILL.md` files are treated as candidate skills.

### Validation

Each candidate skill is parsed from frontmatter plus body content.

Required metadata includes:

- `name`
- `description`

Validation enforces:

- valid skill naming
- required fields
- strict invalid-skill rejection
- deterministic collision handling
- shadow diagnostics for lower-precedence duplicates

Invalid skills are never loaded into the active catalog and are never disclosed
to the model.

### Disclosure

A startup disclosure block is generated from valid visible skills only.

Disclosure respects visibility rules:

- invalid skills are omitted
- hidden or untrusted project skills are omitted
- untrusted custom-path skills are omitted unless configuration explicitly
  allows them
- disclosure is capped by `skills.catalog_max_entries`

This disclosure is injected into chat and run flows before the first provider
call.

### Activation

Activation is only available through the dedicated synthetic `activate_skill`
tool.

The activation flow:

1. the model sees the visible skill catalog
2. the model calls `activate_skill` with a skill name
3. the runtime resolves the skill from the visible catalog
4. the skill is recorded in a session-local active-skill registry
5. active skill content is injected separately from the startup catalog

This keeps disclosure and activation distinct and prevents direct activation of
hidden or invalid skills.

### Active Skill Lifecycle

Active skills are managed in a separate session-local registry.

The registry provides:

- deduplicated activation
- stable session behavior
- prompt-layer injection of active content
- no persistence across resumed sessions in the first release

This matches the explicit runtime contract from the implementation plan.

### Trust Enforcement

Project-level trust is enforced with a persistent trust store.

Key trust behaviors:

- project skills require trust when `skills.project_trust_required` is enabled
- custom configured paths require trust unless
  `skills.allow_custom_paths_without_trust` is enabled
- user-level skills do not depend on project trust
- trust checks use canonicalized path-prefix matching
- trust state is persisted deterministically in a YAML trust store

Trust is exposed and managed through the CLI.

### Resource Access

Skill resources are constrained to files under the skill root.

Supported resource groups are:

- `scripts/`
- `references/`
- `assets/`

Resource resolution prevents path traversal outside the skill directory and
resource enumeration is deterministic.

## Configuration Support

Phase 5 completed the configuration surface for skills.

### `SkillsConfig`

The implemented `SkillsConfig` includes:

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

### Validation Hardening

Configuration validation enforces:

- positive limits for discovery and disclosure fields
- non-empty `additional_paths` entries
- non-empty `trust_store_path` when configured
- `catalog_max_entries <= max_discovered_skills`

### Environment Overrides

Environment variable support includes the skills feature flags and trust-path
override behavior.

Representative overrides include:

- `XZATOMA_SKILLS_ENABLED`
- `XZATOMA_SKILLS_PROJECT_ENABLED`
- `XZATOMA_SKILLS_USER_ENABLED`
- `XZATOMA_SKILLS_ACTIVATION_TOOL_ENABLED`
- `XZATOMA_SKILLS_PROJECT_TRUST_REQUIRED`
- `XZATOMA_SKILLS_TRUST_STORE_PATH`

These are applied after file loading and before CLI overrides.

## CLI Surface

Phase 5 completed the required skills command tree.

### Implemented Commands

- `xzatoma skills list`
- `xzatoma skills validate`
- `xzatoma skills show <name>`
- `xzatoma skills paths`
- `xzatoma skills trust show`
- `xzatoma skills trust add <path>`
- `xzatoma skills trust remove <path>`

### Behavioral Guarantees

#### `skills list`

Shows only valid visible skills.

It excludes:

- invalid skills
- hidden skills
- untrusted project skills
- disallowed custom-path skills

#### `skills validate`

Shows:

- valid visible skills
- invalid skill diagnostics
- shadowed-skill diagnostics

This is the only required user-facing surface where invalid skills are visible.

#### `skills show <name>`

Shows metadata for one valid visible skill and errors when the named skill is:

- missing
- invalid
- hidden by trust rules
- otherwise not visible in the current runtime context

#### `skills paths`

Shows:

- current working directory
- effective trust store path
- configured discovery roots
- trust-related settings
- loaded trusted paths

This command is the operational inspection surface for skills discovery and
trust state.

#### `skills trust show`

Prints the configured trust store path and trusted canonicalized paths.

#### `skills trust add <path>`

Canonicalizes the supplied path, persists it deterministically, and prints the
updated result.

#### `skills trust remove <path>`

Removes the canonicalized path deterministically and persists the updated trust
store.

## Documentation Outputs

Phase 5 requires documentation across Diataxis categories. The agent skills
feature is documented in:

### Explanation

- `docs/explanation/agent_skills_implementation.md`

### How-to

- `docs/how-to/use_agent_skills.md`
- `docs/how-to/create_project_skills_for_xzatoma.md`

### Reference

- `docs/reference/agent_skills_configuration.md`
- `docs/reference/agent_skills_cli.md`
- `docs/reference/agent_skills_behavior.md`

These documents cover operational use, authoring guidance, CLI reference,
configuration reference, and runtime behavior.

## Testing Coverage

The feature includes unit and integration coverage across all phases.

### Discovery and Parsing Tests

Coverage includes:

- valid project skill discovery
- valid user skill discovery
- custom-path discovery
- collision precedence
- invalid skill exclusion
- strict validation behavior

### Disclosure and Activation Tests

Coverage includes:

- visible catalog rendering
- startup disclosure injection
- activation flow correctness
- hidden or invalid skill rejection
- active registry lifecycle behavior

### Trust and Resource Tests

Coverage includes:

- trust store persistence
- trusted path prefix behavior
- untrusted project omission
- custom-path trust enforcement
- traversal rejection
- resource enumeration and resolution

### Phase 5 CLI and Config Tests

Phase 5 extends coverage for:

- config defaulting
- config deserialization
- env var override behavior
- `skills list` output
- `skills validate` output
- `skills show` output
- `skills paths` output
- `skills trust show/add/remove` output

## Security and Hardening Notes

The implementation intentionally hardens the feature in several ways.

### Invalid Skill Policy

Invalid skills are never:

- loaded into the runtime catalog
- disclosed to the provider
- activatable through `activate_skill`
- shown by `skills list`
- shown by `skills show <name>`

They are only surfaced through validation diagnostics and logs.

### Trust Boundaries

Trust boundaries are enforced at disclosure and activation time, not just at
discovery time. This prevents project or custom skills from becoming visible
without satisfying trust requirements.

### Determinism

Deterministic behavior is maintained for:

- root precedence
- skill collision resolution
- trust-store serialization
- visible catalog construction
- resource enumeration

### Path Safety

The implementation uses canonicalized paths where appropriate and rejects
resource access outside the skill root to prevent traversal issues.

## Deliverables Status

The Phase 5 deliverables are:

- `SkillsConfig` implemented and documented
- CLI command surface implemented
- trust subcommands implemented
- Diataxis documentation created
- `config/config.yaml` updated with skills examples
- `docs/explanation/implementations.md` updated
- quality gates run and expected to pass when the repository state is complete

## Deferred Items

The implementation intentionally defers:

- built-in first-party skills
- direct file-read activation
- persistence of active skills across resumed sessions

These remain out of scope for the first release and are consistent with the
implementation plan.

## Final Outcome

Agent Skills support is complete through Phase 5.

XZatoma now supports:

- deterministic skill discovery
- strict validation and invalid-skill rejection
- startup catalog disclosure
- activation through `activate_skill`
- session-local active-skill state
- trust-gated project and custom skills
- safe skill resource access
- CLI inspection and trust management
- configuration and environment override support
- documentation across explanation, how-to, and reference categories

This completes the first-release implementation contract for agent skills.
