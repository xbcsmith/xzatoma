#!/bin/sh
# Watcher Demo - Run
#
# Starts the XZatoma generic watcher, which consumes plan events from the
# Kafka topic "demo.plan.events", matches them against the "demo.*" action
# pattern, and executes the embedded plan using the Ollama provider.
#
# Usage:
#   sh ./run.sh
#   ./run.sh    (after chmod +x run.sh)
#
# Prerequisites:
#   - Ollama is running at http://localhost:11434
#   - granite4:3b model has been pulled (ollama pull granite4:3b)
#   - A Kafka or Redpanda broker is running at localhost:9092
#   - XZatoma binary is on PATH or in ../../target/release/xzatoma
#   - ./setup.sh has been run at least once
#
# Two-terminal workflow:
#   Terminal 1: ./run.sh            (starts watcher, runs until Ctrl+C)
#   Terminal 2: ./scripts/produce_event.sh   (injects a test event)

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

echo "XZatoma Watcher Demo"
echo "Provider     : Ollama"
echo "Model        : granite4:3b"
echo "Config       : $DEMO_DIR/config.yaml"
echo "Storage      : $DEMO_DIR/tmp/xzatoma.db"
echo "Watcher type : generic"
echo "Input topic  : demo.plan.events"
echo "Output topic : demo.plan.results"
echo "Broker       : localhost:9092"
echo "Match action : demo.*"
echo "Log file     : $DEMO_DIR/tmp/watcher.log"
echo ""
echo "The watcher will process any event where:"
echo "  event_type = \"plan\""
echo "  action     matches \"demo.*\" (case-insensitive)"
echo ""
echo "To inject a test event from a second terminal:"
echo "  cd $DEMO_DIR"
echo "  ./scripts/produce_event.sh"
echo ""
echo "To consume result events from a second terminal (requires kcat):"
echo "  kcat -C -b localhost:9092 -t demo.plan.results -o end"
echo ""
echo "Press Ctrl+C to stop the watcher."
echo ""

"$XZATOMA" \
    --config ./config.yaml \
    --storage-path ./tmp/xzatoma.db \
    watch \
    --watcher-type generic \
    --topic demo.plan.events \
    --brokers localhost:9092 \
    --group-id xzatoma-demo-watcher \
    --action "demo.*" \
    --create-topics \
    --log-file ./tmp/watcher.log \
    2>&1 | tee tmp/output/watcher_run.txt

echo ""
echo "Watcher stopped."
echo "Execution log written to: tmp/output/watcher_run.txt"
echo "Structured log written to: tmp/watcher.log"
echo "Any plan output files are under: tmp/output/"
