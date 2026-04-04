# Phase 1 Demo Framework Implementation

## Overview

This document records the implementation of Phase 1: Demo Framework and Shared
Conventions from the XZatoma demo implementation plan. Phase 1 establishes the
foundational rules, directory contracts, and conventions that every subsequent
demo phase depends on.

## Scope

Phase 1 covers the following tasks from the implementation plan:

| Task | Description                                  |
| ---- | -------------------------------------------- |
| 1.1  | Define global demo framework and conventions |
| 1.2  | Add foundation functionality constraints     |
| 1.3  | Integrate foundation work into demos index   |
| 1.4  | Validate required demo index sections        |
| 1.5  | Deliver demo conventions and index           |
| 1.6  | Verify success criteria                      |

## Deliverables

### docs/explanation/demo_implementation_plan.md

The master implementation plan already existed before Phase 1 execution. It
defines the following global contracts:

- Required demo directory structure for all seven demos
- Required README sections per demo
- Portable script requirements
- Sandboxing contract (demo-directory-scoped execution)
- Ollama-only model contract
- temp/output isolation rules

These contracts are the authoritative source of truth for all subsequent phases.
This document does not duplicate them.

### demos/README.md

The top-level demos index was expanded from a stub into the authoritative demo
index. It contains:

- An overview of all seven demos with purpose descriptions
- A model requirements table mapping each demo to its required Ollama model
- A demo directory layout table showing the required per-demo structure
- Quickstart instructions for pulling models, building the binary, and running
  any demo
- Self-containment rules explaining demo portability
- Isolation rules describing the temp/output and sandboxing boundaries
- A per-demo walkthrough section describing the required README sections
- A demo status table listing all seven demos

## Design Decisions

### No Shared Framework Directory

Phase 1 explicitly forbids a `demos/_shared/` directory or any shared helper
mechanism outside individual demo directories. This decision ensures that every
demo remains independently portable: copying a single demo directory to any
filesystem location produces a fully functional demo without requiring any
sibling directories or repository context.

The cost of this decision is intentional duplication. Every required file,
script, fixture, and configuration must be duplicated inside each demo that
needs it. This is the correct trade-off for a demo suite that must be runnable
from any directory on any machine that has Ollama and the XZatoma binary.

### Ollama-Only Contract

All seven demos are restricted to Ollama as the provider. GitHub Copilot is
explicitly excluded from the demo suite. This decision avoids the need for users
to authenticate with a remote service before running any demo. A local Ollama
instance with the required models is the only runtime dependency.

Two models are required:

- `granite4:3b` for Chat, Run, Skills, MCP, Subagents, and Watcher
- `granite3.2-vision:2b` for Vision

### temp/output Isolation

All generated state must live under `<demo>/tmp/`. All result artifacts must
live under `<demo>/tmp/output/`. This boundary is enforced by the scripts inside
each demo and by the demo-local `config.yaml` which scopes XZatoma storage and
history paths to the `tmp/` directory.

Every `tmp/` directory includes a `.gitignore` that excludes all generated
content from version control. This prevents demo runs from polluting the
repository with transient state.

### Script Portability

Every `setup.sh`, `run.sh`, and `reset.sh` resolves the demo root from the
script's own location rather than depending on the current working directory or
the repository root. This ensures that running a script directly (e.g.,
`./demos/chat/setup.sh` from the repository root) produces the same result as
running it from inside the demo directory (e.g., `cd demos/chat && ./setup.sh`).

The standard pattern for all demo scripts is:

```sh
DEMO_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$DEMO_DIR"
```

This resolves the absolute path of the script's directory and changes into it
before any other work is performed. All paths derived from `$DEMO_DIR` are
absolute at runtime, which satisfies the portability requirement.

## File Changes

| File                                                       | Action  | Description                          |
| ---------------------------------------------------------- | ------- | ------------------------------------ |
| `docs/explanation/demo_implementation_plan.md`             | Existed | Defines all global demo contracts    |
| `demos/README.md`                                          | Updated | Expanded into the authoritative demo |
| `docs/explanation/phase1_demo_framework_implementation.md` | Created | This document                        |

## Success Criteria Verification

| Criterion                                                        | Status |
| ---------------------------------------------------------------- | ------ |
| `docs/explanation/demo_implementation_plan.md` exists            | Pass   |
| `demos/README.md` defines all 7 demos                            | Pass   |
| Plan defines self-containment, sandboxing, and temp/output rules | Pass   |
| Plan defines Ollama-only model usage                             | Pass   |

## Validation Checklist

The following items must be verified before Phase 1 is considered complete:

- `demos/README.md` contains a section for each of: Chat, Run, Skills, MCP,
  Subagents, Vision, Watcher
- `demos/README.md` contains a model requirements table listing all seven demos
- `demos/README.md` contains a demo directory layout section
- `demos/README.md` contains quickstart instructions
- `demos/README.md` contains isolation rules
- `demos/README.md` contains a self-containment statement
- All seven demo names are listed in the demo status table
- No `demos/_shared/` directory exists
- No shared script outside a demo directory exists
- Markdown files pass `markdownlint --fix --config .markdownlint.json`
- Markdown files pass `prettier --write --parser markdown --prose-wrap always`

## References

- `docs/explanation/demo_implementation_plan.md` - Master plan with all global
  contracts
- `demos/README.md` - Authoritative demo index
- `AGENTS.md` - Development guidelines and coding standards
