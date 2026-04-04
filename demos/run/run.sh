#!/bin/sh
# XZatoma Run Demo - run.sh
#
# Executes the hello_world plan against a local Ollama model using
# demo-local configuration and storage paths.
#
# Usage:
#   sh ./run.sh
#   ./run.sh          (after chmod +x run.sh)
#
# All output is written to tmp/output/run_output.txt in addition to stdout.
# Run ./setup.sh before this script if tmp/ does not exist.

set -e

DEMO_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$DEMO_DIR"

mkdir -p tmp/output

echo "XZatoma Run Demo"
echo "Provider : Ollama"
echo "Model    : granite4:3b"
echo "Plan     : plans/hello_world.yaml"
echo ""

xzatoma \
    --config ./config.yaml \
    --storage-path ./tmp/xzatoma.db \
    run \
    --plan ./plans/hello_world.yaml \
    2>&1 | tee tmp/output/run_output.txt

echo ""
echo "Output saved to tmp/output/run_output.txt"
