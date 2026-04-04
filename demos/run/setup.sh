#!/bin/sh
# XZatoma Demo: Run - Setup Script
#
# Prepares the run demo for execution.
# Creates the tmp/output directory and verifies all required plan files exist.
#
# Usage:
#   sh ./setup.sh
#   # or, after chmod +x:
#   ./setup.sh
#
# This script resolves its own location and changes into the demo root before
# performing any work. It does not depend on the repository root or any path
# outside this demo directory.

set -e

DEMO_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$DEMO_DIR"

echo "Setting up Run demo..."
echo "Demo root: $DEMO_DIR"
echo ""

# Create tmp/output if it does not already exist
mkdir -p tmp/output

echo "Checking required plan files..."

missing=0
for plan in plans/hello_world.yaml plans/system_info.yaml; do
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
echo "Checking that xzatoma is available..."
if command -v xzatoma > /dev/null 2>&1; then
    echo "  OK  xzatoma found at: $(command -v xzatoma)"
else
    echo "  WARNING  xzatoma not found in PATH."
    echo "           Build it with: cargo build --release"
    echo "           Then add the binary to your PATH or copy it here."
fi

echo ""
echo "Checking that Ollama is reachable..."
if command -v curl > /dev/null 2>&1; then
    if curl -sf http://localhost:11434/api/tags > /dev/null 2>&1; then
        echo "  OK  Ollama is running at http://localhost:11434"
    else
        echo "  WARNING  Could not reach Ollama at http://localhost:11434"
        echo "           Start Ollama with: ollama serve"
    fi
elif command -v wget > /dev/null 2>&1; then
    if wget -q --spider http://localhost:11434/api/tags > /dev/null 2>&1; then
        echo "  OK  Ollama is running at http://localhost:11434"
    else
        echo "  WARNING  Could not reach Ollama at http://localhost:11434"
        echo "           Start Ollama with: ollama serve"
    fi
else
    echo "  SKIP  curl and wget not available; skipping Ollama connectivity check"
fi

echo ""
echo "Checking that the required model is available..."
if command -v ollama > /dev/null 2>&1; then
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
echo "To execute a specific plan:"
echo "  xzatoma --config ./config.yaml --storage-path ./tmp/xzatoma.db run --plan ./plans/hello_world.yaml"
echo "  xzatoma --config ./config.yaml --storage-path ./tmp/xzatoma.db run --plan ./plans/system_info.yaml"
