#!/bin/sh
# Subagents Demo - Setup
#
# Prepares the demo-local state required before running the subagents demo.
# Creates the tmp/ directory structure and verifies prerequisites.
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

echo "Setting up Subagents demo..."
echo "Demo root: $DEMO_DIR"
echo ""

# Create the tmp directory structure
mkdir -p tmp/output

echo "Checking required plan files..."

missing=0
for plan in plans/subagents_demo.yaml; do
    if [ -f "$plan" ]; then
        echo "  OK  $plan"
    else
        echo "  MISSING  $plan"
        missing=1
    fi
done

if [ "$missing" -eq 1 ]; then
    echo ""
    echo "ERROR: One or more plan files are missing. The demo directory may be incomplete."
    exit 1
fi

echo ""
echo "Checking prerequisites..."

# Check that xzatoma is available on PATH or in the build output
if command -v xzatoma >/dev/null 2>&1; then
    echo "  OK  xzatoma found at: $(command -v xzatoma)"
elif [ -x "../../target/release/xzatoma" ]; then
    echo "  OK  xzatoma found at: ../../target/release/xzatoma"
elif [ -x "../../target/debug/xzatoma" ]; then
    echo "  OK  xzatoma found at: ../../target/debug/xzatoma"
else
    echo "  WARNING  xzatoma not found on PATH or in ../../target/."
    echo "           Build from the repository root with: cargo build --release"
    echo ""
fi

# Check that Ollama is reachable
if command -v curl >/dev/null 2>&1; then
    if curl -sf http://localhost:11434/api/tags >/dev/null 2>&1; then
        echo "  OK  Ollama is running at http://localhost:11434"
    else
        echo "  WARNING  Could not reach Ollama at http://localhost:11434"
        echo "           Start Ollama with: ollama serve"
    fi
elif command -v wget >/dev/null 2>&1; then
    if wget -q --spider http://localhost:11434/api/tags >/dev/null 2>&1; then
        echo "  OK  Ollama is running at http://localhost:11434"
    else
        echo "  WARNING  Could not reach Ollama at http://localhost:11434"
        echo "           Start Ollama with: ollama serve"
    fi
else
    echo "  SKIP  curl and wget not available; skipping Ollama connectivity check"
fi

# Check that the required model is available
if command -v ollama >/dev/null 2>&1; then
    if ollama list 2>/dev/null | grep -q "granite4:3b"; then
        echo "  OK  granite4:3b is available"
    else
        echo "  WARNING  granite4:3b not found in ollama list"
        echo "           Pull it with: ollama pull granite4:3b"
    fi
else
    echo "  SKIP  ollama CLI not found; skipping model check"
fi

echo ""
echo "Setup complete."
echo ""
echo "Run the demo:"
echo "  ./run.sh"
echo ""
echo "To run a specific plan directly:"
echo "  xzatoma --config ./config.yaml --storage-path ./tmp/xzatoma.db run --plan ./plans/subagents_demo.yaml"
