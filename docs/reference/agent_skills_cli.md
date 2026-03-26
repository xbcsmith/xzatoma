# Agent Skills CLI Reference

This reference documents the `xzatoma skills` command tree for discovering,
validating, inspecting, and trusting agent skills.

## Overview

The agent skills CLI provides operational visibility into the skill system
without starting an interactive chat or run session.

Use these commands to:

- list valid visible skills
- validate configured discovery roots
- inspect one visible skill
- view effective discovery paths and trust state
- manage trusted project and custom skill roots

## Command Tree

```/dev/null/agent_skills_cli.txt#L1-8
xzatoma skills list
xzatoma skills validate
xzatoma skills show <name>
xzatoma skills paths
xzatoma skills trust show
xzatoma skills trust add <path>
xzatoma skills trust remove <path>
```

## General Behavior

### Visibility rules

The CLI follows the same visibility model used by runtime skill disclosure:

- invalid skills are never listed by `skills list`
- invalid skills are never returned by `skills show <name>`
- hidden skills are not shown by `skills list`
- hidden skills cause `skills show <name>` to fail
- `skills validate` is the diagnostic command and may report invalid skills
- project-level skills may require trust before they become visible
- custom configured roots may require trust unless configuration allows
  trust-free custom paths

### Trust model

Trust is enforced for skill visibility, not just discovery.

This means a skill can exist on disk and still be omitted from `skills list`
when its root is not trusted under the active configuration.

### Determinism

Command output is intended to be deterministic:

- discovery roots are processed in a fixed precedence order
- trust entries are stored in deterministic order
- visible skill results are displayed in stable name order

## `xzatoma skills list`

Lists valid visible skills only.

### Purpose

Use this command to see which skills are currently available for runtime
disclosure and activation.

### Output semantics

The command shows only skills that are:

- valid
- not shadowed by a higher-precedence skill
- visible under the current trust configuration

If no visible skills exist, the command prints a no-results message.

### Example

```/dev/null/agent_skills_cli.txt#L10-13
$ xzatoma skills list
build_docs Generate and refresh project documentation /workspace/project/.xzatoma/skills/build_docs/SKILL.md
release_notes Create structured release notes /Users/alice/.xzatoma/skills/release_notes/SKILL.md
```

### Notes

This command does not show:

- invalid skills
- hidden skills
- trust diagnostics
- shadowed skills

Use `xzatoma skills validate` for diagnostics.

## `xzatoma skills validate`

Validates configured discovery roots and prints discovery diagnostics.

### Purpose

Use this command to inspect the full health of the skill system, including valid
visible skills and invalid skill diagnostics.

### Output semantics

The command may show:

- valid visible skills
- invalid skill diagnostics
- shadowed skill diagnostics

This is the primary operational troubleshooting command for agent skills.

### Example

```/dev/null/agent_skills_cli.txt#L15-27
$ xzatoma skills validate
Valid visible skills:
- build_docs
  description: Generate and refresh project documentation
  location: /workspace/project/.xzatoma/skills/build_docs/SKILL.md
  scope: project_client_specific

Invalid skill diagnostics:
- [missing_description] Skill frontmatter is missing a required description field (/workspace/project/.xzatoma/skills/bad_skill/SKILL.md)

Shadowed skill diagnostics:
- [shadowed_skill] Skill 'build_docs' was shadowed by a higher precedence definition (/Users/alice/.xzatoma/skills/build_docs/SKILL.md) shadowed_by=/workspace/project/.xzatoma/skills/build_docs/SKILL.md
```

### Notes

Unlike `skills list`, this command is allowed to surface invalid skill state.

## `xzatoma skills show <name>`

Shows metadata for one valid visible skill.

### Purpose

Use this command to inspect the metadata for a specific visible skill.

### Arguments

- `name` - skill name

### Output semantics

The command succeeds only when the named skill is:

- present
- valid
- visible in the current trust context

If the skill is missing, invalid, shadowed, or hidden by trust rules, the
command returns an error.

### Example

```/dev/null/agent_skills_cli.txt#L29-38
$ xzatoma skills show build_docs
name: build_docs
description: Generate and refresh project documentation
scope: project_client_specific
skill_dir: /workspace/project/.xzatoma/skills/build_docs
skill_file: /workspace/project/.xzatoma/skills/build_docs/SKILL.md
license: MIT
compatibility: xzatoma>=0.2.0
allowed_tools: read_file, grep, edit_file
```

### Advisory metadata

If present, the command may also print:

- `license`
- `compatibility`
- `allowed_tools`
- arbitrary metadata key-value pairs

The `allowed_tools` field is advisory metadata only. It does not grant tool
permissions by itself.

## `xzatoma skills paths`

Shows effective discovery paths and trust state.

### Purpose

Use this command to understand where XZatoma is looking for skills and how trust
is configured.

### Output semantics

The command prints:

- current working directory
- resolved trust store path
- trust-related configuration flags
- configured discovery roots
- trusted paths

Depending on configuration, roots can include:

- project client-specific root
- project shared convention root
- user client-specific root
- user shared convention root
- configured custom roots

### Example

```/dev/null/agent_skills_cli.txt#L40-55
$ xzatoma skills paths
working_dir: /workspace/project
trust_store: /Users/alice/.xzatoma/skills_trust.yaml
project_trust_required: true
allow_custom_paths_without_trust: false

configured discovery roots:
- project_client_specific: /workspace/project/.xzatoma/skills
- project_shared_convention: /workspace/project/.agents/skills
- user_client_specific: /Users/alice/.xzatoma/skills
- user_shared_convention: /Users/alice/.agents/skills
- custom_0: /opt/xzatoma/skills

trusted paths:
- /workspace/project
- /opt/xzatoma/skills
```

### Interpretation guidance

Use this command to answer questions such as:

- why is a project skill not visible?
- which roots are active right now?
- where is trust state stored?
- which custom roots are currently trusted?

## `xzatoma skills trust show`

Shows trust configuration and trusted paths.

### Purpose

Use this command to inspect the persistent trust store for skills.

### Output semantics

The command prints:

- trust store file path
- trust-related configuration flags
- trusted paths from the trust store

### Example

```/dev/null/agent_skills_cli.txt#L57-65
$ xzatoma skills trust show
trust_store: /Users/alice/.xzatoma/skills_trust.yaml
project_trust_required: true
allow_custom_paths_without_trust: false
trusted_paths:
- /workspace/project
- /opt/xzatoma/skills
```

If no paths are trusted, the command prints an explicit empty-state indicator.

## `xzatoma skills trust add <path>`

Marks a path as trusted.

### Purpose

Use this command to trust a project root or custom skill root so that skills
under that root can become visible when trust is required.

### Arguments

- `path` - path to trust

### Behavior

The path is normalized and persisted in the trust store. Repeated additions of
the same path are idempotent at the stored-data level.

### Example

```/dev/null/agent_skills_cli.txt#L67-70
$ xzatoma skills trust add /workspace/project
Trusted path added: /workspace/project
trust_store: /Users/alice/.xzatoma/skills_trust.yaml
```

### When to use

Typical cases include:

- trusting the current project root for project-local skills
- trusting a shared custom skills directory
- preparing a new environment for skill development

## `xzatoma skills trust remove <path>`

Removes a trusted path.

### Purpose

Use this command to revoke trust for a previously trusted root.

### Arguments

- `path` - path to remove from trust

### Behavior

If the path is present in the trust store, it is removed and persisted. If it
was not trusted, the command reports that state explicitly.

### Example

```/dev/null/agent_skills_cli.txt#L72-78
$ xzatoma skills trust remove /workspace/project
Trusted path removed: /workspace/project
trust_store: /Users/alice/.xzatoma/skills_trust.yaml

$ xzatoma skills trust remove /workspace/project
Path was not trusted: /workspace/project
trust_store: /Users/alice/.xzatoma/skills_trust.yaml
```

## Exit Behavior

The exact exit-code behavior is implementation-defined by the CLI runtime, but
in general:

- successful inspection commands return success
- invalid configuration returns failure
- missing or hidden skills for `show` return failure
- trust store persistence failures return failure
- malformed command usage returns failure

## Relationship to Runtime Skill Usage

These commands are operational interfaces. They do not themselves activate
skills for a chat or run session.

Runtime activation still occurs through the dedicated skill activation contract,
not through direct CLI inspection commands.

## See Also

- [Agent Skills Configuration](agent_skills_configuration.md)
- [Agent Skills Behavior](agent_skills_behavior.md)
- [Use Agent Skills](../how-to/use_agent_skills.md)
- [Create Project Skills for XZatoma](../how-to/create_project_skills_for_xzatoma.md)
