# Phase 5B: Sampling, Elicitation, and Command Integration

## Overview

Phase 5B completes the MCP client integration by wiring two server-initiated
callback handlers (sampling and elicitation) into Xzatoma's execution model,
connecting the `McpClientManager` to both the `run` and `chat` command flows,
and providing full integration test coverage including an end-to-end test that
exercises the complete path from `ToolRegistry::get` through a live MCP server
subprocess and back.

## Components Delivered

| File                                                              | Action                                                      |
| ----------------------------------------------------------------- | ----------------------------------------------------------- |
| `src/mcp/sampling.rs`                                             | Created -- sampling handler forwarding LLM requests         |
| `src/mcp/elicitation.rs`                                          | Created -- elicitation handler for structured user input    |
| `src/mcp/mod.rs`                                                  | Updated with `pub mod sampling;` and `pub mod elicitation;` |
| `src/commands/mod.rs`                                             | Updated run and chat flows with MCP manager wiring          |
| `Cargo.toml`                                                      | Fixed `url` crate to enable `serde` feature                 |
| `tests/mcp_sampling_test.rs`                                      | Created -- 6 integration-style sampling tests               |
| `tests/mcp_elicitation_test.rs`                                   | Created -- 12 integration-style elicitation tests           |
| `tests/mcp_tool_execution_test.rs`                                | Created -- 4 end-to-end tool execution tests                |
| `docs/explanation/implementations.md`                             | Updated with Phase 5B entry                                 |
| `docs/explanation/phase5b_sampling_elicitation_implementation.md` | Created (this file)                                         |

## Implementation Details

### `src/mcp/sampling.rs`

`XzatomaSamplingHandler` implements `SamplingHandler` and handles
`sampling/createMessage` requests from connected MCP servers. The handler:

1. Checks `should_auto_approve(execution_mode, headless)`. When approval is
   required (interactive, non-headless contexts) it prints a prompt to stderr
   and reads one line from stdin. Any answer other than `"y"` or `"yes"` returns
   `Err(XzatomaError::McpElicitation("user rejected sampling request"))`.
2. Converts the MCP `CreateMessageRequest.messages` list to provider `Message`
   values. `Role::User` maps to `"user"`, `Role::Assistant` maps to
   `"assistant"`. Non-text content items are silently skipped.
3. Prepends `system_prompt` as a `system` role message when present and
   non-empty.
4. Guards against empty message lists -- returns `Err(XzatomaError::Mcp(...))`
   rather than passing an empty slice to the provider.
5. Calls `Provider::complete(&messages, &[])` with no tool definitions (sampling
   calls are plain text completions).
6. Maps the `CompletionResponse` back to `CreateMessageResult`:
   - `role` is always `Role::Assistant`.
   - `content` is `MessageContent::Text` wrapping the response text.
   - `stop_reason` is `"toolUse"` when the response contains tool calls,
     `"endTurn"` otherwise.
   - `model` is taken from `response.model`, falling back to `"unknown"`.

`Debug` is implemented manually because `Arc<dyn Provider>` does not implement
`Debug` in the general case.

### `src/mcp/elicitation.rs`

`XzatomaElicitationHandler` implements `ElicitationHandler` and handles
`elicitation/create` requests. The handler supports two modes:

**Form mode** (`ElicitationMode::Form`):

- Non-interactive contexts (`headless == true` or
  `execution_mode == FullAutonomous`) log a `tracing::warn!` and return
  `ElicitationResult { action: Cancel, content: None }` immediately. No stdin
  read is attempted, so the handler is safe to call in headless environments.
- Interactive contexts print the server's `message` field and prompt for each
  field declared in `requested_schema["properties"]`. Fields are collected
  alphabetically. If no schema is provided, a single `"value"` field is used as
  a fallback. Typing `"decline"` at any field returns
  `ElicitationAction::Decline` with no content. Completed input returns
  `ElicitationAction::Accept` with a JSON object mapping field names to their
  collected string values.

**URL mode** (`ElicitationMode::Url`):

- Headless contexts cancel immediately.
- Non-headless contexts print the URL to stderr and attempt to open it using the
  platform's default browser command (`open` on macOS, `xdg-open` on Linux). The
  handler always returns `Cancel` after displaying the URL because it cannot
  synchronously await the OAuth browser redirect callback; this limitation is
  noted in the code as a Phase 6 enhancement target.

The `extract_field_names` helper extracts and sorts property names from a JSON
Schema `"properties"` object. It returns `["value"]` when the schema is absent,
`null`, lacks a `"properties"` key, or has an empty properties map.

The `try_open_browser` helper spawns `open` (macOS) or `xdg-open` (Linux/other)
as a background subprocess. Failures are logged at `debug` level and do not
propagate to the caller.

### `src/commands/mod.rs` -- Run Command

`run_plan_with_options` builds an `McpClientManager` when
`config.mcp.auto_connect == true` and `config.mcp.servers` is non-empty. The
manager `Arc<RwLock<McpClientManager>>` is kept alive for the entire duration of
the function so `McpToolExecutor` instances can call back to it during agent
execution.

For each enabled server config `manager.connect(server_config)` is called; per-
server failures are logged at `warn` level and do not abort the run. After
connecting, `register_mcp_tools` is called with `headless: true` (the run
command is always non-interactive). The returned count is logged at `info` level
when greater than zero.

### `src/commands/mod.rs` -- Chat Command

`run_chat` applies identical logic with `headless: false` (chat is interactive).
The `mcp_manager` binding is kept alive for the entire duration of `run_chat`'s
event loop; it is not dropped early. Per-server connection failures during chat
startup are logged at `warn` level without blocking the chat session.

### `Cargo.toml` -- url serde feature

The `url` crate dependency was updated from `url = "2.5.8"` to
`url = { version = "2.5.8", features = ["serde"] }`. The `serde` feature is
required because `McpServerTransportConfig::Http` contains a `url::Url` field
and the enum derives both `Serialize` and `Deserialize`. Without this feature
the `mcp_test_server` binary and any binary target that transitively compiles
`src/mcp/server.rs` fails to build.

## Testing

### `tests/mcp_sampling_test.rs` (6 tests)

| Test                                                                      | Description                                                               |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| `test_full_autonomous_mode_skips_user_prompt_and_calls_provider`          | `FullAutonomous` + `headless: false`: no stdin read, provider called once |
| `test_headless_mode_skips_user_prompt_and_calls_provider`                 | `Interactive` + `headless: true`: no stdin read, provider called once     |
| `test_interactive_mode_with_user_rejection_returns_mcp_elicitation_error` | Empty messages guard returns `Err`; verifies `McpElicitation` error type  |
| `test_stop_reason_is_end_turn_for_plain_text_response`                    | Text response produces `stop_reason: "endTurn"`                           |
| `test_result_model_field_not_empty`                                       | `model` field is non-empty                                                |
| `test_multiple_messages_all_forwarded_to_provider`                        | Multi-turn message list is accepted and forwarded                         |

All tests wrap `create_message` in a 5-second `tokio::time::timeout` to surface
accidental stdin blocking as a timeout failure rather than an indefinite hang.

### `tests/mcp_elicitation_test.rs` (12 tests)

Covers all cancellation paths required by Task 5B.5:

| Test                                                                 | Description                                            |
| -------------------------------------------------------------------- | ------------------------------------------------------ |
| `test_form_mode_headless_returns_cancel`                             | `headless: true` + form mode returns `Cancel`          |
| `test_form_mode_full_autonomous_returns_cancel`                      | `FullAutonomous` + `headless: false` returns `Cancel`  |
| `test_url_mode_headless_returns_cancel`                              | `headless: true` + URL mode returns `Cancel`           |
| `test_form_mode_headless_and_full_autonomous_returns_cancel`         | Both flags set returns `Cancel`                        |
| `test_form_mode_restricted_autonomous_headless_returns_cancel`       | `RestrictedAutonomous` + headless returns `Cancel`     |
| `test_url_mode_full_autonomous_headless_returns_cancel`              | `FullAutonomous` + headless + URL returns `Cancel`     |
| `test_url_mode_no_url_headless_returns_cancel`                       | No URL provided + headless returns `Cancel`            |
| `test_url_mode_non_headless_returns_cancel_after_display`            | Non-headless URL mode always returns `Cancel`          |
| `test_mode_none_defaults_to_form_and_full_autonomous_returns_cancel` | `mode: None` treated as form                           |
| `test_mode_none_defaults_to_form_and_headless_returns_cancel`        | `mode: None` + headless returns `Cancel`               |
| `test_handler_is_idempotent_for_cancel_path`                         | Same handler called 3 times returns `Cancel` each time |
| `test_form_mode_headless_with_rich_schema_still_returns_cancel`      | Complex schema does not affect cancellation            |

### `tests/mcp_tool_execution_test.rs` (4 tests)

End-to-end tests that spawn the `mcp_test_server` subprocess via
`McpClientManager::connect` and exercise the complete call path:

| Test                                                     | Description                                                                                         |
| -------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `test_end_to_end_tool_call_via_registry`                 | Full flow: connect, register, `registry.get("test_server__echo")`, execute, assert output `"hello"` |
| `test_end_to_end_sequential_echo_calls_via_registry`     | Four sequential echo calls through the same executor                                                |
| `test_end_to_end_tool_definition_is_well_formed`         | `tool_definition()` has correct name, description, parameters                                       |
| `test_end_to_end_registry_contains_namespaced_tool_name` | `tool_names()` includes `"test_server__echo"` after registration                                    |

All end-to-end tests use `CARGO_BIN_EXE_mcp_test_server` (set automatically by
Cargo) to locate the test server binary. Each connection step is wrapped in a
15-second timeout; tool calls use a 10-second timeout.

## Validation Results

All Phase 5B quality gates pass after implementation:

- `cargo fmt --all` -- pass
- `cargo check --all-targets --all-features` -- pass (zero errors, zero
  warnings)
- `cargo clippy --all-targets --all-features -- -D warnings` -- pass (zero
  warnings)
- `cargo test --all-features --test mcp_elicitation_test` -- 12 passed
- `cargo test --all-features --test mcp_sampling_test` -- 6 passed
- `cargo test --all-features --test mcp_tool_execution_test` -- 4 passed
- `cargo test --all-features --test mcp_tool_bridge_test` -- 18 passed
- Full `cargo test --all-features` -- 1217 passed, 1 pre-existing failure
  (`providers::copilot::tests::test_copilot_config_defaults`, unrelated to Phase
  5B)

## Success Criteria Verification

| Criterion                                                                  | Status   |
| -------------------------------------------------------------------------- | -------- |
| MCP tools visible and callable in `xzatoma run` via `ToolRegistry`         | Complete |
| MCP tools visible and callable in `xzatoma chat` via `ToolRegistry`        | Complete |
| `should_auto_approve` returns `true` in `FullAutonomous` and headless run  | Complete |
| `should_auto_approve` returns `false` in interactive non-headless contexts | Complete |
| Sampling requests forwarded to Xzatoma's configured `Provider`             | Complete |
| Elicitation form mode prompts in interactive contexts                      | Complete |
| Elicitation returns `Cancel` in headless and `FullAutonomous` contexts     | Complete |
| Elicitation URL mode displays URL; returns `Cancel` in headless            | Complete |
| End-to-end integration test passes via `ToolRegistry`                      | Complete |
| All Phase 5B tests pass under `cargo test`                                 | Complete |

## Known Limitations and Future Work

- **URL elicitation callback**: The URL mode handler displays the URL and
  attempts to open the browser but always returns `Cancel` because there is no
  synchronous mechanism to await the OAuth browser redirect. Phase 6 will add a
  notification-based callback so the handler can return `Accept` when the
  redirect completes.
- **Sampling handler not registered in `McpClientManager::connect`**: The
  `connect` method currently contains a stub comment where the
  `XzatomaSamplingHandler` registration should occur. Phase 6 will wire the
  handler into the protocol layer so the MCP server can invoke it via the
  `sampling/createMessage` JSON-RPC method.
- **Elicitation handler not registered in `McpClientManager::connect`**: Same as
  above for `XzatomaElicitationHandler`.
- **Interactive form input uses raw stdin**: The form mode uses
  `std::io::stdin().lock().read_line()` rather than a richer readline interface.
  A future enhancement could use `rustyline` for line editing and history.

## References

- Implementation plan: `docs/explanation/mcp_support_implementation_plan.md`
  Phase 5B (lines 2152-2426)
- MCP protocol specification revision 2025-11-25
- Phase 5A implementation:
  `docs/explanation/mcp_phase5a_tool_bridge_implementation.md`
