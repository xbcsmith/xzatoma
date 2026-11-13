#!/bin/sh
# Sandbox Demo - Run
#
# Runs the sandbox boundary verification plan using the demo-local config and
# demo-local storage. The plan exercises each enforcement layer: filesystem
# root boundary, git directory protection, and network blocking.
#
# Some plan steps are intentionally expected to fail. The demo verifies that
# the sandbox correctly allows permitted operations and blocks forbidden ones.
#
# Usage:
#   sh ./run.sh
#   ./run.sh          (after chmod +x run.sh)
#
# Output is written to tmp/output/ and printed to the terminal.
# Run ./setup.sh at least once before running this script.

set -e

DEMO_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$DEMO_DIR"

# Locate the xzatoma binary. Prefer PATH, then fall back to the repository
# build output relative to this demo directory.
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

echo "XZatoma Sandbox Demo"
echo "Provider : Ollama"
echo "Model    : granite4:3b"
echo "Config   : $DEMO_DIR/config.yaml"
echo "Plan     : $DEMO_DIR/plans/sandbox_boundaries.yaml"
echo "Output   : $DEMO_DIR/tmp/output/"
echo ""
echo "Sandbox settings active during this run:"
echo "  enabled         : true"
echo "  filesystem_root : $DEMO_DIR  (working directory)"
echo "  protect_git     : true"
echo "  network_enabled : false"
echo ""
echo "Some plan steps are intentionally expected to fail."
echo "The demo verifies that the sandbox blocks forbidden operations."
echo "--------------------------------------------------------------"
echo ""

"$XZATOMA" \
    --config ./config.yaml \
    --storage-path ./tmp/xzatoma.db \
    run \
    --plan ./plans/sandbox_boundaries.yaml \
    2>&1 | tee tmp/output/sandbox_run.txt

echo ""
echo "--------------------------------------------------------------"
echo "Demo complete."
echo ""
echo "Files written by the agent:"
ls -1 tmp/output/ 2>/dev/null | grep -v '.gitkeep' | sed 's/^/  /' || echo "  (none)"
echo ""
echo "To view the summary report:"
echo "  cat tmp/output/sandbox_report.txt"
