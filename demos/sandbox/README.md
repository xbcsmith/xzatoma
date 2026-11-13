# Sandbox Demo

## Goal

Demonstrate XZatoma's sandbox enforcement layer against a local Ollama model.
This demo proves two things:

1. **Allowed operations succeed.** File writes, reads, and searches inside the
   sandbox filesystem root complete without error.
2. **Forbidden operations are blocked.** An attempt to write outside the sandbox
   filesystem root is denied by `SandboxGuard`. The error returned by the
   sandbox is the proof that enforcement is active.

The demo uses autonomous `run` mode across two plan files:

| Plan                            | Expected result               | Purpose                                             |
| ------------------------------- | ----------------------------- | --------------------------------------------------- |
| `plans/allowed_operations.yaml` | All steps succeed             | Shows the sandbox permits normal in-root operations |
| `plans/enforcement_demo.yaml`   | Plan fails with sandbox error | Shows the sandbox blocks out-of-root writes         |

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
  README.md                             This walkthrough
  config.yaml                           Demo-local XZatoma configuration
  setup.sh                              Prepare demo-local state
  run.sh                                Execute both demo phases
  reset.sh                              Remove all generated state
  plans/
    allowed_operations.yaml             Five-step plan; all steps succeed
    enforcement_demo.yaml               Single-step plan; blocked by sandbox
  tmp/
    .gitignore                          Excludes generated files from version control
    output/
      .gitkeep                          Preserves the empty output directory in git
```

Generated at runtime (inside `tmp/`):

| File                                       | Description                                                |
| ------------------------------------------ | ---------------------------------------------------------- |
| `tmp/xzatoma.db`                           | SQLite run history                                         |
| `tmp/output/phase1_run.txt`                | Full Phase 1 agent output                                  |
| `tmp/output/allowed_write.txt`             | File written in Phase 1 Step 1 (shows in-root writes work) |
| `tmp/output/allowed_operations_report.txt` | Summary written at the end of Phase 1                      |
| `tmp/output/phase2_enforcement.txt`        | Phase 2 output containing the sandbox denial message       |

## Setup

Run the setup script from anywhere; it resolves the demo root from its own
location:

```sh
sh ./setup.sh
```

The script creates `tmp/output/` and verifies that Ollama is reachable and the
`granite4:3b` model is available.

If `xzatoma` is not on your `PATH`, build and install it:

```sh
# From the repository root
cargo build --release
export PATH="$PATH:$(pwd)/target/release"
```

## Run

Execute both demo phases:

```sh
sh ./run.sh
```

Alternatively, if you have marked the scripts executable:

```sh
chmod +x setup.sh run.sh reset.sh
./run.sh
```

The script runs the following two commands internally:

```sh
# Phase 1: allowed operations (all steps succeed)
xzatoma run \
  --config ./config.yaml \
  --storage-path ./tmp/xzatoma.db \
  --allow-dangerous \
  --plan ./plans/allowed_operations.yaml

# Phase 2: enforcement demo (expected to fail with sandbox error)
xzatoma run \
  --config ./config.yaml \
  --storage-path ./tmp/xzatoma.db \
  --allow-dangerous \
  --plan ./plans/enforcement_demo.yaml
```

## Expected Output

### Phase 1

All five steps complete successfully. The agent writes files inside the demo
directory, reads `config.yaml`, searches it, lists the output directory, and
writes `tmp/output/allowed_operations_report.txt`.

The report confirms each step succeeded and notes the sandbox configuration.

### Phase 2

The plan attempts to write to `/tmp/xzatoma_sandbox_escape.txt`. The sandbox
rejects the write before any I/O occurs. The terminal shows:

```text
Execution failed: Tool execution error: Tool 'write_file' execution failed:
  Tool execution error: Absolute path not allowed: /tmp/xzatoma_sandbox_escape.txt
```

`run.sh` treats the non-zero exit code as confirmation:

```text
Phase 2 result: SANDBOX ENFORCED (exit 1)
The error above confirms the sandbox blocked the out-of-root write.
```

This failure is the correct and expected result. The file
`/tmp/xzatoma_sandbox_escape.txt` is never created.

## Reset

Remove all generated state and return the demo to its initial condition:

```sh
sh ./reset.sh
```

The reset script removes `tmp/xzatoma.db` and all files in `tmp/output/` except
`.gitkeep`. Plan files and `config.yaml` are never modified.

After reset, run `setup.sh` again before starting a new run.

## Sandbox Boundaries

XZatoma is scoped to this demo directory during execution:

1. `run.sh` changes into the demo root before invoking `xzatoma`. The sandbox
   `filesystem_root` is `null` in `config.yaml`, which resolves to the current
   working directory -- the demo directory. Only paths inside this directory are
   permitted for file reads and writes.

2. `protect_git: true` blocks any write whose path contains a `.git` component,
   even when the path is otherwise inside the filesystem root.

3. `network_enabled: false` prevents terminal programs that make outbound
   network connections (`ssh`, `curl`, `nc`, etc.) from being executed. The
   `CommandValidator` rejects them before the process is ever spawned.

4. The `--config ./config.yaml` flag ensures the repository-level config is
   never loaded at demo runtime.

5. The `--storage-path ./tmp/xzatoma.db` flag directs all run history into
   `tmp/`.

## Troubleshooting

### xzatoma: command not found

Add `target/release/` to your `PATH` or run the binary directly. The `run.sh`
script also searches for the binary at `../../target/release/xzatoma` and
`../../target/debug/xzatoma` relative to the demo directory.

### Ollama connection refused

Ollama is not running. Start it with `ollama serve`.

### Model not found: granite4:3b

Pull it with `ollama pull granite4:3b`.

### Phase 1 fails

Check that Ollama is running and the model is available. Verify that
`config.yaml` has `agent.chat.default_mode: write` so the agent has access to
the `write_file` and `terminal` tools.

### Phase 2 succeeds (no sandbox denial)

The sandbox may be disabled. Verify that `config.yaml` has
`sandbox.enabled: true` and that you are running from inside the demo directory
so that `filesystem_root: null` resolves to the demo directory and not a parent
path that includes `/tmp`.

### Permission denied when running scripts

```sh
chmod +x setup.sh run.sh reset.sh
```
