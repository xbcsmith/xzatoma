# XZatoma Demo Implementation Plan

## Overview

This plan creates a complete, phased demo suite for XZatoma under
`xzatoma/demos/`. The demo suite must cover the following product areas:

1. Chat
2. Run
3. Skills
4. MCP
5. Subagents
6. Vision
7. Watcher

Each demo must be fully self-contained inside its own subdirectory under
`xzatoma/demos/` and must not require files outside that demo directory during
normal execution. Every demo must sandbox XZatoma to the demo directory, store
all generated data in a local `tmp/` directory, and write all demo outputs to
`tmp/output/`.

All demos must use Ollama only. The required models are:

- `granite4:3b` for Chat, Run, Skills, MCP, Subagents, and Watcher demos
- `granite3.2-vision:2b` for the Vision demo

This plan is written for AI-agent execution and uses explicit file paths,
directory structures, deliverables, and validation criteria.

## Explicit First-Release Decisions

The following first-release decisions are locked and MUST NOT be changed during
implementation:

| Decision Area                                    | Decision                                                                                                       | Status       |
| ------------------------------------------------ | -------------------------------------------------------------------------------------------------------------- | ------------ |
| Demo root                                        | All demos live directly under `xzatoma/demos/`                                                                 | REQUIRED     |
| Demo coverage                                    | Create exactly 7 demos: chat, run, skills, mcp, subagents, vision, watcher                                     | REQUIRED     |
| Self containment                                 | Each demo must include its own README, config, setup scripts, and sample data                                  | REQUIRED     |
| Sandboxing                                       | XZatoma execution must be scoped to the demo directory only                                                    | REQUIRED     |
| Temp data location                               | All generated files must live under that demo's `tmp/` directory                                               | REQUIRED     |
| Output location                                  | All result artifacts must live under that demo's `tmp/output/` directory                                       | REQUIRED     |
| Git safety                                       | Every demo `tmp/` directory must include a `.gitignore` file                                                   | REQUIRED     |
| Provider                                         | Ollama only                                                                                                    | REQUIRED     |
| Standard model                                   | `granite4:3b`                                                                                                  | REQUIRED     |
| Vision model                                     | `granite3.2-vision:2b`                                                                                         | REQUIRED     |
| Demo automation                                  | Each demo must include runnable setup and execution commands/scripts                                           | REQUIRED     |
| External dependencies                            | No demo may require files outside its own directory after creation                                             | REQUIRED     |
| Built shared framework                           | No shared demo helper framework is allowed; every required file must live inside the individual demo directory | REQUIRED     |
| Copilot demos                                    | Not included                                                                                                   | OUT OF SCOPE |
| Remote services beyond demo-specific local setup | Not included unless strictly required by the demo and documented inside the demo directory                     | DEFERRED     |

## Current State Analysis

### Existing Infrastructure

| Area                    | File or Directory                                                                             | Symbol or Responsibility                      | Relevance                                                                       |
| ----------------------- | --------------------------------------------------------------------------------------------- | --------------------------------------------- | ------------------------------------------------------------------------------- |
| Demos root              | `xzatoma/demos/`                                                                              | demo content root                             | Existing location for all demos                                                 |
| Demos README            | `xzatoma/demos/README.md`                                                                     | top-level demos index                         | Must be expanded to document the new demo suite                                 |
| Main CLI                | `xzatoma/src/cli.rs`                                                                          | `Commands` enum                               | Defines chat, run, watch, and mcp entrypoints used by demos                     |
| Chat flow               | `xzatoma/src/commands/mod.rs`                                                                 | `chat::run_chat`                              | Used by chat, skills, subagents, and vision demos                               |
| Run flow                | `xzatoma/src/commands/mod.rs`                                                                 | `run::run_plan_with_options`                  | Used by run demo and likely some automation in other demos                      |
| Watcher flow            | `xzatoma/src/cli.rs`                                                                          | `Commands::Watch`                             | Used by watcher demo                                                            |
| MCP flow                | `xzatoma/src/cli.rs` and `xzatoma/src/commands/mcp.rs`                                        | `Commands::Mcp`, `handle_mcp`                 | Used by MCP demo                                                                |
| Skills feature          | planned in docs                                                                               | skills support                                | Demo plan must reserve a complete demo for the upcoming skills feature          |
| Subagents               | `xzatoma/src/tools/subagent.rs`, `xzatoma/src/tools/parallel_subagent.rs`                     | subagent execution                            | Used by subagents demo                                                          |
| Vision-related support  | `xzatoma/src/tools/read_file.rs`, `xzatoma/src/tools/mod.rs`, `xzatoma/src/mention_parser.rs` | image reading and prompt augmentation support | Used by vision demo                                                             |
| Watcher config examples | `xzatoma/config/watcher.yaml`, `xzatoma/config/generic_watcher.yaml`                          | watcher configuration references              | Can inform demo-local config structure                                          |
| Global config example   | `xzatoma/config/config.yaml`                                                                  | application configuration reference           | Can inform demo-local config structure but must not be used directly at runtime |

### Identified Issues

| ID  | Issue                                                    | Impact                                            |
| --- | -------------------------------------------------------- | ------------------------------------------------- |
| 1   | There is no phased demo implementation plan file yet     | No execution roadmap exists                       |
| 2   | `xzatoma/demos/README.md` is only a stub                 | No usable demo index exists                       |
| 3   | No individual demo directories currently exist           | No runnable demos exist                           |
| 4   | No demo-specific config files exist                      | Demos cannot be run reproducibly                  |
| 5   | No demo-local temp/output conventions are implemented    | Demo runs may pollute the repository              |
| 6   | No sandboxing rules are defined per demo                 | XZatoma may access files outside a demo directory |
| 7   | No setup scripts exist for preparing demo-local state    | Users cannot run demos consistently               |
| 8   | No Ollama model policy is encoded in demos               | Users may run demos with unsupported models       |
| 9   | No self-contained MCP, watcher, or vision fixtures exist | Those demos cannot be reproduced safely           |
| 10  | No validation criteria exist for demo completeness       | Future contributors cannot verify demo quality    |

## Scope Definition

### In Scope

| Item                                                      |
| --------------------------------------------------------- |
| Create the implementation plan for 7 demos                |
| Define demo directory structure                           |
| Define required files per demo                            |
| Define setup, run, and cleanup expectations               |
| Define temp/output isolation rules                        |
| Define README documentation requirements                  |
| Define model requirements and provider constraints        |
| Define validation criteria for demo completeness          |
| Define update requirements for the top-level demos README |

### Out of Scope

| Item                                                                        | Reason                              |
| --------------------------------------------------------------------------- | ----------------------------------- |
| Implementing the demos themselves                                           | This document is planning only      |
| Changing core runtime behavior unless needed for sandbox-friendly execution | Separate implementation work        |
| Supporting non-Ollama providers in demos                                    | Explicitly disallowed               |
| Reusing repository-level config files directly at runtime                   | Violates self-contained requirement |

## Demo Inventory

The implementation MUST create the following demo directories:

| Demo Name | Directory                  | Primary CLI Surface                       | Required Model         |
| --------- | -------------------------- | ----------------------------------------- | ---------------------- |
| Chat      | `xzatoma/demos/chat/`      | `chat`                                    | `granite4:3b`          |
| Run       | `xzatoma/demos/run/`       | `run`                                     | `granite4:3b`          |
| Skills    | `xzatoma/demos/skills/`    | `chat` or `run` with skills enabled       | `granite4:3b`          |
| MCP       | `xzatoma/demos/mcp/`       | `mcp` and `chat` or `run` with MCP config | `granite4:3b`          |
| Subagents | `xzatoma/demos/subagents/` | `chat` and/or `run` with subagent flows   | `granite4:3b`          |
| Vision    | `xzatoma/demos/vision/`    | `chat` and/or `run` with image inputs     | `granite3.2-vision:2b` |
| Watcher   | `xzatoma/demos/watcher/`   | `watch`                                   | `granite4:3b`          |

## Required Demo Directory Contract

Every demo directory MUST follow this structure unless a phase explicitly
documents a justified addition:

| Path Pattern                          | Required    | Purpose                                             |
| ------------------------------------- | ----------- | --------------------------------------------------- |
| `xzatoma/demos/<demo>/README.md`      | Yes         | User walkthrough                                    |
| `xzatoma/demos/<demo>/config.yaml`    | Yes         | Demo-local XZatoma config                           |
| `xzatoma/demos/<demo>/setup.sh`       | Yes         | Prepare demo-local state                            |
| `xzatoma/demos/<demo>/run.sh`         | Yes         | Execute the main demo flow                          |
| `xzatoma/demos/<demo>/reset.sh`       | Yes         | Reset demo-local generated state                    |
| `xzatoma/demos/<demo>/tmp/`           | Yes         | Generated files root                                |
| `xzatoma/demos/<demo>/tmp/.gitignore` | Yes         | Ignore temp data                                    |
| `xzatoma/demos/<demo>/tmp/output/`    | Yes         | Demo output destination                             |
| `xzatoma/demos/<demo>/input/`         | Yes         | Static demo input data                              |
| `xzatoma/demos/<demo>/plans/`         | Conditional | Required if demo uses `run` with plan files         |
| `xzatoma/demos/<demo>/skills/`        | Conditional | Required for skills demo or skill-enabled scenarios |
| `xzatoma/demos/<demo>/mcp/`           | Conditional | Required for MCP server fixtures                    |
| `xzatoma/demos/<demo>/watcher/`       | Conditional | Required for watcher event/config fixtures          |
| `xzatoma/demos/<demo>/scripts/`       | Optional    | Additional helper scripts if needed                 |

## Required README Contract Per Demo

Each `README.md` inside a demo directory MUST contain the following sections in
this order:

| Section                 | Required Content                                    |
| ----------------------- | --------------------------------------------------- |
| `# <Demo Name> Demo`    | Exact demo title                                    |
| `## Goal`               | What feature the demo proves                        |
| `## Prerequisites`      | Ollama model, any local services, required commands |
| `## Directory Layout`   | Explain files and folders in the demo directory     |
| `## Setup`              | Exact commands to prepare the demo                  |
| `## Run`                | Exact commands to execute the demo                  |
| `## Expected Output`    | What appears in `tmp/output/`                       |
| `## Reset`              | Exact commands to clean generated state             |
| `## Sandbox Boundaries` | Explain how XZatoma is scoped to the demo directory |
| `## Troubleshooting`    | Common failure modes and fixes                      |

Each README must use only paths relative to its own demo directory where
possible.

## Portable Script Contract

Every demo script must be portable with the demo directory and must remain
runnable after the individual demo directory is copied outside the repository.

The implementation plan MUST require that every `setup.sh`, `run.sh`, and
`reset.sh` in every demo directory follows these rules:

1. resolve the demo root from the script location
2. change into the demo root before performing any work
3. use only paths relative to the demo root or absolute paths derived from the
   demo root at runtime
4. never depend on the repository root being the current working directory
5. never reference files in sibling demo directories
6. never reference files in `xzatoma/config/`, `xzatoma/demos/`, or any other
   repository path outside the copied demo directory
7. write all generated state only under `tmp/`
8. write all result artifacts only under `tmp/output/`
9. remove only demo-local generated state during reset
10. document the exact invocation command in the demo-local `README.md`

The implementation plan MUST also require that every per-demo `README.md`
documents commands in a form that still works after copying the demo directory
to another location on disk.

## Sandboxing Contract

Each demo MUST ensure that XZatoma is limited to the demo directory.

The implementation must choose one consistent sandboxing strategy and use it
across all demos. The plan for implementation MUST require all demo scripts to:

1. resolve the demo root from the script's own location
2. run XZatoma with the demo directory as working directory
3. reference only demo-local `config.yaml`
4. write storage/history data to demo-local `tmp/`
5. ensure all file operations target paths under the demo directory
6. ensure all outputs are written to `tmp/output/`
7. avoid any dependency on repository-root-relative paths
8. remain runnable after copying only the individual demo directory to a new
   filesystem location

The implementation plan MUST require explicit setting of any runtime variables or
CLI flags needed to keep history, storage, and generated data inside the demo.

## Ollama Model Contract

| Demo      | Provider Type | Model                  |
| --------- | ------------- | ---------------------- |
| Chat      | `ollama`      | `granite4:3b`          |
| Run       | `ollama`      | `granite4:3b`          |
| Skills    | `ollama`      | `granite4:3b`          |
| MCP       | `ollama`      | `granite4:3b`          |
| Subagents | `ollama`      | `granite4:3b`          |
| Vision    | `ollama`      | `granite3.2-vision:2b` |
| Watcher   | `ollama`      | `granite4:3b`          |

No demo may document or use Copilot. No demo may reference another provider as
primary or fallback behavior.

## Implementation Phases

### Phase 1: Demo Framework and Shared Conventions

#### Task 1.1 Foundation Work

Define the global demo framework and common conventions for all demos.

**Files to create or update:**

- `xzatoma/docs/explanation/demo_implementation_plan.md`
- `xzatoma/demos/README.md`

**Required outputs:**

- a top-level demos index design
- a mandatory per-demo directory contract
- a mandatory temp/output contract
- a mandatory sandboxing contract
- a mandatory README contract
- a mandatory Ollama-only contract

#### Task 1.2 Add Foundation Functionality

Do not create any shared helper directory under `xzatoma/demos/`.

Every demo must be independently portable and runnable when its directory is
copied outside the repository. That means every required asset, script,
configuration file, fixture, sample input, and walkthrough file must live inside
that demo's own directory.

**Not allowed:**

- `xzatoma/demos/_shared/`
- shared helper scripts outside a demo directory
- shared runtime config outside a demo directory
- shared fixture files outside a demo directory
- cross-demo references at runtime

#### Task 1.3 Integrate Foundation Work

Expand `xzatoma/demos/README.md` into the authoritative demo index with:

- overview of all 7 demos
- model requirements table
- demo directory table
- quickstart instructions
- repository rules for demo isolation
- note that every demo is self-contained

#### Task 1.4 Testing Requirements

The implementation plan for this phase must require validation of:

- required demo index sections exist
- every demo directory name is reserved in the index
- sandboxing and temp/output rules are documented once centrally

#### Task 1.5 Deliverables

| Deliverable                        | Verification                                            |
| ---------------------------------- | ------------------------------------------------------- |
| top-level demo conventions defined | `demo_implementation_plan.md` contains global contracts |
| demos index design completed       | `xzatoma/demos/README.md` updated                       |
| all 7 demos listed centrally       | demo index contains all required demo names             |

#### Task 1.6 Success Criteria

This phase is complete only if:

1. `xzatoma/docs/explanation/demo_implementation_plan.md` exists
2. `xzatoma/demos/README.md` defines all 7 demos
3. the plan explicitly defines self-containment, sandboxing, and temp/output rules
4. the plan explicitly defines Ollama-only model usage

---

### Phase 2: Core Demo Scaffolding for Chat, Run, and Vision

#### Task 2.1 Feature Work

Create the first three core demo directories:

- `xzatoma/demos/chat/`
- `xzatoma/demos/run/`
- `xzatoma/demos/vision/`

These demos cover the most direct end-user flows and establish the base
structure used by all later demos.

#### Task 2.2 Integrate Feature

Define the exact required files for each of the three demo directories:

**Chat demo required files:**

- `xzatoma/demos/chat/README.md`
- `xzatoma/demos/chat/config.yaml`
- `xzatoma/demos/chat/setup.sh`
- `xzatoma/demos/chat/run.sh`
- `xzatoma/demos/chat/reset.sh`
- `xzatoma/demos/chat/input/`
- `xzatoma/demos/chat/tmp/.gitignore`
- `xzatoma/demos/chat/tmp/output/`

**Run demo required files:**

- `xzatoma/demos/run/README.md`
- `xzatoma/demos/run/config.yaml`
- `xzatoma/demos/run/setup.sh`
- `xzatoma/demos/run/run.sh`
- `xzatoma/demos/run/reset.sh`
- `xzatoma/demos/run/plans/`
- `xzatoma/demos/run/input/`
- `xzatoma/demos/run/tmp/.gitignore`
- `xzatoma/demos/run/tmp/output/`

**Vision demo required files:**

- `xzatoma/demos/vision/README.md`
- `xzatoma/demos/vision/config.yaml`
- `xzatoma/demos/vision/setup.sh`
- `xzatoma/demos/vision/run.sh`
- `xzatoma/demos/vision/reset.sh`
- `xzatoma/demos/vision/input/`
- `xzatoma/demos/vision/tmp/.gitignore`
- `xzatoma/demos/vision/tmp/output/`

#### Task 2.3 Configuration Updates

Each demo-local `config.yaml` must explicitly set:

- `provider.type: ollama`
- the correct Ollama model for that demo
- demo-local storage/history location if configurable
- terminal safety appropriate for the walkthrough
- any feature-specific settings required by the demo

The phase plan must require that no demo references `xzatoma/config/config.yaml`
at runtime.

#### Task 2.4 Testing Requirements

The implementation plan must require verification that:

- each demo directory contains all mandatory files
- each config file uses `provider.type: ollama`
- chat and run use `granite4:3b`
- vision uses `granite3.2-vision:2b`
- every demo `tmp/` includes `.gitignore`
- every demo README contains the mandatory sections

#### Task 2.5 Deliverables

| Deliverable                       | Verification                                |
| --------------------------------- | ------------------------------------------- |
| chat demo scaffold                | required files exist                        |
| run demo scaffold                 | required files exist                        |
| vision demo scaffold              | required files exist                        |
| per-demo config files created     | config files exist and use Ollama           |
| temp/output isolation established | `tmp/` and `tmp/output/` exist in each demo |

#### Task 2.6 Success Criteria

This phase is complete only if:

1. chat, run, and vision demo directories exist
2. each has a complete self-contained scaffold
3. each has a demo-local config file
4. vision demo uses only `granite3.2-vision:2b`
5. generated output is directed to demo-local `tmp/output/`

---

### Phase 3: Advanced Feature Demo Scaffolding for Skills, MCP, and Subagents

#### Task 3.1 Foundation Work

Create the next three advanced demo directories:

- `xzatoma/demos/skills/`
- `xzatoma/demos/mcp/`
- `xzatoma/demos/subagents/`

These demos depend on richer feature flows and must include feature-local
fixtures.

#### Task 3.2 Add Foundation Functionality

Define exact required files per demo.

**Skills demo required files:**

- `xzatoma/demos/skills/README.md`
- `xzatoma/demos/skills/config.yaml`
- `xzatoma/demos/skills/setup.sh`
- `xzatoma/demos/skills/run.sh`
- `xzatoma/demos/skills/reset.sh`
- `xzatoma/demos/skills/skills/`
- `xzatoma/demos/skills/input/`
- `xzatoma/demos/skills/tmp/.gitignore`
- `xzatoma/demos/skills/tmp/output/`

**MCP demo required files:**

- `xzatoma/demos/mcp/README.md`
- `xzatoma/demos/mcp/config.yaml`
- `xzatoma/demos/mcp/setup.sh`
- `xzatoma/demos/mcp/run.sh`
- `xzatoma/demos/mcp/reset.sh`
- `xzatoma/demos/mcp/mcp/`
- `xzatoma/demos/mcp/input/`
- `xzatoma/demos/mcp/tmp/.gitignore`
- `xzatoma/demos/mcp/tmp/output/`

**Subagents demo required files:**

- `xzatoma/demos/subagents/README.md`
- `xzatoma/demos/subagents/config.yaml`
- `xzatoma/demos/subagents/setup.sh`
- `xzatoma/demos/subagents/run.sh`
- `xzatoma/demos/subagents/reset.sh`
- `xzatoma/demos/subagents/input/`
- `xzatoma/demos/subagents/tmp/.gitignore`
- `xzatoma/demos/subagents/tmp/output/`

#### Task 3.3 Integrate Foundation Work

The plan for implementation must require:

**Skills demo behavior:**

- demo-local skill files only
- no skill dependencies outside `xzatoma/demos/skills/`
- walkthrough proving skill discovery and activation within the demo sandbox

**MCP demo behavior:**

- demo-local MCP fixture or demo-local MCP server configuration only
- any helper process or script must live in the demo directory
- README must explain startup order if a helper server is required

**Subagents demo behavior:**

- subagent flows must be visible in walkthrough output
- all artifacts from delegated work must remain under `tmp/`
- demo must use Ollama `granite4:3b`

#### Task 3.4 Testing Requirements

The implementation plan must require verification that:

- skills demo does not scan skills outside its own directory during normal use
- MCP demo helper scripts/config remain inside the MCP demo directory
- subagents demo generates only demo-local files
- every README documents setup, run, reset, and expected output

#### Task 3.5 Deliverables

| Deliverable                  | Verification                                            |
| ---------------------------- | ------------------------------------------------------- |
| skills demo scaffold         | required files exist                                    |
| MCP demo scaffold            | required files exist                                    |
| subagents demo scaffold      | required files exist                                    |
| feature-local fixtures added | `skills/`, `mcp/`, or input assets exist where required |

#### Task 3.6 Success Criteria

This phase is complete only if:

1. skills, MCP, and subagents demo directories exist
2. each demo is self-contained
3. each demo uses Ollama with `granite4:3b`
4. feature-local assets live only under that demo directory
5. all generated output is contained under `tmp/output/`

---

### Phase 4: Watcher Demo and End-to-End Isolation Hardening

#### Task 4.1 Foundation Work

Create the watcher demo directory:

- `xzatoma/demos/watcher/`

This demo is the most integration-heavy and must demonstrate watcher behavior
without requiring repository-global state.

#### Task 4.2 Add Foundation Functionality

Define exact required files:

- `xzatoma/demos/watcher/README.md`
- `xzatoma/demos/watcher/config.yaml`
- `xzatoma/demos/watcher/setup.sh`
- `xzatoma/demos/watcher/run.sh`
- `xzatoma/demos/watcher/reset.sh`
- `xzatoma/demos/watcher/watcher/`
- `xzatoma/demos/watcher/input/`
- `xzatoma/demos/watcher/tmp/.gitignore`
- `xzatoma/demos/watcher/tmp/output/`

The demo plan for implementation must require demo-local watcher fixtures, such
as:

- demo-local plan events
- demo-local topic simulation assets or instructions
- demo-local result collection paths

#### Task 4.3 Integrate Foundation Work

This phase must harden isolation across all demos.

The implementation plan must require a cross-demo audit ensuring:

- every setup script writes only under its own demo directory
- every run script writes outputs only under `tmp/output/`
- every reset script removes only demo-local generated state
- no demo requires repository-root writes
- no demo depends on another demo's files at runtime
- no demo script uses repository-root-relative paths
- every demo script derives its working paths from the script location and demo
  root
- every demo remains runnable after copying only that demo directory to a new
  filesystem location
- all `tmp/.gitignore` files ignore generated data consistently

The plan must also require the top-level demos index to include the watcher demo
and any global notes about service-heavy demos.

#### Task 4.4 Testing Requirements

The implementation plan must require validation of:

- watcher demo file completeness
- watcher config is self-contained
- every demo has `setup.sh`, `run.sh`, and `reset.sh`
- reset scripts are safe and scoped to demo-local state
- every `tmp/output/` directory is documented in the corresponding README

#### Task 4.5 Deliverables

| Deliverable                  | Verification                                 |
| ---------------------------- | -------------------------------------------- |
| watcher demo scaffold        | required files exist                         |
| isolation audit defined      | plan includes cross-demo isolation checklist |
| all demo temp dirs protected | `.gitignore` exists in every demo `tmp/`     |

#### Task 4.6 Success Criteria

This phase is complete only if:

1. watcher demo directory exists with complete scaffold
2. all seven demos exist
3. all seven demos have setup, run, and reset scripts
4. all demos are isolated to their own directories
5. all demos write outputs only to their own `tmp/output/`

---

### Phase 5: Documentation, Validation Matrix, and Completion Hardening

#### Task 5.1 Feature Work

Complete all documentation and validation requirements for the full demo suite.

**Files to create or update:**

- `xzatoma/demos/README.md`
- `xzatoma/docs/explanation/implementations.md`
- `xzatoma/docs/explanation/demo_implementation.md`

The implementation plan must require the mandatory implementation summary in:

- `xzatoma/docs/explanation/demo_implementation.md`

#### Task 5.2 Integrate Feature

The top-level `xzatoma/demos/README.md` must include:

- a summary table of all demos
- required model table
- expected directory layout
- quickstart to choose and run a demo
- note that all demos are sandboxed
- note that all generated content goes into `tmp/` and `tmp/output/`

Each per-demo README must be reviewed for consistency against the required README
contract.

#### Task 5.3 Configuration Updates

The implementation plan must require that every demo `config.yaml`:

- is local to the demo
- uses `.yaml` extension
- uses only Ollama
- points to demo-local temp/state paths
- documents any feature-specific configuration in the README

If helper scripts require environment variables, those variables must be fully
documented in the demo-local README and must not depend on repository-global
shell state.

#### Task 5.4 Testing Requirements

The implementation plan must require a final validation matrix with one row per
demo and at least the following columns:

| Column               | Meaning                   |
| -------------------- | ------------------------- |
| Demo Name            | demo identifier           |
| Directory Exists     | scaffold exists           |
| README Complete      | required sections present |
| Config Local         | uses local config         |
| Provider Ollama      | true/false                |
| Model Correct        | true/false                |
| Setup Script Exists  | true/false                |
| Run Script Exists    | true/false                |
| Reset Script Exists  | true/false                |
| Tmp Gitignore Exists | true/false                |
| Output Dir Defined   | true/false                |
| Self Contained       | true/false                |
| Sandbox Documented   | true/false                |

The implementation plan must also require the standard quality gates for any
code or scripts added to the repository, plus Markdown quality for all new docs.

#### Task 5.5 Deliverables

| Deliverable                            | Verification                                                      |
| -------------------------------------- | ----------------------------------------------------------------- |
| top-level demos documentation complete | `xzatoma/demos/README.md` updated                                 |
| demo implementation summary written    | `xzatoma/docs/explanation/demo_implementation.md` exists          |
| implementations index updated          | `xzatoma/docs/explanation/implementations.md` contains demo entry |
| validation matrix defined              | plan includes explicit final matrix                               |

#### Task 5.6 Success Criteria

The full demo initiative is complete only if all of the following are true:

1. all seven demo directories exist under `xzatoma/demos/`
2. every demo contains a complete self-contained scaffold
3. every demo includes `README.md`, `config.yaml`, `setup.sh`, `run.sh`,
   `reset.sh`, `tmp/.gitignore`, and `tmp/output/`
4. every demo uses Ollama only
5. all non-vision demos use `granite4:3b`
6. the vision demo uses `granite3.2-vision:2b`
7. every demo documents sandbox boundaries and output paths
8. all generated demo files are constrained to the demo-local `tmp/`
9. all output artifacts are constrained to demo-local `tmp/output/`
10. top-level demo documentation and implementation tracking docs are updated

## Required Test and Validation Inventory

The implementation plan for the eventual demo work MUST require the following
verification classes:

| Validation Class                 | Scope                                               |
| -------------------------------- | --------------------------------------------------- |
| File existence validation        | every required demo file and directory              |
| Config validation                | provider and model correctness                      |
| README validation                | required sections and commands documented           |
| Isolation validation             | no writes outside demo-local directory              |
| Temp/output validation           | generated data stays under `tmp/` and `tmp/output/` |
| Feature-local fixture validation | MCP, watcher, skills fixtures live locally          |
| Reset safety validation          | reset scripts only touch demo-local generated state |
| Top-level index validation       | demos README lists and explains all demos           |

## Required Documentation Outputs

| File                                                   | Purpose                        |
| ------------------------------------------------------ | ------------------------------ |
| `xzatoma/docs/explanation/demo_implementation_plan.md` | phased implementation plan     |
| `xzatoma/docs/explanation/demo_implementation.md`      | implementation summary         |
| `xzatoma/demos/README.md`                              | top-level demos index          |
| `xzatoma/docs/explanation/implementations.md`          | implementation tracking update |

## Deferred Work

The following items are explicitly deferred and MUST NOT be included in the
first implementation unless a follow-up plan is created:

| Deferred Item                                              | Reason                       |
| ---------------------------------------------------------- | ---------------------------- |
| Copilot-based demos                                        | Out of scope by decision     |
| Built-in remote demo orchestration service                 | Out of scope                 |
| Shared mutable state or shared helper assets between demos | Violates self-contained rule |
| Demo output outside `tmp/output/`                          | Violates isolation rule      |
| Cross-demo runtime dependencies                            | Violates self-contained rule |
| Non-Ollama provider walkthroughs                           | Out of scope by decision     |

## Risks and Mitigations

| Risk                                                     | Mitigation                                                                     |
| -------------------------------------------------------- | ------------------------------------------------------------------------------ |
| Demo writes outside its sandbox                          | Require explicit working directory and local temp/output paths in every script |
| Demo uses wrong model                                    | Require per-demo config validation against the model matrix                    |
| README instructions drift from scripts                   | Require README commands to match script names exactly                          |
| Feature-heavy demos become non-self-contained            | Require all helper configs and scripts to live inside the demo directory       |
| Temp data is committed accidentally                      | Require `tmp/.gitignore` in every demo                                         |
| Watcher or MCP demos depend on external repository state | Require demo-local fixtures and demo-local startup instructions only           |

## Final Recommended Implementation Order

1. Phase 1: Demo framework and shared conventions
2. Phase 2: Core demo scaffolding for chat, run, and vision
3. Phase 3: Advanced feature demo scaffolding for skills, MCP, and subagents
4. Phase 4: Watcher demo and end-to-end isolation hardening
5. Phase 5: Documentation, validation matrix, and completion hardening
