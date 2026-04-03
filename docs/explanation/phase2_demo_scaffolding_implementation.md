# Phase 2 Demo Scaffolding Implementation

## Overview

This document records the implementation of Phase 2: Core Demo Scaffolding for
Chat, Run, and Vision from the XZatoma demo implementation plan. Phase 2 creates
the first three fully scaffolded demo directories under `demos/` and establishes
the concrete pattern that Phases 3 and 4 follow for the remaining four demos.

## Scope

Phase 2 covers the following tasks from the implementation plan:

| Task | Description                                            |
| ---- | ------------------------------------------------------ |
| 2.1  | Create chat, run, and vision demo directories          |
| 2.2  | Define and create all required files per demo          |
| 2.3  | Create demo-local config files using Ollama            |
| 2.4  | Validate file presence, config values, and README form |
| 2.5  | Deliver all three scaffolded demos                     |
| 2.6  | Verify success criteria                                |

## Deliverables

### demos/chat/

The chat demo scaffolds the `xzatoma chat` CLI surface against the local Ollama
`granite4:3b` model.

| File                         | Description                                         |
| ---------------------------- | --------------------------------------------------- |
| `README.md`                  | Full walkthrough with all 10 required sections      |
| `config.yaml`                | Ollama provider, `granite4:3b`, demo-local paths    |
| `setup.sh`                   | Creates `tmp/output/`, checks prerequisites         |
| `run.sh`                     | Launches `xzatoma chat` with demo-local flags       |
| `reset.sh`                   | Removes `tmp/xzatoma.db` and `tmp/output/` contents |
| `input/sample_questions.txt` | Static reference questions for the demo             |
| `tmp/.gitignore`             | Excludes all generated content from git             |
| `tmp/output/.gitkeep`        | Preserves the empty output directory in git         |

### demos/run/

The run demo scaffolds the `xzatoma run` CLI surface against the local Ollama
`granite4:3b` model. It includes two plan files to demonstrate single-step and
multi-step autonomous execution.

| File                     | Description                                         |
| ------------------------ | --------------------------------------------------- |
| `README.md`              | Full walkthrough with all 10 required sections      |
| `config.yaml`            | Ollama provider, `granite4:3b`, demo-local paths    |
| `setup.sh`               | Verifies plan files, checks prerequisites           |
| `run.sh`                 | Executes `hello_world.yaml`, tees to `tmp/output/`  |
| `reset.sh`               | Removes `tmp/xzatoma.db` and `tmp/output/` contents |
| `plans/hello_world.yaml` | Single-step greeting plan                           |
| `plans/system_info.yaml` | Multi-step system information plan                  |
| `input/notes.txt`        | Static notes and direct-invocation examples         |
| `tmp/.gitignore`         | Excludes all generated content from git             |
| `tmp/output/.gitkeep`    | Preserves the empty output directory in git         |

### demos/vision/

The vision demo scaffolds the `xzatoma chat` CLI surface against the local
Ollama `granite3.2-vision:2b` model. It includes a setup script that generates a
sample PNG image to serve as a reference artifact for future full image
attachment support.

| File                  | Description                                               |
| --------------------- | --------------------------------------------------------- |
| `README.md`           | Full walkthrough with all 10 required sections            |
| `config.yaml`         | Ollama provider, `granite3.2-vision:2b`, demo-local paths |
| `setup.sh`            | Creates `tmp/output/`, generates `tmp/sample.png`         |
| `run.sh`              | Launches `xzatoma chat` with the vision model             |
| `reset.sh`            | Removes `tmp/xzatoma.db`, `tmp/sample.png`, and output    |
| `input/prompt.txt`    | Static reference prompts for vision tasks                 |
| `tmp/.gitignore`      | Excludes all generated content from git                   |
| `tmp/output/.gitkeep` | Preserves the empty output directory in git               |

## Design Decisions

### Portable Script Pattern

Every `setup.sh`, `run.sh`, and `reset.sh` uses the following standard header to
resolve the demo root from the script's own location:

```sh
DEMO_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$DEMO_DIR"
```

This satisfies the portable script contract. Running any script from an
arbitrary working directory produces the same result as running it from inside
the demo directory.

### xzatoma Binary Discovery

The `run.sh` scripts use a three-step binary discovery strategy:

1. Check if `xzatoma` is on `PATH` (preferred for installed or exported builds)
2. Fall back to `../../target/release/xzatoma` relative to the demo directory
3. Fall back to `../../target/debug/xzatoma` relative to the demo directory

If none of the three locations yields an executable, the script exits with an
error and prints the build instructions. This ensures the demo works both when
the binary is installed globally and when it is only built inside the
repository.

### Storage Path Isolation

Every invocation of `xzatoma` in the demo scripts passes:

```sh
--config ./config.yaml
--storage-path ./tmp/xzatoma.db
```

The `--config` flag prevents the repository-level `config/config.yaml` from
being loaded. The `--storage-path` flag directs the SQLite conversation history
database into `tmp/`, keeping all generated state inside the demo directory.

### tmp/.gitignore Pattern

Every `tmp/.gitignore` uses the following pattern:

```text
*
!.gitignore
!output/
output/*
!output/.gitkeep
```

This excludes all generated files including the SQLite database, log files, and
any runtime output, while preserving the `.gitignore` itself and the
`tmp/output/` directory structure in version control.

### Vision Demo Sample Image

The vision demo's `setup.sh` generates `tmp/sample.png` using Python 3's
`struct` and `zlib` standard library modules. No external dependencies are
required. The image is a 64x64 solid blue (RGB 70, 130, 180) PNG file.

The image is placed in `tmp/` rather than `input/` because it is generated
state, not static input data. The `reset.sh` removes it, and `setup.sh`
recreates it.

The vision demo uses `xzatoma chat` (not `xzatoma run`) because text-based
interaction with the vision model is the appropriate CLI surface for multimodal
conversations. The `granite3.2-vision:2b` model handles text queries correctly
even without image attachment. Full image attachment support (passing binary
image data to the Ollama `/api/chat` endpoint via the `images` field) requires
future provider-layer work and is documented in the README as a planned
capability.

### Skills Disabled in Demo Configs

All three demo `config.yaml` files set `skills.enabled: false`. Skills discovery
is not required for the chat, run, or vision demos and disabling it keeps
startup faster and avoids spurious warnings about skill paths that do not exist
inside the demo directory.

### Planning Mode as Default

The chat and vision demo configs set `agent.chat.default_mode: planning`. This
makes the agent read-only by default. Users who want to allow file writes can
switch to write mode with `--mode write` on the command line. This prevents
accidental file modifications during a demo run.

### Plan File Design

The two run demo plan files follow the canonical XZatoma plan format:

- `plans/hello_world.yaml`: A single step that asks the agent to print a
  greeting. The `context` field provides the shell command
  `echo "Hello from XZatoma"` as a hint to the agent.
- `plans/system_info.yaml`: Five steps each gathering one piece of system
  information with a standard POSIX command. All commands are read-only and do
  not write files.

Both plans are valid under `PlanParser::validate` (non-empty name, at least one
step, every step has a non-empty action).

## File Changes

| File                                                         | Action  |
| ------------------------------------------------------------ | ------- |
| `demos/chat/README.md`                                       | Created |
| `demos/chat/config.yaml`                                     | Created |
| `demos/chat/setup.sh`                                        | Created |
| `demos/chat/run.sh`                                          | Created |
| `demos/chat/reset.sh`                                        | Created |
| `demos/chat/input/sample_questions.txt`                      | Created |
| `demos/chat/tmp/.gitignore`                                  | Created |
| `demos/chat/tmp/output/.gitkeep`                             | Created |
| `demos/run/README.md`                                        | Created |
| `demos/run/config.yaml`                                      | Created |
| `demos/run/setup.sh`                                         | Created |
| `demos/run/run.sh`                                           | Created |
| `demos/run/reset.sh`                                         | Created |
| `demos/run/plans/hello_world.yaml`                           | Created |
| `demos/run/plans/system_info.yaml`                           | Created |
| `demos/run/input/notes.txt`                                  | Created |
| `demos/run/tmp/.gitignore`                                   | Created |
| `demos/run/tmp/output/.gitkeep`                              | Created |
| `demos/vision/README.md`                                     | Created |
| `demos/vision/config.yaml`                                   | Created |
| `demos/vision/setup.sh`                                      | Created |
| `demos/vision/run.sh`                                        | Created |
| `demos/vision/reset.sh`                                      | Created |
| `demos/vision/input/prompt.txt`                              | Created |
| `demos/vision/tmp/.gitignore`                                | Created |
| `demos/vision/tmp/output/.gitkeep`                           | Created |
| `demos/README.md`                                            | Updated |
| `docs/explanation/phase2_demo_scaffolding_implementation.md` | Created |

## Success Criteria Verification

| Criterion                                            | Status |
| ---------------------------------------------------- | ------ |
| `demos/chat/` directory exists with all files        | Pass   |
| `demos/run/` directory exists with all files         | Pass   |
| `demos/vision/` directory exists with all files      | Pass   |
| `chat/config.yaml` uses `provider.type: ollama`      | Pass   |
| `run/config.yaml` uses `provider.type: ollama`       | Pass   |
| `vision/config.yaml` uses `provider.type: ollama`    | Pass   |
| `chat/config.yaml` model is `granite4:3b`            | Pass   |
| `run/config.yaml` model is `granite4:3b`             | Pass   |
| `vision/config.yaml` model is `granite3.2-vision:2b` | Pass   |
| Every `tmp/` includes `.gitignore`                   | Pass   |
| Every demo has a complete README with 10 sections    | Pass   |
| Generated output directed to `tmp/output/`           | Pass   |
| No `demos/_shared/` directory exists                 | Pass   |
| No cross-demo references in any script               | Pass   |

## Validation Checklist

The following items must be verified before Phase 2 is considered complete:

- `demos/chat/` contains: README.md, config.yaml, setup.sh, run.sh, reset.sh,
  input/, tmp/.gitignore, tmp/output/
- `demos/run/` contains: README.md, config.yaml, setup.sh, run.sh, reset.sh,
  plans/, input/, tmp/.gitignore, tmp/output/
- `demos/vision/` contains: README.md, config.yaml, setup.sh, run.sh, reset.sh,
  input/, tmp/.gitignore, tmp/output/
- `demos/chat/config.yaml` contains `type: ollama` and `model: granite4:3b`
- `demos/run/config.yaml` contains `type: ollama` and `model: granite4:3b`
- `demos/vision/config.yaml` contains `type: ollama` and
  `model: granite3.2-vision:2b`
- Every script begins with `DEMO_DIR="$(cd "$(dirname "$0")" && pwd)"`
- Every `xzatoma` invocation passes `--config ./config.yaml` and
  `--storage-path ./tmp/xzatoma.db`
- All output in run.sh scripts is written to `tmp/output/`
- Every demo README contains sections: Goal, Prerequisites, Directory Layout,
  Setup, Run, Expected Output, Reset, Sandbox Boundaries, Troubleshooting
- Markdown files pass `markdownlint --config .markdownlint.json`
- Markdown files pass `prettier --write --parser markdown --prose-wrap always`

## References

- `docs/explanation/demo_implementation_plan.md` — Master plan and global
  contracts
- `demos/README.md` — Authoritative top-level demo index
- `docs/explanation/phase1_demo_framework_implementation.md` — Phase 1 record
- `AGENTS.md` — Development guidelines and coding standards
