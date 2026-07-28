#!/bin/sh
# Chat Commands Demo - Run
#
# Runs XZatoma in non-interactive chat mode using the demo-local config and
# demo-local storage. Pipes the commands demo script into xzatoma chat and
# captures output to tmp/output/commands_demo.txt.
#
# Usage:
#   sh ./run.sh
#   ./run.sh          (after: chmod +x run.sh)
#
# Prerequisites:
#   - Ollama is running at http://localhost:11434
#   - granite4:3b model has been pulled (ollama pull granite4:3b)
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

echo "XZatoma Chat Commands Demo"
echo "Provider : Ollama"
echo "Model    : granite4:3b"
echo "Config   : $DEMO_DIR/config.yaml"
echo "Storage  : $DEMO_DIR/tmp/xzatoma.db"
echo "Script   : $DEMO_DIR/input/commands_demo_script.txt"
echo "Output   : $DEMO_DIR/tmp/output/commands_demo.txt"
echo ""
echo "Running non-interactive demo. Output is saved to tmp/output/commands_demo.txt"
echo ""

"$XZATOMA" \
    --config ./config.yaml \
    --storage-path ./tmp/xzatoma.db \
    chat < input/commands_demo_script.txt | tee tmp/output/commands_demo.txt
