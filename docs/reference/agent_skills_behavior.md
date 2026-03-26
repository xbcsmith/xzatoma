# Agent Skills Behavior Reference

This reference defines the runtime behavior of agent skills in XZatoma.

## Overview

Agent skills are discoverable capability documents stored as `SKILL.md` files in
well-known directories. XZatoma discovers valid skills, filters them according
to trust and visibility rules, discloses visible skills to the model, and allows
runtime activation through the dedicated `activate_skill` tool.

The first release intentionally keeps the behavior simple and explicit:

- invalid skills are never loaded into the usable catalog
- invalid skills are never disclosed to the model
- activation only happens through `activate_skill`
- active skill content is session-local and not persisted across resumed
  sessions
- project trust is enforced separately from discovery
- `allowed-tools` is advisory metadata only

## Discovery Behavior

### Discovery roots

When enabled, XZatoma scans skill roots in deterministic order.

Project-scoped roots:

1. `./.xzatoma/skills/`
2. `./.agents/skills/`

User-scoped roots:

1. `~/.xzatoma/skills/`
2. `~/.agents/skills/`

Additional configured roots:

1. entries from `skills.additional_paths`, in listed order

### Discovery ordering and precedence

Discovery order matters because collisions are resolved deterministically.

Higher-priority roots win over lower-priority roots. In practice:

1. project client-specific roots win first
2. project shared-convention roots win next
3. user client-specific roots win next
4. user shared-convention roots win next
5. configured additional paths are considered after built-in roots, in listed
   order

If two valid skills share the same `name`, the higher-priority skill is kept and
the lower-priority one is recorded as shadowed.

### Discovery limits

Discovery is bounded by configuration:

- `skills.max_discovered_skills`
- `skills.max_scan_directories`
- `skills.max_scan_depth`

These limits protect startup behavior and prevent unbounded scanning.

## Skill File Requirements

A skill must be stored in a file named `SKILL.md`.

Each skill file must contain valid YAML frontmatter and a Markdown body.

Required frontmatter fields:

- `name`
- `description`

Optional frontmatter fields:

- `license`
- `compatibility`
- `allowed-tools`
- additional metadata fields

### Skill name rules

Skill names must be valid normalized identifiers suitable for deterministic
lookup and activation. Invalid names are rejected during validation.

### Frontmatter strictness

When `skills.strict_frontmatter` is enabled, malformed frontmatter is rejected
without fallback behavior. Invalid files do not enter the usable catalog.

## Valid, Invalid, and Shadowed Skills

### Valid skills

A valid skill:

- is discovered from an enabled root
- contains parseable frontmatter
- passes field validation
- survives precedence resolution

Valid skills are eligible for catalog disclosure and activation, subject to
visibility and trust rules.

### Invalid skills

An invalid skill is any discovered `SKILL.md` that fails parsing or validation.

Invalid skills:

- are not loaded into the runtime catalog
- are not disclosed at startup
- cannot be activated
- can appear in validation output and logs

### Shadowed skills

A shadowed skill is valid but loses precedence to another valid skill with the
same name.

Shadowed skills:

- are excluded from the active runtime catalog
- are excluded from startup disclosure
- are reported by validation output

## Visibility Rules

Visibility controls whether a valid discovered skill is exposed to the model and
to user-facing list commands.

A skill must be both valid and visible to appear in normal operational surfaces.

### Project skills

When `skills.project_trust_required` is `true`, project-level skills are visible
only if the current project path is trusted.

When `skills.project_trust_required` is `false`, valid project-level skills are
visible without trust gating.

### User skills

User-level skills do not require project trust. Valid user skills are visible if
skill discovery is enabled and the user roots are enabled.

### Custom configured paths

Valid skills found through `skills.additional_paths` are visible only when the
path is trusted, unless `skills.allow_custom_paths_without_trust` is `true`.

## Trust Behavior

Trust determines whether project and custom skill roots are operationally
visible.

### Trust store

Trusted paths are persisted in a YAML trust store. The default trust store path
is under the XZatoma home directory unless overridden by
`skills.trust_store_path`.

### Trust checks

Trust checks use canonicalized paths and prefix matching.

This means:

- trusting a project root trusts matching skill paths underneath it
- nested skill directories are covered by a trusted ancestor path
- path comparisons are deterministic after canonicalization

### Trust mutations

Trust is managed through CLI commands:

- `xzatoma skills trust show`
- `xzatoma skills trust add <path>`
- `xzatoma skills trust remove <path>`

Trust updates are persisted deterministically.

## Catalog Disclosure Behavior

At startup, XZatoma may build a skill catalog disclosure section for the model.

The disclosure includes only skills that are:

- valid
- visible
- within disclosure limits

Disclosure behavior is bounded by `skills.catalog_max_entries`.

If no valid visible skills are available, no skill disclosure block is injected.

Invalid and shadowed skills are never disclosed.

## Activation Behavior

### Activation contract

Activation is only available through the dedicated `activate_skill` tool.

Direct activation through arbitrary file reads is not part of the supported
contract.

### Activation lookup

When the model invokes `activate_skill`, the requested skill name is resolved
against the visible catalog for the session.

Activation fails when the skill is:

- missing
- invalid
- shadowed
- hidden by trust or visibility rules

### Activation result

Successful activation registers the skill in a session-local active-skill
registry.

The activation result may surface metadata such as:

- skill name
- description
- source location
- advisory `allowed-tools`

### Deduplication

Repeated activation of the same skill should not create duplicate active-skill
entries for the session.

## Active-Skill Runtime Model

Active skills are stored in a separate session-local registry.

This runtime model exists to keep discovery separate from active prompt
injection.

### Session-local lifecycle

Active skills:

- exist only for the current running session
- are not persisted across resumed sessions in the first release
- can be used after activation in both chat and run flows

### Prompt injection behavior

Active skill content is injected into the provider prompt from the session-local
registry, not from the disclosed startup catalog itself.

This separation is intentional:

- disclosure advertises what can be activated
- activation controls what becomes active context
- active skill content is distinct from passive catalog visibility

## Resource Access Behavior

Skills may contain supporting resources under their skill root.

Supported resource directories are:

- `scripts/`
- `references/`
- `assets/`

### Resource enumeration

XZatoma can enumerate supported resources within the skill root.

Enumeration is constrained to supported directories and does not permit escape
outside the skill root.

### Resource resolution

Resource paths are resolved relative to the owning skill root only.

Path traversal outside the skill root is rejected. Absolute paths and unsafe
relative paths are not allowed.

## Advisory Metadata Behavior

The `allowed-tools` field is parsed and surfaced as advisory metadata only.

In the first release, `allowed-tools` does not enforce hard tool restrictions.
It is informational and may be shown in activation or inspection output.

## CLI Surface Behavior

### `xzatoma skills list`

Shows only valid visible skills.

This command excludes:

- invalid skills
- shadowed skills
- hidden project skills
- hidden custom-path skills

### `xzatoma skills validate`

Shows the discovery result, including:

- valid visible skills
- invalid skill diagnostics
- shadowed skill diagnostics

This is the primary operator-facing command for understanding why a skill is not
usable.

### `xzatoma skills show <name>`

Shows metadata for one valid visible skill.

This command errors if the skill is:

- missing
- invalid
- shadowed
- hidden by trust or visibility rules

### `xzatoma skills paths`

Shows effective discovery roots and trust-related state, including:

- working directory
- effective trust store path
- configured discovery roots
- trusted paths
- relevant trust configuration

### `xzatoma skills trust show`

Shows current trust configuration and trusted paths.

### `xzatoma skills trust add <path>`

Adds a path to the trust store.

### `xzatoma skills trust remove <path>`

Removes a path from the trust store.

## Environment Variable Override Behavior

Selected skill behavior can be overridden with environment variables.

These overrides are applied after file configuration is loaded.

Supported override areas include:

- skills enabled state
- project root discovery enabled state
- user root discovery enabled state
- activation tool enabled state
- project trust required state
- trust store path

Environment overrides do not change the runtime behavior contract. They only
change effective configuration values.

## Failure and Diagnostics Behavior

Skill failures are intended to degrade safely.

### Safe failure properties

When a skill is malformed or hidden:

- it is not loaded into the operational catalog
- it is not disclosed to the model
- it cannot be activated accidentally

### Operator diagnostics

Operators can inspect problems with:

- `xzatoma skills validate`
- log output
- trust inspection commands

This design prevents malformed or untrusted skills from silently becoming active
runtime context.

## First-Release Non-Goals

The following behaviors are intentionally out of scope for the first release:

- built-in first-party skill bundles
- direct file-read activation as an official mechanism
- persistent active skills across resumed sessions
- hard enforcement of `allowed-tools`

## Behavior Summary

The effective runtime contract is:

1. discover skills from deterministic roots
2. reject invalid skills
3. resolve name collisions by precedence
4. apply trust-based visibility filtering
5. disclose only valid visible skills
6. allow activation only through `activate_skill`
7. inject active skill content from a session-local registry
8. keep resource access constrained to the skill root
9. expose diagnostics through validation and trust commands
