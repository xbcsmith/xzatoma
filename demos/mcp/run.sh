#!/bin/sh
# MCP Demo - Run
#
# Executes the MCP integration demo plan using the demo-local config and
# demo-local storage. The agent connects to the demo-filesystem MCP server
# (scoped to ./tmp/output/) and exercises its tools.
#
# Usage:
#   sh ./run.sh
#   ./run.sh          (after chmod +x run.sh)
#
# All output is written to tmp/output/mcp_run.txt in addition to stdout.
#
# Prerequisites:
#   - Ollama is running at http://localhost:11434
#   - granite4:3b model has been pulled (ollama pull granite4:3b)
#   - Node.js >= 18 and npx are available on PATH
#   - XZatoma binary is on PATH or in ../../target/release/xzatoma
#   - ./setup.sh has been run at least once

set -e

DEMO_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$DEMO_DIR"

# Locate the xzatoma binary. Prefer the one on PATH; fall back to the
# repository build output relative to this demo directory.
if command -v xzatoma >/dev/null 2>&1; then
    XZATOMA="xzatoma"
elif [ -x "../../target/release/xzatoma" ]; then
    XZATOMA="../../target/release/xzatoma"
elif [ -x "../../target/debug/xzatoma" ]; then
    XZATOMA="../../target/debug/xzatoma"
else
    echo "ERROR: xzatoma binary not found." >&2
    echo "Build with: cargo build --release" >&2
    exit 1
fi

# Ensure the output directory exists (setup.sh creates it, but be defensive).
mkdir -p tmp/output

echo "XZatoma MCP Demo"
echo "Provider : Ollama"
echo "Model    : granite4:3b"
echo "Config   : $DEMO_DIR/config.yaml"
echo "Storage  : $DEMO_DIR/tmp/xzatoma.db"
echo "MCP      : demo-filesystem -> $DEMO_DIR/tmp/output/"
echo ""
echo "The agent will connect to the demo-filesystem MCP server and"
echo "exercise its file operations within tmp/output/."
echo ""

"$XZATOMA" \
    --config ./config.yaml \
    --storage-path ./tmp/xzatoma.db \
    run \
    --plan ./plans/mcp_demo.yaml \
    2>&1 | tee tmp/output/mcp_run.txt

echo ""
echo "Demo complete. Output written to:"
echo "  tmp/output/mcp_run.txt    (full plan execution transcript)"
echo "  tmp/output/mcp_hello.txt  (file created through the MCP server)"
