# Watcher Demos

This directory contains two fully self-contained watcher demos for XZatoma.
Each demo lives in its own subdirectory with its own configuration, scripts,
Redpanda stack, and README.

---

## Demos

### [`generic/`](generic/) — Generic Watcher Demo

XZatoma consumes **Plan JSON events** directly from a Redpanda topic, executes
each plan autonomously through the configured AI provider, and publishes a
`PlanResultEvent` back to an output topic.

- No XZepr or Janus Gatekeeper infrastructure required
- Three built-in plan presets: `hello` (greet), `health` (report), `audit`
  (subagent doc-comment audit)
- Event matching via `generic_match` (filter on `action`, `name`, `version`)

**Quick start:**

```sh
cd generic
docker compose -f docker-compose.redpanda.yaml up -d
xzatoma --config config.yaml watch
# In a second terminal:
./seed_plan.sh hello
```

---

### [`xzepr/`](xzepr/) — XZepr Watcher Demo

XZatoma consumes **XZepr CloudEvents** (CloudEvents 1.0.1) from a Redpanda
topic, extracts the plan embedded in each event payload, executes it, and
publishes a result event to an output topic.

- Designed for XZepr software supply chain event pipelines
- Two built-in event presets: `build` (`build.success`) and `deploy`
  (`deployment.success`)
- Event filtering via `filters` (by `event_types`, `source_pattern`,
  `platform_id`, `package`, `api_version`, `success_only`)

**Quick start:**

```sh
cd xzepr
docker compose -f docker-compose.redpanda.yaml up -d
xzatoma --config config.yaml watch
# In a second terminal:
./seed_event.sh build
```

---

## Shared Infrastructure

Both demos use the same Redpanda single-broker stack defined in their local
`docker-compose.redpanda.yaml`. The Redpanda container is named `redpanda` and
exposes:

| Port  | Service                          |
| ----- | -------------------------------- |
| 19092 | Kafka API (external / host)      |
| 9092  | Kafka API (internal / container) |
| 8081  | Redpanda Console (web UI)        |
| 18081 | Schema Registry                  |
| 18082 | HTTP Proxy                       |

Scripts in each demo use `docker exec redpanda rpk ...` (the internal port
`9092`) for topic administration and event injection.

XZatoma itself connects from the host via the external port `localhost:19092`.

---

## Prerequisites

| Requirement                                | Notes                                                 |
| ------------------------------------------ | ----------------------------------------------------- |
| Docker with Compose                        | Used to run Redpanda                                  |
| Ollama running at `http://localhost:11434` | `ollama pull granite4:3b`                             |
| `xzatoma` binary on PATH                   | `cargo build --release && cargo install --path .`     |
| `jq` (optional)                            | Pretty-prints result events                           |
| `ulid` (optional, generic demo only)       | `go install github.com/oklog/ulid/v2/cmd/ulid@latest` |

---

## Further Reading

- [`generic/README.md`](generic/README.md) — full generic watcher walkthrough
- [`xzepr/README.md`](xzepr/README.md) — full XZepr watcher walkthrough
- `docs/how-to/watcher_demo.md` — combined how-to guide for both watcher modes
- `docs/reference/architecture.md` — overall XZatoma architecture
