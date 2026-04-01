# Skills Command Evals

Data-driven evaluation suite for the `xzatoma skills` command family.

## Overview

This directory contains offline, deterministic eval scenarios that exercise the
skills discovery, validation, visibility, trust, and catalog subsystems without
requiring a network connection, a live AI provider, or a real home directory.

Each scenario installs skill fixture files into a temporary directory, builds a
`Config` pointing at that directory, calls the appropriate library function, and
asserts the return value.

## Directory Structure

```text
evals/skills_command/
├── README.md           -- this file
├── scenarios.yaml      -- all scenario definitions
└── skills/             -- skill fixture files
    ├── valid_simple.md
    ├── valid_with_tools.md
    ├── valid_with_inputs.md
    ├── invalid_no_name.md
    ├── invalid_no_description.md
    ├── invalid_empty_body.md
    ├── invalid_bad_yaml.md
    └── not_a_skill.txt
```

The integration test lives at `tests/eval_skills_command.rs`.

## Skill Fixture Format

Every `*.md` fixture uses the YAML frontmatter + Markdown body format expected
by `src/skills/parser.rs`.

```text
---
name: skill_name
description: One-line description of the skill
allowed-tools: read_file, grep        # optional
license: MIT                          # optional
compatibility: xzatoma>=0.1.0         # optional
metadata:                             # optional key-value map
  key: value
---

# Skill Name

Markdown instruction body.
```

### Validation rules

| Rule                                          | Effect                                                             |
| --------------------------------------------- | ------------------------------------------------------------------ |
| `name` field is required                      | Missing name produces a `MissingName` diagnostic                   |
| `name` must match `^[a-z][a-z0-9_]*$`         | Invalid format produces an `InvalidName` diagnostic                |
| `name` must match the parent directory name   | Mismatch produces a `NameDirectoryMismatch` diagnostic             |
| `description` field is required and non-empty | Missing or blank value produces a `MissingDescription` diagnostic  |
| `metadata` YAML must be well-formed           | Malformed frontmatter produces a `MalformedFrontmatter` diagnostic |
| Instruction body may be empty                 | Empty body is not a validation error in the current implementation |

Invalid skills do not appear in the visible catalog. They are surfaced only as
diagnostics through the `validate` command.

## Fixture Files

| File                        | Purpose                                                                    |
| --------------------------- | -------------------------------------------------------------------------- |
| `valid_simple.md`           | Minimal valid skill with name, description, and a brief body               |
| `valid_with_tools.md`       | Valid skill declaring allowed tools in frontmatter                         |
| `valid_with_inputs.md`      | Valid skill with optional license, compatibility, and metadata fields      |
| `invalid_no_name.md`        | Missing `name` field -- produces a MissingName diagnostic                  |
| `invalid_no_description.md` | Missing `description` field -- produces a MissingDescription diagnostic    |
| `invalid_empty_body.md`     | Valid frontmatter but empty body -- counted as valid by the current parser |
| `invalid_bad_yaml.md`       | Malformed YAML frontmatter -- produces a MalformedFrontmatter diagnostic   |
| `not_a_skill.txt`           | Wrong file extension -- ignored entirely by the skill scanner              |

## Scenarios

Scenarios are defined in `scenarios.yaml`. Each scenario has the following
fields:

```yaml
- id: unique_scenario_id
  description: "Human-readable description"
  command: build_catalog # see Commands below
  setup:
    skills_enabled: true # default: true
    allow_custom_paths_without_trust: true # default: true
    fixtures:
      - fixture: valid_simple.md
        dir: valid_simple # dir name must match the skill name field
    extra_files:
      - file: not_a_skill.txt
        dest: not_a_skill.txt # path relative to skills_root
  input:
    skill_name: valid_simple # required by show command
    trust_path: trusted_project # required by trust commands
  expect:
    outcome: ok # "ok" or "error"
    error_contains: "substring" # optional, checked against error message
    catalog_size: 1 # optional, checked by build_catalog command
```

### Commands

| Command            | Library function called                                                        |
| ------------------ | ------------------------------------------------------------------------------ |
| `build_catalog`    | `build_visible_skill_catalog` -- asserts `catalog_size` when present           |
| `list`             | `list_skills`                                                                  |
| `validate`         | `validate_skills`                                                              |
| `show`             | `show_skill` -- requires `input.skill_name`                                    |
| `paths`            | `show_paths`                                                                   |
| `trust_show`       | `handle_trust(SkillsTrustCommand::Show, ...)`                                  |
| `trust_add`        | `handle_trust(SkillsTrustCommand::Add, ...)` -- requires `input.trust_path`    |
| `trust_remove`     | `handle_trust(SkillsTrustCommand::Remove, ...)` -- requires `input.trust_path` |
| `trust_round_trip` | trust_add followed by trust_remove -- requires `input.trust_path`              |

### Setup isolation

Every scenario runs in its own temporary directory. Project-level and user-level
discovery roots are disabled. Only the `additional_paths` root (pointing at the
temporary skills root) is active. The trust store is also written into the
temporary directory so no persistent state is shared between scenarios.

## Running the Evals

Run the full eval suite:

```sh
cargo test --test eval_skills_command -- --nocapture
```

Run a single scenario by name (partial match on test function):

```sh
cargo test --test eval_skills_command eval_skills_command_scenarios -- --nocapture
```

## Adding a New Scenario

1. Add a new fixture file to `evals/skills_command/skills/` if the scenario
   requires a new skill document.

2. Add a new entry to `scenarios.yaml` following the schema above.

   - Choose a `dir` value that matches the `name` field in the fixture
     frontmatter, otherwise the validator will produce a `NameDirectoryMismatch`
     diagnostic.

3. Run the eval suite and confirm the new scenario passes.

4. No changes to `tests/eval_skills_command.rs` are required unless you need a
   new command type.

## Covered Branches

| Branch                         | Scenarios                                                                                             |
| ------------------------------ | ----------------------------------------------------------------------------------------------------- |
| Discovery with no skills       | `discovery_empty_directory`, `non_skill_txt_file_is_ignored`                                          |
| Discovery with valid skills    | `discovery_valid_simple_skill`, `discovery_multiple_valid_skills`                                     |
| Discovery with skills disabled | `discovery_skills_disabled`                                                                           |
| List command                   | `list_valid_skills_succeeds`                                                                          |
| Validate with valid skills     | `validate_valid_skill_succeeds`                                                                       |
| Validate with invalid skills   | `validate_skill_missing_name`, `validate_skill_missing_description`, `validate_malformed_frontmatter` |
| Empty body counted valid       | `validate_empty_body_counted_as_valid`                                                                |
| Show existing skill            | `show_existing_skill_succeeds`                                                                        |
| Show missing skill             | `show_nonexistent_skill_returns_error`                                                                |
| Trust show on empty store      | `trust_show_empty_store_succeeds`                                                                     |
| Trust add and remove           | `trust_add_remove_round_trip_succeeds`                                                                |
| Paths listing                  | `paths_listing_succeeds`                                                                              |
