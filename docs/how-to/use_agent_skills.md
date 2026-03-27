# Use agent skills

This guide shows you how to discover, trust, inspect, and use agent skills in
XZatoma.

## What agent skills are

Agent skills are reusable capability packages discovered from supported skill
directories. A skill is defined by a `SKILL.md` file with required frontmatter
metadata and optional supporting resources such as scripts, references, or
assets.

A valid skill can be:

- discovered by XZatoma
- disclosed to the model when visible in the current trust context
- activated through the `activate_skill` tool during a session
- injected into the session from a separate active-skill registry

Invalid skills are never loaded, never disclosed, and never available for
activation.

## Before you start

Make sure you have:

- a working XZatoma installation
- a valid `config/config.yaml`
- one or more skill directories containing `SKILL.md`
- permission to trust project skill roots when project trust is required

## Skill discovery locations

XZatoma discovers skills from these roots, in effective precedence order:

1. project client-specific skills: `./.xzatoma/skills/`
2. project shared convention skills: `./.agents/skills/`
3. user client-specific skills: `~/.xzatoma/skills/`
4. user shared convention skills: `~/.agents/skills/`
5. custom configured roots from `skills.additional_paths`

If multiple valid skills share the same skill name, higher-precedence roots win
and lower-precedence matches are shadowed.

## Step 1: Check effective paths and trust state

Start by inspecting the paths XZatoma will use:

```text
xzatoma skills paths
```

This command shows:

- the current working directory
- the effective trust-store location
- whether project trust is required
- whether custom paths can bypass trust
- the configured discovery roots
- the currently trusted paths

Use this first whenever skills are not appearing as expected.

## Step 2: Validate all skills

Validate your configured roots before trying to use a skill:

```text
xzatoma skills validate
```

This command shows:

- valid visible skills
- invalid skill diagnostics
- shadowed skill diagnostics

Use `skills validate` when:

- a skill is not showing up
- you want to confirm frontmatter is valid
- you suspect a naming collision
- you want to inspect discovery behavior safely

## Step 3: List visible skills

List the skills that are both valid and visible:

```text
xzatoma skills list
```

This output includes only skills that are eligible for use in the current trust
context.

If a skill exists on disk but does not appear here, the likely causes are:

- the skill is invalid
- the skill is shadowed by a higher-precedence skill
- the skill is hidden by trust rules
- the skill root is not enabled or not configured

## Step 4: Trust a project or custom skill root

If project trust is required, project-level skills remain hidden until you trust
the relevant path.

Show the current trust store contents:

```text
xzatoma skills trust show
```

Trust a project root:

```text
xzatoma skills trust add /path/to/project
```

Remove trust later if needed:

```text
xzatoma skills trust remove /path/to/project
```

### When to trust

Trust the project root when you want XZatoma to expose valid project skills
from:

- `./.xzatoma/skills/`
- `./.agents/skills/`

Trust a custom configured root when your configuration points
`skills.additional_paths` at a path that should be treated as trusted.

### Trust behavior notes

- trust is persisted in the skills trust store
- trust checks are path-prefix based for canonicalized paths
- trust changes are deterministic
- hidden skills become visible only after trust requirements are satisfied

## Step 5: Inspect a specific skill

Once a skill is visible, inspect its metadata:

```text
xzatoma skills show my_skill
```

This shows metadata such as:

- `name`
- `description`
- source scope
- skill directory
- skill file
- optional license
- optional compatibility
- advisory `allowed-tools`
- custom metadata entries

If the skill is missing, invalid, or hidden, the command returns an error.

## Step 6: Use the skill in a session

After discovery and trust are correct, start a normal XZatoma session:

```text
xzatoma chat
```

Or run a prompt directly:

```text
xzatoma run --prompt "Use the appropriate skill for this task."
```

When skills are available, XZatoma can disclose visible skills to the model at
startup. The model can then activate a skill through the `activate_skill` tool.

Important behavior:

- skills are not activated just because they exist
- activation happens only through `activate_skill`
- active skill content is injected from a session-local registry
- active skills do not persist across resumed sessions in the first release

## Example workflow

This is a typical end-to-end workflow for project skills.

### 1. Check paths

```text
xzatoma skills paths
```

### 2. Validate skill discovery

```text
xzatoma skills validate
```

### 3. Trust the project root if required

```text
xzatoma skills trust add .
```

### 4. Confirm the skill is now visible

```text
xzatoma skills list
```

### 5. Inspect the skill

```text
xzatoma skills show release_checklist
```

### 6. Start a session and request the work

```text
xzatoma chat
```

Then describe your task naturally. If the model determines the skill is
relevant, it can activate it using the supported activation flow.

## Example skill layout

A typical project skill may look like this:

```text
.xzatoma/skills/release_checklist/
├── SKILL.md
├── references/
│   └── release_steps.md
└── scripts/
    └── verify_release.sh
```

## Troubleshooting

### `skills list` shows nothing

Check:

1. `skills.enabled` is `true`
2. the relevant roots are enabled
3. the skill file is named exactly `SKILL.md`
4. the skill metadata is valid
5. trust requirements are satisfied

Run:

```text
xzatoma skills validate
xzatoma skills paths
```

### `skills show <name>` fails

This usually means the skill is:

- not discovered
- invalid
- shadowed
- hidden by trust rules

Use:

```text
xzatoma skills validate
```

### A project skill exists but is not visible

If `skills.project_trust_required: true`, trust the project root:

```text
xzatoma skills trust add .
```

Then rerun:

```text
xzatoma skills list
```

### A custom path skill is not visible

Check whether:

- the path appears in `skills.additional_paths`
- the root is trusted when custom trust is required
- `allow_custom_paths_without_trust` is disabled

### A skill is valid but another one wins

This is usually a shadowing case. Run:

```text
xzatoma skills validate
```

Look for shadowed skill diagnostics and then rename the lower-precedence skill
if needed.

## Recommended operating practice

For reliable usage:

1. place project skills under `./.xzatoma/skills/`
2. validate after adding or editing a skill
3. trust only roots you intend to use
4. inspect important skills with `skills show`
5. keep names unique to avoid shadowing
6. treat `allowed-tools` as advisory metadata only

## Related documentation

- `docs/how-to/create_project_skills_for_xzatoma.md`
- `docs/reference/agent_skills_cli.md`
- `docs/reference/agent_skills_configuration.md`
- `docs/reference/agent_skills_behavior.md`
