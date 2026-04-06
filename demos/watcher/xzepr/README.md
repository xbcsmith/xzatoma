# XZepr Watcher Demo

This demo shows XZatoma running in **XZepr watcher mode**: a long-running process
that consumes XZepr CloudEvents from a Redpanda topic, extracts the embedded plan
from each event payload, executes it autonomously through the configured AI
provider, and publishes a result event back to an output topic.

XZepr is a software supply chain event platform. XZatoma's XZepr watcher bridges
XZepr's event bus with autonomous AI-driven plan execution: when a qualifying
build, deployment, or pipeline event arrives, XZatoma automatically runs the
plan embedded in that event and reports the outcome.

The demo runs entirely on localhost. A Redpanda broker supplies the event bus.
No live XZepr server is required — the demo produces synthetic CloudEvents
directly into Redpanda.

> **See also**: `demos/watcher/README.md` for an overview of both watcher modes,
> and `demos/watcher/generic/` for the standalone generic watcher demo.

---

## What is XZepr Watcher Mode

In XZepr watcher mode XZatoma:

1. Subscribes to a Kafka **input topic** (default: `xzepr.events`).
2. Deserialises each message as an XZepr **CloudEvent** (CloudEvents 1.0.1).
3. Applies the configured **event filter** (`filters:` section) to decide
   whether to process the event. Filtered events are committed and skipped.
4. Extracts the embedded **plan** from the event payload using a cascade of
   extraction strategies:
   - `data.events[0].payload.plan` — preferred; used by the demo fixtures
   - `data.events[0].payload` — entire payload treated as the plan
   - `data.plan` — top-level plan field in data
   - `data` — entire data object treated as the plan
5. Executes the plan through the configured AI provider.
6. Publishes a result event to a Kafka **output topic** (default: `xzepr.results`).

This demo includes:

- `config.yaml` — XZepr watcher configuration using Ollama `granite4:3b`
- `docker-compose.redpanda.yaml` — Redpanda single-broker stack
- `events/build_success_event.json` — sample `build.success` CloudEvent
- `events/deploy_success_event.json` — sample `deployment.success` CloudEvent
- `seed_event.sh` — publishes a CloudEvent to Redpanda
- `read_results.sh` — reads result events from the output topic as they arrive

---

## Prerequisites

| Requirement                                | Notes                                                 |
| ------------------------------------------ | ----------------------------------------------------- |
| Docker with Compose                        | Used to run Redpanda                                  |
| `docker-compose.redpanda.yaml`             | In this demo directory                                |
| Ollama running at `http://localhost:11434` | `ollama pull granite4:3b`                             |
| `xzatoma` binary on PATH                   | `cargo build --release && cargo install --path .`     |
| `jq` (optional)                            | Pretty-prints result events in `read_results.sh`      |

---

## Running the Demo

All commands are run from the `demos/watcher/xzepr/` directory:

```bash
cd demos/watcher/xzepr
```

### Step 1: Start the Redpanda stack

```bash
docker compose -f docker-compose.redpanda.yaml up -d
```

Wait a few seconds for Redpanda to become ready. Verify it is up:

```bash
docker exec redpanda rpk cluster health
docker exec redpanda rpk topic list
```

You can also open the Redpanda Console at `http://localhost:8081`.

### Step 2: Start XZatoma in XZepr watcher mode

Open a terminal, change to `demos/watcher/xzepr/`, and run:

```bash
xzatoma --config config.yaml watch
```

XZatoma connects to Redpanda, subscribes to `xzepr.events`, and waits for
CloudEvents. The startup banner shows the watcher type, input topic, output
topic, and active event filters.

### Step 3: Open a results terminal

Open a second terminal, change to `demos/watcher/xzepr/`, and run:

```bash
./read_results.sh
```

This tails the `xzepr.results` topic. Result events appear here as XZatoma
finishes each plan.

Alternatively, tail the topic directly:

```bash
docker exec redpanda rpk topic consume xzepr.results --brokers localhost:9092 | jq .
```

### Step 4: Seed a CloudEvent

Open a third terminal, change to `demos/watcher/xzepr/`, and run:

```bash
# Send a build.success event (triggers the build-verify plan)
./seed_event.sh build

# Send a deployment.success event (triggers the deploy-verify plan)
./seed_event.sh deploy
```

XZatoma picks up the CloudEvent, extracts the embedded plan, executes it, and
publishes a result event. Switch back to the results terminal to watch the
result appear.

### Step 5: Observe the output

Watch the XZatoma terminal for the agent transcript. When the plan finishes the
results terminal shows the published result event.

Check the report files the plans write:

```bash
cat tmp/build-verify-report.txt
cat tmp/deploy-verify-report.txt
```

---

## Event Presets

`seed_event.sh` ships two built-in event presets:

| Preset   | Event type           | Plan name      | What it does                                                          |
| -------- | -------------------- | -------------- | --------------------------------------------------------------------- |
| `build`  | `build.success`      | `build-verify` | Verifies build environment, writes `tmp/build-verify-report.txt`      |
| `deploy` | `deployment.success` | `deploy-verify`| Verifies deployment environment, writes `tmp/deploy-verify-report.txt`|

Use `--stdin` to pipe any CloudEvent JSON directly:

```bash
cat events/build_success_event.json | ./seed_event.sh --stdin
```

---

## Event Filtering

The `config.yaml` `filters:` section controls which CloudEvents XZatoma
processes. The demo is pre-configured to accept `build.success` and
`deployment.success` events with `success: true`:

```yaml
filters:
  event_types:
    - "deployment.success"
    - "build.success"
  success_only: true
```

Other available filter options:

```yaml
filters:
  # List of event types to accept (empty list = accept all event types)
  event_types:
    - "deployment.success"

  # Regex matched against the "source" field
  source_pattern: "xzepr\\.event\\.receiver\\..*"

  # Exact match on the "platform_id" field
  platform_id: "kubernetes"

  # Exact match on the "package" field
  package: "my-service"

  # Exact match on the "api_version" field
  api_version: "v1"

  # Only process events with "success": true
  success_only: true
```

To accept all events regardless of type, set `event_types` to an empty list and
`success_only` to `false`:

```yaml
filters:
  event_types: []
  success_only: false
```

---

## The `events/` Directory

The JSON files under `events/` are XZepr CloudEvent fixtures that `seed_event.sh`
publishes to Redpanda. Each fixture embeds a complete XZatoma plan in the
`data.events[0].payload.plan` field as a compact JSON string.

| File                               | Event type           | Embedded plan   |
| ---------------------------------- | -------------------- | --------------- |
| `events/build_success_event.json`  | `build.success`      | `build-verify`  |
| `events/deploy_success_event.json` | `deployment.success` | `deploy-verify` |

### CloudEvent Structure

Each event fixture follows the XZepr CloudEvents 1.0.1 format:

```json
{
  "success": true,
  "id": "<ULID>",
  "specversion": "1.0.1",
  "type": "build.success",
  "source": "xzepr.event.receiver.demo-receiver-001",
  "api_version": "v1",
  "name": "build.success",
  "version": "1.0.0",
  "release": "1.0.0",
  "platform_id": "local",
  "package": "xzatoma-demo",
  "data": {
    "events": [
      {
        "id": "<ULID>",
        "name": "build-verify",
        "version": "1.0.0",
        "release": "1.0.0",
        "platform_id": "local",
        "package": "xzatoma-demo",
        "description": "Verify build artifacts",
        "payload": { "plan": "<compact JSON plan string>" },
        "success": true,
        "event_receiver_id": "receiver-demo-001",
        "created_at": "2025-07-15T14:00:00Z"
      }
    ],
    "event_receivers": [],
    "event_receiver_groups": []
  }
}
```

### Plan Extraction

XZatoma's plan extractor tries to locate the plan in these locations in order:

1. `data.events[0].payload.plan` — preferred; used by the demo fixtures
2. `data.events[0].payload` — entire payload treated as the plan
3. `data.plan` — top-level plan field in data
4. `data` — entire data object treated as the plan

The first location that yields a non-null value is used. The demo fixtures
always embed the plan at `data.events[0].payload.plan`.

---

## Tearing Down

Stop the watcher with `Ctrl+C` and shut down Redpanda:

```bash
docker compose -f docker-compose.redpanda.yaml down
```

To remove all demo output files:

```bash
rm -rf tmp
```

---

## Configuration File

| File          | Provider | Model         | Watcher Type |
| ------------- | -------- | ------------- | ------------ |
| `config.yaml` | Ollama   | `granite4:3b` | `xzepr`      |

The config sets `watcher_type: xzepr` and configures the `kafka` section with
`topic: "xzepr.events"` and `output_topic: "xzepr.results"`. Running from
`demos/watcher/xzepr/` is recommended but not required.

## Files in This Demo

| File                               | Purpose                                                      |
| ---------------------------------- | ------------------------------------------------------------ |
| `config.yaml`                      | XZepr watcher configuration using Ollama `granite4:3b`      |
| `docker-compose.redpanda.yaml`     | Redpanda single-broker stack for local development           |
| `seed_event.sh`                    | Publishes a XZepr CloudEvent to the Redpanda input topic     |
| `read_results.sh`                  | Reads result events from the Redpanda output topic           |
| `events/build_success_event.json`  | Sample `build.success` CloudEvent fixture                    |
| `events/deploy_success_event.json` | Sample `deployment.success` CloudEvent fixture               |

---

## Further Reading

- `demos/watcher/README.md` — overview of both watcher modes
- `demos/watcher/generic/` — the generic plan-event watcher demo
- `docs/how-to/watcher_demo.md` — combined how-to guide for both watcher modes
- `docs/reference/architecture.md` — overall XZatoma architecture
