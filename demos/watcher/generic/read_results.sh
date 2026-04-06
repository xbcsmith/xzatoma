#!/usr/bin/env bash
# read_results.sh
#
# Reads PlanResultEvent messages from the Atoma generic watcher output topic
# and pretty-prints each result as it arrives.  Blocks until Ctrl+C.
#
# Usage:
#   ./read_results.sh [TOPIC]
#
# Examples:
#   ./read_results.sh
#   ./read_results.sh atoma.results
#   ./read_results.sh atoma.custom-results
#
# Prerequisites:
#   - Redpanda container running (from the repository root):
#       docker compose -f docker-compose.redpanda.yaml up -d
#   - Atoma running in generic watcher mode:
#       xzatoma --config config.yaml watch

set -euo pipefail

TOPIC="${1:-atoma.results}"
REDPANDA_CONTAINER="redpanda"
INTERNAL_BROKER="localhost:9092"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
log() { printf '[read_results] %s\n' "$*"; }
die() { printf '[read_results] ERROR: %s\n' "$*" >&2; exit 1; }

# ---------------------------------------------------------------------------
# Dependency check
# ---------------------------------------------------------------------------
if ! docker inspect "${REDPANDA_CONTAINER}" >/dev/null 2>&1; then
    die "Container '${REDPANDA_CONTAINER}' is not running.
Start the Redpanda stack first (from the repository root):
  docker compose -f docker-compose.redpanda.yaml up -d"
fi

# ---------------------------------------------------------------------------
# Ensure the output topic exists before consuming.
# rpk errors if the topic does not exist and auto-create is disabled.
# ---------------------------------------------------------------------------
docker exec "${REDPANDA_CONTAINER}" \
    rpk topic create "${TOPIC}" \
    --brokers "${INTERNAL_BROKER}" \
    --partitions 1 \
    --replicas 1 \
    2>/dev/null || true

# ---------------------------------------------------------------------------
# Consume
#
# rpk topic consume emits one JSON envelope per line:
#   {"topic":"...","key":"...","value":"<json-string>","partition":0,"offset":0,...}
#
# With jq:   parse .value (the PlanResultEvent JSON) and pretty-print it.
# Without jq: print the raw rpk envelopes.
# ---------------------------------------------------------------------------
log "Waiting for result events on topic '${TOPIC}' ..."
log "Press Ctrl+C to stop."
printf '\n'

if command -v jq >/dev/null 2>&1; then
    docker exec "${REDPANDA_CONTAINER}" \
        rpk topic consume "${TOPIC}" \
        --brokers "${INTERNAL_BROKER}" \
    | jq --unbuffered '.value | fromjson'
else
    log "jq not found - install it for formatted output (brew install jq)."
    log "Raw rpk output follows:"
    printf '\n'
    docker exec "${REDPANDA_CONTAINER}" \
        rpk topic consume "${TOPIC}" \
        --brokers "${INTERNAL_BROKER}"
fi
