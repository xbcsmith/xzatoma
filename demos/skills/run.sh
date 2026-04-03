#!/bin/sh
# Skills Demo - Run
#
# Demonstrates skill discovery, loading, and activation within the demo sandbox.
# The script runs three phases:
#   1. List all skills discovered from ./skills/ (proves discovery isolation)
#   2. Validate the discovered skills (proves all fixtures are well-formed)
#   3. Execute the skills_demo plan (proves activation during agent execution)
#
# All output is written to tmp/output/ in addition to stdout.
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

echo "XZatoma Skills Demo"
echo "Provider : Ollama"
echo "Model    : granite4:3b"
echo "Config   : $DEMO_DIR/config.yaml"
echo "Storage  : $DEMO_DIR/tmp/xzatoma.db"
echo "Skills   : $DEMO_DIR/skills/"
echo ""

# Phase 1: List discovered skills
echo "========================================"
echo "Phase 1: Skill Discovery"
echo "========================================"
echo ""
"$XZATOMA" \
    --config ./config.yaml \
    skills list \
    2>&1 | tee tmp/output/skills_list.txt
echo ""

# Phase 2: Validate discovered skills
echo "========================================"
echo "Phase 2: Skill Validation"
echo "========================================"
echo ""
"$XZATOMA" \
    --config ./config.yaml \
    skills validate \
    2>&1 | tee tmp/output/skills_validate.txt
echo ""

# Phase 3: Execute the skills demo plan
echo "========================================"
echo "Phase 3: Skill Activation via Plan"
echo "========================================"
echo ""
"$XZATOMA" \
    --config ./config.yaml \
    --storage-path ./tmp/xzatoma.db \
    run \
    --plan ./plans/skills_demo.yaml \
    2>&1 | tee tmp/output/skills_run.txt
echo ""

echo "Demo complete. Output written to:"
echo "  tmp/output/skills_list.txt     (skill discovery results)"
echo "  tmp/output/skills_validate.txt (skill validation results)"
echo "  tmp/output/skills_run.txt      (plan execution output)"
