# Run Demo

## Goal

Demonstrate XZatoma's autonomous plan execution capability using a local Ollama
model. This demo proves that the `run` CLI command can load a YAML plan file,
parse its steps, and execute them through the agent without any interactive
input from the user.

The demo includes two plan files:

- `plans/hello_world.yaml` — a single-step plan that verifies the end-to-end
  setup is working
- `plans/system_info.yaml` — a multi-step plan that gathers basic system
  information using shell commands

## Prerequisites

1. Install [Ollama](https://ollama.com) and start the server:

   ```sh
   ollama serve
   ```

2. Pull the required model:

   ```sh
   ollama pull granite4:3b
   ```

3. Build XZatoma from the repository root:

   ```sh
   cargo build --release
   ```

4. Ensure the `xzatoma` binary is on your `PATH`, or note the path to
   `target/release/xzatoma` for direct invocation.

## Directory Layout

```text
run/
  README.md                    This walkthrough
  config.yaml                  Demo-local XZatoma configuration
  setup.sh                     Verify prerequisites and prepare tmp/
  run.sh                       Execute the hello_world plan
  reset.sh                     Remove all generated state
  plans/
    hello_world.yaml           Single-step greeting plan
    system_info.yaml           Multi-step system information plan
  input/
    notes.txt                  Notes and example invocation commands
  tmp/
    .gitignore                 Excludes generated files from version control
    output/
      .gitkeep                 Preserves the empty output directory in git
```

Generated at runtime (inside `tmp/`):

| File                        | Description                              |
| --------------------------- | ---------------------------------------- |
| `tmp/xzatoma.db`            | SQLite conversation and history database |
| `tmp/output/run_output.txt` | Captured output from the last `run.sh`   |

## Setup

Run the setup script from anywhere inside or outside the demo directory. It
resolves the demo root from its own location:

```sh
sh ./setup.sh
```

The script:

1. Creates `tmp/output/` if it does not already exist
2. Verifies that `plans/hello_world.yaml` and `plans/system_info.yaml` exist
3. Checks that `xzatoma` is available on `PATH`
4. Checks that Ollama is reachable at `http://localhost:11434`
5. Checks that the `granite4:3b` model has been pulled

If `xzatoma` is not on your `PATH`, build and install it from the repository
root:

```sh
cargo build --release
export PATH="$PATH:$(pwd)/target/release"
```

## Run

Execute the default `hello_world` plan:

```sh
sh ./run.sh
```

Alternatively, after marking the scripts executable:

```sh
chmod +x setup.sh run.sh reset.sh
./run.sh
```

The script writes all output to both the terminal and
`tmp/output/run_output.txt`.

To run a different plan directly:

```sh
xzatoma \
  --config ./config.yaml \
  --storage-path ./tmp/xzatoma.db \
  run --plan ./plans/system_info.yaml
```

To run a direct prompt instead of a plan file:

```sh
xzatoma \
  --config ./config.yaml \
  --storage-path ./tmp/xzatoma.db \
  run --prompt "List the files in the current directory and summarize what you find."
```

To allow the agent to run commands without confirmation:

```sh
xzatoma \
  --config ./config.yaml \
  --storage-path ./tmp/xzatoma.db \
  run --plan ./plans/system_info.yaml --allow-dangerous
```

## Expected Output

After running `sh ./run.sh`, the file `tmp/output/run_output.txt` is created and
contains the full agent execution transcript. A successful hello world run
produces output similar to:

```text
Executing task...

Result:
Hello from XZatoma
```

A successful `system_info` run produces a multi-step transcript showing the
agent executing `hostname`, `uname -a`, `pwd`, `df -h .`, and `uptime` in
sequence.

All output artifacts are written exclusively to `tmp/output/`.

## Reset

Remove all generated state and return the demo to its initial condition:

```sh
sh ./reset.sh
```

The reset script removes:

- `tmp/xzatoma.db` (conversation and history database)
- All files in `tmp/output/` except `.gitkeep`
- Any other generated files under `tmp/` except `.gitignore`

Static files — `README.md`, `config.yaml`, the scripts, `plans/`, and `input/` —
are never removed by `reset.sh`.

After reset, run `setup.sh` again before executing a new run.

## Sandbox Boundaries

XZatoma is scoped to this demo directory during execution. The following
mechanisms enforce the boundary:

1. `run.sh` changes into the demo root before invoking `xzatoma`, so the agent
   treats this directory as the working directory for all file operations.

2. The `--config ./config.yaml` flag ensures the repository-level
   `config/config.yaml` is never loaded at demo runtime.

3. The `--storage-path ./tmp/xzatoma.db` flag directs all history and session
   data into `tmp/`.

4. The terminal execution mode in `config.yaml` is set to
   `restricted_autonomous`, which limits the commands the agent may run without
   confirmation.

5. The plan files reference only standard POSIX commands (`hostname`, `uname`,
   `pwd`, `df`, `uptime`, `echo`) that produce read-only output and do not write
   files outside the demo directory.

6. All output artifacts are written to `tmp/output/` by the `run.sh` script.

7. The demo directory can be copied to any filesystem location and all commands
   documented in this README will work without modification.

## Troubleshooting

### xzatoma: command not found

The `xzatoma` binary is not on your `PATH`. Build it and export the path:

```sh
cargo build --release
export PATH="$PATH:$(pwd)/target/release"
```

The `run.sh` script also checks for the binary at `../../target/release/xzatoma`
relative to the demo directory.

### Either --plan or --prompt must be provided

You invoked `xzatoma run` without either flag. Always supply one of:

- `--plan ./plans/hello_world.yaml`
- `--prompt "your task here"`

### Plan file not found

The path to the plan file must be relative to the demo directory. Confirm you
are running `run.sh` from inside the demo directory, or that you used the
correct relative path when invoking `xzatoma` directly.

### Ollama connection refused

Ollama is not running. Start it with:

```sh
ollama serve
```

### Model not found: granite4:3b

Pull the model before running the demo:

```sh
ollama pull granite4:3b
```

### Execution times out

The default `agent.timeout_seconds` in `config.yaml` is 300 seconds. If your
machine is slow or Ollama is under heavy load, increase this value in
`config.yaml`:

```yaml
agent:
  timeout_seconds: 600
```

### Permission denied when running scripts

Mark the scripts as executable:

```sh
chmod +x setup.sh run.sh reset.sh
```
