# XZatoma Demos

XZatoma provides self-contained demos. Each demo lives in its own directory
under `demos/` and can be copied to any filesystem location and run without
modification.

## Overview

| Demo      | Directory                    | Purpose                                                       |
| --------- | ---------------------------- | ------------------------------------------------------------- |
| Chat      | `demos/chat/`                | Interactive conversation with a local Ollama model            |
| Run       | `demos/run/`                 | Autonomous plan execution from a plan file or prompt          |
| Skills    | `demos/skills/`              | Skill discovery, loading, and activation                      |
| MCP       | `demos/mcp/`                 | Model Context Protocol server integration                     |
| Subagents | `demos/subagents/`           | Nested agent delegation and parallel execution                |
| Vision    | `demos/vision/`              | Image understanding with a multimodal Ollama model            |
| Watcher   | `demos/watcher/`             | Event-driven plan execution via a Kafka-compatible topic      |
| llama.cpp | `demos/providers/llama_cpp/` | OpenAI-compatible provider backed by a local llama.cpp server |

## Model Requirements

Most demos use Ollama as the provider. The `providers/llama_cpp` demo uses
XZatoma's OpenAI-compatible provider pointed at a local `llama-server` instance.
No demo uses or references GitHub Copilot.

| Demo      | Provider           | Model                            |
| --------- | ------------------ | -------------------------------- |
| Chat      | ollama             | `granite4:3b`                    |
| Run       | ollama             | `granite4:3b`                    |
| Skills    | ollama             | `granite4:3b`                    |
| MCP       | ollama             | `granite4:3b`                    |
| Subagents | ollama             | `granite4:3b`                    |
| Vision    | ollama             | `granite3.2-vision:2b`           |
| Watcher   | ollama             | `granite4:3b`                    |
| llama.cpp | openai (llama.cpp) | `granite-3.3-2b-instruct` (GGUF) |

Pull the Ollama models before running the Ollama-based demos:

```sh
ollama pull granite4:3b
ollama pull granite3.2-vision:2b
```

The llama.cpp demo requires a GGUF model file and a running `llama-server`
instance. See `demos/providers/llama_cpp/README.md` for download and startup
instructions.

## Demo Directory Layout

Every demo follows this required directory contract:

```text
demos/<demo>/
  README.md           # User walkthrough for this demo
  config.yaml         # Demo-local XZatoma configuration
  setup.sh            # Prepare demo-local state
  run.sh              # Execute the main demo flow
  reset.sh            # Remove all generated state
  input/              # Static demo input data
  tmp/
    .gitignore        # Excludes generated files from version control
    output/           # All result artifacts are written here
```

Provider demos live under `demos/providers/<provider>/` and follow the same
contract. The `models/` directory is additional and optional — used by the
llama.cpp demo to store GGUF files locally (not tracked by git).

Conditional directories are included per demo when required:

| Directory  | Condition                                               |
| ---------- | ------------------------------------------------------- |
| `plans/`   | Required when the demo uses `run` with plan files       |
| `skills/`  | Required for the skills demo or skill-enabled scenarios |
| `mcp/`     | Required for MCP server fixtures                        |
| `watcher/` | Required for watcher event and configuration fixtures   |
| `scripts/` | Optional additional helper scripts                      |

## Quickstart

### Prerequisites

1. Install [Ollama](https://ollama.com) and pull the required models:

   ```sh
   ollama pull granite4:3b
   ollama pull granite3.2-vision:2b
   ```

2. Build XZatoma from the repository root:

   ```sh
   cargo build --release
   ```

### Running a Demo

Each demo follows the same three-command pattern:

```sh
cd demos/<demo>
./setup.sh
./run.sh
```

Replace `<demo>` with one of: `chat`, `run`, `skills`, `mcp`, `subagents`,
`vision`, `watcher`, or `providers/llama_cpp`.

The llama.cpp demo requires `llama-server` to be running before `./setup.sh`
is invoked. See `demos/providers/llama_cpp/README.md` for the startup command.

### Resetting a Demo

```sh
cd demos/<demo>
./reset.sh
```

This removes all generated files under `tmp/` and returns the demo to its
initial state. Static input files and configuration are never removed by
`reset.sh`.

## Self-Containment

Every demo directory is independently portable. The rules below apply to all
demos without exception:

1. Copying a single demo directory to any filesystem location produces a fully
   functional demo. No files outside the copied directory are required at
   runtime.

2. No demo references files in sibling demo directories, in `xzatoma/config/`,
   or in any repository path outside its own directory.

3. No shared helper directory exists under `demos/`. There is no
   `demos/_shared/` directory. Every required file lives inside the demo that
   needs it.

4. Each demo uses its own `config.yaml`. The repository-level
   `config/config.yaml` is never referenced at demo runtime.

5. Every demo script resolves the demo root from the script's own location. No
   script assumes the repository root is the current working directory.

## Isolation Rules

Every demo enforces the following boundaries:

1. All generated state is written under `<demo>/tmp/`. No generated file may
   appear outside that directory.

2. All result artifacts are written to `<demo>/tmp/output/`. No output file may
   be written outside that subdirectory.

3. Every `tmp/` directory contains a `.gitignore` file that excludes all
   generated content from version control.

4. XZatoma is invoked with the demo directory as the working directory and is
   given only the demo-local `config.yaml` via the `--config` flag.

5. History, storage, and all other runtime data produced by XZatoma are directed
   into the demo-local `tmp/` directory.

6. Only Ollama is used as the provider. GitHub Copilot is not documented or
   activated in any demo as a primary or fallback provider.

## Per-Demo README Contract

Each demo directory contains a `README.md` with the following sections in order:

| Section                 | Required Content                                    |
| ----------------------- | --------------------------------------------------- |
| `# <Demo Name> Demo`    | Exact demo title                                    |
| `## Goal`               | What feature the demo proves                        |
| `## Prerequisites`      | Ollama model, any local services, required commands |
| `## Directory Layout`   | Explanation of files and folders in the demo        |
| `## Setup`              | Exact commands to prepare the demo                  |
| `## Run`                | Exact commands to execute the demo                  |
| `## Expected Output`    | What appears in `tmp/output/`                       |
| `## Reset`              | Exact commands to clean generated state             |
| `## Sandbox Boundaries` | How XZatoma is scoped to the demo directory         |
| `## Troubleshooting`    | Common failure modes and fixes                      |

Every command documented in a demo README must remain correct after the demo
directory is copied to a new filesystem location.

## Demo Status

| Demo      | Status     |
| --------- | ---------- |
| Chat      | Scaffolded |
| Run       | Scaffolded |
| Skills    | Scaffolded |
| MCP       | Scaffolded |
| Subagents | Scaffolded |
| Vision    | Scaffolded |
| Watcher   | Complete   |
| llama.cpp | Complete   |
