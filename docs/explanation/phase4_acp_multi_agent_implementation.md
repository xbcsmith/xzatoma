# Phase 4: ACP Multi-Agent Infrastructure Implementation

## Overview

Phase 4 extends XZatoma with the infrastructure required for federated
multi-agent workflows. It adds per-agent configuration, an outbound ACP HTTP
client, three inter-agent tools (`call_acp_agent`, `discover_acp_agents`, and
`await_input`), and wires per-agent provider and system-prompt overrides into
the ACP executor.

## Tasks Completed

### Task 4.1: Multi-Agent Configuration (Gap 3h)

**File modified**: `src/config.rs`

Added `AcpAgentConfig` struct:

| Field                  | Type             | Default | Notes                          |
| ---------------------- | ---------------- | ------- | ------------------------------ |
| `name`                 | `String`         | n/a     | Must be non-blank              |
| `description`          | `String`         | `""`    |                                |
| `provider`             | `Option<String>` | `None`  | Overrides global provider      |
| `input_content_types`  | `Vec<String>`    | `[]`    |                                |
| `output_content_types` | `Vec<String>`    | `[]`    |                                |
| `thinking_mode`        | `Option<String>` | `None`  | Stored; logged when set        |
| `system_prompt`        | `Option<String>` | `None`  | Overrides global system prompt |

Added `agents: Vec<AcpAgentConfig>` to `AcpConfig` with `#[serde(default)]`.

Implemented
`AcpConfig::effective_agents(provider_type: &str) -> Vec<AcpAgentConfig>`:

- When `agents` is empty, returns a single synthesised entry:
  `name = "xzatoma"`, `provider = Some(provider_type)`.
- When `agents` is non-empty, returns the configured list as-is.

Validation rejects blank `agents[].name` and blank `agents[].system_prompt`.

### Task 4.2: Outbound ACP Client Configuration (Gap 3i)

**File modified**: `src/config.rs`

Added `AcpClientConfig` struct:

| Field                     | Type          | Default | Notes                                      |
| ------------------------- | ------------- | ------- | ------------------------------------------ |
| `default_timeout_seconds` | `u64`         | `30`    | `0` disables inter-agent tool registration |
| `allowed_base_urls`       | `Vec<String>` | `[]`    | SSRF allow-list; empty blocks all          |

Added `client: AcpClientConfig` to `AcpConfig` with `#[serde(default)]`.

Tool registration is gated in `src/tools/registry_builder.rs`: `call_acp_agent`
and `discover_acp_agents` are only registered when
`acp.client.default_timeout_seconds > 0`. When the value is `0`, these tools are
absent from the registry.

### Task 4.3: `call_acp_agent` Tool (Gap 4a)

**New file**: `src/tools/acp_agent.rs`

`AcpAgentTool` accepts three parameters:

| Parameter | Type     | Required | Description                       |
| --------- | -------- | -------- | --------------------------------- |
| `url`     | `String` | yes      | Base URL of the remote ACP server |
| `input`   | `String` | yes      | Message text to send              |
| `mode`    | `String` | yes      | `"sync"` or `"async"`             |

**Sync mode**:

1. Validates `url` against `acp.client.allowed_base_urls`. Returns a tool error
   immediately if not in the allow-list; no network call is made.
2. Calls `POST {url}/runs` with the input message and `mode = "sync"`.
3. Polls `GET {url}/runs/{run_id}` every 500 ms until the run reaches a terminal
   state (`completed`, `failed`, or `cancelled`).
4. Returns the run output text on success, or a tool error on
   failure/cancellation.

**Async mode**:

1. Validates `url` against the SSRF allow-list.
2. Calls `POST {url}/runs` with the input message.
3. Returns the `run_id` immediately without polling.

### Task 4.4: `discover_acp_agents` Tool (Gap 4b)

**New file**: `src/tools/acp_discover.rs`

`DiscoverAcpAgentsTool` accepts a single `url: String` parameter:

1. Validates `url` against `acp.client.allowed_base_urls`.
2. Calls `GET {url}/agents`.
3. Returns the agent list as pretty-printed JSON.

### Task 4.5: `await_input` Tool and `Awaiting` State (Gap 4c)

**New file**: `src/tools/await_input.rs`

`AwaitInputTool` is registered per-run by the ACP executor. On invocation:

1. Creates a `tokio::sync::oneshot` channel `(tx, rx)`.
2. Registers `tx` with the runtime via `AcpRuntime::register_await_channel`.
3. Transitions the run from `Running` to `Awaiting` via
   `AcpRuntime::set_awaiting`.
4. Blocks on `rx.await` until the resume payload arrives.
5. Returns the resume payload as the tool result.

**Files modified**:

- `src/acp/runtime.rs`:
  - Added `await_resume_tx: Option<oneshot::Sender<Value>>` to
    `AcpRuntimeRunRecord`.
  - Added `agent_name: Option<String>` to `AcpRuntimeRunRecord` (populated from
    `AcpRuntimeCreateRequest::agent_name` at run-creation time).
  - Added `AcpRuntime::register_await_channel` to store the oneshot sender.
  - Added `AcpRuntime::agent_name_for_run` accessor for the executor.
  - Modified `AcpRuntime::resume_run` to deliver the payload to the stored
    oneshot sender (in addition to updating `record.resume_payload`).

The `Awaiting` state, its transitions, persistence (`save_acp_await_state`,
`load_acp_await_state`), and the server-side `handle_resume_run` handler were
already fully implemented from prior phases. Phase 4 adds only the live channel
mechanism that allows the tool to block asynchronously rather than poll.

### Tool Registration

**File modified**: `src/tools/registry_builder.rs`

- Added `acp_client_config: Option<Arc<AcpClientConfig>>` field.
- Added `with_acp_client_config` builder method.
- Added `register_acp_inter_agent_tools` helper that conditionally registers
  `call_acp_agent` and `discover_acp_agents` in Write-mode builds.

**File modified**: `src/tools/mod.rs`

Added `pub mod acp_agent`, `pub mod acp_discover`, and `pub mod await_input`.

### Executor Per-Agent Overrides

**File modified**: `src/acp/executor.rs`

`execute_prompt` now:

1. Calls `runtime.agent_name_for_run(run_id)` to retrieve the optional named
   agent for the current run.
2. Looks up the matching `AcpAgentConfig` entry in `config.acp.agents`.
3. Applies provider override: `agent.provider` replaces
   `config.provider.provider_type` when set.
4. Applies system-prompt priority: `agent.system_prompt` > `acp.system_prompt` >
   `agent.system_prompt`.
5. Logs the `thinking_mode` override when set (full wiring to provider pending
   provider-level thinking-mode API).
6. Registers `AwaitInputTool` for every run, enabling agents to pause execution
   and await external input.

## Architecture Notes

### SSRF Allow-List

Both `call_acp_agent` and `discover_acp_agents` validate the caller-supplied URL
against `acp.client.allowed_base_urls` before making any network call. An empty
allow-list blocks all outbound calls. This is the primary SSRF mitigation; the
implementation does not perform IP-level resolution checks (unlike the `fetch`
tool's `SsrfValidator`), relying instead on the operator-maintained allow-list.

### Oneshot Channel Lifecycle

The oneshot channel for `await_input` is created inside the tool's `execute`
method, stored in the runtime before the state transition to `Awaiting`, and
consumed in `resume_run`. If the receiver is dropped (e.g. the run was cancelled
before a resume arrived), `tx.send` returns an error that is intentionally
ignored. The `resume_payload` field in `AcpRuntimeRunRecord` is still populated
for callers that poll state directly rather than using the channel.

### Per-Run Tool Registration

`AwaitInputTool` holds a clone of `AcpRuntime` and the run ID. It must not be
placed in a shared registry across runs; the executor registers it individually
for each run in `execute_prompt`.

## Testing

All new tests use `AcpRuntime::new_in_memory` to avoid writing to the production
`history.db`. See `AGENTS.md` for the rationale.

New tests added:

- `config::tests` -- 13 tests covering `AcpAgentConfig`, `AcpClientConfig`,
  `effective_agents`, and `validate_acp_config` error paths.
- `acp::runtime::tests` -- 2 tests for `register_await_channel` and the channel
  round-trip through `resume_run`.
- `tools::acp_agent::tests` -- 8 tests covering SSRF validation, mode rejection,
  tool-definition shape, and the no-network-call guarantee.
- `tools::acp_discover::tests` -- 6 tests covering SSRF validation, tool
  definition, and no-network-call guarantee.
- `tools::await_input::tests` -- 3 tests covering tool definition, Debug impl,
  and the full await/resume round-trip.
- `tools::registry_builder::tests` -- 3 tests verifying conditional registration
  when `default_timeout_seconds > 0`, `= 0`, and absent.
- `acp::executor::tests` -- 2 tests for per-agent system-prompt override and the
  fallback path when no matching agent is configured.

## Quality Gates

All four mandatory quality gates passed:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth
```

Result: **2566 passed, 0 failed**.
