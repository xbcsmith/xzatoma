#!/bin/sh
# Watcher Demo - Produce Test Event
#
# Publishes the demo plan event fixture to the local Kafka topic so the
# running watcher can process it. Requires kcat (formerly kafkacat).
#
# Usage:
#   sh ./scripts/produce_event.sh [broker] [topic]
#   ./scripts/produce_event.sh              (defaults: localhost:9092, demo.plan.events)
#   ./scripts/produce_event.sh localhost:9092 my.topic
#
# Prerequisites:
#   - kcat or kafkacat installed
#   - Kafka or Redpanda broker running at localhost:9092
#   - ./run.sh is running in another terminal
#
# Install kcat:
#   macOS:  brew install kcat
#   Linux:  apt install kcat  OR  brew install kcat

set -e

DEMO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$DEMO_DIR"

BROKER="${1:-localhost:9092}"
TOPIC="${2:-demo.plan.events}"
EVENT_FILE="./watcher/demo_plan_event.json"

if [ ! -f "$EVENT_FILE" ]; then
    echo "ERROR: Event fixture not found at $EVENT_FILE" >&2
    echo "Ensure you are running this script from the watcher demo directory." >&2
    exit 1
fi

# Locate kcat or kafkacat
if command -v kcat >/dev/null 2>&1; then
    KCAT="kcat"
elif command -v kafkacat >/dev/null 2>&1; then
    KCAT="kafkacat"
else
    echo "ERROR: kcat or kafkacat not found." >&2
    echo "" >&2
    echo "Install kcat:" >&2
    echo "  macOS:   brew install kcat" >&2
    echo "  Ubuntu:  sudo apt install kafkacat" >&2
    echo "" >&2
    echo "Alternatively, paste the contents of $EVENT_FILE" >&2
    echo "into the Redpanda Console producer for topic $TOPIC." >&2
    exit 1
fi

echo "Publishing demo plan event..."
echo "  Broker     : $BROKER"
echo "  Topic      : $TOPIC"
echo "  Event file : $EVENT_FILE"
echo "  kcat       : $KCAT"
echo ""

"$KCAT" -P -b "$BROKER" -t "$TOPIC" < "$EVENT_FILE"

echo "Event published successfully."
echo ""
echo "The watcher (run.sh) should log a match and begin plan execution."
echo "Plan output will appear in tmp/output/watcher_result.txt"
echo ""
echo "To watch for result events:"
echo "  $KCAT -C -b $BROKER -t demo.plan.results -o end"
