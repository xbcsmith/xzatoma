# Phase 1: Watcher Resilience and MCP Correctness Implementation

## Overview

Phase 1 closes five high-priority gaps in XZatoma's watcher and MCP subsystems.
The work consists of two correctness defects and three new infrastructure
components. Every later phase builds on this foundation.

Gaps addressed: 9 (MCP auto-approve), 1a (circuit breaker), 1b (back-off
policy), 1c (startup stabilization context), 2c (Kafka startup stabilization
config), 2d (Kafka `max_poll_interval_ms`).

---

## Files Changed

| File                                   | Change Type | Description                                                                           |
| -------------------------------------- | ----------- | ------------------------------------------------------------------------------------- |
| `src/mcp/approval.rs`                  | Modified    | `should_auto_approve` now returns correct value                                       |
| `src/mcp/sampling.rs`                  | Modified    | Tests updated to reflect new approval semantics                                       |
| `src/commands/environment.rs`          | Modified    | MCP manager execution context set for headless runs                                   |
| `src/config.rs`                        | Modified    | Two new fields added to `KafkaWatcherConfig`                                          |
| `src/watcher/mod.rs`                   | Modified    | `BackOffPolicy` added; new submodules declared and re-exported                        |
| `src/watcher/circuit_breaker.rs`       | Created     | `CircuitState`, `CircuitBreakerConfig`, `CircuitBreaker`                              |
| `src/watcher/startup_context.rs`       | Created     | `QuietStartupContext` implementing rdkafka `ClientContext`                            |
| `src/watcher/xzepr/consumer/config.rs` | Modified    | Two new fields added to `KafkaConsumerConfig`                                         |
| `src/watcher/xzepr/consumer/kafka.rs`  | Modified    | `max_poll_interval_ms` applied; `QuietStartupContext` used; `BackOffPolicy` wired     |
| `src/watcher/xzepr/watcher.rs`         | Modified    | New config fields passed through; `CircuitBreaker` wired into `WatcherMessageHandler` |
| `src/watcher/generic/consumer.rs`      | Modified    | `inner` changed to `StreamConsumer<QuietStartupContext>`                              |
| `src/watcher/generic/watcher.rs`       | Modified    | `max_poll_interval_ms` applied; `BackOffPolicy` wired into `start()` loop             |

---

## Task 1.1 -- MCP Auto-Approval Fix (Gap 9)

### Problem

`should_auto_approve` in `src/mcp/approval.rs` always returned `false`. Headless
`run` and `watch` commands could not auto-approve MCP sampling requests, making
them non-functional when MCP servers required sampling.

### Solution

`should_auto_approve` now implements:

```text
headless || execution_mode == ExecutionMode::FullAutonomous
```

| `execution_mode` | `headless` | Result  |
| ---------------- | ---------- | ------- |
| `FullAutonomous` | `false`    | `true`  |
| `FullAutonomous` | `true`     | `true`  |
| Any other mode   | `true`     | `true`  |
| Any other mode   | `false`    | `false` |

`build_agent_environment` in `src/commands/environment.rs` now calls
`set_execution_context(execution_mode, headless)` on the MCP manager immediately
after construction. This ensures that sampling handlers spawned from the `run`
and `watch` command paths receive `headless = true`, while the interactive
`chat` path keeps `headless = false` (the manager default).

### Design decisions

The `should_auto_approve` function is a legacy gate for the MCP sampling path
only. Policy-based tool call decisions use the separate `approval_decision`
function with per-server trust metadata. The two mechanisms are independent.

---

## Task 1.2 -- Exponential Back-Off Policy (Gap 1b)

### Problem

Consecutive Kafka errors triggered tight retry loops with no delay, causing log
spam and CPU waste during broker outages.

### Solution

`BackOffPolicy` in `src/watcher/mod.rs` implements doubling back-off:

- Initial delay: 500 ms
- On each `increment()`: delay doubles
- Maximum delay: 30 000 ms (30 seconds)
- `reset()` returns to the initial delay

The policy is applied in both watcher backends:

- **XZepr backend** (`src/watcher/xzepr/consumer/kafka.rs`): in both `run()` and
  `run_with_channel()`, `back_off.increment()` and `tokio::time::sleep` are
  called on every transient Kafka error; `back_off.reset()` is called after each
  successfully processed message.
- **Generic backend** (`src/watcher/generic/watcher.rs`): `back_off.increment()`
  is called when `process_event` returns an error or a fatal consumer error
  occurs; `back_off.reset()` is called after each successfully processed event.

No external crate dependency is required -- only `std::time::Duration`.

---

## Task 1.3 -- Circuit Breaker (Gap 1a)

### Problem

No mechanism existed to stop processing after a sustained run of failures.
Persistent downstream problems caused continuous plan execution attempts.

### Solution

`CircuitBreaker` in `src/watcher/circuit_breaker.rs` implements a three-state
machine:

```text
Closed --[failure_threshold consecutive failures]--> Open
Open   --[reset_timeout_secs elapsed]            --> HalfOpen
HalfOpen --[on_success()]                        --> Closed
HalfOpen --[on_failure()]                        --> Open
```

Default thresholds: `failure_threshold = 5`, `reset_timeout_secs = 60`.

The circuit breaker is wired into `WatcherMessageHandler` in
`src/watcher/xzepr/watcher.rs`:

- `is_open()` is checked at the start of `handle()`; an open circuit causes the
  event to be skipped immediately.
- `on_success()` is called before each successful `handle()` return.
- `on_failure()` is called on any error path in `handle()`.

The `is_open()` method also drives the `Open` to `HalfOpen` transition: when
called on an `Open` circuit, it checks whether `reset_timeout_secs` have elapsed
and promotes to `HalfOpen` if so.

---

## Task 1.4 -- Kafka `max_poll_interval_ms` (Gap 2d)

### Problem

Long-running plans caused the rdkafka consumer to be evicted from its consumer
group because `max.poll.interval.ms` was not configured and defaulted to a value
shorter than typical plan execution time.

### Solution

`max_poll_interval_ms: u64` (default `3_600_000`, 1 hour) was added to
`KafkaWatcherConfig` in `src/config.rs` and to `KafkaConsumerConfig` in
`src/watcher/xzepr/consumer/config.rs`.

The value is applied as the rdkafka config key `max.poll.interval.ms` in both
backends:

- XZepr backend: via `KafkaConsumerConfig::with_max_poll_interval_ms` in the
  builder chain inside `Watcher::new()`, and included in `get_kafka_config()`.
- Generic backend: directly included in `GenericWatcher::get_kafka_config()`.

---

## Task 1.5 -- Startup Stabilization Context (Gaps 1c and 2c)

### Problem

Normal Kafka consumer startup involved broker probing that emitted a burst of
`WARN`/`ERROR` log lines, making it hard to distinguish genuine problems from
expected startup noise.

### Solution

`QuietStartupContext` in `src/watcher/startup_context.rs` implements rdkafka's
`ClientContext` and `ConsumerContext` traits. During the first
`startup_stabilization_secs` seconds after construction, all rdkafka log
callbacks are downgraded to `DEBUG`. After the window elapses, callbacks are
forwarded at their original severity.

`startup_stabilization_secs: u64` (default `10`) was added to both
`KafkaWatcherConfig` and `KafkaConsumerConfig`.

Both backends now use `StreamConsumer<QuietStartupContext>`:

- **XZepr backend**: `XzeprConsumer::create_subscribed_consumer()` calls
  `ClientConfig::create_with_context(QuietStartupContext::new(...))` instead of
  `create()`.
- **Generic backend**: `RealGenericConsumer::inner` is now
  `StreamConsumer<QuietStartupContext>`. `from_config` creates a context with
  `Duration::ZERO` (immediate expiry, no suppression). The new
  `from_config_with_startup` method (used by `GenericWatcher::build_consumer`)
  passes the configured seconds.

Passing `Duration::ZERO` to `QuietStartupContext::new` creates a context whose
window has already expired at construction time, so it behaves identically to
the default consumer context at zero cost.

---

## Validation

All quality gates passed on the combined result:

```bash
cargo fmt --all                                              # no diff
cargo check --all-targets --all-features                    # 0 errors
cargo clippy --all-targets --all-features -- -D warnings    # 0 warnings
cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth
# 2474 passed; 2 failed (pre-existing history binary tests); 36 ignored
```

The 2 failing tests
(`commands::history::test_handle_history_list_displays_sessions` and
`commands::history::test_handle_history_delete_removes_session`) are
pre-existing integration tests that spawn the compiled `xzatoma` binary. They
fail in the absence of a `cargo build` step and are unrelated to Phase 1.

---

## New Tests Added

| Module                     | Test                                                                 | Verifies                            |
| -------------------------- | -------------------------------------------------------------------- | ----------------------------------- |
| `mcp::approval`            | `test_should_auto_approve_full_autonomous_non_headless_returns_true` | `(FullAutonomous, false)` -> `true` |
| `mcp::approval`            | `test_should_auto_approve_full_autonomous_headless_returns_true`     | `(FullAutonomous, true)` -> `true`  |
| `mcp::approval`            | `test_should_auto_approve_headless_any_mode_returns_true`            | `(Interactive, true)` -> `true`     |
| `mcp::approval`            | `test_should_auto_approve_interactive_non_headless_returns_false`    | `(Interactive, false)` -> `false`   |
| `watcher`                  | `test_back_off_policy_initial_delay_is_500ms`                        | Initial delay                       |
| `watcher`                  | `test_back_off_policy_increment_doubles_delay`                       | Doubling                            |
| `watcher`                  | `test_back_off_policy_caps_at_30_seconds`                            | Cap enforcement                     |
| `watcher`                  | `test_back_off_policy_reset_returns_to_initial`                      | Reset behavior                      |
| `watcher`                  | `test_back_off_policy_current_delay_returns_correct_duration`        | Duration return type                |
| `watcher::circuit_breaker` | `test_circuit_breaker_starts_closed`                                 | Initial state                       |
| `watcher::circuit_breaker` | `test_circuit_breaker_opens_after_threshold_failures`                | Closed -> Open                      |
| `watcher::circuit_breaker` | `test_circuit_breaker_does_not_open_before_threshold`                | Threshold exact                     |
| `watcher::circuit_breaker` | `test_circuit_breaker_is_open_returns_true_when_open`                | is_open guard                       |
| `watcher::circuit_breaker` | `test_circuit_breaker_transitions_to_half_open_after_timeout`        | Open -> HalfOpen                    |
| `watcher::circuit_breaker` | `test_circuit_breaker_half_open_success_closes_circuit`              | HalfOpen -> Closed                  |
| `watcher::circuit_breaker` | `test_circuit_breaker_half_open_failure_reopens`                     | HalfOpen -> Open                    |
| `watcher::circuit_breaker` | `test_circuit_breaker_on_success_resets_failure_counter_when_closed` | Counter reset                       |
| `watcher::startup_context` | `test_quiet_startup_context_new_with_nonzero_duration_is_in_window`  | Window active                       |
| `watcher::startup_context` | `test_quiet_startup_context_new_with_zero_duration_is_not_in_window` | Zero duration                       |
| `watcher::startup_context` | `test_quiet_startup_context_expired_after_deadline`                  | Expiry                              |
