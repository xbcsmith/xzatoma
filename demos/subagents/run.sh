#!/bin/sh
# Subagents Demo - Run
#
# Executes the subagents delegation demo plan. The coordinator agent spawns
# three subagents in parallel, each writing a result to tmp/output/. After
# all subagents complete the coordinator writes a summary to tmp/output/.
#
# Usage:
#   sh ./run.sh
#   ./run.sh    (after chmod +x run.sh)
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

echo "XZatoma Subagents Demo"
echo "Provider  : Ollama"
echo "Model     : granite4:3b"
echo "Config    : $DEMO_DIR/config.yaml"
echo "Storage   : $DEMO_DIR/tmp/xzatoma.db"
echo "Subagents : max_depth=2, max_executions=5, persistence=tmp/subagent_conversations.db"
echo "Plan      : $DEMO_DIR/plans/subagents_demo.yaml"
echo ""
echo "The coordinator agent will delegate three tasks to parallel subagents:"
echo "  haiku-writer    -> tmp/output/haiku.txt"
echo "  mcp-describer   -> tmp/output/mcp_description.txt"
echo "  rust-advocate   -> tmp/output/rust_benefits.txt"
echo "  (coordinator)   -> tmp/output/summary.txt"
echo ""

"$XZATOMA" \
    --config ./config.yaml \
    --storage-path ./tmp/xzatoma.db \
    run \
    --plan ./plans/subagents_demo.yaml \
    2>&1 | tee tmp/output/subagents_run.txt

echo ""
echo "Demo complete. Output written to:"
echo "  tmp/output/subagents_run.txt    (full execution transcript)"
echo "  tmp/output/haiku.txt            (haiku from subagent)"
echo "  tmp/output/mcp_description.txt  (MCP description from subagent)"
echo "  tmp/output/rust_benefits.txt    (Rust benefits list from subagent)"
echo "  tmp/output/summary.txt          (coordinator summary)"
