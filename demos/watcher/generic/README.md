# Generic Watcher Demo

This demo shows XZatoma running in **generic watcher mode**: a long-running
process that consumes a plan serialised as a JSON event from a Redpanda topic,
executes it autonomously through the configured AI provider, and publishes the
result as a JSON event to an output topic.

The generic watcher requires no XZepr or Janus Gatekeeper API and no receipt
infrastructure. The plan is fully contained in each Kafka message; XZatoma
executes it and posts a compact result event back to Redpanda. Every processed
plan publishes the same `PlanResultEvent` JSON shape to the results topic,
whether execution succeeds or fails with an error.

The demo runs entirely on localhost. A Redpanda broker supplies the event bus.

> **See also**: `demos/watcher/README.md` for an overview of both watcher modes,
> and `demos/watcher/xzepr/` for the XZepr watcher demo.

---

## What is Generic Watcher Mode

In generic watcher mode XZatoma:

1. Subscribes to a Kafka **input topic** (default: `atoma.plans`).
2. Deserialises each message payload as a `Plan` JSON document.
3. Optionally filters the plan using a `GenericEventMatcher` (match on `action`,
   `name`, `version`, or combinations).
4. Executes the plan through the configured AI provider.
5. Publishes a `PlanResultEvent` JSON to a Kafka **output topic** (default:
   `atoma.results`; may equal the input topic) for every processed plan,
   including both success and failure outcomes.

This demo includes:

- `config.yaml` — watcher configuration using Ollama `granite4:3b`
- `config_ollama_granite.yaml` — watcher configuration using Ollama `granite4:3b`
- `plans/hello_world.yaml` — reference plan payload: `action: greet`
- `plans/system_health.yaml` — reference plan payload: `action: report`
- `plans/doc_audit.yaml` — reference plan payload: `action: audit` (subagent demo)
- `seed_plan.sh` — publishes a plan JSON event to Redpanda
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
| `ulid`                                     | `go install github.com/oklog/ulid/v2/cmd/ulid@latest` |

---

## Running the Demo

All commands except `docker compose` are run from the `demos/watcher/generic/`
directory:

```bash
cd demos/watcher/generic
```

### Step 1: Start the Redpanda stack

Run from the **repository root**:

```bash
docker compose -f demos/watcher/generic/docker-compose.redpanda.yaml up -d
```

Wait a few seconds for Redpanda to become ready. Verify it is up by opening the
Redpanda Console at `http://localhost:8081` or running:

```bash
docker exec redpanda rpk cluster health
docker exec redpanda rpk topic list
```

### Step 2: Start XZatoma in generic watcher mode

Open a terminal, change to `demos/watcher/generic/`, and run:

```bash
xzatoma --config config.yaml watch
```

XZatoma connects to Redpanda, subscribes to `atoma.plans`, and waits for plan
events. The startup banner shows the input and output topics.

### Step 3: Open a results terminal

Open a second terminal, change to `demos/watcher/generic/`, and run:

```bash
./read_results.sh
```

This tails the `atoma.results` topic. Result events appear here as XZatoma
finishes each plan, using the same JSON structure for success and failure.

### Step 4: Seed a plan event

Open a third terminal, change to `demos/watcher/generic/`, and run:

```bash
# Send the hello-world plan (action: greet)
./seed_plan.sh hello

# Send the system-health plan (action: report)
./seed_plan.sh health

# Send the doc-comment audit plan (action: audit, requires pyyaml)
./seed_plan.sh audit
```

XZatoma picks up the plan JSON, executes it, and publishes the result. Switch
back to the results terminal to see the `PlanResultEvent` appear.

### Step 5: Observe the output

Watch the XZatoma terminal for the agent transcript. When the plan finishes the
results terminal shows the published `PlanResultEvent`. Success and failure
results share the same fields; `status`, `summary`, and `iterations` carry the
outcome details:

```text
{
  "id": "01HZ...",
  "name": "hello-world",
  "action": "greet",
  "version": "1.0.0",
  "status": "success",
  "summary": "XZatoma generic watcher is alive ...",
  "iterations": 2,
  "completed_at": "2025-07-15T14:22:07Z"
}
```

Check the report file the plan writes:

```bash
cat tmp/hello-world-report.txt
cat tmp/system-health-report.txt
```

---

## Plan Presets

`seed_plan.sh` ships three built-in plan presets:

| Preset   | Plan name           | Action   | What it does                                                       |
| -------- | ------------------- | -------- | ------------------------------------------------------------------ |
| `hello`  | `hello-world`       | `greet`  | Prints a greeting, timestamps, writes `tmp/hello-world-report.txt` |
| `health` | `system-health`     | `report` | Runs system diagnostics, writes `tmp/system-health-report.txt`     |
| `audit`  | `doc-comment-audit` | `audit`  | Audits `src/` for missing doc comments (requires `pyyaml`)         |

Use `--stdin` to pipe in any plan JSON directly:

```bash
echo '{"name":"my-plan","action":"build","version":"1.0.0","tasks":[{"description":"Run: echo hello"}],"max_iterations":3}' \
  | ./seed_plan.sh --stdin
```

---

## Event Matching

The `config.yaml` includes commented-out `generic_match` examples. Uncomment
one to filter which plans XZatoma executes. Non-matching plans are committed
and skipped without running.

```yaml
# Execute only plans with action "report"
generic_match:
  action: "report"

# Execute only plans named "system-health" at version >=1.0.0
generic_match:
  name: "system-health"
  version: ">=1.0.0"

# Execute plans matching both name and action
generic_match:
  name: "system-health"
  action: "report"

# Execute plans matching name, version, and action (most specific)
generic_match:
  name: "system-health"
  version: ">=1.0.0"
  action: "report"
```

When no matcher is configured every plan event that arrives on the input topic
is executed.

---

## The `plans/` Directory

The YAML files under `plans/` are human-readable reference copies of the plan
payloads used by `seed_plan.sh`. They document the plan structure and can be
used as templates for custom plans.

| File                       | Plan name           | Action   |
| -------------------------- | ------------------- | -------- |
| `plans/hello_world.yaml`   | `hello-world`       | `greet`  |
| `plans/system_health.yaml` | `system-health`     | `report` |
| `plans/doc_audit.yaml`     | `doc-comment-audit` | `audit`  |

To send a custom plan from a YAML file (requires `pyyaml`):

```bash
python3 -c "import sys, json, yaml; print(json.dumps(yaml.safe_load(sys.stdin)))" \
  < plans/hello_world.yaml \
  | ./seed_plan.sh --stdin
```

---

## Using the Same Topic for Input and Output

Set `topic` and `output_topic` to the same value in the config to create a
closed loop where plans and results share one topic. Use a `generic_match` to
avoid executing result events. Result events have no `action` field:

```yaml
kafka:
  topic: "atoma.work"
  output_topic: "atoma.work"
generic_match:
  action: "run"
```

Only messages with `action: "run"` trigger execution. Result events published
by XZatoma have no `action` field and are skipped by the matcher.

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

## Configuration Files

| File                         | Provider | Model         |
| ---------------------------- | -------- | ------------- |
| `config.yaml`                | Ollama   | `granite4:3b` |
| `config_ollama_granite.yaml` | Ollama   | `granite4:3b` |

Both configs set `watcher_type: generic` and configure the `kafka` section with
`topic: "atoma.plans"` and `output_topic: "atoma.results"`. Running from
`demos/watcher/generic/` is recommended but not required.

## Files in This Demo

| File                           | Purpose                                                    |
| ------------------------------ | ---------------------------------------------------------- |
| `config.yaml`                  | Generic watcher configuration using Ollama `granite4:3b`   |
| `config_ollama_granite.yaml`   | Generic watcher configuration using Ollama `granite4:3b`   |
| `docker-compose.redpanda.yaml` | Redpanda single-broker stack for local development         |
| `seed_plan.sh`                 | Publishes a plan JSON event to the Redpanda input topic    |
| `read_results.sh`              | Reads result events from the Redpanda output topic         |
| `plans/hello_world.yaml`       | Reference plan payload: greet action                       |
| `plans/system_health.yaml`     | Reference plan payload: report action                      |
| `plans/doc_audit.yaml`         | Reference plan payload: audit action (subagent delegation) |

---

## Further Reading

- `demos/watcher/README.md` — overview of both watcher modes
- `demos/watcher/xzepr/` — the XZepr watcher demo
- `docs/how-to/watcher_demo.md` — combined how-to guide for both watcher modes
- `docs/reference/architecture.md` — overall XZatoma architecture
