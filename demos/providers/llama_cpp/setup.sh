#!/bin/sh
# XZatoma llama.cpp Provider Demo - Setup
#
# Prepares the demo for execution. Creates the tmp/output directory and
# verifies all required prerequisites are met before running the demo.
#
# Prerequisites checked:
#   - xzatoma binary is available
#   - llama.cpp server (llama-server) is running at http://localhost:8080
#   - The /v1/models endpoint responds (confirms the server is ready)
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

LLAMA_HOST="http://localhost:8080"

echo "Setting up llama.cpp Provider demo..."
echo "Demo root: $DEMO_DIR"
echo ""

# ---------------------------------------------------------------------------
# Create required directories
# ---------------------------------------------------------------------------
mkdir -p tmp/output
echo "  OK  tmp/output/ ready"
echo ""

# ---------------------------------------------------------------------------
# Verify plan files
# ---------------------------------------------------------------------------
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

# ---------------------------------------------------------------------------
# Verify xzatoma binary
# ---------------------------------------------------------------------------
echo "Checking that xzatoma is available..."

if command -v xzatoma >/dev/null 2>&1; then
    echo "  OK  xzatoma found at: $(command -v xzatoma)"
elif [ -x "../../target/release/xzatoma" ]; then
    echo "  OK  xzatoma found at: ../../target/release/xzatoma"
    echo "      Add it to PATH for easier invocation:"
    echo "        export PATH=\"\$PATH:$(cd ../../target/release && pwd)\""
elif [ -x "../../target/debug/xzatoma" ]; then
    echo "  OK  xzatoma found at: ../../target/debug/xzatoma (debug build)"
    echo "      Consider building a release binary for better inference throughput:"
    echo "        cargo build --release"
else
    echo "  WARNING  xzatoma not found on PATH or in ../../target/."
    echo "           Build from the repository root with:"
    echo "             cargo build --release"
    echo "           Then either install it or export the path:"
    echo "             export PATH=\"\$PATH:\$(pwd)/target/release\""
fi

echo ""

# ---------------------------------------------------------------------------
# Verify llama.cpp server
# ---------------------------------------------------------------------------
echo "Checking that llama-server is running at ${LLAMA_HOST} ..."

LLAMA_OK=0

if command -v curl >/dev/null 2>&1; then
    if curl -sf "${LLAMA_HOST}/v1/models" >/dev/null 2>&1; then
        echo "  OK  llama-server is responding at ${LLAMA_HOST}/v1/models"
        LLAMA_OK=1

        # Print the loaded model name if jq is available
        if command -v jq >/dev/null 2>&1; then
            MODEL_ID=$(curl -sf "${LLAMA_HOST}/v1/models" | jq -r '.data[0].id // "(unknown)"' 2>/dev/null || echo "(unknown)")
            echo "  OK  Loaded model: ${MODEL_ID}"
        fi
    else
        echo "  WARNING  Could not reach llama-server at ${LLAMA_HOST}/v1/models"
    fi
elif command -v wget >/dev/null 2>&1; then
    if wget -q -O /dev/null "${LLAMA_HOST}/v1/models" 2>/dev/null; then
        echo "  OK  llama-server is responding at ${LLAMA_HOST}/v1/models"
        LLAMA_OK=1
    else
        echo "  WARNING  Could not reach llama-server at ${LLAMA_HOST}/v1/models"
    fi
else
    echo "  SKIP  curl and wget not available; skipping llama-server connectivity check"
    LLAMA_OK=1
fi

if [ "$LLAMA_OK" -eq 0 ]; then
    echo ""
    echo "  To start the server, download a GGUF model and run:"
    echo ""
    echo "    llama-server \\"
    echo "      --model ./models/granite-3.3-2b-instruct-Q4_K_M.gguf \\"
    echo "      --port 8080 \\"
    echo "      --ctx-size 4096 \\"
    echo "      --alias granite-3.3-2b-instruct"
    echo ""
    echo "  See README.md for model download instructions and alternative models."
fi

echo ""

# ---------------------------------------------------------------------------
# Verify model name consistency (best-effort)
# ---------------------------------------------------------------------------
if [ "$LLAMA_OK" -eq 1 ] && command -v curl >/dev/null 2>&1 && command -v jq >/dev/null 2>&1; then
    CONFIG_MODEL=$(grep 'model:' config.yaml | grep -v '#' | head -1 | sed 's/.*model: *["'"'"']*//;s/["'"'"'].*//' | tr -d ' ')
    SERVER_MODEL=$(curl -sf "${LLAMA_HOST}/v1/models" | jq -r '.data[0].id // ""' 2>/dev/null || echo "")

    if [ -n "$CONFIG_MODEL" ] && [ -n "$SERVER_MODEL" ] && [ "$CONFIG_MODEL" != "$SERVER_MODEL" ]; then
        echo "  NOTE  Model name mismatch:"
        echo "          config.yaml    : ${CONFIG_MODEL}"
        echo "          server reports : ${SERVER_MODEL}"
        echo "        llama.cpp ignores the model field when a single model is loaded."
        echo "        To silence this note, set --alias ${CONFIG_MODEL} when starting the server"
        echo "        or update config.yaml to set model: ${SERVER_MODEL}."
        echo ""
    fi
fi

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo "Setup complete."
echo ""
if [ "$LLAMA_OK" -eq 1 ]; then
    echo "llama-server is ready. Run the demo:"
    echo ""
    echo "  ./run.sh"
    echo ""
    echo "Or run a plan directly:"
    echo ""
    echo "  xzatoma --config ./config.yaml --storage-path ./tmp/xzatoma.db \\"
    echo "    run --plan ./plans/hello_world.yaml"
    echo ""
    echo "  xzatoma --config ./config.yaml --storage-path ./tmp/xzatoma.db \\"
    echo "    run --plan ./plans/system_info.yaml"
else
    echo "Start llama-server first (see above), then run:"
    echo ""
    echo "  ./run.sh"
    echo ""
    echo "See README.md for full setup instructions."
fi
