# llama.cpp Provider Demo

## Goal

Demonstrate XZatoma's **OpenAI-compatible provider** backed by a local
[llama.cpp](https://github.com/ggerganov/llama.cpp) inference server. This
proves that XZatoma can run fully offline against any GGUF model — no API key,
no network egress, no external service dependency.

`llama-server` (the built-in HTTP server shipped with llama.cpp) speaks the
OpenAI chat completions API on `http://localhost:8080/v1`. XZatoma's
`provider.type: openai` is pointed at that URL with `api_key: ""`. Everything
else — plan execution, tool calls, streaming, conversation management — works
identically to the hosted OpenAI API.

The demo includes two plans:

- `plans/hello_world.yaml` — single-step greeting plan; confirms end-to-end
  connectivity
- `plans/system_info.yaml` — multi-step plan that collects hostname, OS, disk
  usage, and uptime, then writes a summary report to `tmp/output/`

---

## Prerequisites

| Requirement              | Notes                                                                   |
| ------------------------ | ----------------------------------------------------------------------- |
| `llama-server` on PATH   | Via Homebrew (`brew install llama.cpp`) or pre-built binary — see below |
| A GGUF model file        | Recommended: `granite-3.3-2b-instruct-Q4_K_M.gguf` — see below         |
| `xzatoma` binary on PATH | `cargo build --release && cargo install --path .`                       |
| `curl` or `wget`         | Used by `setup.sh` to verify the server is running                      |
| `jq` (optional)          | Pretty-prints the `/v1/models` response during setup                    |

---

## Installing llama.cpp

### macOS — Homebrew (recommended)

```sh
brew install llama.cpp
```

The Homebrew formula includes `llama-server` and is updated regularly. After
installation the binary is on your PATH.

### Pre-built binaries

Download the latest release for your platform from:

```
https://github.com/ggerganov/llama.cpp/releases
```

Extract and add `llama-server` to a directory on your PATH:

```sh
tar -xzf llama-b<version>-bin-*.tar.gz
sudo mv llama-server /usr/local/bin/
```

### Build from source

```sh
git clone https://github.com/ggerganov/llama.cpp
cd llama.cpp

# macOS Apple Silicon (Metal GPU acceleration)
cmake -B build -DLLAMA_METAL=ON
cmake --build build --config Release -j$(nproc)
sudo cmake --install build

# NVIDIA GPU (CUDA)
cmake -B build -DLLAMA_CUDA=ON
cmake --build build --config Release -j$(nproc)
sudo cmake --install build

# CPU-only
cmake -B build
cmake --build build --config Release -j$(nproc)
sudo cmake --install build
```

---

## Downloading a Model

Create a `models/` directory inside this demo to keep the GGUF file local:

```sh
mkdir -p models
```

### Option A — huggingface-cli (recommended)

```sh
pip install huggingface_hub

# Granite 3.3 2B Instruct Q4_K_M (primary recommendation — matches the
# Granite model family used by the other XZatoma demos)
huggingface-cli download \
  bartowski/granite-3.3-2b-instruct-GGUF \
  granite-3.3-2b-instruct-Q4_K_M.gguf \
  --local-dir ./models
```

### Option B — wget / curl

```sh
# Granite 3.3 2B Instruct Q4_K_M
wget "https://huggingface.co/bartowski/granite-3.3-2b-instruct-GGUF/resolve/main/granite-3.3-2b-instruct-Q4_K_M.gguf" \
  -O ./models/granite-3.3-2b-instruct-Q4_K_M.gguf
```

### Alternative models

Any instruction-tuned GGUF model works. `Q4_K_M` quantization offers a good
balance of quality and size.

| Model                          | Quant   | Size    | Notes                          |
| ------------------------------ | ------- | ------- | ------------------------------ |
| `granite-3.3-2b-instruct`      | Q4\_K\_M | ~1.8 GB | Primary recommendation         |
| `granite-3.1-2b-instruct`      | Q4\_K\_M | ~1.8 GB | Stable alternative             |
| `Llama-3.2-3B-Instruct`        | Q4\_K\_M | ~2.0 GB | Meta model (login required)    |
| `Qwen2.5-3B-Instruct`          | Q4\_K\_M | ~2.0 GB | Strong small model             |
| `Phi-3.5-mini-instruct`        | Q4\_K\_M | ~2.2 GB | Microsoft small model          |

See `input/notes.txt` for the full download commands for each model.

---

## Directory Layout

```text
demos/providers/llama_cpp/
  README.md                        This walkthrough
  config.yaml                      Demo-local XZatoma configuration
  setup.sh                         Verify prerequisites and prepare tmp/
  run.sh                           Execute the hello_world plan (default)
  reset.sh                         Remove all generated state
  plans/
    hello_world.yaml               Single-step greeting plan
    system_info.yaml               Multi-step system information plan
  input/
    notes.txt                      Model download instructions, server flags,
                                   environment variable reference
  models/                          Put your GGUF file here (not tracked by git)
  tmp/
    .gitignore                     Excludes generated files from version control
    output/
      .gitkeep                     Preserves the empty output directory in git
```

Generated at runtime (inside `tmp/`):

| File                              | Description                              |
| --------------------------------- | ---------------------------------------- |
| `tmp/xzatoma.db`                  | SQLite conversation and history database |
| `tmp/output/run_output.txt`       | Captured output from the last `run.sh`   |
| `tmp/output/system_info_report.txt` | Report written by the system_info plan |

The `models/` directory is not tracked by git. Add your GGUF files there so
they stay local to the demo.

---

## Setup

### Step 1: Start the llama.cpp server

From the `demos/providers/llama_cpp/` directory:

```sh
llama-server \
  --model ./models/granite-3.3-2b-instruct-Q4_K_M.gguf \
  --port 8080 \
  --ctx-size 4096 \
  --alias granite-3.3-2b-instruct
```

For GPU acceleration on Apple Silicon, add `--n-gpu-layers 99`:

```sh
llama-server \
  --model ./models/granite-3.3-2b-instruct-Q4_K_M.gguf \
  --port 8080 \
  --ctx-size 4096 \
  --n-gpu-layers 99 \
  --alias granite-3.3-2b-instruct
```

Leave this terminal running. The server listens on `http://localhost:8080` and
serves the OpenAI-compatible API at `http://localhost:8080/v1`.

Verify the server is ready:

```sh
curl http://localhost:8080/v1/models | jq .
```

### Step 2: Run setup

Open a second terminal, change to `demos/providers/llama_cpp/`, and run:

```sh
sh ./setup.sh
```

`setup.sh`:

1. Creates `tmp/output/` if it does not exist.
2. Verifies that the plan files are present.
3. Checks that `xzatoma` is on PATH or in the build output.
4. Probes `http://localhost:8080/v1/models` and reports the loaded model name.
5. Notes any mismatch between the model name in `config.yaml` and the alias
   reported by the server.

---

## Run

Execute the hello world plan:

```sh
sh ./run.sh
```

Execute the system information plan:

```sh
sh ./run.sh system_info
```

Both plans can also be run directly without `run.sh`:

```sh
# Hello world
xzatoma \
  --config ./config.yaml \
  --storage-path ./tmp/xzatoma.db \
  run \
  --plan ./plans/hello_world.yaml

# System information
xzatoma \
  --config ./config.yaml \
  --storage-path ./tmp/xzatoma.db \
  run \
  --plan ./plans/system_info.yaml
```

To run a direct prompt instead of a plan file:

```sh
xzatoma \
  --config ./config.yaml \
  --storage-path ./tmp/xzatoma.db \
  run \
  --prompt "List the files in the current directory and summarize what you find."
```

---

## Expected Output

### hello_world

`run.sh` streams the agent transcript to the terminal and simultaneously writes
it to `tmp/output/run_output.txt`. A successful run produces output similar to:

```text
XZatoma llama.cpp Provider Demo
Provider  : openai (llama.cpp)
Server    : http://localhost:8080/v1
Model     : granite-3.3-2b-instruct
Plan      : ./plans/hello_world.yaml (Hello World)
Output    : tmp/output/run_output.txt

Hello from XZatoma! The llama.cpp inference server is responding correctly
via the OpenAI-compatible API.

Output saved to tmp/output/run_output.txt
```

### system_info

The agent executes five shell commands in sequence, then writes a structured
report to `tmp/output/system_info_report.txt`. The report begins with:

```text
XZatoma llama.cpp Provider Demo - System Information Report

Hostname:
  <your hostname>

Operating System:
  Darwin <version> arm64 ...
...
--- end of report ---
```

Inspect the report directly:

```sh
cat tmp/output/system_info_report.txt
```

---

## Reset

Remove all generated state and return the demo to its initial condition:

```sh
sh ./reset.sh
```

The reset script removes:

- `tmp/xzatoma.db` (conversation and history database)
- All files in `tmp/output/` except `.gitkeep`
- Any other generated files under `tmp/` except `.gitignore`

Static files — `README.md`, `config.yaml`, the scripts, `plans/`, `input/`,
and any GGUF files in `models/` — are never removed by `reset.sh`.

---

## Configuration

`config.yaml` sets `provider.type: openai` and points the `base_url` at the
llama.cpp server:

```yaml
provider:
  type: openai
  openai:
    api_key: ""                          # not required for llama.cpp
    base_url: "http://localhost:8080/v1" # llama-server default port
    model: "granite-3.3-2b-instruct"     # must match --alias
    enable_streaming: true
```

### Using a different model

1. Download the GGUF file into `models/`.
2. Start `llama-server` with `--alias <your-alias>`.
3. Either update `config.yaml`:
   ```yaml
   openai:
     model: "<your-alias>"
   ```
   Or override at runtime without touching the file:
   ```sh
   XZATOMA_OPENAI_MODEL=<your-alias> sh ./run.sh
   ```

### Using a different port

If you run `llama-server` on a different port, either update `config.yaml`:

```yaml
openai:
  base_url: "http://localhost:9090/v1"
```

Or override at runtime:

```sh
XZATOMA_OPENAI_BASE_URL=http://localhost:9090/v1 sh ./run.sh
```

### Environment variable overrides

All OpenAI provider fields can be overridden at runtime without editing
`config.yaml`:

| Variable                    | Default (`config.yaml`)                  |
| --------------------------- | ---------------------------------------- |
| `XZATOMA_OPENAI_BASE_URL`   | `http://localhost:8080/v1`               |
| `XZATOMA_OPENAI_MODEL`      | `granite-3.3-2b-instruct`                |
| `XZATOMA_OPENAI_API_KEY`    | *(empty — not required for llama.cpp)*   |
| `XZATOMA_OPENAI_STREAMING`  | `true`                                   |

---

## Sandbox Boundaries

XZatoma is scoped to the demo directory during execution:

1. `run.sh` changes into the demo root before invoking `xzatoma`, so the agent
   treats this directory as the working directory for all file operations.
2. `--config ./config.yaml` ensures the repository-level `config/config.yaml`
   is never loaded at demo runtime.
3. `--storage-path ./tmp/xzatoma.db` directs all history and session data into
   `tmp/`.
4. `agent.terminal.default_mode: restricted_autonomous` limits the commands the
   agent may run without confirmation.
5. The `system_info` plan writes its report to `tmp/output/system_info_report.txt`
   only. No generated file may appear outside `tmp/`.

---

## Troubleshooting

### `xzatoma: command not found`

The `xzatoma` binary is not on your PATH. Build it and export the path:

```sh
cargo build --release
export PATH="$PATH:$(pwd)/target/release"
```

`run.sh` also checks `../../target/release/xzatoma` relative to the demo
directory as a fallback.

### `llama-server: command not found`

Install llama.cpp via Homebrew:

```sh
brew install llama.cpp
```

Or download a pre-built binary from:

```
https://github.com/ggerganov/llama.cpp/releases
```

### `Connection refused` at `http://localhost:8080`

The llama.cpp server is not running. Start it with:

```sh
llama-server \
  --model ./models/granite-3.3-2b-instruct-Q4_K_M.gguf \
  --port 8080 \
  --ctx-size 4096 \
  --alias granite-3.3-2b-instruct
```

### Model file not found

Ensure the GGUF file exists at the path passed to `--model`. Run from the
`demos/providers/llama_cpp/` directory so the relative path `./models/...` is
correct, or use an absolute path:

```sh
llama-server --model /absolute/path/to/model.gguf --port 8080
```

### Agent produces very short or truncated responses

The model context window may be too small. Restart `llama-server` with a larger
`--ctx-size` and increase `agent.conversation.max_tokens` in `config.yaml`
proportionally:

```sh
llama-server \
  --model ./models/granite-3.3-2b-instruct-Q4_K_M.gguf \
  --port 8080 \
  --ctx-size 8192 \
  --alias granite-3.3-2b-instruct
```

```yaml
agent:
  conversation:
    max_tokens: 64000
```

### Inference is very slow

Offload layers to the GPU if available:

```sh
# Apple Silicon
llama-server --model ./models/<model>.gguf --port 8080 --n-gpu-layers 99

# NVIDIA (requires CUDA build)
llama-server --model ./models/<model>.gguf --port 8080 --n-gpu-layers 99
```

For CPU-only inference, try a smaller quantization such as `Q2_K` or a
smaller model (2B parameters rather than 7B+).

### Model name mismatch warning in `setup.sh`

`setup.sh` compares the `model:` field in `config.yaml` with the alias
reported by `llama-server`. If they differ, the warning is informational only —
llama.cpp ignores the model field in requests when a single model is loaded.
To silence the warning either:

- Pass `--alias granite-3.3-2b-instruct` to `llama-server`, or
- Set `model:` in `config.yaml` to match whatever the server reports.

### `Permission denied` when running scripts

Mark the scripts executable:

```sh
chmod +x setup.sh run.sh reset.sh
```

---

## Further Reading

- `input/notes.txt` — extended model download instructions, server flags, and
  all environment variable overrides
- `config/openai_config.yaml` — full reference for all OpenAI provider fields
  and other compatible local servers (vLLM, Mistral.rs)
- `docs/how-to/configure_providers.md` — provider configuration how-to guide
- `docs/reference/configuration.md` — complete configuration reference
