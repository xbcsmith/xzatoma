#!/bin/sh
# XZatoma llama.cpp Provider Demo - run.sh
#
# Executes the hello_world plan against a local llama.cpp inference server
# using the OpenAI-compatible API via demo-local configuration.
#
# Usage:
#   sh ./run.sh
#   ./run.sh          (after chmod +x run.sh)
#
# To run the system_info plan instead:
#   sh ./run.sh system_info
#
# All output is written to tmp/output/run_output.txt in addition to stdout.
# Run ./setup.sh before this script if tmp/ does not exist.
#
# Prerequisites:
#   - llama.cpp server running at http://localhost:8080
#       llama-server \
#         --model ./models/granite-3.3-2b-instruct-Q4_K_M.gguf \
#         --port 8080 \
#         --ctx-size 4096 \
#         --alias granite-3.3-2b-instruct
#   - xzatoma binary on PATH or in ../../target/release/xzatoma

set -e

DEMO_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$DEMO_DIR"

# ---------------------------------------------------------------------------
# Locate the xzatoma binary
# ---------------------------------------------------------------------------
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

# ---------------------------------------------------------------------------
# Select plan
# ---------------------------------------------------------------------------
PLAN_ARG="${1:-hello_world}"

case "${PLAN_ARG}" in
    hello_world|hello)
        PLAN_FILE="./plans/hello_world.yaml"
        PLAN_LABEL="Hello World"
        ;;
    system_info|system)
        PLAN_FILE="./plans/system_info.yaml"
        PLAN_LABEL="System Information"
        ;;
    *)
        echo "ERROR: Unknown plan '${PLAN_ARG}'." >&2
        echo "Valid plans: hello_world, system_info" >&2
        exit 1
        ;;
esac

if [ ! -f "${PLAN_FILE}" ]; then
    echo "ERROR: Plan file not found: ${PLAN_FILE}" >&2
    echo "Run ./setup.sh to verify the demo directory is complete." >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# Ensure output directory exists
# ---------------------------------------------------------------------------
mkdir -p tmp/output

# ---------------------------------------------------------------------------
# Check that the llama.cpp server is reachable before starting
# ---------------------------------------------------------------------------
LLAMA_URL="http://localhost:8080"

if command -v curl >/dev/null 2>&1; then
    if ! curl -sf "${LLAMA_URL}/v1/models" >/dev/null 2>&1; then
        echo "WARNING: llama.cpp server not reachable at ${LLAMA_URL}" >&2
        echo "         Start it with:" >&2
        echo "           llama-server \\" >&2
        echo "             --model ./models/<your-model>.gguf \\" >&2
        echo "             --port 8080 \\" >&2
        echo "             --ctx-size 4096 \\" >&2
        echo "             --alias granite-3.3-2b-instruct" >&2
        echo "" >&2
        echo "         Proceeding anyway - xzatoma will report a connection error" >&2
        echo "         if the server is not running when the request is made." >&2
        echo ""
    fi
fi

# ---------------------------------------------------------------------------
# Run the demo
# ---------------------------------------------------------------------------
echo "XZatoma llama.cpp Provider Demo"
echo "Provider  : openai (llama.cpp)"
echo "Server    : http://localhost:8080/v1"
echo "Model     : granite-3.3-2b-instruct"
echo "Plan      : ${PLAN_FILE} (${PLAN_LABEL})"
echo "Output    : tmp/output/run_output.txt"
echo ""

"$XZATOMA" \
    --config ./config.yaml \
    --storage-path ./tmp/xzatoma.db \
    run \
    --plan "${PLAN_FILE}" \
    2>&1 | tee tmp/output/run_output.txt

echo ""
echo "Output saved to tmp/output/run_output.txt"
echo ""
echo "To run the other plan:"
if [ "${PLAN_ARG}" = "hello_world" ] || [ "${PLAN_ARG}" = "hello" ]; then
    echo "  sh ./run.sh system_info"
else
    echo "  sh ./run.sh hello_world"
fi
