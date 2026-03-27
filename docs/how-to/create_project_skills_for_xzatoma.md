# Create project skills for XZatoma

This guide shows you how to create project-local agent skills for XZatoma,
organize them correctly, validate them, and make them available safely in a
repository.

## When to use project skills

Project skills are useful when you want XZatoma to have repository-specific
instructions such as:

- build and test workflows
- deployment steps
- coding conventions for the current repository
- troubleshooting playbooks
- architecture context
- safe operational runbooks

Use project skills when the instructions belong with the project rather than
your personal user profile.

## Before you start

Make sure you understand the two project-level discovery locations XZatoma
checks:

- `.xzatoma/skills/`
- `.agents/skills/`

For XZatoma-specific project skills, prefer `.xzatoma/skills/`.

Project-level skills may also require trust before they become visible,
depending on your `skills.project_trust_required` setting.

## Skill directory layout

Each skill lives in its own directory and must contain a `SKILL.md` file.

Example:

```text
.your-project/
└── .xzatoma/
    └── skills/
        └── release_checklist/
            └── SKILL.md
```

You can also include supporting resources inside the skill directory. XZatoma
supports resource discovery under these subdirectories:

- `scripts/`
- `references/`
- `assets/`

Example with resources:

```text
.xzatoma/
└── skills/
    └── release_checklist/
        ├── SKILL.md
        ├── scripts/
        │   └── verify_release.sh
        ├── references/
        │   └── release_process.md
        └── assets/
            └── architecture.png
```

## Write the `SKILL.md` file

A skill file contains frontmatter followed by the skill body.

Required frontmatter fields:

- `name`
- `description`

Optional metadata includes:

- `license`
- `compatibility`
- `allowed-tools`
- additional metadata fields your team wants to preserve

### Naming rules

Use a stable lowercase name with underscores.

Good examples:

- `release_checklist`
- `investigate_ci_failures`
- `service_bootstrap`

Avoid names with spaces, uppercase letters, or punctuation.

## Minimal example

```md
---
name: release_checklist
description: Steps for validating and preparing a release in this repository.
allowed-tools: read_file, grep, list_directory, terminal
---

# Release checklist

Use this skill when preparing or validating a release.

## Steps

1. Confirm the working tree state with the repository owner.
2. Review release notes inputs in `references/release_process.md`.
3. Run the required validation commands for this repository.
4. Summarize any blockers before proceeding.

## Resources

- `references/release_process.md`
- `scripts/verify_release.sh`
```

## Recommended authoring pattern

Write skills so they are:

- specific to one job
- short enough to activate intentionally
- explicit about inputs, outputs, and constraints
- careful about destructive actions
- easy to maintain with the repository

A strong skill usually includes:

1. purpose
2. when to use it
3. required context
4. ordered steps
5. validation checks
6. resource references
7. escalation or stop conditions

## Example: CI investigation skill

```md
---
name: investigate_ci_failures
description:
  Investigate failing CI jobs for this repository and summarize likely causes.
allowed-tools: read_file, grep, find_path, terminal
compatibility: xzatoma-phase5
---

# Investigate CI failures

Use this skill when CI is failing and you need a structured debugging pass.

## Goals

- identify the first failing stage
- locate the relevant configuration and source files
- summarize likely root causes
- propose the smallest safe next step

## Procedure

1. Inspect CI configuration files in the repository.
2. Locate the failing job definitions and referenced scripts.
3. Review the code and configuration touched by the failing area.
4. Run safe local validation commands if appropriate.
5. Produce a concise findings summary.

## Stop conditions

Stop and report immediately if the issue appears to require secrets, production
access, or irreversible actions.

## Resources

- `references/ci_overview.md`
- `scripts/reproduce_ci.sh`
```

## Add supporting resources

Resources should stay inside the same skill directory. This keeps the skill
self-contained and avoids unsafe cross-directory assumptions.

Recommended structure:

- `scripts/` for helper scripts
- `references/` for procedural or architectural notes
- `assets/` for images or static files

Keep resource references relative to the skill root. Do not design skills that
depend on reading files outside the skill directory as if they were bundled
resources.

## Validate the skill from the CLI

After creating the skill, use the skills commands to validate discovery and
visibility.

List visible skills:

```text
xzatoma skills list
```

Validate all configured skill roots and inspect diagnostics:

```text
xzatoma skills validate
```

Show one visible skill:

```text
xzatoma skills show release_checklist
```

Show discovery roots and trust state:

```text
xzatoma skills paths
```

## Trust the project skill root when required

If project trust is enabled, a project-local skill may exist on disk but remain
hidden until the project path is trusted.

Check trust state:

```text
xzatoma skills trust show
```

Add trust for the current project path:

```text
xzatoma skills trust add .
```

Remove trust later if needed:

```text
xzatoma skills trust remove .
```

After adding trust, run validation again:

```text
xzatoma skills validate
```

## What makes a skill visible

A skill is usable only when it is:

- discovered from an enabled root
- valid
- not shadowed by a higher-precedence skill with the same name
- allowed by trust rules

`xzatoma skills list` shows only valid visible skills.

`xzatoma skills validate` is the command to use when you want to inspect invalid
skills, hidden skills, and diagnostics.

## Precedence and collisions

If multiple skills share the same name, XZatoma applies deterministic
precedence. Project-local skills can override lower-priority locations.

To avoid confusion:

- keep names unique within your repository
- avoid reusing a shared or user-level name unless override behavior is
  intentional
- use `xzatoma skills validate` to detect collisions and shadowing

## Writing effective project skills

Use these guidelines:

- describe exactly what the skill is for
- prefer repository terminology your team already uses
- include concrete file or directory hints when they are stable
- state the expected validation commands
- explain when the agent should stop and ask for confirmation
- keep instructions current as the repository evolves

## Common mistakes

### Missing required frontmatter

If `name` or `description` is missing, the skill will be treated as invalid.

### Invalid skill name

If the name uses unsupported characters or formatting, the skill will not load.

### Putting the file in the wrong location

A skill must live under a discovered root and inside its own directory with a
`SKILL.md` file.

### Assuming project skills are always visible

When trust is required, untrusted project paths remain hidden from normal skill
listing and activation.

### Treating `allowed-tools` as enforcement

`allowed-tools` is advisory metadata. It helps document intended usage, but it
is not the runtime enforcement mechanism.

## Suggested team workflow

A practical team workflow is:

1. create the skill under `.xzatoma/skills/<skill_name>/SKILL.md`
2. add any local `scripts/`, `references/`, or `assets/`
3. run `xzatoma skills validate`
4. trust the project path if required
5. run `xzatoma skills list`
6. review the skill output with `xzatoma skills show <name>`
7. keep the skill updated as the repository changes

## Example repository layout

```text
my_repo/
├── .xzatoma/
│   └── skills/
│       ├── release_checklist/
│       │   ├── SKILL.md
│       │   └── references/
│       │       └── release_process.md
│       └── investigate_ci_failures/
│           ├── SKILL.md
│           └── scripts/
│               └── reproduce_ci.sh
├── src/
├── Cargo.toml
└── README.md
```

## Next steps

After you create project skills, you may also want to:

- configure global user skills for personal reusable workflows
- document team conventions in skill references
- add repository-specific validation scripts under each skill
- review the skills CLI reference for operational commands

See also:

- `docs/how-to/use_agent_skills.md`
- `docs/reference/agent_skills_configuration.md`
- `docs/reference/agent_skills_cli.md`
- `docs/reference/agent_skills_behavior.md`
