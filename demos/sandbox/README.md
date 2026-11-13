# Sandbox Demo

## Goal

Demonstrate XZatoma's sandbox enforcement layer against a local Ollama model.
This demo proves that the sandbox correctly allows permitted file operations and
correctly blocks forbidden ones, specifically:

- File writes inside the sandbox filesystem root succeed.
- File writes outside the sandbox filesystem root are denied by `SandboxGuard`.
- Writes to `.git` directories are denied when `protect_git` is true.
- Terminal commands that use network tools (such as `ssh`) are blocked when
  `network_enabled` is false.

The demo uses autonomous `run` mode: no interactive input is required. The agent
executes a plan that intentionally attempts forbidden operations, records the
sandbox denial messages, and writes a summary report.

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
   `target/release/xzatoma` for use with `run.sh`.

## Directory Layout

```text
sandbox/
  README.md                         This walkthrough
  config.yaml                       Demo-local XZatoma configuration
  setup.sh                          Prepare demo-local state
  run.sh                            Execute the sandbox boundary verification
  reset.sh                          Remove all generated state
  plans/
    sandbox_boundaries.yaml         Five-step plan exercising sandbox enforcement
  tmp/
    .gitignore                      Excludes generated files from version control
    output/
      .gitkeep                      Preserves the empty output directory in git
```

Generated at runtime (inside `tmp/`):

| File                            | Description                            |
| ------------------------------- | -------------------------------------- |
| `tmp/xzatoma.db`                | SQLite conversation and run history    |
| `tmp/output/sandbox_run.txt`    | Full agent output captured from run.sh |
| `tmp/output/inside_root.txt`    | File written by Step 1 (allowed write) |
| `tmp/output/sandbox_report.txt` | Summary report written by Step 5       |

The agent also attempts (and fails) to write to
`/tmp/xzatoma_sandbox_escape.txt` and `.git/sandbox_test.txt`. Those files are
never created because the sandbox blocks them before any I/O occurs.

## Setup

Run the setup script from anywhere; it resolves the demo root from its own
location:

```sh
sh ./setup.sh
```

The script creates `tmp/output/` and verifies that Ollama is reachable and the
`granite4:3b` model is available. Warnings are printed for any missing
prerequisites; the script does not fail hard, allowing you to address issues
before running.

If `xzatoma` is not on your `PATH`, build and install it:

```sh
# From the repository root
cargo build --release
export PATH="$PATH:$(pwd)/target/release"
```

## Run

Execute the sandbox boundary verification plan:

```sh
sh ./run.sh
```

Alternatively, if you have marked the scripts executable:

```sh
chmod +x setup.sh run.sh reset.sh
./run.sh
```

The script runs the following command internally:

```sh
xzatoma \
  --config ./config.yaml \
  --storage-path ./tmp/xzatoma.db \
  run \
  --plan ./plans/sandbox_boundaries.yaml
```

The agent will:

1. Write `tmp/output/inside_root.txt` (succeeds).
2. Attempt to write `/tmp/xzatoma_sandbox_escape.txt` (sandbox denies it).
3. Attempt to write `.git/sandbox_test.txt` (protect_git denies it).
4. Attempt to run `ssh` (network_enabled=false blocks it).
5. Write `tmp/output/sandbox_report.txt` with results from all steps.

## Expected Output

The terminal shows each agent step as it runs. Steps 2, 3, and 4 produce error
messages from the sandbox; the agent records these and continues.

After the run, `tmp/output/` contains:

```text
inside_root.txt       -- file created by Step 1 (write allowed)
sandbox_run.txt       -- complete agent output transcript
sandbox_report.txt    -- summary with result for each step
```

The report at `tmp/output/sandbox_report.txt` follows this structure:

```text
Sandbox Boundary Verification Report
=====================================

Step 1 - Write inside root     : succeeded
Step 2 - Write outside root    : blocked - <sandbox denial message>
Step 3 - Write to .git dir     : blocked - <sandbox denial message>
Step 4 - SSH command           : blocked - <command validator message>

Sandbox configuration active during this run:
  enabled            : true
  filesystem_root    : (working directory)
  protect_git        : true
  network_enabled    : false
  sandboxed_tools    : terminal=true, file_ops=true, fetch=true
```

The exact error messages in the report depend on the sandbox enforcement
messages produced at runtime.

## Reset

Remove all generated state and return the demo to its initial condition:

```sh
sh ./reset.sh
```

The reset script removes:

- `tmp/xzatoma.db` (run history)
- All files in `tmp/output/` except `.gitkeep`
- Any other generated files in `tmp/` except `.gitignore`

Static files (`README.md`, `config.yaml`, `setup.sh`, `run.sh`, `reset.sh`, and
the `plans/` directory) are never modified by `reset.sh`.

After reset, run `setup.sh` again before starting a new run.

## Sandbox Boundaries

XZatoma is scoped to this demo directory during execution. The following
mechanisms enforce the boundary:

1. `run.sh` changes into the demo root before invoking `xzatoma`. The sandbox
   `filesystem_root` defaults to `null`, which resolves to the current working
   directory -- the demo directory. Only paths inside this directory are
   permitted for file reads and writes.

2. The `--config ./config.yaml` flag ensures the repository-level config is
   never loaded at demo runtime. All sandbox settings are defined in the
   demo-local `config.yaml`.

3. The `--storage-path ./tmp/xzatoma.db` flag directs all run history into
   `tmp/`.

4. `protect_git: true` in `config.yaml` blocks any write whose path contains a
   `.git` component, even when the path is otherwise inside the filesystem root.

5. `network_enabled: false` in `config.yaml` blocks terminal programs that make
   outbound network connections (`ssh`, `curl`, `nc`, etc.) at the
   command-validator layer before the process is spawned.

6. All paths in the demo are relative to the demo root. The demo directory can
   be copied to any filesystem location and run without modification.

## Troubleshooting

### xzatoma: command not found

The binary is not on your `PATH`. Either add `target/release/` to your `PATH` or
run the binary directly. The `run.sh` script also searches for the binary at
`../../target/release/xzatoma` and `../../target/debug/xzatoma` relative to the
demo directory.

### Ollama connection refused

Ollama is not running. Start it with:

```sh
ollama serve
```

### Model not found: granite4:3b

The model has not been pulled. Run:

```sh
ollama pull granite4:3b
```

### All steps report success (no sandbox denials observed)

The sandbox may be disabled. Verify that `config.yaml` has
`sandbox.enabled: true` and that you are running from inside the demo directory
so that `filesystem_root` resolves correctly.

### sandbox_report.txt is missing after the run

The agent may have stopped early due to an error unrelated to sandbox
enforcement (for example, Ollama being unreachable). Check
`tmp/output/sandbox_run.txt` for the full output transcript and error details.

### Permission denied when running scripts

Mark the scripts as executable before running them:

```sh
chmod +x setup.sh run.sh reset.sh
```
