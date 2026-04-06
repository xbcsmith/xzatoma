#!/usr/bin/env bash
# seed_plan.sh
#
# Publishes a Plan JSON event to the Redpanda input topic that the Atoma
# generic watcher is subscribed to.  The plan payload is consumed directly
# by Atoma; no plan files on disk are required on the watcher side.
#
# Usage:
#   ./seed_plan.sh [PRESET]
#   ./seed_plan.sh --stdin
#
# Presets:
#   hello   (default)  hello-world plan with action "greet"
#   health             system-health plan with action "report"
#   audit              doc-comment audit plan with subagent delegation
#
# Stdin mode:
#   Pipe any valid Plan JSON to the script via stdin:
#     echo '{"name":"my-plan","action":"build","version":"1.0.0","tasks":[{"id":"01KN79MEKAVXTSG2PZJ3X0YS3Z","description":"Run: echo hello"}],"max_iterations":3}' \
#       | ./seed_plan.sh --stdin
#
#   Or convert a YAML plan file (requires pyyaml):
#     python3 -c "import sys, json, yaml; print(json.dumps(yaml.safe_load(sys.stdin)))" \
#       < plans/system_health.yaml \
#       | ./seed_plan.sh --stdin
#
# Examples:
#   ./seed_plan.sh              # sends the hello-world plan
#   ./seed_plan.sh hello        # same as above
#   ./seed_plan.sh health       # sends the system-health plan
#   ./seed_plan.sh audit        # sends the doc-comment audit plan (requires pyyaml)
#   ./seed_plan.sh --stdin      # reads plan JSON from stdin

set -euo pipefail

PRESET="${1:-hello}"

TOPIC="atoma.plans"

# ---------------------------------------------------------------------------
# Task ID generator - uses ulid CLI when available, falls back to date+random
# ---------------------------------------------------------------------------
gen_id() {
    if command -v ulid >/dev/null 2>&1; then
        ulid
    else
        printf 'task-%s-%s' "$(date +%s)" "${RANDOM}"
    fi
}

REDPANDA_CONTAINER="redpanda"
INTERNAL_BROKER="localhost:9092"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
log()  { printf '[seed_plan] %s\n' "$*"; }
warn() { printf '[seed_plan] WARNING: %s\n' "$*" >&2; }
die()  { printf '[seed_plan] ERROR: %s\n' "$*" >&2; exit 1; }

# ---------------------------------------------------------------------------
# Dependency check
# ---------------------------------------------------------------------------
if ! docker inspect "${REDPANDA_CONTAINER}" >/dev/null 2>&1; then
    die "Container '${REDPANDA_CONTAINER}' is not running.
Start the Redpanda stack first (from the repository root):
  docker compose -f docker-compose.redpanda.yaml up -d"
fi

# ---------------------------------------------------------------------------
# Select plan payload
# ---------------------------------------------------------------------------
case "${PRESET}" in

    hello)
        PLAN_NAME="hello-world"
        PLAN_ACTION="greet"
        PLAN_VERSION="1.0.0"
        PLAN_ID=$(gen_id)
        TASK_ID_1=$(gen_id)
        PLAN_JSON=$(cat <<ENDJSON
{
  "id": "${PLAN_ID}",
  "name": "hello-world",
  "description": "Simple greeting plan to verify the generic watcher is running.",
  "action": "greet",
  "version": "1.0.0",
  "goals": ["Confirm the generic watcher received and executed this plan"],
  "tasks": [
    {
      "id": "${TASK_ID_1}",
      "description": "You are Atoma running in generic watcher mode. Run these three commands and report each one with its output: (1) echo Generic watcher is alive (2) date -u (3) uname -s. Then run mkdir -p tmp and write a brief report to tmp/hello-world-report.txt containing: a header line Atoma Generic Watcher - Hello World, the timestamp from command 2, and the platform from command 3. Finish with cat tmp/hello-world-report.txt to confirm the file was written.",
      "priority": "low"
    }
  ],
  "max_iterations": 5,
  "allow_dangerous": false,
  "result_mentions": ["tmp/hello-world-report.txt"]
}
ENDJSON
)
        ;;

    health)
        PLAN_NAME="system-health"
        PLAN_ACTION="report"
        PLAN_VERSION="1.0.0"
        PLAN_ID=$(gen_id)
        TASK_ID_1=$(gen_id)
        TASK_ID_2=$(gen_id)
        PLAN_JSON=$(cat <<ENDJSON
{
  "id": "${PLAN_ID}",
  "name": "system-health",
  "description": "Basic system health check - runs diagnostics and writes a report file.",
  "action": "report",
  "version": "1.0.0",
  "goals": ["Collect and report current system health metrics"],
  "tasks": [
    {
      "id": "${TASK_ID_1}",
      "description": "Gather system information by running each of the following commands and reporting the command name and its output clearly: uname -a, date -u, df -h .",
      "priority": "high"
    },
    {
      "id": "${TASK_ID_2}",
      "description": "Write a plain-text system health report to ./tmp/system-health-report.txt. First run mkdir -p ./tmp to ensure the directory exists. The report must contain: a header line System Health Report, a timestamp from date -u, the platform info from uname -a, a disk usage section from df -h ., and a footer line end of report. After writing confirm the file exists with head -3 tmp/system-health-report.txt.",
      "priority": "medium"
    }
  ],
  "max_iterations": 8,
  "allow_dangerous": false,
  "result_mentions": ["tmp/system-health-report.txt"]
}
ENDJSON
)
        ;;

    audit)
        PLAN_NAME="doc-comment-audit"
        PLAN_ACTION="audit"
        PLAN_VERSION="1.0.0"
        PLAN_ID=$(gen_id)
        if ! command -v python3 >/dev/null 2>&1; then
            die "The audit preset requires python3 with pyyaml.  Install: pip install pyyaml"
        fi
        _plan_file="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/plans/doc_audit.yaml"
        PLAN_JSON=$(
            ATOMA_PLAN_ID="${PLAN_ID}" ATOMA_PLAN_FILE="${_plan_file}" \
            python3 -c "
import json, yaml, os
with open(os.environ['ATOMA_PLAN_FILE']) as f:
    plan = yaml.safe_load(f)
plan['id'] = os.environ['ATOMA_PLAN_ID']
print(json.dumps(plan))
"
        ) || die "Failed to load plans/doc_audit.yaml.  Install pyyaml: pip install pyyaml"
        ;;

    --stdin)
        PLAN_JSON=$(cat)
        if [ -z "${PLAN_JSON}" ]; then
            die "No input received on stdin.  Pipe a Plan JSON document to this script."
        fi
        # Extract name and action for logging (best-effort; requires python3)
        if command -v python3 >/dev/null 2>&1; then
            PLAN_NAME=$(printf '%s' "${PLAN_JSON}" | python3 -c "import sys, json; d=json.load(sys.stdin); print(d.get('name','(unknown)'))") || PLAN_NAME="(unknown)"
            PLAN_ACTION=$(printf '%s' "${PLAN_JSON}" | python3 -c "import sys, json; d=json.load(sys.stdin); print(d.get('action','(none)'))") || PLAN_ACTION="(none)"
            PLAN_VERSION=$(printf '%s' "${PLAN_JSON}" | python3 -c "import sys, json; d=json.load(sys.stdin); print(d.get('version','(none)'))") || PLAN_VERSION="(none)"
            PLAN_ID=$(printf '%s' "${PLAN_JSON}" | python3 -c "import sys, json; d=json.load(sys.stdin); print(d.get('id','(none)'))") || PLAN_ID="(none)"
        else
            PLAN_NAME="(stdin)"
            PLAN_ACTION="(stdin)"
            PLAN_VERSION="(stdin)"
            PLAN_ID="(stdin)"
        fi
        ;;

    *)
        die "Unknown preset '${PRESET}'.
Valid presets: hello, health, audit
Use --stdin to pipe a custom plan JSON document."
        ;;

esac

# ---------------------------------------------------------------------------
# Validate and compact the JSON to a single line before publishing.
#
# rpk topic produce is newline-delimited: every newline in stdin becomes a
# separate Kafka record.  Compacting to one line guarantees the entire plan
# is delivered as a single message regardless of how the heredoc or stdin
# payload was formatted.
# ---------------------------------------------------------------------------
if command -v python3 >/dev/null 2>&1; then
    PLAN_JSON=$(printf '%s' "${PLAN_JSON}" | \
        python3 -c "import sys, json; print(json.dumps(json.load(sys.stdin)))" 2>/dev/null) \
        || die "Plan payload is not valid JSON.  Check the plan content and try again."
elif command -v jq >/dev/null 2>&1; then
    PLAN_JSON=$(printf '%s' "${PLAN_JSON}" | jq -c .) \
        || die "Plan payload is not valid JSON.  Check the plan content and try again."
else
    warn "Neither python3 nor jq found; skipping JSON validation and compaction."
    warn "The plan JSON must be a single line or rpk will split it into multiple records."
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
    2>/dev/null || true   # ignore "topic already exists" error

# ---------------------------------------------------------------------------
# Publish the plan event
# ---------------------------------------------------------------------------
log "Publishing plan event to topic '${TOPIC}' ..."
log "  ID      : ${PLAN_ID:-}"
log "  Name    : ${PLAN_NAME:-}"
log "  Action  : ${PLAN_ACTION:-}"
log "  Version : ${PLAN_VERSION:-}"

printf '%s\n' "${PLAN_JSON}" | docker exec -i "${REDPANDA_CONTAINER}" \
    rpk topic produce "${TOPIC}" \
    --brokers "${INTERNAL_BROKER}"

log "Plan event published successfully."

# ---------------------------------------------------------------------------
# Pretty-print the payload if jq is available
# ---------------------------------------------------------------------------
if command -v jq >/dev/null 2>&1; then
    printf '\nPayload published:\n'
    printf '%s' "${PLAN_JSON}" | jq .
else
    warn "jq not found; install it for pretty payload output."
    printf '\nPayload published (raw):\n%s\n' "${PLAN_JSON}"
fi

# ---------------------------------------------------------------------------
# Helpful next steps
# ---------------------------------------------------------------------------
cat <<HINTS

Next steps:
  1. If XZatoma is not yet running in generic watcher mode, start it from
     demos/watcher/generic/:
       cd demos/watcher/generic
       xzatoma --config config.yaml watch

  2. XZatoma should pick up the plan event within a few seconds, execute it,
     and publish a PlanResultEvent to the output topic.

  3. Watch the result in a separate terminal:
       ./read_results.sh

  4. To send another event with a different preset:
       ./seed_plan.sh hello
       ./seed_plan.sh health
       ./seed_plan.sh audit   # subagent doc-comment audit (requires pyyaml)

  5. To send a fully custom plan via stdin:
       echo '{"name":"my-plan","action":"deploy","version":"2.0.0","tasks":[{"id":"deploy-task","description":"Run: echo deployed"}],"max_iterations":3}' \\
         | ./seed_plan.sh --stdin

  6. Inspect the input topic in the Redpanda Console at http://localhost:8081

HINTS
