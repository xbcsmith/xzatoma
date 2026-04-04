# Phase 6: Skills Command Evals Implementation

## Overview

Phase 6 extends the XZatoma eval system to cover the skills command subsystem.
It adds data-driven, offline evaluation scenarios that exercise skill discovery,
catalog visibility, frontmatter validation, trust store management, and path
reporting without requiring a live AI provider or a real home directory.

## Deliverables

| Deliverable             | Path                                                             |
| ----------------------- | ---------------------------------------------------------------- |
| Skill fixture files (8) | `evals/skills_command/skills/`                                   |
| Scenario definitions    | `evals/skills_command/scenarios.yaml`                            |
| Eval README             | `evals/skills_command/README.md`                                 |
| Integration test        | `tests/eval_skills_command.rs`                                   |
| Implementation doc      | `docs/explanation/phase6_skills_command_evals_implementation.md` |

## Fixture Files

Eight fixture files were created under `evals/skills_command/skills/`. Each
fixture exercises a distinct parser or validator branch.

| File                        | Branch exercised                                                                 |
| --------------------------- | -------------------------------------------------------------------------------- |
| `valid_simple.md`           | Minimal valid skill -- name and description only                                 |
| `valid_with_tools.md`       | Valid skill with `allowed-tools` frontmatter field                               |
| `valid_with_inputs.md`      | Valid skill with optional `license`, `compatibility`, and `metadata` fields      |
| `invalid_no_name.md`        | Missing `name` field -- produces a `MissingName` diagnostic                      |
| `invalid_no_description.md` | Missing `description` field -- produces a `MissingDescription` diagnostic        |
| `invalid_empty_body.md`     | Valid frontmatter, empty Markdown body -- counted as valid by the current parser |
| `invalid_bad_yaml.md`       | Malformed YAML frontmatter -- produces a `MalformedFrontmatter` diagnostic       |
| `not_a_skill.txt`           | Wrong file extension -- ignored entirely by the skill scanner                    |

### Fixture naming convention

The skill `name` field in the frontmatter must match the parent directory name
for validation to succeed. For example, `valid_simple.md` declares
`name: valid_simple` and is installed into the directory `valid_simple/` during
test setup. Fixtures that are expected to fail before the name check (missing
name, malformed YAML) may use an arbitrary directory name.

## Scenarios

Sixteen scenarios are defined in `evals/skills_command/scenarios.yaml`. The
scenarios cover all major branches of the skills command subsystem.

### Discovery scenarios

| Scenario ID                       | Command         | What it tests                                              |
| --------------------------------- | --------------- | ---------------------------------------------------------- |
| `discovery_empty_directory`       | `build_catalog` | Empty skills root returns catalog size 0                   |
| `discovery_valid_simple_skill`    | `build_catalog` | One valid skill returns catalog size 1                     |
| `discovery_multiple_valid_skills` | `build_catalog` | Two valid skills return catalog size 2                     |
| `discovery_skills_disabled`       | `build_catalog` | `skills.enabled: false` returns catalog size 0 immediately |

### List scenarios

| Scenario ID                  | Command | What it tests                                            |
| ---------------------------- | ------- | -------------------------------------------------------- |
| `list_valid_skills_succeeds` | `list`  | `list_skills` returns `Ok` when valid skills are present |

### Validate scenarios

| Scenario ID                            | Command         | What it tests                                                                            |
| -------------------------------------- | --------------- | ---------------------------------------------------------------------------------------- |
| `validate_valid_skill_succeeds`        | `validate`      | `validate_skills` returns `Ok` when all skills are valid                                 |
| `validate_skill_missing_name`          | `validate`      | `validate_skills` returns `Ok` even when a `MissingName` diagnostic is produced          |
| `validate_skill_missing_description`   | `validate`      | `validate_skills` returns `Ok` even when a `MissingDescription` diagnostic is produced   |
| `validate_malformed_frontmatter`       | `validate`      | `validate_skills` returns `Ok` even when a `MalformedFrontmatter` diagnostic is produced |
| `validate_empty_body_counted_as_valid` | `build_catalog` | A skill with an empty body is counted as valid (catalog size 1)                          |

### Show scenarios

| Scenario ID                            | Command | What it tests                                                            |
| -------------------------------------- | ------- | ------------------------------------------------------------------------ |
| `show_existing_skill_succeeds`         | `show`  | `show_skill` returns `Ok` for a known visible skill                      |
| `show_nonexistent_skill_returns_error` | `show`  | `show_skill` returns `Err` containing "missing, invalid, or not visible" |

### Trust scenarios

| Scenario ID                            | Command            | What it tests                                              |
| -------------------------------------- | ------------------ | ---------------------------------------------------------- |
| `trust_show_empty_store_succeeds`      | `trust_show`       | `handle_trust(Show)` returns `Ok` on an empty trust store  |
| `trust_add_remove_round_trip_succeeds` | `trust_round_trip` | Trust add followed by trust remove returns `Ok` both times |

### Paths scenarios

| Scenario ID              | Command | What it tests                                                    |
| ------------------------ | ------- | ---------------------------------------------------------------- |
| `paths_listing_succeeds` | `paths` | `show_paths` returns `Ok` and reports configured discovery roots |

### Ignore scenarios

| Scenario ID                     | Command         | What it tests                                                   |
| ------------------------------- | --------------- | --------------------------------------------------------------- |
| `non_skill_txt_file_is_ignored` | `build_catalog` | A `.txt` file in the skills root does not appear in the catalog |

## Test Architecture

### Test file: `tests/eval_skills_command.rs`

The test is a single `#[test]` function `eval_skills_command_scenarios` that:

1. Reads `evals/skills_command/scenarios.yaml`.
2. Iterates over every scenario, calling `run_scenario` for each.
3. Collects pass/fail counts and reports a summary.
4. Panics if any scenario fails.

Each scenario runs in isolation through `run_scenario`, which:

1. Creates a `tempfile::TempDir` for the scenario.
2. Creates a `skills_root` subdirectory inside the temp dir.
3. Installs fixture files from `evals/skills_command/skills/` as
   `<skills_root>/<dir>/SKILL.md` (for `fixtures`) or at verbatim destination
   paths (for `extra_files`).
4. Builds a `Config` with project and user discovery disabled, and a single
   `additional_paths` root pointing at the temp `skills_root`.
5. Sets `trust_store_path` to a file inside the temp dir so no persistent state
   is shared between scenarios.
6. Sets `allow_custom_paths_without_trust: true` (unless overridden) so
   additional paths are visible without trust entries.
7. Calls `run_command` to dispatch to the appropriate library function.
8. Calls `check_result` to assert the outcome and optional error substring.

### Commands

The test supports these command values:

| Command            | Library function                                             |
| ------------------ | ------------------------------------------------------------ |
| `build_catalog`    | `build_visible_skill_catalog` -- also asserts `catalog_size` |
| `list`             | `list_skills`                                                |
| `validate`         | `validate_skills`                                            |
| `show`             | `show_skill` (requires `input.skill_name`)                   |
| `paths`            | `show_paths`                                                 |
| `trust_show`       | `handle_trust(Show, ...)`                                    |
| `trust_add`        | `handle_trust(Add, ...)` (requires `input.trust_path`)       |
| `trust_remove`     | `handle_trust(Remove, ...)` (requires `input.trust_path`)    |
| `trust_round_trip` | trust add then trust remove (requires `input.trust_path`)    |

### Design decisions

**No stdout capture.** The command functions (`list_skills`, `validate_skills`,
etc.) write to stdout. Capturing stdout in Rust integration tests requires
additional infrastructure. Instead, the eval test asserts function return
values. The `validate` command always returns `Ok(())` regardless of how many
diagnostics are printed, which is an intentional design of the validate command.

**Catalog size assertion via `build_catalog`.** Direct catalog size assertions
are made through the `build_catalog` command, which calls
`build_visible_skill_catalog` and checks the returned `SkillCatalog::len()`.
This is more precise than asserting through `list_skills` output.

**Trust store isolation.** Each scenario uses its own `skills_trust.yaml` file
inside the temporary directory. This prevents any persistent trust state from
leaking between scenarios and makes scenarios independent of the developer's
real trust store.

**Project and user discovery disabled.** By setting `project_enabled: false` and
`user_enabled: false`, each scenario is fully controlled by the
`additional_paths` list and is not affected by the current working directory or
`HOME`.

## Validation Behaviors Observed

During implementation the following behaviors were confirmed by running the eval
suite:

- `validate_skills` always returns `Ok(())`. Invalid skill diagnostics are
  printed to stdout but do not cause the function to return an error. This is
  consistent with the design where `validate` is a reporting command, not a
  gating command.

- An empty instruction body is not a validation error. The validator only checks
  frontmatter fields (`name`, `description`). A skill with valid frontmatter and
  an empty body is counted as a valid catalog entry.

- The `show_skill` function returns `Err` when the requested skill is not
  present in the visible catalog. The error message contains "missing, invalid,
  or not visible".

- The `.txt` file extension is not recognized as a skill file. Only files named
  exactly `SKILL.md` are loaded by the scanner.

- The `trust_round_trip` command (add then remove) leaves the trust store with
  zero trusted paths, which matches the expected behavior of the round-trip
  test.

## Quality Gates

All gates passed before marking Phase 6 complete:

```sh
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --test eval_skills_command -- --nocapture
```

Test results: 16 passed, 0 failed out of 16 scenarios.

Previous eval suites were also confirmed passing:

- `cargo test --test eval_config_validation` -- 28 passed, 0 failed
- `cargo test --test eval_run_command` -- 19 passed, 0 failed

All Markdown files were linted and formatted:

```sh
markdownlint --fix --config .markdownlint.json <file>
prettier --write --parser markdown --prose-wrap always <file>
```
