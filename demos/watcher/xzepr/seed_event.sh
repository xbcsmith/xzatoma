#!/usr/bin/env bash
# seed_event.sh
#
# Publishes a XZepr CloudEvent JSON to the Redpanda input topic that the
# XZatoma XZepr watcher is subscribed to.
#
# Usage:
#   ./seed_event.sh [PRESET]
#   ./seed_event.sh --stdin
#
# Presets:
#   build    (default)  build.success CloudEvent with embedded build-verify plan
#   deploy              deployment.success CloudEvent with embedded deploy-verify plan
#
# Stdin mode:
#   Pipe any valid XZepr CloudEvent JSON to the script via stdin:
#     cat events/build_success_event.json | ./seed_event.sh --stdin
#
# Examples:
#   ./seed_event.sh            # sends the build.success event
#   ./seed_event.sh build      # same as above
#   ./seed_event.sh deploy     # sends the deployment.success event
#   ./seed_event.sh --stdin    # reads event JSON from stdin

set -euo pipefail

PRESET="${1:-build}"

TOPIC="xzepr.events"

REDPANDA_CONTAINER="redpanda"
INTERNAL_BROKER="localhost:9092"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
log()  { printf '[seed_event] %s\n' "$*"; }
warn() { printf '[seed_event] WARNING: %s\n' "$*" >&2; }
die()  { printf '[seed_event] ERROR: %s\n' "$*" >&2; exit 1; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# ---------------------------------------------------------------------------
# Dependency check
# ---------------------------------------------------------------------------
if ! docker inspect "${REDPANDA_CONTAINER}" >/dev/null 2>&1; then
    die "Container '${REDPANDA_CONTAINER}' is not running.
Start the Redpanda stack first (from this directory):
  docker compose -f docker-compose.redpanda.yaml up -d"
fi

# ---------------------------------------------------------------------------
# Select event payload
# ---------------------------------------------------------------------------
case "${PRESET}" in

    build)
        EVENT_FILE="${SCRIPT_DIR}/events/build_success_event.json"
        if [ ! -f "${EVENT_FILE}" ]; then
            die "Event fixture not found: ${EVENT_FILE}"
        fi
        EVENT_JSON=$(cat "${EVENT_FILE}")
        EVENT_TYPE="build.success"
        EVENT_ID="01JXZEPR0BUILD0DEMO000001"
        ;;

    deploy)
        EVENT_FILE="${SCRIPT_DIR}/events/deploy_success_event.json"
        if [ ! -f "${EVENT_FILE}" ]; then
            die "Event fixture not found: ${EVENT_FILE}"
        fi
        EVENT_JSON=$(cat "${EVENT_FILE}")
        EVENT_TYPE="deployment.success"
        EVENT_ID="01JXZEPR0DEPLOY0DEMO00001"
        ;;

    --stdin)
        EVENT_JSON=$(cat)
        if [ -z "${EVENT_JSON}" ]; then
            die "No input received on stdin.  Pipe a XZepr CloudEvent JSON document."
        fi
        # Extract fields for logging (best-effort; requires python3)
        if command -v python3 >/dev/null 2>&1; then
            EVENT_TYPE=$(printf '%s' "${EVENT_JSON}" | python3 -c "import sys, json; d=json.load(sys.stdin); print(d.get('type','(unknown)'))") || EVENT_TYPE="(unknown)"
            EVENT_ID=$(printf '%s' "${EVENT_JSON}" | python3 -c "import sys, json; d=json.load(sys.stdin); print(d.get('id','(unknown)'))") || EVENT_ID="(unknown)"
        else
            EVENT_TYPE="(stdin)"
            EVENT_ID="(stdin)"
        fi
        ;;

    *)
        die "Unknown preset '${PRESET}'.
Valid presets: build, deploy
Use --stdin to pipe a custom XZepr CloudEvent JSON document."
        ;;

esac

# ---------------------------------------------------------------------------
# Validate and compact the JSON to a single line before publishing.
# rpk topic produce is newline-delimited: every newline becomes a separate
# Kafka record.  Compact to one line to guarantee one message per event.
# ---------------------------------------------------------------------------
if command -v python3 >/dev/null 2>&1; then
    EVENT_JSON=$(printf '%s' "${EVENT_JSON}" | \
        python3 -c "import sys, json; print(json.dumps(json.load(sys.stdin)))" 2>/dev/null) \
        || die "Event payload is not valid JSON.  Check the event content and try again."
elif command -v jq >/dev/null 2>&1; then
    EVENT_JSON=$(printf '%s' "${EVENT_JSON}" | jq -c .) \
        || die "Event payload is not valid JSON."
else
    warn "Neither python3 nor jq found; skipping JSON validation and compaction."
    warn "The event JSON must be a single line or rpk will split it into multiple records."
fi

# ---------------------------------------------------------------------------
# Ensure the input topic exists
# ---------------------------------------------------------------------------
log "Ensuring topic '${TOPIC}' exists ..."
docker exec "${REDPANDA_CONTAINER}" \
    rpk topic create "${TOPIC}" \
    --brokers "${INTERNAL_BROKER}" \
    --partitions 1 \
    --replicas 1 \
    2>/dev/null || true

# ---------------------------------------------------------------------------
# Publish the event
# ---------------------------------------------------------------------------
log "Publishing XZepr CloudEvent to topic '${TOPIC}' ..."
log "  ID   : ${EVENT_ID}"
log "  Type : ${EVENT_TYPE}"

printf '%s\n' "${EVENT_JSON}" | docker exec -i "${REDPANDA_CONTAINER}" \
    rpk topic produce "${TOPIC}" \
    --brokers "${INTERNAL_BROKER}"

log "CloudEvent published successfully."

# ---------------------------------------------------------------------------
# Pretty-print the payload if jq is available
# ---------------------------------------------------------------------------
if command -v jq >/dev/null 2>&1; then
    printf '\nPayload published:\n'
    printf '%s' "${EVENT_JSON}" | jq .
else
    warn "jq not found; install it for pretty payload output."
    printf '\nPayload published (raw):\n%s\n' "${EVENT_JSON}"
fi

# ---------------------------------------------------------------------------
# Helpful next steps
# ---------------------------------------------------------------------------
cat <<HINTS

Next steps:
  1. If XZatoma is not yet running in XZepr watcher mode, start it from
     demos/watcher/xzepr/:
       cd demos/watcher/xzepr
       xzatoma --config config.yaml watch

  2. XZatoma should pick up the event within a few seconds, execute the
     embedded plan, and publish a result event to the output topic.

  3. Watch the result in a separate terminal:
       ./read_results.sh

  4. To send the other event preset:
       ./seed_event.sh build     # build.success event
       ./seed_event.sh deploy    # deployment.success event

  5. To send a custom event via stdin:
       cat events/build_success_event.json | ./seed_event.sh --stdin

  6. Inspect the input topic in the Redpanda Console at http://localhost:8081

HINTS
