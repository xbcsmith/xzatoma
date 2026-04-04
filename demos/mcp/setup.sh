#!/bin/sh
# MCP Demo - Setup
#
# Prepares the demo-local state required before running the MCP demo.
# Creates the tmp/ directory structure and verifies all prerequisites including
# Node.js and npx for the MCP filesystem server.
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

echo "Setting up MCP demo..."
echo "Demo root: $DEMO_DIR"
echo ""

# Create tmp/output if it does not already exist.
# The MCP filesystem server uses ./tmp/output as its root, so this directory
# must exist before the server starts.
mkdir -p tmp/output

echo "Checking required files..."

missing=0
for f in plans/mcp_demo.yaml mcp/server_config.yaml mcp/tool_examples.md; do
    if [ -f "$f" ]; then
        echo "  OK  $f"
    else
        echo "  MISSING  $f"
        missing=1
    fi
done

if [ "$missing" -eq 1 ]; then
    echo ""
    echo "ERROR: One or more required files are missing. The demo directory may be incomplete."
    exit 1
fi

echo ""
echo "Checking prerequisites..."

# Check that xzatoma is available
if command -v xzatoma >/dev/null 2>&1; then
    echo "  OK  xzatoma found at: $(command -v xzatoma)"
elif [ -x "../../target/release/xzatoma" ]; then
    echo "  OK  xzatoma found at ../../target/release/xzatoma"
elif [ -x "../../target/debug/xzatoma" ]; then
    echo "  OK  xzatoma found at ../../target/debug/xzatoma"
else
    echo "  WARNING  xzatoma not found on PATH or in build output."
    echo "           Build from the repository root with: cargo build --release"
    echo ""
fi

# Check that Ollama is running
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

# Check that Node.js is available (required for the MCP filesystem server)
if command -v node >/dev/null 2>&1; then
    NODE_VERSION="$(node --version 2>/dev/null || echo 'unknown')"
    echo "  OK  node found: $NODE_VERSION"
else
    echo "  WARNING  node not found on PATH."
    echo "           Install Node.js 18 or later from https://nodejs.org"
    echo "           The MCP filesystem server requires Node.js to run via npx."
    echo ""
fi

# Check that npx is available (used to launch the MCP server)
if command -v npx >/dev/null 2>&1; then
    echo "  OK  npx found at: $(command -v npx)"
else
    echo "  WARNING  npx not found on PATH."
    echo "           npx is included with Node.js. Install Node.js 18 or later."
    echo "           The demo uses: npx -y @modelcontextprotocol/server-filesystem"
    echo ""
fi

echo ""
echo "Setup complete."
echo ""
echo "Run the demo:"
echo "  ./run.sh"
echo ""
echo "The MCP filesystem server will be started automatically by XZatoma"
echo "when the demo plan runs. The server is scoped to ./tmp/output/."
