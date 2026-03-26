# Agent Skills Configuration Reference

This reference documents the `skills` configuration section for XZatoma.

Agent skills let XZatoma discover, validate, disclose, and activate
project-local, user-local, and explicitly configured skill directories. Skills
are loaded from `SKILL.md` files and are subject to validation, visibility, and
trust rules.

## Configuration Location

Add the `skills` section to your main configuration file:

```/dev/null/config.yaml#L1-24
skills:
  enabled: true
  project_enabled: true
  user_enabled: true
  additional_paths: []
  max_discovered_skills: 256
  max_scan_directories: 2000
  max_scan_depth: 6
  catalog_max_entries: 128
  activation_tool_enabled: true
  project_trust_required: true
  trust_store_path: ~/.xzatoma/skills_trust.yaml
  allow_custom_paths_without_trust: false
  strict_frontmatter: true
```

## Default Configuration

The effective default values are:

```/dev/null/default_skills_config.yaml#L1-13
skills:
  enabled: true
  project_enabled: true
  user_enabled: true
  additional_paths: []
  max_discovered_skills: 256
  max_scan_directories: 2000
  max_scan_depth: 6
  catalog_max_entries: 128
  activation_tool_enabled: true
  project_trust_required: true
  trust_store_path: null
  allow_custom_paths_without_trust: false
  strict_frontmatter: true
```

## Fields

### `enabled`

- **Type:** `boolean`
- **Default:** `true`

Global feature flag for agent skills.

When `false`, XZatoma disables skills discovery, disclosure, activation, and
skills-related runtime behavior.

### `project_enabled`

- **Type:** `boolean`
- **Default:** `true`

Enables project-level discovery roots relative to the current working directory.

When enabled, XZatoma checks these roots in precedence order:

1. `./.xzatoma/skills/`
2. `./.agents/skills/`

Project skills are subject to trust enforcement when
`project_trust_required: true`.

### `user_enabled`

- **Type:** `boolean`
- **Default:** `true`

Enables user-level discovery roots under the current user's home directory.

When enabled, XZatoma checks these roots in precedence order:

1. `~/.xzatoma/skills/`
2. `~/.agents/skills/`

User-level skills do not require project trust.

### `additional_paths`

- **Type:** `array<string>`
- **Default:** `[]`

Adds extra discovery roots after the built-in project and user roots.

Relative paths are resolved from the current working directory. Absolute paths
are used directly.

Example:

```/dev/null/additional_paths_example.yaml#L1-7
skills:
  additional_paths:
    - ./custom_skills
    - /opt/xzatoma/skills
    - /srv/shared/agent_skills
```

Important behavior:

- Empty path entries are invalid
- Paths are scanned in the order listed
- These paths participate in normal name-collision precedence based on overall
  discovery order
- Custom paths may require trust depending on `allow_custom_paths_without_trust`

### `max_discovered_skills`

- **Type:** `integer`
- **Default:** `256`

Hard cap on the number of valid loaded skills.

This limits how many valid skills may be loaded into the skill catalog.

Validation rules:

- Must be greater than `0`

### `max_scan_directories`

- **Type:** `integer`
- **Default:** `2000`

Hard cap on the number of directories visited during discovery.

This protects the discovery process from excessive traversal in very large or
unexpected directory trees.

Validation rules:

- Must be greater than `0`

### `max_scan_depth`

- **Type:** `integer`
- **Default:** `6`

Maximum traversal depth for each discovery root.

This limits how far discovery descends below each root when searching for
`SKILL.md` files.

Validation rules:

- Must be greater than `0`

### `catalog_max_entries`

- **Type:** `integer`
- **Default:** `128`

Maximum number of skill catalog entries disclosed to the model at startup.

This affects startup disclosure only. It does not change the full discovery
result.

Validation rules:

- Must be greater than `0`
- Must be less than or equal to `max_discovered_skills`

### `activation_tool_enabled`

- **Type:** `boolean`
- **Default:** `true`

Controls whether the synthetic `activate_skill` tool is registered when visible
skills are available.

If this is `false`, skills may still be discovered and validated, but the model
cannot activate them through the supported activation contract.

### `project_trust_required`

- **Type:** `boolean`
- **Default:** `true`

Requires explicit trust for project-level skills.

When enabled:

- project skills can still be discovered and validated
- project skills are hidden from visible runtime surfaces until trusted
- hidden project skills do not appear in `skills list`
- hidden project skills cannot be shown with `skills show <name>`
- hidden project skills are not disclosed at startup
- hidden project skills cannot be activated

When disabled, project-level skills are visible without trust.

### `trust_store_path`

- **Type:** `string | null`
- **Default:** `null`

Optional path override for the persistent trust store file.

When omitted or `null`, XZatoma uses its default trust store location under the
user configuration directory.

A typical explicit configuration looks like this:

```/dev/null/trust_store_example.yaml#L1-4
skills:
  project_trust_required: true
  trust_store_path: ~/.xzatoma/skills_trust.yaml
```

Validation rules:

- If set, it must not be an empty string
- `~/` paths are resolved relative to the user's home directory

### `allow_custom_paths_without_trust`

- **Type:** `boolean`
- **Default:** `false`

Controls trust behavior for `additional_paths`.

When `false`, custom discovery paths are hidden from visible runtime surfaces
unless they are trusted.

When `true`, custom discovery paths bypass trust checks and become visible
without trust.

This setting does not affect project trust behavior.

### `strict_frontmatter`

- **Type:** `boolean`
- **Default:** `true`

Rejects invalid skills without lenient fallback behavior.

When enabled, malformed or invalid skill frontmatter causes the skill to be
excluded from the valid catalog.

Invalid skills remain visible only in validation diagnostics and logs, not in
normal runtime disclosure or activation surfaces.

## Discovery Roots and Precedence

Discovery uses the following effective root order:

1. project client-specific: `./.xzatoma/skills/`
2. project shared convention: `./.agents/skills/`
3. user client-specific: `~/.xzatoma/skills/`
4. user shared convention: `~/.agents/skills/`
5. configured `additional_paths` in listed order

If multiple valid skills share the same name, earlier roots take precedence.
Lower-precedence duplicates are treated as shadowed and excluded from the active
catalog.

## Trust Model

Trust primarily affects visibility, not raw validation.

### Trusted Path Storage

Trusted paths are stored persistently in a YAML trust store.

Example trust store contents:

```/dev/null/skills_trust.yaml#L1-4
trusted_paths:
  - /workspace/my_project
  - /opt/xzatoma/skills
```

### What Trust Affects

Trust affects whether a valid discovered skill is visible to runtime surfaces:

- startup skill catalog disclosure
- `skills list`
- `skills show <name>`
- `activate_skill`

### What Trust Does Not Affect

Trust does not prevent:

- low-level discovery scanning
- invalid skill diagnostics in `skills validate`
- shadowed-skill diagnostics in `skills validate`

## Environment Variable Overrides

XZatoma supports environment variable overrides for key skills settings.

```/dev/null/skills_env_overrides.txt#L1-7
XZATOMA_SKILLS_ENABLED
XZATOMA_SKILLS_PROJECT_ENABLED
XZATOMA_SKILLS_USER_ENABLED
XZATOMA_SKILLS_ACTIVATION_TOOL_ENABLED
XZATOMA_SKILLS_PROJECT_TRUST_REQUIRED
XZATOMA_SKILLS_TRUST_STORE_PATH
```

These override configuration file values when set.

Boolean environment values accept common truthy and falsy values such as:

- true values: `1`, `true`, `yes`, `on`
- false values: `0`, `false`, `no`, `off`

## Validation Rules Summary

The `skills` configuration is invalid if any of the following are true:

- `max_discovered_skills == 0`
- `max_scan_directories == 0`
- `max_scan_depth == 0`
- `catalog_max_entries == 0`
- `catalog_max_entries > max_discovered_skills`
- `additional_paths` contains an empty entry
- `trust_store_path` is set to an empty string
- `trust_store_path` resolves to an empty path

## Complete Example

```/dev/null/full_skills_config.yaml#L1-19
skills:
  enabled: true
  project_enabled: true
  user_enabled: true
  additional_paths:
    - ./custom_skills
    - /opt/xzatoma/skills
  max_discovered_skills: 256
  max_scan_directories: 2000
  max_scan_depth: 6
  catalog_max_entries: 128
  activation_tool_enabled: true
  project_trust_required: true
  trust_store_path: ~/.xzatoma/skills_trust.yaml
  allow_custom_paths_without_trust: false
  strict_frontmatter: true
```

## Related References

- `docs/reference/agent_skills_cli.md`
- `docs/reference/agent_skills_behavior.md`
- `docs/how-to/use_agent_skills.md`
- `docs/how-to/create_project_skills_for_xzatoma.md`
