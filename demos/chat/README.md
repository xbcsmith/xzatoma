# Chat Demo

## Goal

Demonstrate XZatoma's interactive chat capability against a local Ollama model.
This demo proves that the `chat` CLI command can connect to Ollama, maintain a
multi-turn conversation, and respond to general questions using the
`granite4:3b` model.

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
chat/
  README.md                  This walkthrough
  config.yaml                Demo-local XZatoma configuration
  setup.sh                   Prepare demo-local state
  run.sh                     Launch the interactive chat session
  reset.sh                   Remove all generated state
  input/
    sample_questions.txt     Reference questions to try during the demo
  tmp/
    .gitignore               Excludes generated files from version control
    output/
      .gitkeep               Preserves the empty output directory in git
```

Generated at runtime (inside `tmp/`):

| File             | Description                         |
| ---------------- | ----------------------------------- |
| `tmp/xzatoma.db` | SQLite conversation history         |
| `tmp/output/`    | Destination for any saved artifacts |

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

Start the interactive chat session:

```sh
sh ./run.sh
```

Alternatively, if you have marked the scripts executable:

```sh
chmod +x setup.sh run.sh reset.sh
./run.sh
```

The script launches `xzatoma chat` with the demo-local `config.yaml` and writes
the conversation history database to `tmp/xzatoma.db`. You may type any question
at the prompt. The file `input/sample_questions.txt` contains suggested
questions you can copy and paste.

To invoke the chat command directly (after `cd` into this directory):

```sh
xzatoma \
  --config ./config.yaml \
  --storage-path ./tmp/xzatoma.db \
  chat
```

To start in write mode (allows the agent to make file changes):

```sh
xzatoma \
  --config ./config.yaml \
  --storage-path ./tmp/xzatoma.db \
  chat --mode write
```

Type `exit` or press `Ctrl-D` to end the session.

## Expected Output

The chat command outputs to the terminal in real time. No files are written to
`tmp/output/` automatically during a normal chat session.

The conversation history is stored in `tmp/xzatoma.db` (SQLite) so that sessions
can be resumed with `--resume <id>`.

If you redirect the chat output manually, write only to `tmp/output/`:

```sh
sh ./run.sh 2>&1 | tee tmp/output/session.txt
```

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

4. The demo is in `planning` mode by default (`config.yaml` sets
   `agent.chat.default_mode: planning`), which makes the agent read-only. It
   will not write files unless you explicitly switch to `write` mode.

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

### Chat session exits immediately

If standard input is a pipe rather than a terminal, the readline library may
close on the first EOF. Run `run.sh` directly in an interactive terminal rather
than piping input to it.

### Permission denied when running scripts

Mark the scripts as executable before running them:

```sh
chmod +x setup.sh run.sh reset.sh
```

### History database locked

Only one `xzatoma` process may write to `tmp/xzatoma.db` at a time. Ensure no
other chat session is running against this demo directory before starting a new
one.

## Oneliner

Answer the questions in the @input/sample_questions.txt  file. Use subagents to write each question and answer to individual files in ./tmp/output
