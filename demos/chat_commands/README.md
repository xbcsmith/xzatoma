# Chat Commands Demo

## Goal

Demonstrate the 13 unified slash commands available in XZatoma's chat interface.
This demo runs non-interactively by piping a prepared script into `xzatoma chat`
and capturing the output, so you can review every command and its response in
one pass.

The commands exercised are:

- `/help` - list all available commands
- `/mode status` - check the active chat mode
- `/mode planning` - switch to planning mode
- `/safety status` - check the active safety policy
- `/safety off` - disable safety confirmations
- `/model status` - check the active model and provider
- `/streaming` - check streaming status
- `/system status` - check the active system prompt
- `/system <text>` - replace the system prompt mid-session
- `/subagents status` - check subagent delegation state
- `/context info` - inspect context window usage

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

4. Ensure the `xzatoma` binary is available on your `PATH`, or note the path to
   `target/release/xzatoma` for use with `run.sh`.

## Directory Layout

```text
chat_commands/
  README.md                        This walkthrough
  config.yaml                      Demo-local XZatoma configuration
  setup.sh                         Prepare demo-local state
  run.sh                           Run the non-interactive demo
  reset.sh                         Remove all generated state
  input/
    commands_demo_script.txt       Sequence of commands piped into xzatoma chat
  tmp/
    .gitignore                     Excludes generated files from version control
    output/
      .gitkeep                     Preserves the empty output directory in git
```

Generated at runtime (inside `tmp/`):

| File                           | Description                       |
| ------------------------------ | --------------------------------- |
| `tmp/xzatoma.db`               | SQLite conversation history       |
| `tmp/output/commands_demo.txt` | Captured output from the demo run |

## Setup

Run the setup script from anywhere; it resolves the demo root from its own
location:

```sh
sh ./setup.sh
```

The script creates `tmp/output/` and verifies that Ollama is reachable and the
`granite4:3b` model is available. Warnings are printed for any missing
prerequisites but the script does not fail hard on them, allowing you to address
issues before running.

If `xzatoma` is not on your `PATH`, build and install it:

```sh
# From the repository root
cargo build --release
export PATH="$PATH:$(pwd)/target/release"
```

## Run

Run the non-interactive demo:

```sh
sh ./run.sh
```

Alternatively, if you have marked the scripts executable:

```sh
chmod +x setup.sh run.sh reset.sh
./run.sh
```

The script pipes `input/commands_demo_script.txt` into `xzatoma chat` and writes
all output to both the terminal and `tmp/output/commands_demo.txt`:

```sh
xzatoma \
  --config ./config.yaml \
  --storage-path ./tmp/xzatoma.db \
  chat < input/commands_demo_script.txt | tee tmp/output/commands_demo.txt
```

To run the demo manually from inside this directory:

```sh
xzatoma \
  --config ./config.yaml \
  --storage-path ./tmp/xzatoma.db \
  chat < input/commands_demo_script.txt
```

## Expected Output

Each command in the script produces a response from the chat subsystem. The
output file at `tmp/output/commands_demo.txt` will contain something similar to:

```text
[PLANNING][SAFE] >> /help
Available commands: ...

[PLANNING][SAFE] >> /mode status
Current mode: planning
...

[PLANNING][SAFE] >> /safety off
Safety policy set to: never_confirm

[PLANNING][SAFE] >> /model status
Current model: granite4:3b
Provider: ollama

[PLANNING][SAFE] >> /system You are a concise assistant. Reply in one sentence.
System prompt updated.

[PLANNING][SAFE] >> /context info
Context window: .../32000 tokens used
Remaining: ... tokens
```

No AI inference is required for slash commands that query or change session
state. Commands that require a model response (such as sending a regular
question) will invoke the Ollama API.

## Reset

Remove all generated state and return the demo to its initial condition:

```sh
sh ./reset.sh
```

The reset script removes:

- `tmp/xzatoma.db` (conversation history)
- All files in `tmp/output/` except `.gitkeep`
- Any other generated files in `tmp/` except `.gitignore`

Static files (`README.md`, `config.yaml`, `setup.sh`, `run.sh`, `reset.sh`, and
the `input/` directory) are never modified by `reset.sh`.

After reset, run `setup.sh` again before starting a new session.

## Sandbox Boundaries

XZatoma is scoped to this demo directory during execution. The following
mechanisms enforce the boundary:

1. `run.sh` changes into the demo root before invoking `xzatoma`. The agent
   therefore treats this directory as the working directory for all file
   operations.

2. The `--config ./config.yaml` flag ensures the repository-level
   `config/config.yaml` is never loaded at demo runtime.

3. The `--storage-path ./tmp/xzatoma.db` flag directs all conversation history
   into `tmp/`.

4. The demo runs in `planning` mode by default (`config.yaml` sets
   `agent.chat.default_mode: planning`), which makes the agent read-only. It
   will not write files unless the session is switched to `write` mode.

5. All paths this demo uses are relative to the demo root. The demo directory
   can be copied to any filesystem location and run without modification.

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

### Demo exits immediately with no output

If `input/commands_demo_script.txt` is empty or unreadable, `xzatoma chat` will
receive no input and exit at once. Verify the file exists and is non-empty:

```sh
cat input/commands_demo_script.txt
```

### Permission denied when running scripts

Mark the scripts as executable before running them:

```sh
chmod +x setup.sh run.sh reset.sh
```

### History database locked

Only one `xzatoma` process may write to `tmp/xzatoma.db` at a time. Ensure no
other session is running against this demo directory before starting a new one.
Run `reset.sh` to remove a stale database if needed.
