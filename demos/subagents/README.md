# Subagents Demo

## Goal

Demonstrate nested agent delegation in XZatoma. The demo proves that:

- The coordinator agent can delegate independent tasks to subagents using the
  `parallel_subagent` tool
- Each subagent runs in an isolated conversation context and cannot interfere
  with the parent conversation
- All artifacts produced by subagents are written to `tmp/output/`
- Subagent telemetry and conversation persistence are captured in `tmp/`

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

   Add the binary to `PATH` or ensure it is reachable at
   `../../target/release/xzatoma` relative to this directory.

## Directory Layout

```text
demos/subagents/
  README.md                     # This file
  config.yaml                   # Demo-local configuration with subagents enabled
  setup.sh                      # Prepares tmp/ and verifies prerequisites
  run.sh                        # Runs the subagents delegation demo plan
  reset.sh                      # Removes all generated state
  plans/
    subagents_demo.yaml         # Plan delegating three tasks to parallel subagents
  input/
    tasks.txt                   # Reference description of each delegated task
  tmp/
    .gitignore                  # Excludes all generated files from version control
    output/                     # All subagent artifacts are written here
```

## Setup

```sh
cd demos/subagents
./setup.sh
```

`setup.sh` performs the following steps:

1. Creates `tmp/output/` if it does not exist.
2. Verifies that `plans/subagents_demo.yaml` is present.
3. Checks that `xzatoma` is available on `PATH` or in the build output.
4. Checks that Ollama is running and `granite4:3b` is available.

## Run

```sh
./run.sh
```

`run.sh` executes `xzatoma run --plan ./plans/subagents_demo.yaml`. The
coordinator agent uses the `parallel_subagent` tool to spawn three subagents
concurrently:

- **haiku-writer** writes a haiku about autonomous agents to
  `tmp/output/haiku.txt`
- **mcp-describer** writes a three-sentence description of the Model Context
  Protocol to `tmp/output/mcp_description.txt`
- **rust-advocate** writes a numbered list of five Rust CLI benefits to
  `tmp/output/rust_benefits.txt`

After all subagents complete, the coordinator reads each output file and writes
a one-paragraph summary to `tmp/output/summary.txt`.

The full execution transcript is saved to `tmp/output/subagents_run.txt`.

To run a specific command manually:

```sh
xzatoma --config ./config.yaml --storage-path ./tmp/xzatoma.db \
  run --plan ./plans/subagents_demo.yaml
```

## Expected Output

After `./run.sh` completes, the following files appear in `tmp/output/`:

| File                  | Contents                                           |
| --------------------- | -------------------------------------------------- |
| `subagents_run.txt`   | Full plan execution transcript including telemetry |
| `haiku.txt`           | A 5-7-5 haiku about autonomous agents              |
| `mcp_description.txt` | Three-sentence description of MCP                  |
| `rust_benefits.txt`   | Numbered list of five Rust CLI benefits            |
| `summary.txt`         | One-paragraph coordinator summary of all results   |

Subagent telemetry events (spawn, complete, error) are written to the transcript
in `tmp/output/subagents_run.txt`. Each subagent completion event includes its
label, duration, and token count.

## Reset

```sh
./reset.sh
```

`reset.sh` removes:

- `tmp/xzatoma.db` (coordinator conversation history database)
- `tmp/subagent_conversations.db` (subagent persistence database)
- All files under `tmp/output/` except `.gitkeep`
- Any other generated files under `tmp/`

Plan files, `input/tasks.txt`, and `config.yaml` are never modified.

## Sandbox Boundaries

XZatoma is constrained to this demo directory by the following configuration:

- `--config ./config.yaml` is passed on every invocation. The repository-level
  `config/config.yaml` is never loaded.
- `--storage-path ./tmp/xzatoma.db` directs all coordinator conversation history
  into `tmp/`.
- `agent.subagent.persistence_path: ./tmp/subagent_conversations.db` directs all
  subagent conversation persistence into `tmp/`.
- `agent.subagent.max_depth: 2` limits recursion. Subagents spawned by this demo
  run at depth 1 and cannot spawn further nested subagents beyond depth 2.
- `agent.subagent.max_executions: 5` caps the total number of subagent
  invocations per session to prevent runaway execution.
- `agent.subagent.telemetry_enabled: true` records structured telemetry events
  into the execution transcript for inspection.
- `skills.enabled: false` prevents skill discovery from running.

All files written by the coordinator and by each subagent are directed into
`tmp/output/` by the plan instructions. No generated file may appear outside the
`tmp/` directory.

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

### Subagent depth limit reached

The error message `subagent depth limit reached` means the recursion depth
exceeded `agent.subagent.max_depth`. The default for this demo is `2`. If the
plan attempts deeper nesting, increase `max_depth` in `config.yaml`.

### Subagent execution quota reached

The error message `subagent execution quota reached` means more than
`agent.subagent.max_executions` subagents were spawned in one session. Increase
`max_executions` in `config.yaml` or run `./reset.sh` to start a fresh session.

### Output files not created

If subagent output files are missing after `./run.sh`, inspect
`tmp/output/subagents_run.txt` for tool call errors. The most common cause is
the agent using a path outside `tmp/output/`. Ensure the working directory is
the demo root when running the plan. The `run.sh` script always sets
`cd "$DEMO_DIR"` before invoking `xzatoma`.

### Plan execution times out

The subagents demo uses a longer timeout (`agent.timeout_seconds: 600`) because
three subagents run concurrently and each makes multiple model calls. If the
demo still times out, verify Ollama has enough resources to serve concurrent
requests and consider pulling a lighter model variant.
