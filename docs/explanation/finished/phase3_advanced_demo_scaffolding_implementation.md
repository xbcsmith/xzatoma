# Phase 3 Advanced Demo Scaffolding Implementation

## Overview

This document records the implementation of Phase 3: Advanced Feature Demo
Scaffolding for Skills, MCP, and Subagents from the XZatoma demo implementation
plan. Phase 3 creates three fully scaffolded demo directories for the advanced
feature set and establishes the fixture and configuration patterns that Phases 4
and 5 will extend.

## Scope

Phase 3 covers the following tasks from the implementation plan:

| Task | Description                                                    |
| ---- | -------------------------------------------------------------- |
| 3.1  | Create skills, mcp, and subagents demo directories             |
| 3.2  | Define and create all required files per demo                  |
| 3.3  | Implement demo-local isolation for each advanced feature       |
| 3.4  | Verify skills isolation, MCP fixture scope, and subagent paths |
| 3.5  | Deliver all three scaffolded advanced feature demos            |
| 3.6  | Verify success criteria                                        |

## Deliverables

### demos/skills/

The skills demo scaffolds the `xzatoma skills` CLI surface and the
`activate_skill` agent tool against the local Ollama `granite4:3b` model.

| File                         | Description                                            |
| ---------------------------- | ------------------------------------------------------ |
| `README.md`                  | Full walkthrough with all 10 required sections         |
| `config.yaml`                | Ollama provider, `granite4:3b`, skills enabled         |
| `setup.sh`                   | Verifies skill fixtures and plan files; checks prereqs |
| `run.sh`                     | Three-phase: list, validate, run plan                  |
| `reset.sh`                   | Removes `tmp/xzatoma.db` and `tmp/output/` contents    |
| `skills/greet/SKILL.md`      | Greet skill fixture with valid frontmatter             |
| `skills/summarize/SKILL.md`  | Summarize skill fixture with valid frontmatter         |
| `skills/write_file/SKILL.md` | Write file skill fixture with allowed-tools field      |
| `plans/skills_demo.yaml`     | Three-step plan activating each skill in sequence      |
| `input/sample_prompts.txt`   | Reference prompts for interactive skill use            |
| `tmp/.gitignore`             | Excludes all generated content from git                |
| `tmp/output/.gitkeep`        | Preserves the empty output directory in git            |

### demos/mcp/

The MCP demo scaffolds the MCP server integration against a local stdio-based
`@modelcontextprotocol/server-filesystem` process and the Ollama `granite4:3b`
model.

| File                     | Description                                                |
| ------------------------ | ---------------------------------------------------------- |
| `README.md`              | Full walkthrough with all 10 required sections             |
| `config.yaml`            | Ollama provider, MCP server entry, demo-local paths        |
| `setup.sh`               | Verifies fixtures; checks Ollama, xzatoma, node, npx       |
| `run.sh`                 | Executes `mcp_demo.yaml`, tees to `tmp/output/mcp_run.txt` |
| `reset.sh`               | Removes `tmp/xzatoma.db` and `tmp/output/` contents        |
| `mcp/server_config.yaml` | Reference copy of the MCP server configuration             |
| `mcp/tool_examples.md`   | Documented examples of available MCP tool invocations      |
| `plans/mcp_demo.yaml`    | Four-step plan exercising MCP filesystem tools             |
| `input/prompts.txt`      | Reference prompts for interactive MCP use                  |
| `tmp/.gitignore`         | Excludes all generated content from git                    |
| `tmp/output/.gitkeep`    | Preserves the empty output directory in git                |

### demos/subagents/

The subagents demo scaffolds the `subagent` and `parallel_subagent` agent tools
against the local Ollama `granite4:3b` model with conversation persistence
enabled.

| File                        | Description                                           |
| --------------------------- | ----------------------------------------------------- |
| `README.md`                 | Full walkthrough with all 10 required sections        |
| `config.yaml`               | Ollama provider, subagent config, persistence in tmp/ |
| `setup.sh`                  | Creates `tmp/output/`, checks prerequisites           |
| `run.sh`                    | Executes `subagents_demo.yaml`, tees to `tmp/output/` |
| `reset.sh`                  | Removes db files and `tmp/output/` contents           |
| `plans/subagents_demo.yaml` | Two-step plan: parallel delegation then summary       |
| `input/tasks.txt`           | Reference description of each delegated task          |
| `tmp/.gitignore`            | Excludes all generated content from git               |
| `tmp/output/.gitkeep`       | Preserves the empty output directory in git           |

## Design Decisions

### Skills Demo: Discovery Isolation

The skills demo demonstrates that XZatoma discovers only demo-local skills. The
`config.yaml` enforces this with three settings working together:

```yaml
skills:
  enabled: true
  project_enabled: false
  user_enabled: false
  additional_paths:
    - ./skills
  allow_custom_paths_without_trust: true
  project_trust_required: false
```

Setting `project_enabled: false` disables scanning of `.xzatoma/skills/` and
`.agents/skills/` relative to the working directory. Setting
`user_enabled: false` disables scanning of `~/.xzatoma/skills/` and
`~/.agents/skills/`. The only active discovery root is the `./skills` entry in
`additional_paths`, which resolves to `$DEMO_DIR/skills/` at runtime.

Setting `allow_custom_paths_without_trust: true` and
`project_trust_required: false` allows the demo to run without a trust store
setup step. This is intentional for a demonstration context. Production
deployments should require trust for custom paths.

### Skills Demo: Fixture Design

Each skill fixture follows the canonical `SKILL.md` format required by the
skills parser. Three fixtures are provided:

- `greet` - no tool dependencies, behavioral instruction only
- `summarize` - no tool dependencies, instructs the agent to write output to
  `tmp/output/summary.txt`
- `write_file` - declares `write_file` in `allowed-tools`; instructs the agent
  to always write to `tmp/output/`

The `name` field in each skill's YAML frontmatter exactly matches the directory
name containing the file. This satisfies the `NameDirectoryMismatch` validation
check. Skill names match the pattern `^[a-z][a-z0-9_]*$` as required by the
skills validator.

### Skills Demo: Three-Phase run.sh

The `run.sh` script runs in three sequential phases to make the demo walkthrough
observable:

1. `xzatoma skills list` - proves discovery isolation (only demo-local skills
   appear, none from user or project paths)
2. `xzatoma skills validate` - proves all three fixtures are well-formed (zero
   diagnostics expected)
3. `xzatoma run --plan ./plans/skills_demo.yaml` - proves activation works
   during autonomous plan execution

Each phase writes its output to a separate file in `tmp/output/` via `tee` so
the results can be inspected after the run.

The `skills list` and `skills validate` commands do not require `--storage-path`
because they do not create a conversation session. The `run` command requires
`--storage-path` because it creates a conversation history database.

### MCP Demo: Server Configuration

The MCP demo uses `@modelcontextprotocol/server-filesystem` as the MCP server
because it is the canonical reference implementation of a stdio-based MCP
server, requires no external service, and is available via `npx` without a
global installation step.

The server is scoped to `./tmp/output` by passing that path as the sole
positional argument to the server executable. The MCP filesystem server enforces
this root and rejects path traversal attempts that would escape the configured
directory.

The server entry in `config.yaml` uses the `demo-filesystem` ID, which satisfies
the server ID validation pattern `^[a-z0-9_-]{1,64}$`. The transport type is
`stdio` with `executable: npx`. With `mcp.auto_connect: true` XZatoma starts the
server subprocess automatically before the agent begins executing the plan.

Node.js (version 18 or later) and `npx` are required prerequisites. The
`setup.sh` script checks for both and prints a clear warning with installation
instructions if they are missing.

### MCP Demo: mcp/ Directory

The `mcp/` directory contains two fixture files:

- `mcp/server_config.yaml` - a reference copy of the server configuration
  section from `config.yaml`, provided for documentation and for independent
  verification that the server works. It includes the command to run the server
  manually for diagnostic purposes.
- `mcp/tool_examples.md` - documents the tools commonly provided by the
  `@modelcontextprotocol/server-filesystem` package and gives example prompts
  for interactive testing.

Both files are static fixtures. They are never modified by `setup.sh`, `run.sh`,
or `reset.sh`.

### Subagents Demo: parallel_subagent Tool

The subagents demo uses the `parallel_subagent` tool rather than the single
`subagent` tool to make the delegation visible and to demonstrate concurrent
execution. Three tasks run in a single `parallel_subagent` call:

- `haiku-writer` writes a 5-7-5 haiku to `tmp/output/haiku.txt`
- `mcp-describer` writes a three-sentence MCP description to
  `tmp/output/mcp_description.txt`
- `rust-advocate` writes five Rust CLI benefits to
  `tmp/output/rust_benefits.txt`

Each subagent task prompt explicitly instructs the subagent to use the
`write_file` tool and specifies the exact output path. This makes the boundary
between subagent output and coordinator output observable in the file tree.

### Subagents Demo: Persistence and Telemetry

The subagents demo enables both `persistence_enabled: true` and
`telemetry_enabled: true` to make the demo educational. Subagent conversation
history is written to `tmp/subagent_conversations.db`. Structured telemetry
events (spawn, complete, error) are included in the execution transcript written
to `tmp/output/subagents_run.txt`.

The `persistence_path: ./tmp/subagent_conversations.db` setting is a relative
path resolved against the working directory at runtime. Because `run.sh` sets
`cd "$DEMO_DIR"` before invoking `xzatoma`, this resolves to
`$DEMO_DIR/tmp/subagent_conversations.db`, keeping all persistence state inside
the demo directory.

The `reset.sh` script removes both `tmp/xzatoma.db` and
`tmp/subagent_conversations.db` to return the demo to its initial state.

### Subagents Demo: Depth and Quota Configuration

The subagents demo sets `max_depth: 2` and `max_executions: 5`. The coordinator
runs at depth 0 and spawns subagents at depth 1. Subagents at depth 1 can spawn
sub-subagents at depth 2 if needed, but the demo plan does not exercise that
level.

The `max_executions: 5` quota prevents runaway execution if the plan is
modified. Three parallel subagents count as three executions against the quota,
leaving two executions available for any retries or additional tool calls.

### Portable Script Pattern

Every `setup.sh`, `run.sh`, and `reset.sh` in Phase 3 uses the same standard
header as Phase 1 and Phase 2:

```sh
DEMO_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$DEMO_DIR"
```

This satisfies the portable script contract. Running any script from an
arbitrary working directory produces the same result as running it from inside
the demo directory.

### xzatoma Binary Discovery

All Phase 3 `run.sh` scripts use the same three-step binary discovery strategy
established in Phase 2:

1. Check if `xzatoma` is on `PATH` (preferred for installed or exported builds)
2. Fall back to `../../target/release/xzatoma` relative to the demo directory
3. Fall back to `../../target/debug/xzatoma` relative to the demo directory

### Storage Path Isolation

Every `xzatoma run` invocation passes:

```sh
--config ./config.yaml
--storage-path ./tmp/xzatoma.db
```

The `--config` flag prevents the repository-level `config/config.yaml` from
being loaded. The `--storage-path` flag directs the SQLite conversation history
database into `tmp/`.

### tmp/.gitignore Pattern

Every `tmp/.gitignore` uses the pattern established in Phase 2:

```text
*
!.gitignore
!output/
output/*
!output/.gitkeep
```

This excludes all generated files including the SQLite databases, log files, and
runtime output, while preserving the `.gitignore` itself and the `tmp/output/`
directory structure in version control.

## File Changes

| File                                                                  | Action  |
| --------------------------------------------------------------------- | ------- |
| `demos/skills/README.md`                                              | Created |
| `demos/skills/config.yaml`                                            | Created |
| `demos/skills/setup.sh`                                               | Created |
| `demos/skills/run.sh`                                                 | Created |
| `demos/skills/reset.sh`                                               | Created |
| `demos/skills/skills/greet/SKILL.md`                                  | Created |
| `demos/skills/skills/summarize/SKILL.md`                              | Created |
| `demos/skills/skills/write_file/SKILL.md`                             | Created |
| `demos/skills/plans/skills_demo.yaml`                                 | Created |
| `demos/skills/input/sample_prompts.txt`                               | Created |
| `demos/skills/tmp/.gitignore`                                         | Created |
| `demos/skills/tmp/output/.gitkeep`                                    | Created |
| `demos/mcp/README.md`                                                 | Created |
| `demos/mcp/config.yaml`                                               | Created |
| `demos/mcp/setup.sh`                                                  | Created |
| `demos/mcp/run.sh`                                                    | Created |
| `demos/mcp/reset.sh`                                                  | Created |
| `demos/mcp/mcp/server_config.yaml`                                    | Created |
| `demos/mcp/mcp/tool_examples.md`                                      | Created |
| `demos/mcp/plans/mcp_demo.yaml`                                       | Created |
| `demos/mcp/input/prompts.txt`                                         | Created |
| `demos/mcp/tmp/.gitignore`                                            | Created |
| `demos/mcp/tmp/output/.gitkeep`                                       | Created |
| `demos/subagents/README.md`                                           | Created |
| `demos/subagents/config.yaml`                                         | Created |
| `demos/subagents/setup.sh`                                            | Created |
| `demos/subagents/run.sh`                                              | Created |
| `demos/subagents/reset.sh`                                            | Created |
| `demos/subagents/plans/subagents_demo.yaml`                           | Created |
| `demos/subagents/input/tasks.txt`                                     | Created |
| `demos/subagents/tmp/.gitignore`                                      | Created |
| `demos/subagents/tmp/output/.gitkeep`                                 | Created |
| `demos/README.md`                                                     | Updated |
| `docs/explanation/phase3_advanced_demo_scaffolding_implementation.md` | Created |

## Success Criteria Verification

| Criterion                                                   | Status |
| ----------------------------------------------------------- | ------ |
| `demos/skills/` directory exists with all required files    | Pass   |
| `demos/mcp/` directory exists with all required files       | Pass   |
| `demos/subagents/` directory exists with all required files | Pass   |
| `skills/config.yaml` uses `provider.type: ollama`           | Pass   |
| `mcp/config.yaml` uses `provider.type: ollama`              | Pass   |
| `subagents/config.yaml` uses `provider.type: ollama`        | Pass   |
| All three `config.yaml` files use `granite4:3b`             | Pass   |
| Skills discovery restricted to `./skills/` only             | Pass   |
| MCP server fixture lives only inside `demos/mcp/`           | Pass   |
| Subagent persistence path inside `tmp/`                     | Pass   |
| Every `tmp/` includes `.gitignore`                          | Pass   |
| Every demo has a complete README with all 10 sections       | Pass   |
| All generated output directed to `tmp/output/`              | Pass   |
| No `demos/_shared/` directory exists                        | Pass   |
| No cross-demo references in any script or config            | Pass   |
| Feature-local fixtures exist in `skills/` and `mcp/`        | Pass   |

## Validation Checklist

The following items must be verified before Phase 3 is considered complete:

- `demos/skills/` contains: `README.md`, `config.yaml`, `setup.sh`, `run.sh`,
  `reset.sh`, `skills/greet/SKILL.md`, `skills/summarize/SKILL.md`,
  `skills/write_file/SKILL.md`, `plans/skills_demo.yaml`, `input/`,
  `tmp/.gitignore`, `tmp/output/`
- `demos/mcp/` contains: `README.md`, `config.yaml`, `setup.sh`, `run.sh`,
  `reset.sh`, `mcp/server_config.yaml`, `mcp/tool_examples.md`,
  `plans/mcp_demo.yaml`, `input/`, `tmp/.gitignore`, `tmp/output/`
- `demos/subagents/` contains: `README.md`, `config.yaml`, `setup.sh`, `run.sh`,
  `reset.sh`, `plans/subagents_demo.yaml`, `input/tasks.txt`, `tmp/.gitignore`,
  `tmp/output/`
- All `config.yaml` files contain `type: ollama` and `model: granite4:3b`
- `skills/config.yaml` contains `project_enabled: false`, `user_enabled: false`,
  and `additional_paths: [./skills]`
- `mcp/config.yaml` contains a valid MCP server entry with `id: demo-filesystem`
  and `type: stdio` with `executable: npx`
- `subagents/config.yaml` contains `max_depth: 2`, `persistence_enabled: true`,
  and `persistence_path: ./tmp/subagent_conversations.db`
- Every script begins with `DEMO_DIR="$(cd "$(dirname "$0")" && pwd)"`
- Every `xzatoma run` invocation passes `--config ./config.yaml` and
  `--storage-path ./tmp/xzatoma.db`
- All output in `run.sh` scripts is written to `tmp/output/`
- Every demo README contains all 10 required sections: Goal, Prerequisites,
  Directory Layout, Setup, Run, Expected Output, Reset, Sandbox Boundaries,
  Troubleshooting
- Skill fixture `name:` fields match their containing directory names
- Markdown files pass `markdownlint --config .markdownlint.json`
- Markdown files pass `prettier --write --parser markdown --prose-wrap always`

## References

- `docs/explanation/demo_implementation_plan.md` - Master plan and global
  contracts
- `demos/README.md` - Authoritative top-level demo index
- `docs/explanation/phase1_demo_framework_implementation.md` - Phase 1 record
- `docs/explanation/phase2_demo_scaffolding_implementation.md` - Phase 2 record
- `AGENTS.md` - Development guidelines and coding standards
