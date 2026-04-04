# MCP Demo

## Goal

Demonstrate Model Context Protocol (MCP) server integration in XZatoma. The demo
proves that:

- XZatoma connects to a local stdio-based MCP server on startup
- The agent can list and use tools provided by the MCP server
- All file operations performed through the MCP server remain inside
  `tmp/output/`
- The MCP server process is started and stopped automatically by XZatoma

## Prerequisites

1. [Ollama](https://ollama.com) installed and running:

   ```sh
   ollama serve
   ```

2. The `granite4:3b` model pulled:

   ```sh
   ollama pull granite4:3b
   ```

3. [Node.js](https://nodejs.org) version 18 or later with `npx` available on
   `PATH`. The demo uses `npx @modelcontextprotocol/server-filesystem` as the
   MCP server. `npx` downloads the package on first use if it is not cached.

   ```sh
   node --version   # must be v18.x or later
   npx --version
   ```

4. XZatoma built from the repository root:

   ```sh
   cargo build --release
   ```

   Add the binary to `PATH` or ensure it is reachable at
   `../../target/release/xzatoma` relative to this directory.

## Directory Layout

```text
demos/mcp/
  README.md                     # This file
  config.yaml                   # Demo-local configuration with MCP enabled
  setup.sh                      # Prepares tmp/ and verifies prerequisites
  run.sh                        # Runs the MCP integration demo plan
  reset.sh                      # Removes all generated state
  mcp/
    server_config.yaml          # Reference copy of the MCP server configuration
    tool_examples.md            # Documented examples of available MCP tools
  plans/
    mcp_demo.yaml               # Plan that exercises MCP tool usage
  input/
    prompts.txt                 # Reference prompts for interactive MCP use
  tmp/
    .gitignore                  # Excludes all generated files from version control
    output/                     # MCP server root; all artifacts written here
```

## Setup

```sh
cd demos/mcp
./setup.sh
```

`setup.sh` performs the following steps:

1. Creates `tmp/output/` if it does not exist. The MCP filesystem server uses
   `./tmp/output` as its root path and requires the directory to exist before
   the server starts.
2. Verifies that the plan and fixture files are present in the demo directory.
3. Checks that `xzatoma` is available on `PATH` or in the repository build
   output.
4. Checks that Ollama is running and `granite4:3b` is available.
5. Checks that `node` and `npx` are available on `PATH`.

## Run

```sh
./run.sh
```

`run.sh` executes `xzatoma run --plan ./plans/mcp_demo.yaml`. XZatoma starts the
`demo-filesystem` MCP server automatically as a subprocess via `npx` before the
agent begins executing the plan.

The agent completes four steps:

1. Lists all tools provided by the MCP server and writes the list to
   `tmp/output/mcp_tools.txt`.
2. Writes a file named `mcp_hello.txt` to `tmp/output/` through the MCP server.
3. Reads `mcp_hello.txt` back through the MCP server and confirms the contents.
4. Lists all files in `tmp/output/` through the MCP server and writes the
   directory listing to `tmp/output/mcp_listing.txt`.

All output is also saved to `tmp/output/mcp_run.txt`.

To execute an individual command:

```sh
# Run the demo plan directly
xzatoma --config ./config.yaml --storage-path ./tmp/xzatoma.db \
  run --plan ./plans/mcp_demo.yaml

# Start an interactive session with MCP tools available
xzatoma --config ./config.yaml --storage-path ./tmp/xzatoma.db chat
```

## Expected Output

After `./run.sh` completes, the following files appear in `tmp/output/`:

| File              | Contents                                          |
| ----------------- | ------------------------------------------------- |
| `mcp_run.txt`     | Full plan execution transcript                    |
| `mcp_tools.txt`   | List of tools provided by the MCP server          |
| `mcp_hello.txt`   | File created through the MCP filesystem server    |
| `mcp_listing.txt` | Directory listing produced through the MCP server |

The `demo-filesystem` MCP server is scoped to `./tmp/output/`. The agent cannot
read or write files outside that directory through the MCP server.

## Reset

```sh
./reset.sh
```

`reset.sh` removes:

- `tmp/xzatoma.db` (conversation history database)
- All files under `tmp/output/` except `.gitkeep`
- Any other generated files under `tmp/`

The `mcp/` fixtures, plan files, `input/`, and `config.yaml` are never modified.

## Sandbox Boundaries

XZatoma is constrained to this demo directory by the following configuration:

- `--config ./config.yaml` is passed on every invocation. The repository-level
  `config/config.yaml` is never loaded at demo runtime.
- `--storage-path ./tmp/xzatoma.db` directs all conversation history into
  `tmp/`.
- The `demo-filesystem` MCP server is launched with `./tmp/output` as its root
  path argument. The server rejects path traversal attempts and operations
  outside that directory.
- The `demo-filesystem` server entry lives entirely within this demo directory.
  Its configuration is in `./config.yaml` under the `mcp.servers` key. The
  reference copy is in `./mcp/server_config.yaml`.
- `skills.enabled: false` prevents skill discovery from running during the demo.
- No file in this demo references paths outside the `demos/mcp/` directory.

## Troubleshooting

### xzatoma binary not found

Build from the repository root and export the binary path:

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

### node or npx not found

Install Node.js version 18 or later. Options:

```sh
# Official installer
# Download from https://nodejs.org

# Using nvm
nvm install 18
nvm use 18
```

### MCP server fails to start

Run the server command manually from the demo directory to verify it works:

```sh
cd demos/mcp
npx -y @modelcontextprotocol/server-filesystem ./tmp/output
```

If the command prints usage output then the server binary is working. If it
prints a download or network error, check npm registry access. If it fails with
a Node.js version error, upgrade to Node.js 18 or later.

### MCP tools not available to the agent

Verify that `mcp.auto_connect: true` is set in `config.yaml` and that the
`demo-filesystem` entry has `enabled: true`. Review `tmp/output/mcp_run.txt` for
connection error messages printed during server startup.

### Plan execution completes but output files are missing

Verify that the working directory was set to `demos/mcp/` before the plan ran.
The `run.sh` script sets `cd "$DEMO_DIR"` before invoking `xzatoma`. If running
manually, change into the `demos/mcp/` directory first:

```sh
cd demos/mcp
xzatoma --config ./config.yaml --storage-path ./tmp/xzatoma.db \
  run --plan ./plans/mcp_demo.yaml
```
