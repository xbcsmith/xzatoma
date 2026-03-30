# Codebase Cleanup Implementation Plan

## Overview

This plan addresses five categories of technical debt identified through a
comprehensive audit of the XZatoma codebase: duplicate code patterns, dead code
and suppressed warnings, inconsistent error handling, unfinished TODOs and
placeholders, and stale Phase references. The goal is to reduce maintenance
burden, improve code quality, and bring the codebase to a consistent,
production-ready state without concern for backwards compatibility.

## Current State Analysis

### Existing Infrastructure

XZatoma is a Rust-based autonomous AI agent CLI with mature module structure
spanning `agent/`, `providers/`, `tools/`, `commands/`, `mcp/`, `acp/`,
`skills/`, `storage/`, `watcher/`, and `xzepr/`. The project has functional
Copilot and Ollama providers, an MCP client integration, ACP server support,
agent skills framework, and watcher infrastructure. All code compiles and passes
`cargo clippy` with zero warnings today -- but only because blanket
`#![allow(dead_code)]` and `#![allow(unused_imports)]` annotations suppress
warnings across seven major modules.

### Identified Issues

**Duplicate Code** -- 10 patterns found ranging from critical (duplicate
`ToolExecutor` trait definitions) to low (repeated `PathValidator` boilerplate).
Three separate call sites repeat MCP manager initialization and tool/skill
registration sequences of 30+ lines each. Provider structs and conversion
methods are near-identical between Copilot and Ollama.

**Dead Code and Suppressed Warnings** -- 9 module-level `#![allow(dead_code)]`
blanket suppressions hide unknown quantities of unused code. 28 item-level
`#[allow(dead_code)]` annotations, 7 clippy suppressions, and 6 `#[ignore]`
tests. The `agent/executor.rs` module contains a vestigial `ToolExecutor` trait
that is never imported outside its own test.

**Inconsistent Error Handling** -- 11 distinct error enum types with no `From`
conversions to the central `XzatomaError`. Two competing `Result` type aliases
(`anyhow::Result` in `error.rs` vs `std::result::Result<T, AcpError>` in
`acp/error.rs`). Approximately 15 `let _ =` sites in production code that
silently discard errors. Inconsistent `.map_err(|e| e.to_string())` patterns
that destroy error chains.

**Unfinished Work** -- 12 stub/placeholder implementations in production code.
The watcher subsystem has two backends (XZepr and Generic) with fully
implemented business logic (matching, filtering, plan extraction, message types)
but all Kafka I/O is stubbed: the consumer sleeps in a loop, the producer logs
to stdout, topic admin logs but never creates topics, and the generic watcher
returns synthetic success without executing plans. These stubs will be replaced
with real `rdkafka` implementations. Additionally, the `mcp list` command is a
no-op, the `TaskManager` is hollow, empty test bodies always pass, and
sampling/elicitation handlers are logged as "registering" but never attached.

**Phase References** -- Approximately 120 Phase references in source code across
40 files, 700+ references in 30+ documentation files, 17 files with "phase" in
the filename, and stale project status sections in `README.md` and
`CONTRIBUTING.md` that describe the project as "Planning Complete."

## Implementation Phases

### Phase 1: Dead Code Removal and Warning Cleanup

Remove blanket warning suppressions to let the compiler identify genuine dead
code, then either wire up or delete what it finds. Use the file_edit tool and go file by file CAREFULLY to ensure consistent renaming and avoid missing any references.

#### Task 1.1: Remove Vestigial `agent/executor.rs` Module

Delete [`src/agent/executor.rs`](../../src/agent/executor.rs) entirely. It
contains a duplicate `ToolExecutor` trait that is never imported outside its own
`MockToolExecutor` test. Remove the `pub mod executor;` line from
[`src/agent/mod.rs`](../../src/agent/mod.rs).

#### Task 1.2: Remove Module-Level `#![allow(dead_code)]` Suppressions

Remove blanket `#![allow(dead_code)]` and `#![allow(unused_imports)]` from these
9 locations, one module at a time, fixing or removing whatever the compiler
flags:

| File                                                                           | Lines    | Comment                      |
| ------------------------------------------------------------------------------ | -------- | ---------------------------- |
| [`src/error.rs`](../../src/error.rs)                                           | L6-7     | Start here (smallest module) |
| [`src/agent/mod.rs`](../../src/agent/mod.rs)                                   | L6-8     | "Phase 1" comment            |
| [`src/providers/mod.rs`](../../src/providers/mod.rs)                           | L6-8     | "Phase 1" comment            |
| [`src/tools/mod.rs`](../../src/tools/mod.rs)                                   | L6-8     | "Phase 1" comment            |
| [`src/tools/plan.rs`](../../src/tools/plan.rs)                                 | L1       | Standalone module            |
| [`src/commands/mod.rs`](../../src/commands/mod.rs)                             | L16-17   | No phase comment             |
| [`src/mcp/mod.rs`](../../src/mcp/mod.rs)                                       | L26-27   | Has real placeholders        |
| [`src/xzepr/mod.rs`](../../src/xzepr/mod.rs)                                   | L24      | Re-export shim               |
| [`src/watcher/xzepr/consumer/mod.rs`](../../src/watcher/xzepr/consumer/mod.rs) | L120-135 | Unused re-exports            |

For each module: remove the `#![allow(...)]`, run `cargo check`, and for every
new warning either (a) add a targeted `#[allow(dead_code)]` with a justification
comment if the code is genuinely needed for future use, or (b) delete the dead
code.

#### Task 1.3: Audit Item-Level `#[allow(dead_code)]` Annotations

Review the 28 item-level `#[allow(dead_code)]` annotations and resolve each:

- **`src/chat_mode.rs`** (10 annotations): Verify `ChatMode`, `SafetyMode`, and
  `ChatModeState` are used from `commands/mod.rs` and `acp/executor.rs`. Remove
  annotations on items that are actually used. For unused items, either wire
  them in or delete them.
- **`src/mention_parser.rs`** (8 annotations): `UrlContentCache`,
  `SearchResultsCache`, `resolve_mention_path`, and `format_search_results`
  appear fully implemented but never called. Decide: wire them in or delete.
- **`src/tools/plan_format.rs`** (2 annotations): `detect_plan_format` and
  `validate_plan` are implemented but never called. Wire into plan parsing or
  delete.
- **`src/mcp/task_manager.rs`** (1 annotation): Entire `TaskEntry` struct is a
  placeholder. Keep with a justification comment referencing the task manager
  future work.
- **`src/watcher/generic/watcher.rs`** and `src/watcher/xzepr/watcher.rs` (2
  annotations): Error enums that are defined but unused. Keep with justification
  if watcher is actively developed, otherwise delete.
- **`src/providers/copilot.rs`** (5 annotations on `Deserialize` fields): These
  are correct -- fields exist for deserialization. Add justification comments:
  `// Required for JSON deserialization`.

#### Task 1.4: Resolve Clippy Suppressions

- **`src/providers/ollama.rs` L47** -- `clippy::type_complexity`: Extract a type
  alias `type ModelCache = Arc<RwLock<Option<(Vec<ModelInfo>, Instant)>>>` and
  remove the suppression.
- **`src/watcher/xzepr/consumer/client.rs` L480, L529, L581** --
  `clippy::too_many_arguments`: Create a `WorkEvent` struct to bundle the 7-8
  parameters for `post_work_started`, `post_work_completed`, and
  `post_work_failed`.
- **`src/tools/grep.rs` L389 and `src/tools/list_directory.rs` L22** --
  `clippy::only_used_in_recursion`: Investigate whether the `&self` parameter is
  genuinely needed; if it is a false positive, add a justification comment.
- **`src/watcher/xzepr/mod.rs` L31** -- `clippy::module_inception`: Keep with
  justification comment.

#### Task 1.5: Fix or Un-Ignore `#[ignore]` Tests

- **`src/tools/fetch.rs` L900, L908** -- IPv6 normalization: Update assertions
  to match the `url` crate's normalized IPv6 output.
- **`src/watcher/xzepr/consumer/client.rs` L624 and config.rs L504, L532, L552**
  -- Environment variable mutation: Migrate to the `temp-env` crate or use
  `serial_test` to run these sequentially.

#### Task 1.6: Resolve `#![allow(deprecated)]` in `commands/history.rs`

Investigate which `prettytable` API triggers the deprecation warning. Migrate to
the non-deprecated alternative and remove both the module-level
`#![allow(deprecated)]` at L1 and the test-module `#[allow(deprecated)]` at
L183.

#### Task 1.7: Testing Requirements

- `cargo fmt --all` passes
- `cargo check --all-targets --all-features` produces zero warnings
- `cargo clippy --all-targets --all-features -- -D warnings` produces zero
  warnings with no blanket suppressions
- `cargo test --all-features` passes (including previously-ignored tests)
- All remaining `#[allow(...)]` annotations have inline justification comments

#### Task 1.8: Deliverables

- Deleted `src/agent/executor.rs` and its module declaration
- All 9 module-level `#![allow(dead_code)]` removed
- All 4 module-level `#![allow(unused_imports)]` removed
- All item-level `#[allow(...)]` have justification comments or are removed
- All `#[ignore]` tests are either fixed or have documented reasons

#### Task 1.9: Success Criteria

Running `cargo clippy --all-targets --all-features -- -D warnings` without any
blanket module-level `#![allow(...)]` directives produces zero warnings. All
tests pass including previously-ignored tests.

---

### Phase 2: Error Handling Consolidation

Unify the error handling strategy across the codebase to use a single `Result`
type, add missing `From` implementations, and audit silent error discards. Use the file_edit tool and go file by file CAREFULLY to ensure consistent renaming and avoid missing any references.

#### Task 2.1: Migrate to Typed `Result<T, XzatomaError>`

The codebase has two competing aliases:

- [`src/error.rs` L221](../../src/error.rs):
  `pub type Result<T> = anyhow::Result<T>`
- [`src/acp/error.rs` L42](../../src/acp/error.rs):
  `pub type Result<T> = std::result::Result<T, AcpError>`

Migrate to a single typed alias:
`pub type Result<T> = std::result::Result<T, XzatomaError>`. This gives full
`match` support on error variants without `downcast`, makes error handling
explicit at every boundary, and eliminates the implicit `anyhow` wrapping that
currently hides error provenance. Steps:

1. Change [`src/error.rs` L221](../../src/error.rs) from
   `pub type Result<T> = anyhow::Result<T>` to
   `pub type Result<T> = std::result::Result<T, XzatomaError>`.
2. Remove the `acp::error::Result` alias entirely.
3. Replace all `anyhow::anyhow!(...)` call sites with the appropriate
   `XzatomaError` variant construction.
4. Replace all `anyhow::Context` / `.context(...)` calls with explicit
   `.map_err(|e| XzatomaError::Variant(...))` conversions.
5. Ensure every module-local error type has a `From` impl into `XzatomaError`
   (see Task 2.3) so `?` still works at module boundaries.
6. Remove the `anyhow` dependency from `Cargo.toml` once all usages are
   eliminated. If `anyhow` is still needed transitionally, keep it but remove
   the re-export.

#### Task 2.2: Collapse Overlapping ACP Error Variants

[`src/error.rs`](../../src/error.rs) has 4 ACP-specific variants
(`AcpValidation`, `AcpLifecycle`, `AcpPersistence`, `AcpUnsupportedTransition`)
that mirror [`src/acp/error.rs`](../../src/acp/error.rs) (`AcpError`). Add
`impl From<AcpError> for XzatomaError` and collapse the 4 variants into:

```text
#[error("ACP error: {0}")]
Acp(#[from] AcpError),
```

Update all call sites that construct `XzatomaError::AcpValidation(...)` etc. to
use `AcpError::Validation(...)` and let the `From` impl handle conversion.

#### Task 2.3: Add Missing `From` Implementations

Add `From` conversions for all 10 module-local error types that currently lack
them:

| Module Error Type       | Target `XzatomaError` Variant |
| ----------------------- | ----------------------------- |
| `AcpError`              | New `Acp(#[from] AcpError)`   |
| `AcpValidationError`    | Via `AcpError` chain          |
| `FileMetadataError`     | `Tool(String)`                |
| `FileUtilsError`        | `Tool(String)`                |
| `GenericWatcherError`   | `Watcher(String)`             |
| `WatcherError`          | `Watcher(String)`             |
| `ConsumerError`         | `Watcher(String)`             |
| `ClientError`           | `Provider(String)`            |
| `ConfigError` (watcher) | `Config(String)`              |
| `CommandError`          | `Command(String)`             |

Where a matching variant does not exist, add one to `XzatomaError`.

#### Task 2.4: Standardize `anyhow` Imports

Replace direct `use anyhow::Result` imports in all `watcher/` modules with
`use crate::error::Result` so the project has a single import path. Files to
update:

- `src/watcher/logging.rs`
- `src/watcher/generic/matcher.rs`
- `src/watcher/generic/producer.rs`
- All other files importing `anyhow::Result` directly

#### Task 2.5: Audit `let _ =` Error Discards

Review all ~15 `let _ =` sites in production code. For each, apply one of:

- **Propagate**: Replace `let _ = expr?;` with `expr?;` (several in
  `acp/executor.rs` already use `?` -- the `let _ =` is discarding the success
  value and should become `let _ok = expr?;` or just `expr?;`)
- **Log and continue**: Replace `let _ = expr;` with
  `if let Err(e) = expr { tracing::warn!(...) }`
- **Justify**: Add `// Intentionally discarded: <reason>` comment

Priority sites (errors silently lost):

| File               | Line(s)          | Issue                                               |
| ------------------ | ---------------- | --------------------------------------------------- |
| `acp/executor.rs`  | L319-323         | Background task errors dropped in `tokio::spawn`    |
| `acp/runtime.rs`   | L562             | `restore_from_storage()` failure silently swallowed |
| `agent/metrics.rs` | L250-253         | Prometheus exporter handle discarded (likely bug)   |
| `mcp/auth/flow.rs` | L53-55, L704-708 | Browser launch and HTTP write errors dropped        |
| `mcp/client.rs`    | L552, L602       | Channel send results dropped                        |

#### Task 2.6: Eliminate Double Error Reporting

In `providers/ollama.rs` (L717-725) and `providers/copilot.rs` (similar), the
same error is both `tracing::error!`-logged and returned via `Err(...)`. Remove
the `tracing::error!` call and let the caller decide how to report. Or downgrade
to `tracing::debug!` for diagnostic purposes only.

#### Task 2.7: Create Shared Argument Parsing Helper

Extract a shared tool argument deserialization function in
[`src/tools/mod.rs`](../../src/tools/mod.rs):

```text
pub fn parse_tool_args<T: serde::de::DeserializeOwned>(args: Value) -> Result<T> {
    serde_json::from_value(args)
        .map_err(|e| XzatomaError::Tool(format!("Invalid tool parameters: {}", e)))
}
```

Replace all 11 inconsistent `serde_json::from_value(args)` patterns across tool
`execute()` methods with this single helper.

#### Task 2.8: Testing Requirements

- All existing tests continue to pass
- New tests verify `From` conversions for each new error type mapping
- `cargo clippy` produces no new warnings
- No `let _ =` remains in production code without a justification comment

#### Task 2.9: Deliverables

- Single `Result` type alias used project-wide
- `From` impls for all module-local error types to `XzatomaError`
- All `let _ =` sites audited and resolved
- Shared `parse_tool_args` helper used by all tools
- Consistent `anyhow` import path

#### Task 2.10: Success Criteria

Every function in the codebase returns `crate::error::Result<T>` (which is
`std::result::Result<T, XzatomaError>`). The `anyhow` crate is removed from
`Cargo.toml` or no longer re-exported as the `Result` type. No `let _ =`
discards a `Result` without a justification comment. Error chains are preserved
through `From` impls rather than `.to_string()` flattening.

---

### Phase 3: Code Deduplication

Extract shared patterns into reusable functions and types. This phase depends on
Phase 2 (error handling) being complete so extracted code uses the unified error
types.

#### Task 3.1: Extract Shared MCP Manager Builder

The MCP manager initialization pattern is repeated in three locations:

- [`src/commands/mod.rs` L453-476](../../src/commands/mod.rs) (`run_chat`)
- [`src/commands/mod.rs` L2002-2024](../../src/commands/mod.rs)
  (`run_plan_with_options`)
- [`src/acp/executor.rs` L488-515](../../src/acp/executor.rs)
  (`build_mcp_manager`)

Extract a shared function in `src/mcp/manager.rs`:

```text
pub async fn build_mcp_manager_from_config(
    config: &Config,
) -> Result<Option<Arc<RwLock<McpClientManager>>>>
```

Replace all three call sites with this single function.

#### Task 3.2: Extract Tool and Skill Initialization Sequence

The full tool/skill initialization sequence (parse `ChatMode`, parse
`SafetyMode`, build skill catalog, create `ActiveSkillRegistry`, build tool
registry, register MCP tools) is repeated in:

- [`src/commands/mod.rs` L415-500](../../src/commands/mod.rs) (`run_chat`)
- [`src/commands/mod.rs` L1964-2044](../../src/commands/mod.rs)
  (`run_plan_with_options`)
- [`src/acp/executor.rs` L432-484](../../src/acp/executor.rs) (`build_tools`)

Create a shared builder function or struct:

```text
pub async fn build_agent_environment(
    config: &Config,
    working_dir: &Path,
    headless: bool,
) -> Result<AgentEnvironment>
```

Where `AgentEnvironment` bundles `ToolRegistry`,
`Option<Arc<RwLock<McpClientManager>>>`, `ChatMode`, and `SafetyMode`.

#### Task 3.3: Unify Provider Request/Response Structs

[`src/providers/copilot.rs`](../../src/providers/copilot.rs) (L188-265) and
[`src/providers/ollama.rs`](../../src/providers/ollama.rs) (L99-166) define
structurally identical struct families (`CopilotRequest`/`OllamaRequest`,
`CopilotMessage`/`OllamaMessage`, etc.). Create shared types in
[`src/providers/base.rs`](../../src/providers/base.rs):

| Shared Type        | Replaces                            |
| ------------------ | ----------------------------------- |
| `ProviderRequest`  | `CopilotRequest`, `OllamaRequest`   |
| `ProviderMessage`  | `CopilotMessage`, `OllamaMessage`   |
| `ProviderTool`     | `CopilotTool`, `OllamaTool`         |
| `ProviderFunction` | `CopilotFunction`, `OllamaFunction` |
| `ProviderToolCall` | `CopilotToolCall`, `OllamaToolCall` |

Handle the one-field difference (`tool_call_id` in `CopilotMessage`) with an
`Option` field. Handle the `arguments` type difference (`String` in Copilot vs
`Value` in Ollama) with `serde_json::Value` and a serialization adapter.

#### Task 3.4: Extract Shared `convert_tools` Method

The `convert_tools` methods in `copilot.rs` (L1264-1283) and `ollama.rs`
(L261-315) are structurally identical except for struct names. After Task 3.3
unifies the structs, extract a shared free function in `providers/base.rs`:

```text
pub fn convert_tools_from_json(tools: &[Value]) -> Vec<ProviderTool>
```

Similarly, extract the common parts of `convert_messages` into a shared helper,
with per-provider hooks for the argument format and ID generation differences.

#### Task 3.5: Extract Interactive Approval Helper

Three identical 20-line approval prompt blocks exist in
[`src/mcp/tool_bridge.rs`](../../src/mcp/tool_bridge.rs) (L179-200, L359-378,
L523-545). Extract into a shared function alongside `should_auto_approve`:

```text
pub fn prompt_user_approval(description: &str) -> Result<bool>
```

Each call site becomes a two-liner check.

#### Task 3.6: Consolidate `build_visible_skill_catalog`

Two versions exist:

- [`src/commands/mod.rs` L256-271](../../src/commands/mod.rs) (2 params)
- [`src/commands/skills.rs` L471-489](../../src/commands/skills.rs) (3 params)

Keep only the 3-parameter version in `skills.rs`. Make the 2-parameter version
in `mod.rs` a thin wrapper that loads trusted paths then delegates.

#### Task 3.7: Testing Requirements

- All existing tests pass after refactoring
- New unit tests for each extracted function (`build_mcp_manager_from_config`,
  `build_agent_environment`, `convert_tools_from_json`, `prompt_user_approval`,
  `parse_tool_args`)
- Integration tests confirm end-to-end behavior unchanged

#### Task 3.8: Deliverables

- Shared `build_mcp_manager_from_config` function
- `AgentEnvironment` builder consolidating tool/skill setup
- Unified provider types in `providers/base.rs`
- Shared `convert_tools_from_json` and `convert_messages` helpers
- Shared `prompt_user_approval` function
- Consolidated `build_visible_skill_catalog`

#### Task 3.9: Success Criteria

No code block longer than 5 lines is duplicated verbatim across multiple call
sites. Each extracted function has at least one unit test. The total line count
of `commands/mod.rs` decreases by at least 100 lines.

---

### Phase 4: Watcher Kafka Implementation

Replace all Kafka I/O stubs with real `rdkafka` implementations. The watcher has
two independent backends -- XZepr and Generic -- both selectable via the
`--watcher-type` CLI flag or `watcher.watcher_type` config field. The business
logic (matching, filtering, plan extraction, message types, config assembly) is
already fully implemented and tested. Only the Kafka consumer, producer, topic
admin, and generic plan execution are stubs.

Every stub already has a `get_kafka_config() -> Vec<(String, String)>` method
that produces the correct `rdkafka::ClientConfig` key-value pairs including SASL
and SSL settings. The real implementation reuses these directly.

#### Task 4.1: Add `rdkafka` Dependency

Add `rdkafka` to [`Cargo.toml`](../../Cargo.toml) with the `cmake-build` and
`ssl` features:

```text
rdkafka = { version = "0.36", features = ["cmake-build", "ssl"] }
```

The `cmake-build` feature compiles `librdkafka` from source so there is no
system-library dependency. If build times are a concern, add a `dynamic-linking`
feature that switches to `rdkafka = { features = ["dynamic-linking"] }` for
environments with `librdkafka` installed.

#### Task 4.2: Implement `WatcherTopicAdmin::ensure_topics()`

Replace the log-only stub in
[`src/watcher/topic_admin.rs`](../../src/watcher/topic_admin.rs)
`ensure_topics()` with a real `rdkafka::admin::AdminClient`:

1. Build `AdminClient<DefaultClientContext>` from `self.get_kafka_config()`.
2. For each `TopicEnsureRequest`, call
   `admin_client.create_topics(&[NewTopic::new(&topic, num_partitions, replication)])`.
3. Treat `TopicAlreadyExists` as success (idempotent creation).
4. Add `num_partitions` and `replication_factor` fields to `KafkaWatcherConfig`
   with sensible defaults (`num_partitions: 1`, `replication_factor: 1`).
5. The existing `topics_for_xzepr_watcher()`, `topics_for_generic_watcher()`,
   and deduplication logic are already implemented -- reuse them as-is.

Both `ensure_xzepr_watcher_topics()` and `ensure_generic_watcher_topics()`
delegate to `ensure_topics()`, so fixing the inner method fixes both backends.

Note: `run_watch` in `commands/mod.rs` calls topic admin once before
constructing the watcher, and some watcher `start()` methods call it again
internally. Deduplicate this so topic admin runs exactly once during startup.

#### Task 4.3: Implement `XzeprConsumer::run()` with `StreamConsumer`

Replace the sleep-loop stub in
[`src/watcher/xzepr/consumer/kafka.rs`](../../src/watcher/xzepr/consumer/kafka.rs)
`run()`:

1. Build `rdkafka::ClientConfig` from `self.get_kafka_config()` (method already
   exists and produces all required key-value pairs including SASL/SSL).
2. Create a `StreamConsumer` from that config.
3. Subscribe to `&[&self.config.topic]`.
4. Enter a message-stream loop: for each message, extract the payload as `&str`
   and call the existing `XzeprConsumer::process_message(payload, &handler)`
   method, which already handles JSON deserialization to `CloudEventMessage` and
   handler dispatch.
5. Respect `self.running: Arc<AtomicBool>` for graceful shutdown (the flag and
   `stop()` method already exist).
6. Log consumer group assignment and rebalance events.

Also implement `run_with_channel()` using the same `StreamConsumer` pattern but
sending deserialized `CloudEventMessage` values through the
`mpsc::Sender<CloudEventMessage>` instead of calling a handler.

The `WatcherMessageHandler` in
[`src/watcher/xzepr/watcher.rs`](../../src/watcher/xzepr/watcher.rs) is already
fully implemented -- it filters events, extracts plans, and executes them. Once
the consumer delivers real messages, the XZepr path is complete end-to-end.

#### Task 4.4: Implement `GenericWatcher::start()` with `StreamConsumer`

Replace the log-and-return stub in
[`src/watcher/generic/watcher.rs`](../../src/watcher/generic/watcher.rs)
`start()`:

1. Build `rdkafka::ClientConfig` from `self.get_kafka_config()` (method already
   exists).
2. Create a `StreamConsumer` and subscribe to `&[&self.kafka_config.topic]`.
3. Add a `running: Arc<AtomicBool>` field to `GenericWatcher` (matching the
   XZepr consumer pattern) with a `stop()` method.
4. Enter a message-stream loop: for each message, extract the payload and call
   `self.process_payload(payload).await`.
5. `process_payload()` and `process_event()` are already fully implemented --
   they handle JSON deserialization, event-type gating, matcher evaluation, plan
   extraction, execution (or dry-run), and result publishing. Only the consume
   loop that feeds payloads in is missing.
6. Handle message processing errors gracefully (log and continue rather than
   crashing the consumer loop).

#### Task 4.5: Implement `GenericResultProducer::publish()` with `FutureProducer`

Replace the log-only stub in
[`src/watcher/generic/producer.rs`](../../src/watcher/generic/producer.rs)
`publish()`:

1. Add a `producer: FutureProducer` field to `GenericResultProducer`, created in
   `new()` from `self.get_kafka_config()` (method already builds the correct
   key-value pairs including `message.timeout.ms`).
2. In `publish()`, serialize `result` to JSON, then call
   `self.producer.send(FutureRecord::to(&self.output_topic).payload(&payload).key(&result.trigger_event_id), timeout).await`.
3. Map delivery errors to `XzatomaError` and return them.
4. Keep the existing `tracing::info!` log so published results remain
   observable.

#### Task 4.6: Implement `GenericWatcher::execute_plan()` with Real Execution

Replace the synthetic-success stub in
[`src/watcher/generic/watcher.rs`](../../src/watcher/generic/watcher.rs)
`execute_plan()`:

1. Call `crate::commands::run_plan_with_options()` (the same path the XZepr
   watcher's `WatcherMessageHandler` already uses) to execute the normalized
   plan text through the agent.
2. Capture the execution outcome (success/failure, output messages).
3. Construct a `GenericPlanResult` reflecting the actual result rather than a
   hardcoded success.
4. Respect the `execution_semaphore` that already exists on `GenericWatcher` to
   limit concurrent plan executions.
5. In dry-run mode, the existing behavior (synthetic success without execution)
   is correct and should be preserved.

#### Task 4.7: Fix CLI Gaps for Real Kafka Deployment

The current CLI is missing several flags needed for production Kafka use. Add to
the `Watch` command in [`src/cli.rs`](../../src/cli.rs):

| New Flag                      | Type             | Purpose                                                                                |
| ----------------------------- | ---------------- | -------------------------------------------------------------------------------------- |
| `--brokers`                   | `Option<String>` | Override `kafka.brokers` (most fundamental connection param has no CLI override today) |
| `--version` (generic matcher) | `Option<String>` | Override `generic_match.version` regex (config has it, CLI does not)                   |

Wire the `--filter-config` flag that already exists in the CLI struct but is
silently ignored in `apply_cli_overrides` -- either implement it (load the YAML
file and merge into `config.watcher.event_filter`) or remove the dead flag.

#### Task 4.8: Update `config/watcher.yaml` Example

Update the example configuration file to explicitly show:

1. The `watcher_type: xzepr` field with a comment listing valid values (`xzepr`,
   `generic`).
2. A complete `kafka` section with `brokers`, `topic`, `group_id`,
   `output_topic`, `auto_create_topics`, `num_partitions`, and
   `replication_factor`.
3. A `generic_match` section showing `action`, `name`, and `version` regex
   patterns.
4. A `security` section showing `protocol`, `sasl_mechanism`, `sasl_username`,
   and `sasl_password` fields.

#### Task 4.9: Testing Requirements

- `cargo test --all-features` passes (including all existing watcher unit
  tests).
- New integration tests for each replaced stub:
  - `XzeprConsumer::run()` with a mock Kafka broker or
    `rdkafka::mocking::MockCluster`.
  - `GenericWatcher::start()` end-to-end with a mock cluster producing a
    `GenericPlanEvent` and verifying a `GenericPlanResult` is published.
  - `GenericResultProducer::publish()` verifying delivery to a mock topic.
  - `WatcherTopicAdmin::ensure_topics()` verifying idempotent topic creation.
- Both `--watcher-type xzepr` and `--watcher-type generic` launch successfully
  in `--dry-run` mode with a valid config.
- The previously-ignored tests in `src/watcher/xzepr/consumer/client.rs` and
  `config.rs` (environment variable mutation) are migrated to use `temp-env` or
  `serial_test` and un-ignored.

#### Task 4.10: Deliverables

- `rdkafka` added to `Cargo.toml`
- Real `StreamConsumer` in `XzeprConsumer::run()` and `run_with_channel()`
- Real `StreamConsumer` consume loop in `GenericWatcher::start()`
- Real `FutureProducer` in `GenericResultProducer::publish()`
- Real `AdminClient` in `WatcherTopicAdmin::ensure_topics()`
- Real plan execution in `GenericWatcher::execute_plan()`
- `--brokers` and `--version` CLI flags added
- `--filter-config` flag either wired up or removed
- Updated `config/watcher.yaml` example
- Integration tests for each Kafka I/O path

#### Task 4.11: Success Criteria

Running `grep -r "stub" src/watcher/` returns zero results. Both watcher
backends (`xzepr` and `generic`) can connect to a real Kafka broker, consume
messages, execute plans, and publish results. The `--dry-run` flag works for
both backends. All existing unit tests continue to pass alongside the new
integration tests.

---

### Phase 5: Remaining Stub and Placeholder Resolution

Address non-Kafka stubs that silently fail or return dummy values in production
code paths.

#### Task 5.1: Wire Up `mcp list` Command

[`src/commands/mcp.rs` L49-52](../../src/commands/mcp.rs) prints "MCP support
not yet implemented" despite a full `McpClientManager` existing. Connect the
command handler to `McpClientManager` to list configured and connected servers,
their status, and available tools.

#### Task 5.2: Document MCP Task Manager Limitation

[`src/mcp/task_manager.rs`](../../src/mcp/task_manager.rs) is a placeholder.
[`src/mcp/manager.rs` L684-696](../../src/mcp/manager.rs) returns incomplete
initial responses for long-running tasks. Add clear warning logs and
user-visible messages when a task response is returned without polling. Add a
`tracing::warn!` that says "Long-running MCP task detected but polling is not
yet implemented; returning partial result."

#### Task 5.3: Document MCP Sampling and Elicitation Stubs

[`src/mcp/manager.rs` L388-399](../../src/mcp/manager.rs) logs "Registering
sampling handler" and "Registering elicitation handler" but registers nothing.
Either:

- (a) Remove the log messages so they do not mislead, or
- (b) Add `tracing::warn!` messages that explicitly say "sampling/elicitation
  not yet implemented; MCP servers requiring these capabilities will fail"

Also update [`src/mcp/elicitation.rs` L338](../../src/mcp/elicitation.rs)
`handle_url` to log a warning before returning `Cancel`.

#### Task 5.4: Fill Empty Test Bodies

[`src/providers/copilot.rs` L2912-2929](../../src/providers/copilot.rs) has two
empty tests (`test_set_model_with_valid_model` and
`test_set_model_with_invalid_model`) that always pass. Either:

- (a) Implement them with a mock HTTP server (using `wiremock` or `mockito`), or
- (b) Delete the empty test bodies and replace with `#[ignore]` and a reason
  string: `#[ignore = "requires mock HTTP server for Copilot API"]`

#### Task 5.5: Wire Up Mention Parser Search/Grep Execution

[`src/mention_parser.rs` L1327-1342](../../src/mention_parser.rs) parses
`@search:` and `@grep:` mentions but discards results with a debug log. Connect
them to the existing tool infrastructure:

1. For `@grep:` mentions, invoke the existing `GrepTool` executor from
   [`src/tools/grep.rs`](../../src/tools/grep.rs) with the parsed pattern and
   inject matching results into the prompt context, similar to how `@file:`
   mentions inject file contents.
2. For `@search:` mentions, treat them as aliases for `@grep:` with broader
   default settings (e.g., case-insensitive, all files) or connect to a
   `FindPathTool` for filename-based search.
3. Remove the `UrlContentCache` and `SearchResultsCache` dead-code structs in
   `mention_parser.rs` if they are not needed by this implementation, or wire
   them in as the caching layer for resolved mentions.
4. Add tests for `@grep:"pattern"` producing injected context content.

#### Task 5.6: Log Reasoning Content Instead of Dropping

[`src/providers/copilot.rs` L709-712](../../src/providers/copilot.rs) silently
drops `ResponseInputItem::Reasoning` content. Add a `tracing::debug!` log that
captures reasoning token count or a truncated preview so users can see that
reasoning occurred.

#### Task 5.7: Testing Requirements

- `mcp list` command has at least one integration test
- All previously-empty test bodies either have real assertions or are
  `#[ignore]` with documented reasons
- `cargo test` passes

#### Task 5.8: Deliverables

- Functional `mcp list` command
- Warning logs for all MCP stub code paths
- Empty test bodies resolved
- `@grep:` and `@search:` mentions wired to `GrepTool` / `FindPathTool`

#### Task 5.9: Success Criteria

No production code path silently returns dummy/empty values without a visible
warning log. No test body is empty. Running `grep -r "not yet implemented" src/`
returns zero results or only results accompanied by `tracing::warn!`.

---

### Phase 6: Documentation Overhaul

Update reference, how-to, and tutorial documentation to reflect the current
codebase. This phase depends on Phases 4 and 5 completing first so that watcher
Kafka implementations and MCP stub resolutions are reflected accurately in the
docs.

#### Task 6.1: Rewrite `docs/reference/quick_reference.md`

This file is the most stale document in the project. It references nonexistent
modules (`workflow/`, `repository/`, `docgen/`), nonexistent CLI commands
(`xzatoma scan`, `xzatoma generate`), wrong environment variable names, and a
wrong project structure. Rewrite it completely to reflect the actual module
layout (`agent/`, `providers/`, `tools/`, `commands/`, `mcp/`, `acp/`,
`skills/`, `storage/`, `watcher/`, `xzepr/`), the real CLI commands, and the
real configuration model.

#### Task 6.2: Add Missing CLI Commands to `docs/reference/cli.md`

[`docs/reference/cli.md`](../reference/cli.md) documents `chat`, `run`, `auth`,
`models`, and `history` but is missing four commands that exist in
[`src/cli.rs`](../../src/cli.rs):

1. **`watch`** -- document all flags (`--topic`, `--event-types`,
   `--filter-config`, `--log-file`, `--json-logs`, `--watcher-type`,
   `--group-id`, `--output-topic`, `--create-topics`, `--action`, `--name`,
   `--dry-run`, and the new `--brokers` and `--version` from Task 4.7).
2. **`mcp`** -- document `mcp list` subcommand (wired up in Task 5.1).
3. **`acp`** -- document `acp serve`, `acp config`, `acp runs`, `acp validate`
   subcommands.
4. **`skills`** -- document `skills list`, `skills validate`, `skills show`,
   `skills paths`, `skills trust` subcommands.

#### Task 6.3: Expand `docs/reference/configuration.md`

[`docs/reference/configuration.md`](../reference/configuration.md) is missing
several configuration sections:

1. **MCP configuration** -- the current MCP section is only 3 lines
   (`auto_connect`, `request_timeout_seconds`). Expand to document server
   definitions, transport options (`stdio`, `http`, `sse`), and auth
   configuration. Alternatively, cross-reference to a new
   `docs/reference/mcp_configuration.md`.
2. **Skills configuration** -- document `skills.*` fields (discovery paths,
   trust, activation).
3. **ACP configuration** -- add a section or cross-reference to the existing
   [`docs/reference/acp_configuration.md`](../reference/acp_configuration.md).
4. **Storage configuration** -- document persistence/storage fields if
   applicable.

#### Task 6.4: Create MCP Reference Documentation

The `src/mcp/` module is substantial (client, server, transport, auth,
tool_bridge, sampling, elicitation, protocol, task_manager) but has no reference
documentation outside `docs/explanation/`. Create:

1. `docs/reference/mcp_configuration.md` -- full MCP config reference covering
   server definitions, transport selection, auth flow, sampling/elicitation
   capability flags, and environment variables.
2. Update `docs/reference/api.md` to include the `xzatoma::mcp` public surface.

#### Task 6.5: Create Mention Syntax Reference

[`docs/how-to/use_context_mentions.md`](../how-to/use_context_mentions.md)
thoroughly documents `@file:`, `@dir:`, `@url:`, `@search:`, and `@grep:`
mention syntax, but there is no corresponding reference doc. Create
`docs/reference/mention_syntax.md` as a concise syntax reference companion
covering all mention types, their parameters, and resolution behavior.

#### Task 6.6: Update `docs/reference/architecture.md`

[`docs/reference/architecture.md`](../reference/architecture.md) has three
issues:

1. **Missing modules** -- add `src/skills/` and `src/acp/` to the top-level
   module structure diagram.
2. **Stub-first language** -- after Phase 4 Kafka migration, update L358
   (`consumer/kafka.rs` described as "stub-first"), L508-511
   (`GenericResultProducer` described as "stub-first path"), and L657-658
   ("dry-run still produces through stub-first producer") to reflect real
   `rdkafka` implementations.
3. Add brief architectural sections for the Skills and ACP subsystems.

#### Task 6.7: Update `docs/reference/api.md`

Add the following modules to the public surface summary:

- `xzatoma::mcp` -- MCP client, tool bridge, auth
- `xzatoma::acp` -- ACP server, runtime, types
- `xzatoma::skills` -- skill discovery, activation, trust
- `xzatoma::storage` -- persistence layer

Update the `xzatoma::commands` section to mention `watch`, `mcp`, `acp`, and
`skills` commands.

#### Task 6.8: Fix `docs/reference/provider_abstraction.md`

[`docs/reference/provider_abstraction.md`](../reference/provider_abstraction.md)
lists OpenAI and Anthropic as supported providers with environment variables and
config examples, but `src/providers/` only implements Copilot and Ollama. Either
clarify that OpenAI and Anthropic are documented for API comparison only (not
implemented as separate providers) or remove them from the "supported providers"
framing. Remove the suggested file layout showing `openai.rs` and `anthropic.rs`
which do not exist.

#### Task 6.9: Archive `docs/how-to/implementation_quickstart_checklist.md`

This file is an early-project implementation checklist referencing "Phase 1:
Foundation (Weeks 1-2)" through "Phase 4: Providers" with timeline estimates. It
has no current utility. Move it to `docs/archive/` or delete it.

#### Task 6.10: Update Watcher Documentation After Kafka Migration

After Phase 4 completes:

1. [`docs/how-to/setup_watcher.md`](../how-to/setup_watcher.md) -- add a note
   about the `--create-topics` flag and the new `--brokers` flag from Task 4.7.
2. Update [`config/watcher.yaml`](../../config/watcher.yaml) per Task 4.8.
3. [`docs/reference/watcher_environment_variables.md`](../reference/watcher_environment_variables.md)
   -- verify env var behavior is unchanged after `rdkafka` migration (it should
   be, since `get_kafka_config()` methods are preserved).

#### Task 6.11: Fix Phase References in Non-Explanation Docs

Clean up phase references found in reference, how-to, and tutorial docs:

| File                                  | Line(s)          | Action                                                    |
| ------------------------------------- | ---------------- | --------------------------------------------------------- |
| `docs/reference/acp_configuration.md` | L3-4             | Remove "Phase 5" from description                         |
| `docs/reference/model_management.md`  | L675-677         | Remove phase numbers from See Also links                  |
| `docs/reference/subagent_api.md`      | L328-330         | Remove "Phase 4+" from `persistence_enabled`              |
| `docs/reference/subagent_api.md`      | L526-527         | Remove "Phase 5 adds" -- document features if implemented |
| `docs/tutorials/subagent_usage.md`    | L214-218         | Remove "Phase 4+" from config comment                     |
| `docs/tutorials/subagent_usage.md`    | L382-387         | Rewrite "Next Steps" without phase references             |
| `docs/README.md`                      | L83-85, L125-128 | Remove stale status and phase numbering                   |

#### Task 6.12: Testing Requirements

- `markdownlint` passes on all new and modified `.md` files
- `prettier` formatting passes on all new and modified `.md` files
- All internal cross-references and links resolve correctly
- `cargo doc` builds without broken doc links

#### Task 6.13: Deliverables

- Rewritten `docs/reference/quick_reference.md`
- `docs/reference/cli.md` covering all CLI commands
- Expanded `docs/reference/configuration.md` with MCP, skills, ACP sections
- New `docs/reference/mcp_configuration.md`
- New `docs/reference/mention_syntax.md`
- Updated `docs/reference/architecture.md` with skills, ACP, and real Kafka
- Updated `docs/reference/api.md` with all public modules
- Fixed `docs/reference/provider_abstraction.md`
- Archived `docs/how-to/implementation_quickstart_checklist.md`
- Updated watcher docs reflecting real Kafka behavior
- Phase references cleaned from all non-explanation docs

#### Task 6.14: Success Criteria

Every CLI command has a corresponding section in `docs/reference/cli.md`. Every
configuration section has reference documentation. Running
`grep -ri "phase [0-9]" docs/reference/ docs/how-to/ docs/tutorials/` returns
zero results. The quick reference accurately reflects the project structure,
module layout, and CLI commands.

---

### Phase 7: Phase Reference Cleanup

Remove all stale "Phase N" references from source code, `docs/explanation/`
files, test files, and filenames. Phase 6 already handled reference, how-to, and
tutorial docs. This phase covers the remaining locations: source code comments,
explanation docs, and phase-named files.

#### Task 7.1: Update Project Status in Top-Level Files

Rewrite stale sections in these files to reflect current project state:

| File                                                                            | Section                                                   | Action                                   |
| ------------------------------------------------------------------------------- | --------------------------------------------------------- | ---------------------------------------- |
| [`README.md`](../../README.md) L249-263                                         | "Project Status" and "Implementation Phases"              | Rewrite to describe current capabilities |
| [`docs/README.md`](../README.md) L83-85                                         | "Phase: Planning Complete"                                | Replace with actual current status       |
| [`CONTRIBUTING.md`](../../CONTRIBUTING.md) L390-414                             | "Development Phases" and `phase-*` labels                 | Remove entire section                    |
| [`docs/explanation/competitive_analysis.md`](competitive_analysis.md) L11, L342 | "Design phase"                                            | Update to "Active development"           |
| [`AGENTS.md`](../../AGENTS.md) L255                                             | Example filename `phase4_observability_implementation.md` | Update to non-phase example              |
| [`Cargo.toml`](../../Cargo.toml) L79, L83                                       | `# Phase 4:` and `# Phase 5:` comments                    | Replace with descriptive comments        |

#### Task 7.2: Remove Phase Comments from Source Code

Replace "Phase N" with descriptive text in all `///`, `//!`, and `//` comments
across `src/`. Approximately 120 references in 40 files. Handle by module:

- **`src/acp/`** (~25 references): Replace "Phase 1 uses..." with "The current
  implementation uses...", "Phase 3 intentionally keeps..." with "Execution is
  intentionally kept simple...", etc.
- **`src/mcp/`** (~15 references): Replace "Phase 6 placeholder" with "Not yet
  implemented" or "Placeholder for future task polling support"
- **`src/skills/`** (~15 references): Replace "Phase 1 discovery" with "Skill
  discovery", "Phase 2 startup disclosure" with "Startup disclosure", etc.
- **`src/providers/copilot.rs`** test sections: Replace
  `// PHASE 1 TESTS: Core Data Structures` with `// Core Data Structure Tests`,
  etc.
- **`src/commands/`** (~5 references): Replace phase references with descriptive
  text
- **`src/watcher/`** (~7 references): Replace phase references
- **Remaining files** (`config.rs`, `mention_parser.rs`, `prompts/mod.rs`,
  `xzepr/mod.rs`): Replace individually

#### Task 7.3: Rename Phase-Named Files

Rename 16 documentation files and 1 test file. Update all internal
cross-references (links in `implementations.md`, other docs, `AGENTS.md`).

| Current Filename                                                             | New Filename                                            |
| ---------------------------------------------------------------------------- | ------------------------------------------------------- |
| `acp_phase1_implementation.md`                                               | `acp_domain_model_implementation.md`                    |
| `acp_phase2_implementation.md`                                               | `acp_http_discovery_implementation.md`                  |
| `acp_phase3_implementation.md`                                               | `acp_run_lifecycle_implementation.md`                   |
| `acp_phase4_implementation.md`                                               | `acp_persistence_implementation.md`                     |
| `agent_skills_phase1_implementation.md`                                      | `agent_skills_discovery_implementation.md`              |
| `agent_skills_phase2_implementation.md`                                      | `agent_skills_disclosure_implementation.md`             |
| `agent_skills_phase3_implementation.md`                                      | `agent_skills_activation_implementation.md`             |
| `agent_skills_phase4_implementation.md`                                      | `agent_skills_trust_implementation.md`                  |
| `generic_watcher_phase4_configuration_cli_implementation.md`                 | `generic_watcher_configuration_cli_implementation.md`   |
| `generic_watcher_phase5_integration_testing_documentation_implementation.md` | `generic_watcher_integration_testing_implementation.md` |
| `mcp_phase3_oauth_implementation.md`                                         | `mcp_oauth_implementation.md`                           |
| `mcp_phase5a_tool_bridge_implementation.md`                                  | `mcp_tool_bridge_implementation.md`                     |
| `phase1_xzepr_watcher_restructure_implementation.md`                         | `xzepr_watcher_restructure_implementation.md`           |
| `phase2_plan_action_field_implementation.md`                                 | `plan_action_field_implementation.md`                   |
| `phase4_mcp_client_lifecycle_implementation.md`                              | `mcp_client_lifecycle_implementation.md`                |
| `phase5b_sampling_elicitation_implementation.md`                             | `mcp_sampling_elicitation_implementation.md`            |
| `tests/acp_phase4_persistence.rs`                                            | `tests/acp_persistence.rs`                              |

#### Task 7.4: Update Test File Module Comments

Replace "Phase N:" module doc comments with descriptive text in all `tests/*.rs`
files (~15 files).

#### Task 7.5: Scrub Implementation Summary Docs

Update internal phase references within the renamed implementation summary
documents. Each document has dozens of "Phase N" references that should become
descriptive labels.

#### Task 7.6: Scrub Implementation Plan Docs

Lower priority. Update phase references in the ~8 implementation plan documents
under `docs/explanation/`. These are historical planning artifacts; the phase
structure is inherent to how they were written. Replace "Phase 1: Foundation"
style headers with "Stage 1: Foundation" or purely descriptive titles.

#### Task 7.7: Preserve Legitimate "Phase" Usage

Do NOT change these references where "phase" describes a runtime or protocol
concept rather than a development milestone:

- `src/mcp/protocol.rs` L3: "two phases of an MCP client session" (protocol
  concept)
- `src/acp/streaming.rs` L232: "the live phase is skipped" (runtime behavior)
- `docs/explanation/chat_modes_architecture.md` L184-188: "Planning Phase",
  "Implementation Phase" (user workflow stages)

#### Task 7.8: Testing Requirements

- `cargo test` passes after all renames
- `cargo doc` builds without broken links
- `markdownlint` passes on all modified `.md` files
- `prettier` formatting passes on all modified `.md` files
- `grep -ri "phase [0-9]" src/` returns only the preserved legitimate uses

#### Task 7.9: Deliverables

- All project status sections reflect current reality
- Zero stale "Phase N" comments in source code
- All 17 phase-named files renamed with updated cross-references
- All test files have descriptive module comments

#### Task 7.10: Success Criteria

Running `grep -ri "phase [0-9]" src/ tests/` returns only the 2-3 legitimate
runtime/protocol uses documented in Task 7.7. Running
`find docs/ -name "*phase*"` returns zero results.

## Resolved Decisions

1. **Typed errors**: Migrate to
   `pub type Result<T> = std::result::Result<T, XzatomaError>` and remove the
   `anyhow::Result` alias. This gives full `match` support on error variants
   without `downcast`. See Task 2.1.

2. **Watcher Kafka implementation**: Replace all Kafka I/O stubs with real
   `rdkafka` implementations. Add `rdkafka` as a dependency with `cmake-build`
   and `ssl` features. Both the XZepr and Generic watcher backends get real
   `StreamConsumer` consume loops, the Generic producer gets a real
   `FutureProducer`, topic admin gets a real `AdminClient`, and Generic plan
   execution calls `run_plan_with_options()` instead of returning synthetic
   success. See Phase 4.

3. **Mention search/grep**: Wire up `@search:` and `@grep:` mentions to the
   existing `GrepTool` and `FindPathTool` executors rather than removing the
   parsing code. See Task 5.5.
