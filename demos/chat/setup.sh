#!/bin/sh
# Chat Demo - Setup
#
# Prepares the demo-local state required before running the chat demo.
# Creates the tmp/ directory structure and verifies prerequisites.
#
# Usage:
#   sh ./setup.sh
#   OR (after chmod +x setup.sh):
#   ./setup.sh

set -e

DEMO_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$DEMO_DIR"

echo "Setting up Chat demo..."
echo "Demo directory: $DEMO_DIR"
echo ""

# Create the tmp directory structure
mkdir -p tmp/output

echo "Checking prerequisites..."
echo ""

# Check that xzatoma is available on PATH
if ! command -v xzatoma >/dev/null 2>&1; then
    echo "WARNING: xzatoma not found on PATH."
    echo "Build from the repository root with: cargo build --release"
    echo "Then add the binary to your PATH or use the full path in run.sh."
    echo ""
fi

# Check that Ollama is running
if ! curl -sf http://localhost:11434/api/tags >/dev/null 2>&1; then
    echo "WARNING: Ollama does not appear to be running on http://localhost:11434"
    echo "Start Ollama with: ollama serve"
    echo ""
fi

# Check that the required model is available
if command -v ollama >/dev/null 2>&1; then
    if ollama list 2>/dev/null | grep -q "granite4:3b"; then
        echo "Model granite4:3b is available."
    else
        echo "WARNING: Model granite4:3b not found in Ollama."
        echo "Pull it with: ollama pull granite4:3b"
        echo ""
    fi
fi

echo ""
echo "Setup complete."
echo ""
echo "Run the demo with: sh ./run.sh"
echo "Reset the demo with: sh ./reset.sh"
