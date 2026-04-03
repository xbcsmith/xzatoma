#!/bin/sh
# XZatoma Vision Demo - Run
#
# Launches XZatoma in interactive chat mode using the granite3.2-vision:2b
# model. The demo directory is used as the working directory so that all
# file references resolve relative to it.
#
# Prerequisites:
#   1. Ollama is running at http://localhost:11434
#   2. The granite3.2-vision:2b model has been pulled
#   3. setup.sh has been run to create tmp/sample.png
#
# Usage:
#   sh ./run.sh
#   ./run.sh          (after chmod +x run.sh)
#
# All session state is written to tmp/xzatoma.db.
# Conversation transcripts are written to tmp/output/ when redirected.

set -e

DEMO_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$DEMO_DIR"

# Ensure output directory exists
mkdir -p tmp/output

echo "XZatoma Vision Demo"
echo "Provider: Ollama | Model: granite3.2-vision:2b"
echo ""

# Verify the sample image exists
if [ ! -f tmp/sample.png ]; then
    echo "Sample image not found at tmp/sample.png."
    echo "Run setup.sh first to create the sample image."
    echo ""
    echo "  sh ./setup.sh"
    echo ""
    exit 1
fi

echo "Sample image: tmp/sample.png (64x64 blue rectangle)"
echo ""
echo "Sample prompts are available in input/prompt.txt"
echo ""
echo "Suggested prompts to try at the chat prompt:"
echo "  Describe the visual properties of a solid blue 64x64 pixel square."
echo "  What types of images are you capable of analyzing?"
echo "  What is the difference between object detection and image captioning?"
echo ""
echo "Starting interactive chat with granite3.2-vision:2b..."
echo "Type 'exit' or press Ctrl-D to quit."
echo ""

exec xzatoma \
    --config ./config.yaml \
    --storage-path ./tmp/xzatoma.db \
    chat
