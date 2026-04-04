# Skills Demo

## Goal

Demonstrate skill discovery, loading, and activation within a fully
self-contained XZatoma sandbox. This demo proves that:

- XZatoma discovers only the skills listed in `./skills/` and no external paths
- All three fixture skills pass validation with zero diagnostics
- An agent can activate discovered skills during autonomous plan execution using
  the `activate_skill` tool

## Prerequisites

1. [Ollama](https://ollama.com) installed and running:

   ```sh
   ollama serve
   ```

2. The `granite4:3b` model pulled:

   ```sh
   ollama pull granite4:3b
   ```

3. XZatoma built from the repository root:

   ```sh
   cargo build --release
   ```

   Add the resulting binary to `PATH`, or ensure it is reachable at
   `../../target/release/xzatoma` relative to this demo directory.

## Directory Layout

```text
demos/skills/
  README.md                     # This file
  config.yaml                   # Demo-local configuration with skills enabled
  setup.sh                      # Prepares tmp/ and verifies prerequisites
  run.sh                        # Runs skill discovery, validation, and plan
  reset.sh                      # Removes all generated state
  skills/                       # Demo-local skill fixtures (discovery root)
    greet/
      SKILL.md                  # Skill: greet the user by name
    summarize/
      SKILL.md                  # Skill: condense text into a concise summary
    write_file/
      SKILL.md                  # Skill: write content to a file in tmp/output/
  plans/
    skills_demo.yaml            # Plan that exercises each loaded skill
  input/
    sample_prompts.txt          # Reference prompts for interactive use
  tmp/
    .gitignore                  # Excludes all generated files from version control
    output/                     # All generated artifacts are written here
```

## Setup

```sh
cd demos/skills
./setup.sh
```

`setup.sh` performs the following steps:

1. Creates `tmp/output/` if it does not exist.
2. Verifies that all three skill fixture files are present under `./skills/`.
3. Verifies that `plans/skills_demo.yaml` is present.
4. Checks that `xzatoma` is available on `PATH` or in the build output.
5. Checks that Ollama is running and `granite4:3b` is available.

## Run

```sh
./run.sh
```

`run.sh` executes three phases in sequence:

1. **Skill Discovery** - Runs `xzatoma skills list` to print all skills loaded
   from `./skills/`. Output is saved to `tmp/output/skills_list.txt`.
2. **Skill Validation** - Runs `xzatoma skills validate` to confirm all fixture
   skills are well-formed with no diagnostics. Output is saved to
   `tmp/output/skills_validate.txt`.
3. **Plan Execution** - Runs `xzatoma run --plan ./plans/skills_demo.yaml`. The
   agent activates each skill via the `activate_skill` tool and completes three
   writing tasks. Output is saved to `tmp/output/skills_run.txt`.

To run individual phases:

```sh
# List discovered skills
xzatoma --config ./config.yaml skills list

# Validate all skills
xzatoma --config ./config.yaml skills validate

# Show a specific skill
xzatoma --config ./config.yaml skills show greet

# Execute the full plan
xzatoma --config ./config.yaml --storage-path ./tmp/xzatoma.db \
  run --plan ./plans/skills_demo.yaml
```

## Expected Output

After `./run.sh` completes, the following files appear in `tmp/output/`:

| File                  | Contents                                           |
| --------------------- | -------------------------------------------------- |
| `skills_list.txt`     | Names and descriptions of all three demo skills    |
| `skills_validate.txt` | Validation results confirming all skills are valid |
| `skills_run.txt`      | Full plan execution transcript                     |
| `summary.txt`         | Condensed summary produced by the summarize skill  |
| `greeting.txt`        | Completion notice produced by the write_file skill |

The skills list output contains entries for `greet`, `summarize`, and
`write_file`. No skills outside `./skills/` appear in the list.

## Reset

```sh
./reset.sh
```

`reset.sh` removes the following generated files:

- `tmp/xzatoma.db` (conversation history database)
- All files under `tmp/output/` except `.gitkeep`
- Any other generated files directly under `tmp/`

Skill fixture files under `skills/`, plan files under `plans/`, input files
under `input/`, and `config.yaml` are never modified by `reset.sh`.

## Sandbox Boundaries

XZatoma is constrained to this demo directory by the following configuration:

- `--config ./config.yaml` is passed on every invocation. The repository-level
  `config/config.yaml` is never loaded.
- `--storage-path ./tmp/xzatoma.db` directs all conversation history into
  `tmp/`.
- `skills.project_enabled: false` prevents XZatoma from scanning
  `.xzatoma/skills/` or `.agents/skills/` relative to the working directory.
- `skills.user_enabled: false` prevents XZatoma from scanning
  `~/.xzatoma/skills/` or `~/.agents/skills/`.
- `skills.additional_paths: ["./skills"]` restricts discovery to the demo-local
  `skills/` directory only.
- `skills.allow_custom_paths_without_trust: true` allows the demo to run without
  a separate trust store setup step.
- `skills.project_trust_required: false` removes the project-level trust
  requirement so the demo works out of the box.

No skill outside `./skills/` can be discovered or activated during a demo run.
All output written during plan execution is directed to `tmp/output/`.

## Troubleshooting

### xzatoma binary not found

Build from the repository root and add the binary to `PATH`:

```sh
cargo build --release
export PATH="$PWD/target/release:$PATH"
```

### Ollama not running

```sh
ollama serve
```

### granite4:3b model not available

```sh
ollama pull granite4:3b
```

### Skill validation reports a diagnostic

If `xzatoma skills validate` reports a diagnostic for a fixture skill, verify
that the `name:` field in the skill's `SKILL.md` frontmatter exactly matches the
name of the directory containing that file. Skill names must match the pattern
`^[a-z][a-z0-9_]*$`. For example, the `greet` skill must live at
`skills/greet/SKILL.md` and its frontmatter must contain `name: greet`.

### Plan execution produces no output files

Ensure the working directory is the demo root when the plan runs. The `run.sh`
script sets `cd "$DEMO_DIR"` before invoking `xzatoma`. If running manually,
change into the `demos/skills/` directory first:

```sh
cd demos/skills
xzatoma --config ./config.yaml --storage-path ./tmp/xzatoma.db \
  run --plan ./plans/skills_demo.yaml
```

### Skills not found after changing directory

The `skills.additional_paths` entry `./skills` is resolved relative to the
working directory at runtime. Always run `xzatoma` from the `demos/skills/`
directory, or pass an absolute path in `config.yaml`.
