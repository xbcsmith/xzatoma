# XZatoma Feature Improvements Implementation Plan

## Overview

This plan implements 27 feature improvements across five phases, ordered by
risk, dependency, and priority. Phase 1 addresses correctness defects and
foundational infrastructure that every later phase depends on. Phases 2 through
4 add progressively higher-level capabilities. Phase 5 completes configuration
ergonomics and polish items.

XZatoma-specific extensions (XZepr watcher, `--dry-run`, `replay`,
`parallel_subagent`, `activate_skill`, extended mention parser,
`acp.compatibility_mode`, `acp.auth_token`, `WatcherPlanExecutionMode`) are
preserved in their current form and are not modified by this plan.

---

## Current State Analysis

### Existing Infrastructure

- **Watcher backends**: Two equal-peer backends exist -- `src/watcher/xzepr/`
  and `src/watcher/generic/`. Both share `lifecycle.rs`, `plan_executor.rs`,
  `logging.rs`, `kafka_security.rs`, and `topic_admin.rs` via
  `src/watcher/mod.rs`.
- **ACP server**: A full ACP HTTP server lives in `src/acp/` with session
  management (`session.rs`, `session_mode.rs`), an executor (`executor.rs`),
  streaming (`streaming.rs`), and a complete SQLite-backed storage layer
  (`src/storage/`).
- **MCP approval**: `src/mcp/approval.rs` exports `should_auto_approve` which
  always returns `false`, blocking headless `run` and `watch` execution. The
  module also exports the policy-based `approval_decision` function which
  handles per-server trust decisions correctly but is not wired into the
  sampling path.
- **Configuration**: `src/config.rs` carries `AcpConfig`, `KafkaWatcherConfig`,
  `OllamaConfig`, `CopilotConfig`, and `AgentConfig`, each missing several
  production fields documented in the gap analysis below.
- **Tools**: `src/tools/` contains all current XZatoma tools but is missing the
  ACP inter-agent tools (`call_acp_agent`, `discover_acp_agents`,
  `await_input`).
- **Command modules**: The `run`, `chat`, and `watch` command handlers are
  inline modules defined within `src/commands/mod.rs` (as `pub mod r#run`,
  `pub mod chat`, and `pub mod watch`). There are no separate
  `src/commands/run.rs`, `src/commands/chat.rs`, or `src/commands/watch.rs`
  files.

### Identified Issues

1. **Correctness bug**: `should_auto_approve` always returns `false`. Headless
   `run` and `watch` commands cannot auto-approve MCP tool calls, making them
   non-functional in practice when MCP servers are configured.
2. **Operational risk**: No `BackOffPolicy` or `CircuitBreaker` means sustained
   downstream failures hammer the watcher with rapid retry loops and unbounded
   error bursts.
3. **Kafka consumer eviction**: Missing `max_poll_interval_ms` causes the
   rdkafka consumer to be evicted from its consumer group mid-plan on
   long-running plans.
4. **Missing production ACP controls**: No concurrency limits, run timeouts,
   CORS support, or graceful shutdown leaves the ACP HTTP server unsuitable for
   production deployment.
5. **No multi-agent capability**: Without `acp.agents`, `acp.client`,
   `call_acp_agent`, and `discover_acp_agents`, XZatoma cannot participate in
   federated multi-agent workflows.
6. **Noisy startup logs**: No `QuietStartupContext` causes a burst of
   `WARN`/`ERROR` log lines during normal Kafka consumer startup while brokers
   are probed.

---

## Implementation Phases

---

### Phase 1: Watcher Resilience and MCP Correctness

**Goal**: Fix the two correctness defects and add the foundational
infrastructure that later watcher phases build on. Nothing in Phase 2 or 3 is
safe to ship without this work.

#### Task 1.1 Fix MCP Auto-Approval (Gap 9)

**File**: `src/mcp/approval.rs`

Update `should_auto_approve` to return `true` when either argument satisfies the
headless auto-approval condition:

```text
headless || execution_mode == ExecutionMode::FullAutonomous
```

The current implementation ignores both parameters and always returns `false`.
The correct behavior is:

| `execution_mode` | `headless` | Expected result |
| ---------------- | ---------- | --------------- |
| `FullAutonomous` | `false`    | `true`          |
| `FullAutonomous` | `true`     | `true`          |
| Any other mode   | `true`     | `true`          |
| Any other mode   | `false`    | `false`         |

Thread a `headless: bool` parameter through the `run` and `watch` command paths
inside `src/commands/mod.rs` (modules `r#run` and `watch`), setting it to `true`
in both. Thread `headless = false` through the `chat` module in the same file.
Update all call sites in `src/commands/mod.rs` accordingly.

The existing test
`test_should_auto_approve_legacy_full_autonomous_returns_false` and
`test_should_auto_approve_legacy_headless_returns_false` in
`src/mcp/approval.rs` must be replaced with tests that assert the new correct
behavior (see Task 1.6).

#### Task 1.2 Add Exponential Back-Off Policy (Gap 1b)

**File**: `src/watcher/mod.rs`

Add `BackOffPolicy` as a plain `struct` with no external crate dependency beyond
`std::time`. Use an initial delay of 500 ms and a 30-second cap. Apply it in
both the XZepr watcher loop (`src/watcher/xzepr/`) and the generic watcher loop
(`src/watcher/generic/`): increment the delay on each consecutive Kafka error,
reset to the initial value after any successful event.

Export `BackOffPolicy` from `src/watcher/mod.rs`.

#### Task 1.3 Add Circuit Breaker (Gap 1a)

**New file**: `src/watcher/circuit_breaker.rs`

Define:

- `CircuitState` enum with variants `Closed`, `Open`, `HalfOpen`
- `CircuitBreakerConfig` struct: `failure_threshold: u32` (default `5`),
  `reset_timeout_secs: u64` (default `60`)
- `CircuitBreaker` struct implementing three-state transition logic:
  - `Closed` to `Open` after `failure_threshold` consecutive failures
  - `Open` to `HalfOpen` after `reset_timeout_secs` seconds
  - `HalfOpen` to `Closed` on the next success; back to `Open` on the next
    failure

Wire `CircuitBreaker` into the XZepr watcher's event-receipt and event-posting
path. Export the new types from `src/watcher/mod.rs`.

#### Task 1.4 Add Kafka `max_poll_interval_ms` Config Field (Gap 2d)

**File**: `src/config.rs`

Add `max_poll_interval_ms: u64` with
`#[serde(default = "default_max_poll_interval_ms")]` and
`fn default_max_poll_interval_ms() -> u64 { 3_600_000 }` to
`KafkaWatcherConfig`.

Apply it as the rdkafka consumer config key `max.poll.interval.ms` in both
watcher backends during consumer construction.

Update `KafkaWatcherConfig::default()` to include the new field.

#### Task 1.5 Add Startup Stabilization Context (Gaps 1c and 2c)

**New file**: `src/watcher/startup_context.rs`

Implement `QuietStartupContext` as a struct that implements rdkafka's
`ClientContext`. During the first `startup_stabilization_secs` seconds after
construction, downgrade broker-probe connectivity errors and rdkafka log
callbacks to `DEBUG` level. After the deadline elapses, resume normal log
severity.

**File**: `src/config.rs`

Add `startup_stabilization_secs: u64` with
`#[serde(default = "default_startup_stabilization_secs")]` and
`fn default_startup_stabilization_secs() -> u64 { 10 }` to `KafkaWatcherConfig`.

Pass the configured value to `QuietStartupContext` when constructing the rdkafka
consumer in both watcher backends.

Update `KafkaWatcherConfig::default()` to include the new field.

#### Task 1.6 Testing Requirements

Write unit tests in the same file as each changed module (using `#[cfg(test)]`
blocks):

- **`BackOffPolicy`**: verify delay starts at 500 ms, caps at 30 seconds on
  repeated failures, and resets to 500 ms on the first success.
- **`CircuitBreaker`**: test each of the three state transitions explicitly:
  `Closed` to `Open` after 5 failures; `Open` to `HalfOpen` after 60 seconds;
  `HalfOpen` to `Closed` on success; `HalfOpen` back to `Open` on failure.
- **`should_auto_approve`**: replace the two existing `_returns_false` tests
  with four new tests:
  - `test_should_auto_approve_full_autonomous_non_headless_returns_true`
  - `test_should_auto_approve_full_autonomous_headless_returns_true`
  - `test_should_auto_approve_headless_any_mode_returns_true`
  - `test_should_auto_approve_interactive_non_headless_returns_false`
- **`QuietStartupContext`**: verify that within the stabilization window,
  error-level rdkafka callbacks produce `DEBUG` output; verify that after the
  window, they produce `ERROR` output.
- All new config fields must have `#[serde(default)]` pointing to a named
  default function, and each default function must include a `///` doc comment
  with an inline doc test verifying the default value.

#### Task 1.7 Deliverables

- [ ] `src/mcp/approval.rs` -- `should_auto_approve` implements correct
      headless/autonomous logic; obsolete tests replaced
- [ ] `src/commands/mod.rs` -- `headless = true` threaded through `r#run` and
      `watch` modules; `headless = false` threaded through `chat` module
- [ ] `src/watcher/mod.rs` -- `BackOffPolicy` struct added and exported
- [ ] `src/watcher/xzepr/` -- `BackOffPolicy` and `CircuitBreaker` applied to
      the event loop
- [ ] `src/watcher/generic/` -- `BackOffPolicy` applied to the event loop
- [ ] `src/watcher/circuit_breaker.rs` -- new file with `CircuitState`,
      `CircuitBreakerConfig`, and `CircuitBreaker`
- [ ] `src/watcher/startup_context.rs` -- new file with `QuietStartupContext`
- [ ] `src/config.rs` -- `max_poll_interval_ms` and `startup_stabilization_secs`
      added to `KafkaWatcherConfig` with defaults and doc tests
- [ ] All new unit tests passing

#### Task 1.8 Success Criteria

- `cargo fmt --all` produces no diff.
- `cargo clippy --all-targets --all-features -- -D warnings` reports zero
  warnings on all affected files.
- `cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth`
  passes with no failures.
- `should_auto_approve(ExecutionMode::FullAutonomous, false)` returns `true`.
- `should_auto_approve(ExecutionMode::FullAutonomous, true)` returns `true`.
- `should_auto_approve(ExecutionMode::Interactive, true)` returns `true`.
- `should_auto_approve(ExecutionMode::Cautious, false)` returns `false`.
- Circuit breaker transitions from `Closed` to `Open` after exactly 5
  consecutive failures.
- Circuit breaker transitions from `Open` to `HalfOpen` after 60 seconds.
- `BackOffPolicy` never sleeps longer than 30 seconds regardless of failure
  count.

---

### Phase 2: Watcher CLI and Configuration Completeness

**Goal**: Surface all remaining Kafka consumer configuration options and add the
two missing `watch` command flags. Requires Phase 1 (`startup_context.rs` and
`BackOffPolicy` present, `KafkaWatcherConfig` already extended with
`max_poll_interval_ms` and `startup_stabilization_secs`).

#### Task 2.1 Add `--once` Flag to `watch` Command (Gap 2a)

**Files**: `src/cli.rs`, `src/commands/mod.rs` (module `watch`)

Add `--once: bool` to `Commands::Watch` in `src/cli.rs`. Propagate it into
`WatchCliOverrides` and through `run_watch` in `src/commands/mod.rs`. In both
watcher backend loops (`src/watcher/xzepr/` and `src/watcher/generic/`), break
the consume loop after the first successfully processed event when
`once = true`.

This flag enables single-event processing for CI pipelines and smoke tests
without requiring a configuration file change.

#### Task 2.2 Add `--allow-dangerous` Flag to `watch` Command (Gap 2b)

**Files**: `src/cli.rs`, `src/commands/mod.rs` (module `watch`)

Add `--allow-dangerous: bool` to `Commands::Watch` in `src/cli.rs`. Wire it
through `WatchCliOverrides` into `run_watch` to override
`watcher.execution.allow_dangerous`. This allows operators to unlock dangerous
terminal commands for a single run via a command-line flag rather than a
config-file edit.

#### Task 2.3 Add Remaining Kafka Config Fields (Gaps 2e and 2f)

**File**: `src/config.rs`

In `KafkaWatcherConfig`:

- Add `auto_offset_reset: Option<String>` with `#[serde(default)]` (default
  `None`). When `Some(value)`, apply `value` as the rdkafka consumer key
  `auto.offset.reset` during consumer construction. When `None`, the rdkafka
  default of `"latest"` is preserved.

**File**: `src/watcher/kafka_security.rs`

In `KafkaSecurityConfig`:

- Add `ssl_ca_location: Option<String>` with `#[serde(default)]`. When `Some`,
  apply it as the rdkafka config key `ssl.ca.location` only when `protocol` is
  `"SSL"` or `"SASL_SSL"`. When the protocol is `"PLAINTEXT"` or
  `"SASL_PLAINTEXT"`, this field is silently ignored.

Update both watcher backends to apply the new config keys at consumer
construction time.

#### Task 2.4 Testing Requirements

- **`--once` flag**: write a unit test verifying the watcher loop exits after
  processing exactly one event and does not process a second event placed in the
  channel.
- **`auto_offset_reset`**: unit test that `Some("earliest")` produces
  `auto.offset.reset = earliest` in the rdkafka config map; `None` produces no
  `auto.offset.reset` key.
- **`ssl_ca_location`**: unit test that the key appears in the rdkafka config
  map for `SSL` and `SASL_SSL` protocols; unit test that it is absent for
  `PLAINTEXT` and `SASL_PLAINTEXT` protocols.
- All existing `watch` command tests must continue to pass without modification.

#### Task 2.5 Deliverables

- [ ] `src/cli.rs` -- `--once` and `--allow-dangerous` flags on
      `Commands::Watch`
- [ ] `src/commands/mod.rs` (module `watch`) -- `--once` breaks the consume loop
      after one event; `--allow-dangerous` overrides the execution mode
- [ ] `src/config.rs` -- `auto_offset_reset: Option<String>` added to
      `KafkaWatcherConfig`
- [ ] `src/watcher/kafka_security.rs` -- `ssl_ca_location: Option<String>` added
      to `KafkaSecurityConfig`
- [ ] Both watcher backends updated to apply `auto_offset_reset` and
      `ssl_ca_location` at consumer construction
- [ ] All new unit tests passing

#### Task 2.6 Success Criteria

- `cargo fmt --all` produces no diff.
- `cargo clippy --all-targets --all-features -- -D warnings` reports zero
  warnings on all affected files.
- `cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth`
  passes with no failures.
- Running `xzatoma watch --once` with a single event available processes that
  event and exits with code `0`.
- Running `xzatoma watch --allow-dangerous` overrides
  `watcher.execution.allow_dangerous` to `true` for that invocation.
- `KafkaWatcherConfig { auto_offset_reset: Some("earliest".to_string()), .. }`
  produces an rdkafka consumer whose config map contains
  `auto.offset.reset = earliest`.
- `KafkaSecurityConfig` with `ssl_ca_location = Some(path)` and
  `protocol = "SSL"` applies `ssl.ca.location` to the rdkafka config map.
- `KafkaSecurityConfig` with `ssl_ca_location = Some(path)` and
  `protocol = "PLAINTEXT"` does not apply `ssl.ca.location`.

---

### Phase 3: ACP Server Production Controls

**Goal**: Bring the ACP HTTP server to production readiness by adding
concurrency limits, run timeouts, CORS support, graceful shutdown, session mode
isolation, and the `allow_dangerous` gate. These changes are independent of
multi-agent tooling (Phase 4) and can ship as a standalone release.

#### Task 3.1 Add ACP Session Mode (Gap 3a)

**File**: `src/config.rs`

Add:

```text
pub enum AcpSessionMode { Isolated, Shared }
```

with `#[serde(rename_all = "snake_case")]` and `Default = Isolated`. Add
`session_mode: AcpSessionMode` to `AcpConfig` with `#[serde(default)]`.

**File**: `src/acp/executor.rs`

In `AcpRunExecutor`:

- `Shared` mode: load prior conversation history from `SqliteStorage` using the
  session identifier before each run, and save the updated history after the run
  completes.
- `Isolated` mode: behavior is unchanged (no history loaded or saved across
  runs).

Update `AcpConfig::default()` to include
`session_mode: AcpSessionMode::Isolated`.

#### Task 3.2 Add ACP `allow_dangerous` (Gap 3b)

**File**: `src/config.rs`

Add `allow_dangerous: bool` with `#[serde(default)]` (default `false`) to
`AcpConfig`.

**File**: `src/acp/executor.rs`

Apply `allow_dangerous` to the `ExecutionMode` used when constructing the tool
registry inside `AcpRunExecutor`. When `true`, the tool registry permits
dangerous terminal commands.

Update `AcpConfig::default()` to include `allow_dangerous: false`.

#### Task 3.3 Add Run Concurrency Limit (Gap 3c)

**File**: `src/config.rs`

Add `max_concurrent_runs: usize` with
`#[serde(default = "default_acp_max_concurrent_runs")]` and
`fn default_acp_max_concurrent_runs() -> usize { 4 }` to `AcpConfig`.

**File**: `src/acp/server.rs`

Add a `tokio::sync::Semaphore` to `AcpServerState`, sized to
`config.acp.max_concurrent_runs`. Acquire a permit before spawning each run. If
no permit is available, return HTTP `429 Too Many Requests` from the
`POST /runs` handler immediately without queuing.

Update `AcpConfig::default()` to include `max_concurrent_runs: 4`.

#### Task 3.4 Add Per-Run Timeout (Gap 3d)

**File**: `src/config.rs`

Add `run_timeout_seconds: u64` with
`#[serde(default = "default_acp_run_timeout_seconds")]` and
`fn default_acp_run_timeout_seconds() -> u64 { 300 }` to `AcpConfig`. A value of
`0` disables the timeout.

**File**: `src/acp/executor.rs`

In `AcpRunExecutor::execute()`, when `config.acp.run_timeout_seconds > 0`, wrap
the execution future in `tokio::time::timeout` using the configured duration. On
timeout, mark the run as `Failed` with the reason string
`"run exceeded configured timeout of {n} seconds"`.

Update `AcpConfig::default()` to include `run_timeout_seconds: 300`.

#### Task 3.5 Add Session Eviction Timeout (Gap 3e)

**File**: `src/config.rs`

Add two fields to `AcpConfig`:

- `session_timeout_seconds: u64` with default `3600` -- a `Shared` session idle
  for longer than this value is eligible for eviction.
- `session_eviction_poll_seconds: u64` with default `60` -- how often the
  background eviction task wakes to scan for expired sessions.

**File**: `src/acp/server.rs`

Spawn a background `tokio` task during `run_server` startup that:

1. Sleeps for `session_eviction_poll_seconds`.
2. Acquires the `AcpServerState` lock.
3. Removes all sessions whose last-activity timestamp is older than
   `session_timeout_seconds`.
4. Repeats indefinitely until the server shuts down.

Update `AcpConfig::default()` to include both new fields.

#### Task 3.6 Add CORS Origins (Gap 3f)

**File**: `src/config.rs`

Add `cors_origins: Vec<String>` with `#[serde(default)]` (default empty `Vec`)
to `AcpConfig`.

**File**: `src/acp/server.rs`

In the axum router setup, attach a `tower_http::cors::CorsLayer` configured with
`cors_origins`. An empty `cors_origins` list must deny all cross-origin requests
(preserving current behavior as the default).

Add `tower-http` with the `cors` feature to `Cargo.toml` if not already present.

Update `AcpConfig::default()` to include `cors_origins: vec![]`.

#### Task 3.7 Add Graceful Shutdown Timeout (Gap 3g)

**File**: `src/config.rs`

Add `graceful_shutdown_timeout: Option<u64>` with `#[serde(default)]` (default
`None`) to `AcpConfig`. `None` means drain indefinitely; `Some(n)` means wait at
most `n` seconds before forcing exit.

**File**: `src/acp/server.rs`

Pass the value to the axum server's graceful shutdown future. When `Some(n)`,
use `tokio::time::timeout(Duration::from_secs(n), shutdown_signal)` to bound the
drain period.

Update `AcpConfig::default()` to include `graceful_shutdown_timeout: None`.

#### Task 3.8 Testing Requirements

All tests in this phase must use `AcpRuntime::new_in_memory()`. Never use
`AcpRuntime::new()` in tests.

- **`AcpSessionMode` serialization**: unit test that `"isolated"` deserializes
  to `AcpSessionMode::Isolated` and `"shared"` deserializes to
  `AcpSessionMode::Shared`.
- **Semaphore limit**: unit test that when `max_concurrent_runs = 1`, a second
  concurrent `POST /runs` returns HTTP `429` without starting a second run.
- **Run timeout**: unit test that a run exceeding `run_timeout_seconds` is
  marked `Failed` with a reason string containing the configured timeout
  duration.
- **CORS empty list**: unit test that `cors_origins = []` causes a cross-origin
  preflight to be rejected.
- **CORS configured origin**: unit test that a configured origin receives the
  correct `Access-Control-Allow-Origin` header.
- **Shared session history**: integration test using
  `AcpRuntime::new_in_memory()` that submits two successive runs on the same
  session identifier and verifies that the second run can read conversation
  history produced by the first run.

#### Task 3.9 Deliverables

- [ ] `src/config.rs` -- `AcpSessionMode` enum; `session_mode`,
      `allow_dangerous`, `max_concurrent_runs`, `run_timeout_seconds`,
      `session_timeout_seconds`, `session_eviction_poll_seconds`,
      `cors_origins`, `graceful_shutdown_timeout` added to `AcpConfig` with
      defaults and doc tests
- [ ] `src/acp/executor.rs` -- `session_mode` history loading/saving and
      `allow_dangerous` tool registry gate applied
- [ ] `src/acp/server.rs` -- semaphore gate on `POST /runs`, per-run timeout
      wrapper, `CorsLayer`, graceful shutdown bound, session eviction background
      task
- [ ] `Cargo.toml` -- `tower-http` `cors` feature added if absent
- [ ] All new unit and integration tests passing

#### Task 3.10 Success Criteria

- `cargo fmt --all` produces no diff.
- `cargo clippy --all-targets --all-features -- -D warnings` reports zero
  warnings on all affected files.
- `cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth`
  passes with no failures.
- `AcpConfig` deserializes all nine new fields from YAML with the correct
  defaults when the fields are absent from the YAML.
- `max_concurrent_runs = 1` correctly rejects the second concurrent run with
  HTTP `429`.
- A run that runs longer than `run_timeout_seconds` is marked `Failed` with a
  descriptive reason string.
- `Shared` session mode persists conversation history across two successive runs
  on the same session identifier; `Isolated` mode does not.
- The session eviction task removes sessions idle for longer than
  `session_timeout_seconds`; the wakeup interval matches
  `session_eviction_poll_seconds`.
- A CORS preflight with a configured `cors_origins` entry returns the correct
  `Access-Control-Allow-Origin` header.

---

### Phase 4: ACP Multi-Agent Infrastructure

**Goal**: Enable federated multi-agent workflows by adding per-agent
configuration, an outbound ACP client, and the three inter-agent tools. Depends
on Phase 3: the ACP server state, tool registry infrastructure, and
`AcpRuntime::new_in_memory()` isolation pattern must all be in place.

#### Task 4.1 Add Multi-Agent Configuration (Gap 3h)

**File**: `src/config.rs`

Add `AcpAgentConfig` struct with
`#[derive(Debug, Clone, Serialize, Deserialize)]`:

| Field                  | Type                   | Default  | Notes                          |
| ---------------------- | ---------------------- | -------- | ------------------------------ |
| `name`                 | `String`               | required | Must be a valid RFC 1123 label |
| `description`          | `String`               | `""`     |                                |
| `provider`             | `Option<ProviderType>` | `None`   | Overrides global provider      |
| `input_content_types`  | `Vec<String>`          | `[]`     |                                |
| `output_content_types` | `Vec<String>`          | `[]`     |                                |
| `thinking_mode`        | `Option<String>`       | `None`   | Overrides global thinking mode |
| `system_prompt`        | `Option<String>`       | `None`   | Overrides global system prompt |

Add `agents: Vec<AcpAgentConfig>` with `#[serde(default)]` to `AcpConfig`.

Implement method on `AcpConfig`:

```text
pub fn effective_agents(&self, provider_type: ProviderType) -> Vec<AcpAgentConfig>
```

When `agents` is empty, return a single synthesised entry: `name = "xzatoma"`,
`provider = Some(provider_type)`, all other fields set to their defaults. When
`agents` is non-empty, return the list as-is.

**File**: `src/acp/executor.rs`

Wire `provider`, `system_prompt`, and `thinking_mode` overrides from the
matching `AcpAgentConfig` entry into `AcpRunExecutor` based on which named agent
a run targets.

Update `AcpConfig::default()` to include `agents: vec![]`.

#### Task 4.2 Add Outbound ACP Client Configuration (Gap 3i)

**File**: `src/config.rs`

Add `AcpClientConfig` struct:

| Field                     | Type          | Default | Notes                                      |
| ------------------------- | ------------- | ------- | ------------------------------------------ |
| `default_timeout_seconds` | `u64`         | `30`    | `0` disables inter-agent tool registration |
| `allowed_base_urls`       | `Vec<String>` | `[]`    | SSRF allow-list; empty list blocks all     |

Add `client: AcpClientConfig` with `#[serde(default)]` to `AcpConfig`.

**File**: `src/tools/registry_builder.rs`

Gate registration of `call_acp_agent` and `discover_acp_agents` on
`config.acp.client.default_timeout_seconds > 0`. When the value is `0`, these
tools are not added to the registry.

Update `AcpConfig::default()` to include `client: AcpClientConfig::default()`.

#### Task 4.3 Implement `call_acp_agent` Tool (Gap 4a)

**New file**: `src/tools/acp_agent.rs`

Implement `AcpAgentTool` accepting parameters:

| Parameter | Type     | Required | Description                       |
| --------- | -------- | -------- | --------------------------------- |
| `url`     | `String` | yes      | Base URL of the remote ACP server |
| `input`   | `String` | yes      | Message text to send              |
| `mode`    | `String` | yes      | `"sync"` or `"async"`             |

**Sync mode** (`mode = "sync"`):

1. Validate `url` against `acp.client.allowed_base_urls`. Return a tool error
   immediately if the URL is not in the allow-list; do not make any network
   call.
2. Call `POST {url}/runs` with the input message. Enforce
   `acp.client.default_timeout_seconds` on this individual HTTP call.
3. Poll `GET {url}/runs/{run_id}` until the run reaches a terminal state
   (`Completed`, `Failed`, or `Cancelled`). Each poll request is bounded by
   `acp.client.default_timeout_seconds`.
4. Return the full run output on success, or a tool error on failure.

**Async mode** (`mode = "async"`):

1. Validate `url` against `acp.client.allowed_base_urls` as above.
2. Call `POST {url}/runs` with the input message.
3. Return the `run_id` immediately without polling. The caller is responsible
   for checking run status.

Both modes must propagate all errors as tool errors (type `XzatomaError::Tool`).
Do not use `panic!`, `unwrap()`, or `expect()` without a documented
justification.

Register in `src/tools/registry_builder.rs` when
`acp.client.default_timeout_seconds > 0`.

#### Task 4.4 Implement `discover_acp_agents` Tool (Gap 4b)

**New file**: `src/tools/acp_discover.rs`

Implement `DiscoverAcpAgentsTool` accepting a single `url: String` parameter.

1. Validate `url` against `acp.client.allowed_base_urls`. Return a tool error if
   the URL is not in the allow-list; do not make any network call.
2. Call `GET {url}/agents`. Enforce `acp.client.default_timeout_seconds` on the
   HTTP call.
3. Return the agent list as structured JSON.

Apply the same SSRF allow-list and timeout guards as `call_acp_agent`.

Register alongside `call_acp_agent` in `src/tools/registry_builder.rs` when
`acp.client.default_timeout_seconds > 0`.

#### Task 4.5 Implement `await_input` Tool and `Awaiting` State (Gap 4c)

**File**: `src/acp/types.rs`

Add `Awaiting` variant to `AcpRunState`. Update all match expressions that
handle `AcpRunState` to include the new variant.

**New file**: `src/tools/await_input.rs`

Implement `AwaitInputTool`. The tool is registered only when the run has a live
`RunHandle`. On invocation:

1. Transition the run state from `Running` to `Awaiting`.
2. Block on a `tokio::sync::oneshot::Receiver` until a resume payload arrives.
3. Return the resume payload as the tool result.

**File**: `src/acp/server.rs`

In the `POST /runs/{run_id}` handler (or a dedicated resume endpoint), when the
run is in `Awaiting` state:

1. Deliver the request payload to the `oneshot::Sender`.
2. Transition the run state from `Awaiting` back to `Running`.

**File**: `src/storage/`

Ensure the `Awaiting` state is handled in all SQLite persistence code using
`CREATE TABLE IF NOT EXISTS` patterns (consistent with the existing storage
initialisation convention; no migration runner is required).

#### Task 4.6 Testing Requirements

All tests in this phase must use `AcpRuntime::new_in_memory()`. Never use
`AcpRuntime::new()` in tests.

- **`effective_agents` empty list**: unit test that an empty `agents` list
  returns exactly one synthesised entry with `name = "xzatoma"` and
  `provider = Some(provider_type)`.
- **`effective_agents` non-empty list**: unit test that a non-empty `agents`
  list is returned as-is.
- **`call_acp_agent` URL validation**: unit test that a URL not present in
  `allowed_base_urls` returns a tool error without making any network call.
  Verify by confirming no HTTP client is instantiated (use a mock or inspection
  approach rather than a live network).
- **`call_acp_agent` sync mode**: unit test using a mock HTTP server that
  returns a completed run on the first poll; verify the tool returns the run
  output.
- **`call_acp_agent` async mode**: unit test using a mock HTTP server that
  returns a `run_id`; verify the tool returns the `run_id` immediately without
  polling.
- **`discover_acp_agents` URL validation**: same allow-list rejection test as
  for `call_acp_agent`.
- **`discover_acp_agents` success path**: unit test using a mock HTTP server
  that returns an agent list; verify the tool returns correct JSON.
- **`await_input` round-trip**: unit test that an `await_input` tool call
  transitions the run to `Awaiting`; a subsequent resume call delivers the
  payload and transitions the run back to `Running`.

#### Task 4.7 Deliverables

- [ ] `src/config.rs` -- `AcpAgentConfig` struct, `agents: Vec<AcpAgentConfig>`,
      `AcpClientConfig` struct, `client: AcpClientConfig` added to `AcpConfig`;
      `effective_agents` method implemented
- [ ] `src/acp/executor.rs` -- per-agent provider, system-prompt, and
      thinking-mode overrides applied based on named agent target
- [ ] `src/tools/acp_agent.rs` -- new file; `AcpAgentTool` with sync and async
      modes
- [ ] `src/tools/acp_discover.rs` -- new file; `DiscoverAcpAgentsTool`
- [ ] `src/tools/await_input.rs` -- new file; `AwaitInputTool`
- [ ] `src/tools/registry_builder.rs` -- three tools registered conditionally on
      `acp.client.default_timeout_seconds > 0`
- [ ] `src/acp/types.rs` -- `AcpRunState::Awaiting` variant added; all match
      expressions updated
- [ ] `src/acp/server.rs` -- resume path for `Awaiting` runs implemented
- [ ] `src/storage/` -- `Awaiting` state handled in all persistence code
- [ ] All new unit and mock-server tests passing

#### Task 4.8 Success Criteria

- `cargo fmt --all` produces no diff.
- `cargo clippy --all-targets --all-features -- -D warnings` reports zero
  warnings on all affected files.
- `cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth`
  passes with no failures.
- `effective_agents()` returns a synthesised `"xzatoma"` default entry when
  `agents` is empty.
- `call_acp_agent` rejects a URL not in `allowed_base_urls` without making any
  network call, returning `XzatomaError::Tool`.
- `call_acp_agent` with `mode = "sync"` returns run output after polling
  completes.
- `call_acp_agent` with `mode = "async"` returns a `run_id` string without
  polling.
- An `await_input` invocation transitions the run to `Awaiting`; a subsequent
  resume call transitions it back to `Running` and delivers the payload.
- `call_acp_agent` and `discover_acp_agents` are absent from the tool registry
  when `acp.client.default_timeout_seconds = 0`.

---

### Phase 5: Provider Config, Agent Config, History, and Scoped Validation

**Goal**: Close all remaining low-to-medium priority improvements. These are
self-contained changes with no cross-phase dependencies and can be implemented
in any order within the phase.

#### Task 5.1 Add Ollama `num_ctx` (Gap 5a)

**File**: `src/config.rs`

Add `num_ctx: Option<u32>` with `#[serde(default)]` (default `None`) to
`OllamaConfig`. Update `OllamaConfig::default()` to include the field.

**File**: `src/providers/ollama.rs`

When `config.ollama.num_ctx` is `Some(n)`, include `"options": {"num_ctx": n}`
in the JSON body of every Ollama chat completion request. When `None`, omit the
`options` key entirely.

#### Task 5.2 Add Copilot `editor_version` and `initiator` (Gap 5b)

**File**: `src/config.rs`

Add to `CopilotConfig`:

- `editor_version: String` with
  `#[serde(default = "default_copilot_editor_version")]` and
  `fn default_copilot_editor_version() -> String { "vscode/1.95.0".to_string() }`
- `initiator: String` with `#[serde(default = "default_copilot_initiator")]` and
  `fn default_copilot_initiator() -> String { "agent".to_string() }`

Update `CopilotConfig::default()` to include both fields.

**File**: `src/providers/copilot.rs`

Apply the two values as HTTP request headers on all outbound Copilot API
requests:

- `Editor-Version: {editor_version}`
- `X-Initiator: {initiator}`

#### Task 5.3 Add `history tools` Subcommand (Gap 6a)

**File**: `src/cli.rs`

Add variant to the existing `HistoryCommand` enum:

```text
HistoryCommand::Tools {
    conversation: Option<String>,
    tool: Option<String>,
}
```

`--conversation` accepts a conversation ID string. `--tool` accepts a tool name
string. Both are optional filters; when absent, all records are returned.

**File**: `src/storage/`

Create the `tool_invocations` table using `CREATE TABLE IF NOT EXISTS`
(consistent with the existing storage initialisation convention; no migration
runner is required):

```sql
CREATE TABLE IF NOT EXISTS tool_invocations (
    id              TEXT NOT NULL PRIMARY KEY,
    run_id          TEXT NOT NULL,
    conversation_id TEXT NOT NULL,
    tool_name       TEXT NOT NULL,
    arguments       TEXT NOT NULL,  -- JSON
    result          TEXT NOT NULL,  -- JSON
    success         INTEGER NOT NULL,
    timestamp       TEXT NOT NULL   -- RFC-3339 format
)
```

The `id` column must use ULID values. The `timestamp` column must use RFC-3339
format (example: `2025-11-07T18:12:07.982682Z`).

**File**: `src/commands/history.rs`

Implement `handle_history_tools` which queries the `tool_invocations` table with
optional `WHERE` filters on `conversation_id` and `tool_name`, then formats and
prints the results. Add the dispatch branch for `HistoryCommand::Tools` to the
existing history command handler.

#### Task 5.4 Add Scoped Configuration Validators (Gap 7a)

**File**: `src/config.rs`

Add four new methods to `Config`:

| Method                   | Called by                 | Validation checks                                                          |
| ------------------------ | ------------------------- | -------------------------------------------------------------------------- |
| `validate_for_execution` | `chat` and `run` commands | Provider type is configured; model field is non-empty                      |
| `validate_for_watcher`   | `watch` command           | `kafka.brokers` is non-empty; `kafka.topic` is non-empty                   |
| `validate_for_acp`       | `serve` / `acp` commands  | Bind port is non-zero; each name in `acp.agents` is a valid RFC 1123 label |
| `validate_for_zed_agent` | `agent` command           | Provider type is configured                                                |

**File**: `src/commands/mod.rs` (modules `chat`, `r#run`, `watch`) and
`src/commands/acp.rs`

Call the corresponding scoped validator immediately after the existing
`Config::validate()` call in each command handler.

RFC 1123 label validation rule: the name must match the regex
`^[a-z0-9]([a-z0-9\-]{0,61}[a-z0-9])?$` (lowercase alphanumeric and hyphen, 1 to
63 characters, no leading or trailing hyphen).

#### Task 5.5 Add Agent Chat History Config Fields (Gap 8a)

**File**: `src/config.rs`

Add to `AgentConfig`:

- `chat_history_max_size: usize` with
  `#[serde(default = "default_chat_history_max_size")]` and
  `fn default_chat_history_max_size() -> usize { 1000 }`
- `chat_history_file: Option<PathBuf>` with `#[serde(default)]`

Update `AgentConfig::default()` to include both fields.

**File**: `src/commands/mod.rs` (module `chat`)

Apply the two fields when constructing the `rustyline` editor:

- Pass `chat_history_max_size` as the history size limit.
- When `chat_history_file` is `Some(path)`, use that path for the history file.
- When `chat_history_file` is `None`, fall back to `<data_dir>/chat_history`
  where `<data_dir>` is the platform application data directory.

#### Task 5.6 Add Agent `thinking_mode` Global Default (Gap 8b)

**File**: `src/config.rs`

Add `thinking_mode: Option<String>` with `#[serde(default)]` (default `None`) to
`AgentConfig`. Update `AgentConfig::default()` to include the field.

**File**: `src/commands/mod.rs` (modules `chat`, `r#run`, `watch`)

Apply `agent.thinking_mode` as the fallback thinking mode when no
`--thinking-effort` CLI flag is provided for the current invocation. The CLI
flag always takes precedence over the config value.

#### Task 5.7 Testing Requirements

- **`num_ctx = None`**: unit test that the Ollama request body does not contain
  an `options` key when `num_ctx` is `None`.
- **`num_ctx = Some(n)`**: unit test that the Ollama request body contains
  `"options": {"num_ctx": n}` for a specific value of `n`.
- **`editor_version` / `initiator` headers**: unit test that both HTTP headers
  appear in Copilot outbound requests with the configured values.
- **`history tools` base query**: unit test that querying with no filters
  returns all rows in `tool_invocations`.
- **`history tools --tool` filter**: unit test that `--tool read_file` reduces
  results to only rows where `tool_name = "read_file"`.
- **`history tools --conversation` filter**: unit test that `--conversation`
  with a specific ID reduces results to only rows for that conversation.
- **`validate_for_execution`**: unit test that the method returns an error when
  the provider model field is empty; passes when the model is set.
- **`validate_for_watcher`**: unit test that the method returns an error when
  `kafka.brokers` is empty; passes with a valid brokers string.
- **`validate_for_acp`**: unit test that the method returns an error when an
  agent name fails RFC 1123 validation; passes with a valid name.
- **`validate_for_zed_agent`**: unit test that the method returns an error when
  no provider is configured; passes with a configured provider.
- **`chat_history_file = None`**: unit test that the default path containing
  `"chat_history"` is used.
- **`chat_history_file = Some(path)`**: unit test that the specified path is
  used.
- **`thinking_mode` fallback**: unit test that the CLI `--thinking-effort` flag
  overrides `agent.thinking_mode`; unit test that when the CLI flag is absent,
  `agent.thinking_mode` is used.

#### Task 5.8 Deliverables

- [ ] `src/config.rs` -- `num_ctx` added to `OllamaConfig`; `editor_version` and
      `initiator` added to `CopilotConfig`; `chat_history_max_size`,
      `chat_history_file`, and `thinking_mode` added to `AgentConfig`; four
      scoped validators implemented on `Config`
- [ ] `src/providers/ollama.rs` -- `num_ctx` applied in request body
- [ ] `src/providers/copilot.rs` -- `editor_version` and `initiator` applied as
      HTTP request headers
- [ ] `src/cli.rs` -- `HistoryCommand::Tools` variant added with
      `--conversation` and `--tool` flags
- [ ] `src/commands/history.rs` -- `handle_history_tools` implemented with
      optional filters; dispatch branch added
- [ ] `src/storage/` -- `tool_invocations` table created with ULID primary key
      and RFC-3339 timestamps
- [ ] `src/commands/mod.rs` (module `chat`) -- `rustyline` configured with
      `chat_history_max_size` and `chat_history_file`; `thinking_mode` fallback
      applied
- [ ] `src/commands/mod.rs` (modules `r#run` and `watch`) -- `thinking_mode`
      fallback applied
- [ ] `src/commands/mod.rs` (modules `chat` and `r#run`), `src/commands/acp.rs`
      -- scoped validators called after `Config::validate()`
- [ ] All new unit tests passing

#### Task 5.9 Success Criteria

- `cargo fmt --all` produces no diff.
- `cargo clippy --all-targets --all-features -- -D warnings` reports zero
  warnings on all affected files.
- `cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth`
  passes with no failures.
- `OllamaConfig { num_ctx: Some(16384), .. }` produces a request body containing
  `"options": {"num_ctx": 16384}`.
- `CopilotConfig { editor_version: "vscode/1.95.0".to_string(), initiator: "agent".to_string(), .. }`
  results in the `Editor-Version` and `X-Initiator` headers being present on
  outbound requests.
- `xzatoma history tools --tool read_file` returns only rows where
  `tool_name = "read_file"`.
- `validate_for_watcher` returns an error when `kafka.brokers` is an empty
  string; `validate_for_acp` returns an error when an agent name contains
  uppercase characters.
- `agent.thinking_mode: "high"` is used as the default when `--thinking-effort`
  is omitted from the CLI invocation.
- All five quality gates pass in order:
  1. `cargo fmt --all`
  2. `cargo check --all-targets --all-features`
  3. `cargo clippy --all-targets --all-features -- -D warnings`
  4. `cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth`
  5. `cargo audit --deny warnings`

---

## Documentation Requirements

Each phase must produce a corresponding implementation summary in
`docs/explanation/` following the Diataxis framework naming convention
(`lowercase_with_underscores.md`). Each document must include:

- A brief description of what changed and why.
- The full list of files modified or created.
- Any non-obvious design decisions made during implementation.
- Runnable `cargo test` examples where applicable.

| Phase | Document                                                           |
| ----- | ------------------------------------------------------------------ |
| 1     | `docs/explanation/phase1_watcher_resilience_implementation.md`     |
| 2     | `docs/explanation/phase2_watcher_cli_config_implementation.md`     |
| 3     | `docs/explanation/phase3_acp_server_controls_implementation.md`    |
| 4     | `docs/explanation/phase4_acp_multi_agent_implementation.md`        |
| 5     | `docs/explanation/phase5_provider_agent_history_implementation.md` |

---

## Gap Coverage Summary

| Gap | Description                            | Phase | Priority |
| --- | -------------------------------------- | ----- | -------- |
| 1a  | Circuit Breaker                        | 1     | High     |
| 1b  | BackOffPolicy                          | 1     | High     |
| 1c  | Startup Stabilization                  | 1     | Medium   |
| 2a  | `watch --once` flag                    | 2     | Medium   |
| 2b  | `watch --allow-dangerous` flag         | 2     | Low      |
| 2c  | Kafka `startup_stabilization_secs`     | 1     | Medium   |
| 2d  | Kafka `max_poll_interval_ms`           | 1     | High     |
| 2e  | Kafka `auto_offset_reset`              | 2     | Low      |
| 2f  | Kafka `ssl_ca_location`                | 2     | Medium   |
| 3a  | ACP `session_mode`                     | 3     | High     |
| 3b  | ACP `allow_dangerous`                  | 3     | Medium   |
| 3c  | ACP `max_concurrent_runs`              | 3     | High     |
| 3d  | ACP `run_timeout_seconds`              | 3     | High     |
| 3e  | ACP `session_timeout_seconds`          | 3     | Low      |
| 3f  | ACP `cors_origins`                     | 3     | Medium   |
| 3g  | ACP `graceful_shutdown_timeout`        | 3     | Low      |
| 3h  | ACP `agents` multi-agent config        | 4     | High     |
| 3i  | ACP `client` outbound config           | 4     | High     |
| 4a  | `call_acp_agent` tool                  | 4     | High     |
| 4b  | `discover_acp_agents` tool             | 4     | High     |
| 4c  | `await_input` tool                     | 4     | Medium   |
| 5a  | Ollama `num_ctx`                       | 5     | Low      |
| 5b  | Copilot `editor_version` / `initiator` | 5     | Low      |
| 6a  | `history tools` subcommand             | 5     | Medium   |
| 7a  | Scoped config validators               | 5     | Medium   |
| 8a  | `agent.chat_history_*`                 | 5     | Low      |
| 8b  | `agent.thinking_mode` global default   | 5     | Low      |
| 9   | MCP auto-approve for headless runs     | 1     | High     |
