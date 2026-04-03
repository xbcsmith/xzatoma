#!/bin/sh
# Skills Demo - Setup
#
# Prepares the demo-local state required before running the skills demo.
# Creates the tmp/ directory structure, verifies skill fixture files and plan
# files are present, and checks that all prerequisites are satisfied.
#
# Usage:
#   sh ./setup.sh
#   ./setup.sh    (after chmod +x setup.sh)
#
# This script resolves its own location and changes into the demo root before
# performing any work. It does not depend on the repository root or any path
# outside this demo directory.

set -e

DEMO_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$DEMO_DIR"

echo "Setting up Skills demo..."
echo "Demo root: $DEMO_DIR"
echo ""

# Create the tmp directory structure
mkdir -p tmp/output

echo "Checking skill fixture files..."

missing=0
for skill_file in \
    skills/greet/SKILL.md \
    skills/summarize/SKILL.md \
    skills/write_file/SKILL.md; do
    if [ -f "$skill_file" ]; then
        echo "  OK       $skill_file"
    else
        echo "  MISSING  $skill_file"
        missing=1
    fi
done

if [ "$missing" -eq 1 ]; then
    echo ""
    echo "ERROR: One or more skill fixture files are missing."
    echo "       The demo directory may be incomplete."
    exit 1
fi

echo ""
echo "Checking plan files..."

for plan_file in plans/skills_demo.yaml; do
    if [ -f "$plan_file" ]; then
        echo "  OK       $plan_file"
    else
        echo "  MISSING  $plan_file"
        missing=1
    fi
done

if [ "$missing" -eq 1 ]; then
    echo ""
    echo "ERROR: One or more plan files are missing."
    echo "       The demo directory may be incomplete."
    exit 1
fi

echo ""
echo "Checking prerequisites..."

# Check that xzatoma is available
if command -v xzatoma >/dev/null 2>&1; then
    echo "  OK       xzatoma found at: $(command -v xzatoma)"
elif [ -x "../../target/release/xzatoma" ]; then
    echo "  OK       xzatoma found at ../../target/release/xzatoma"
elif [ -x "../../target/debug/xzatoma" ]; then
    echo "  OK       xzatoma found at ../../target/debug/xzatoma"
else
    echo "  WARNING  xzatoma not found on PATH or in ../../target/."
    echo "           Build from the repository root with: cargo build --release"
    echo "           Then add the binary to your PATH or run from the repo."
    echo ""
fi

# Check that Ollama is running
if command -v curl >/dev/null 2>&1; then
    if curl -sf http://localhost:11434/api/tags >/dev/null 2>&1; then
        echo "  OK       Ollama is running at http://localhost:11434"
    else
        echo "  WARNING  Could not reach Ollama at http://localhost:11434"
        echo "           Start Ollama with: ollama serve"
        echo ""
    fi
elif command -v wget >/dev/null 2>&1; then
    if wget -q --spider http://localhost:11434/api/tags >/dev/null 2>&1; then
        echo "  OK       Ollama is running at http://localhost:11434"
    else
        echo "  WARNING  Could not reach Ollama at http://localhost:11434"
        echo "           Start Ollama with: ollama serve"
        echo ""
    fi
else
    echo "  SKIP     curl and wget not available; skipping Ollama connectivity check"
fi

# Check that the required model is available
if command -v ollama >/dev/null 2>&1; then
    if ollama list 2>/dev/null | grep -q "granite4:3b"; then
        echo "  OK       granite4:3b is available"
    else
        echo "  WARNING  granite4:3b not found in ollama list"
        echo "           Pull it with: ollama pull granite4:3b"
        echo ""
    fi
else
    echo "  SKIP     ollama CLI not found; skipping model check"
fi

echo ""
echo "Setup complete."
echo ""
echo "Run the demo:"
echo "  ./run.sh"
echo ""
echo "List discovered skills:"
echo "  xzatoma --config ./config.yaml skills list"
echo ""
echo "Validate skills:"
echo "  xzatoma --config ./config.yaml skills validate"
