#!/bin/sh
# Sandbox Demo - Run
#
# Runs two plans to demonstrate sandbox enforcement:
#
#   Phase 1 (allowed_operations.yaml)  -- all steps succeed
#   Phase 2 (enforcement_demo.yaml)    -- single step is BLOCKED; the failure
#                                         is the proof that the sandbox works
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

mkdir -p tmp/output

echo "XZatoma Sandbox Demo"
echo "Provider         : Ollama"
echo "Model            : granite4:3b"
echo "Config           : $DEMO_DIR/config.yaml"
echo "Sandbox root     : $DEMO_DIR"
echo "protect_git      : true"
echo "network_enabled  : false"
echo ""

# ---------------------------------------------------------------------------
# Phase 1: Allowed operations -- every step must succeed
# ---------------------------------------------------------------------------
echo "================================================================"
echo "Phase 1: Allowed operations  (all steps must succeed)"
echo "================================================================"
echo ""

"$XZATOMA" \
    run \
    --config ./config.yaml \
    --storage-path ./tmp/xzatoma.db \
    --allow-dangerous \
    --plan ./plans/allowed_operations.yaml \
    2>&1 | tee tmp/output/phase1_run.txt

echo ""
echo "Phase 1 complete."
if [ -f tmp/output/allowed_operations_report.txt ]; then
    echo ""
    echo "--- Allowed-operations report ---"
    cat tmp/output/allowed_operations_report.txt
    echo "---------------------------------"
fi
echo ""

# ---------------------------------------------------------------------------
# Phase 2: Enforcement demo -- the plan is EXPECTED to fail
#   The sandbox must block the write to /tmp/. A non-zero exit code and an
#   "Absolute path not allowed" error confirm that enforcement is active.
# ---------------------------------------------------------------------------
echo "================================================================"
echo "Phase 2: Enforcement demo    (plan MUST be blocked by sandbox)"
echo "================================================================"
echo ""
echo "The plan attempts to write outside the sandbox root."
echo "A sandbox denial error below is the expected and correct result."
echo ""

set +e
"$XZATOMA" \
    run \
    --config ./config.yaml \
    --storage-path ./tmp/xzatoma.db \
    --allow-dangerous \
    --plan ./plans/enforcement_demo.yaml \
    2>&1 | tee tmp/output/phase2_enforcement.txt
ENFORCEMENT_EXIT=$?
set -e

echo ""
if [ $ENFORCEMENT_EXIT -ne 0 ]; then
    echo "Phase 2 result: SANDBOX ENFORCED (exit $ENFORCEMENT_EXIT)"
    echo "The error above confirms the sandbox blocked the out-of-root write."
else
    echo "Phase 2 result: WARNING -- the write succeeded unexpectedly."
    echo "Check that sandbox.enabled is true in config.yaml."
fi

echo ""
echo "================================================================"
echo "Demo complete. Output files:"
ls -1 tmp/output/ 2>/dev/null | grep -v '.gitkeep' | sed 's/^/  /' || true
echo "================================================================"
