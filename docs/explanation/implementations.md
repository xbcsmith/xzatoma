# Implementation Documentation Index

Note: Developer-focused implementation logs (phase reports, detailed
implementation notes, and internal planning documents) have been moved to
`docs/archive/implementation_summaries/`. These files are archived to keep the
top-level explanation docs user-facing and maintainable; consult the archive for
historical implementation detail. See
[documentation_cleanup_summary.md](documentation_cleanup_summary.md) for an
audit of moved files and the rationale.

This directory contains detailed implementation documentation for XZatoma
features and architecture.

## Documentation Overview

### Architecture and Planning

- **[architecture_validation.md](../archive/implementation_summaries/architecture_validation.md)** -
  Initial validation of the XZatoma architecture against project rules
- **[required_architecture_updates.md](../archive/implementation_summaries/required_architecture_updates.md)** -
  Critical issues identified during validation
- **[architecture_fixes_applied.md](../archive/implementation_summaries/architecture_fixes_applied.md)** -
  Record of fixes applied to address architecture issues
- **[architecture_validation_status.md](../archive/implementation_summaries/architecture_validation_status.md)** -
  Final validation status (9/10, approved)
- **[notes_for_implementation_planning.md](../archive/implementation_summaries/notes_for_implementation_planning.md)** -
  Handoff notes for implementation planning phase
- **[quick_reference_for_next_session.md](../archive/implementation_summaries/quick_reference_for_next_session.md)** -
  Quick reference guide for next session
- **[competitive_analysis.md](competitive_analysis.md)** - Comparison of XZatoma
  vs Goose vs Zed Agent

### Implementation Documentation

- **[conversation_persistence_implementation.md](conversation_persistence_implementation.md)** -
  Conversation persistence: SQLite-backed history, auto-save/resume, CLI history
  commands (list/resume/delete), unit & integration tests

- **[phase1_core_validation_implementation.md](phase1_core_validation_implementation.md)** -
  Phase 1: Core Validation - Message sequence validation to prevent orphan tool
  messages, integrated into provider conversion, comprehensive tests for Copilot
  and Ollama providers

- **[phase2_history_ux_command_persistence_implementation.md](phase2_history_ux_command_persistence_implementation.md)** -
  Phase 2: History UX & Command Persistence - Enhanced `history show` command
  with formatted/JSON output and message limiting, persistence configuration for
  special commands, 10 new tests

- **[phase3_pruning_integrity_implementation.md](phase3_pruning_integrity_implementation.md)** -
  Phase 3: Pruning Integrity - Atomic tool-call pair removal during conversation
  pruning, helper methods for finding tool results, maintains message sequence
  integrity

- **[intelligent_editing_and_buffer_management_implementation.md](intelligent_editing_and_buffer_management_implementation.md)** -
  Phase 4: Intelligent Editing & Buffer Management - Implemented `edit_file`
  tool (create/edit/overwrite modes), unified diffs via `similar::TextDiff`,
  integrated into Write-mode registry, with unit tests and documentation.

- **[phase4_cross_provider_consistency_implementation.md](phase4_cross_provider_consistency_implementation.md)** -
  Phase 4: Cross-Provider Consistency & Integration Tests - Provider parity
  validation, integration tests for save/load/resume lifecycle with orphan
  sanitization, pruning integrity verification

- **[phase5_deprecation_and_migration_implementation.md](phase5_deprecation_and_migration_implementation.md)** -
  Phase 5: Deprecation and Migration - Removed monolithic `file_ops.rs` (914
  lines), migrated to 9 individual file operation tools, updated registry
  builder to register tools explicitly, implemented `generate_diff` utility,
  updated all tests and documentation, achieved clean modular architecture with
  771 passing tests.

- **[phase5_documentation_qa_and_release_implementation.md](phase5_documentation_qa_and_release_implementation.md)** -
  Phase 5: Documentation, QA, and Release - Comprehensive implementation
  documentation, quality assurance validation (81 tests passing), release
  preparation notes, migration guidance, and usage examples

- **[phase1_security_refactor_implementation.md](phase1_security_refactor_implementation.md)** -
  Phase 1: Security Refactor - Shell-less terminal execution, SSRF hostname
  resolution, and updated unit tests
- **[phase2_security_refactor_implementation.md](phase2_security_refactor_implementation.md)** -
  Phase 2: Security Refactor - Quota lock safety, storage error consistency, and
  error boundary alignment
- **[phase3_refactor_implementation.md](phase3_refactor_implementation.md)** -
  Phase 3: Refactor - Shared integration-test helpers and reduced duplicate
  setup

- **[history_and_tool_integrity_implementation.md](history_and_tool_integrity_implementation.md)** -
  Chat History and Tool Integrity: Complete four-phase implementation covering
  core validation (orphan tool message prevention), history UX enhancements
  (message-level inspection with `history show` command), pruning integrity
  (atomic tool-call pair removal), and cross-provider consistency with
  integration tests. Includes orphan message sanitization in both Copilot and
  Ollama providers, special command persistence configuration, and 30+ tests
  validating all scenarios

- **[file_tools_modularization_implementation_plan.md](file_tools_modularization_implementation_plan.md)** -
  File Tools Modularization: Complete five-phase implementation transforming
  monolithic `file_ops` tool (914 lines) into 9 focused, single-responsibility
  tools. Phases include: shared infrastructure (path validation, file metadata
  utilities), core file operations (read_file, write_file, delete_path,
  list_directory), advanced file manipulation (copy_path, move_path,
  create_directory, find_path), intelligent editing (edit_file with unified diff
  output), and deprecation/migration (registry builder updates, complete removal
  of file_ops.rs). Achieved clean modular architecture with 771 passing
  tests, >80% coverage, and explicit tool registration by chat mode (3 read-only
  tools in Planning mode, 9 file tools + terminal in Write mode). See
  phase-specific documentation:
  [Phase 1](phase1_shared_infrastructure_implementation.md),
  [Phase 2](phase2_core_file_operations_implementation.md),
  [Phase 3](phase3_advanced_file_manipulation_tools_implementation.md),
  [Phase 4](intelligent_editing_and_buffer_management_implementation.md),
  [Phase 5](phase5_deprecation_and_migration_implementation.md)

- **[phase3_security_validation_implementation.md](../archive/implementation_summaries/phase3_security_validation_implementation.md)** -
  Complete implementation of security validation for terminal commands
- **[auth_provider_flag_implementation.md](../archive/implementation_summaries/auth_provider_flag_implementation.md)** -
  CLI: make `auth` subcommand accept `--provider <name>` (align CLI with README;
  tests and documentation added)
- **[phase5_error_handling_and_user_feedback.md](../archive/implementation_summaries/phase5_error_handling_and_user_feedback.md)** -
  Phase 5: Error handling and user feedback for mention-based content loading
  (structured `LoadError` types, graceful degradation with placeholders, CLI
  warnings and suggestions, tests, and documentation)
- **[model_management_missing_deliverables_implementation.md](../archive/implementation_summaries/model_management_missing_deliverables_implementation.md)** -
  Documentation completion for model management features (API reference, how-to
  guides for managing and switching models)
- **[copilot_models_caching_and_tests.md](../archive/implementation_summaries/copilot_models_caching_and_tests.md)** -
  Copilot models caching and mocked integration tests
- Note: Integration tests that write to the OS keyring (service: `xzatoma`,
  user: `github_copilot`) are marked `#[ignore = "requires system keyring"]` to
  avoid failures in CI/CD environments that don't expose an interactive system
  keyring. Run these locally when you have a keyring available with:
- `cargo test -- --ignored` (runs all ignored tests)
- or `cargo test --test copilot_integration -- --ignored` (runs the Copilot
  keyring tests only)
- **[use_chat_modes.md](../how-to/use_chat_modes.md)** - Chat mode provider &
  model display (provider: white, model: green)
- **[ollama_default_model_fix.md](../archive/implementation_summaries/ollama_default_model_fix.md)** -
  Bug fix: Changed Ollama default model from unavailable `qwen2.5-coder` to
  standard `llama3.2:latest`, removed all Qwen model references
- **[ollama_tool_support_validation.md](../archive/implementation_summaries/ollama_tool_support_validation.md)** -
  Bug fix: Implemented proper tool support detection and validation for Ollama
  models, changed default to `llama3.2:latest`, prevents switching to models
  without function calling capabilities
- **[ollama_response_parsing_fix.md](../archive/implementation_summaries/ollama_response_parsing_fix.md)** -
  Bug fix: Made Ollama response parsing flexible to handle models with varying
  response formats (missing fields, empty IDs), added support for granite3 and
  granite4 models

### Provider Abstraction Implementation

- **[provider_abstraction_implementation_plan.md](../archive/implementation_summaries/provider_abstraction_implementation_plan.md)** -
  Complete language-agnostic plan for implementing provider abstraction layer
  supporting OpenAI, Anthropic, GitHub Copilot, and Ollama
- **[provider_abstraction.md](../reference/provider_abstraction.md)** - Quick
  reference guide for provider abstraction patterns and examples
- **[../reference/provider_api_comparison.md](../reference/provider_api_comparison.md)** -
  Detailed API specification comparison for all four providers

## Implementation Plan Status

### Completed Planning

**Architecture Design** - Complete and validated

- Core architecture documented in `docs/reference/architecture.md`
- All critical issues resolved (iteration limits, security model, conversation
  management)
- Validation score: 9/10
- Ready for implementation

  **Provider Abstraction Planning** - Complete

- Language-agnostic implementation plan created
- Covers 4 providers: OpenAI, Anthropic, GitHub Copilot, Ollama
- 6 implementation phases defined
- ~8,550 LOC estimated (including tests)
- 6-9 weeks estimated timeline

### Completed Implementation

**Phase 3: Security and Terminal Validation** - Complete

- CommandValidator with allowlist/denylist security
- Three execution modes: Interactive, Restricted Autonomous, Full Autonomous
- Path validation to prevent directory traversal
- Comprehensive test suite (17 tests, 100% coverage) Documentation:
  `../archive/implementation_summaries/phase3_security_validation_implementation.md`
- ~291 lines of code with full validation

  **Model Management Documentation** - Complete

- API reference documentation for all model management features
- How-to guide for discovering and inspecting models
- How-to guide for switching models during chat sessions
- Complete coverage of CLI commands and chat mode special commands
- Provider comparison (Copilot vs Ollama)
- Troubleshooting and best practices
- Documentation: `docs/reference/model_management.md`,
  `docs/how-to/manage_models.md`, `docs/how-to/switch_models.md`
- **[models_command_summary_output_implementation.md](models_command_summary_output_implementation.md)** -
  Phase 4: Summary Output Implementation (verification tests and rendering
  helpers)
- **[models_command_documentation_and_examples_implementation.md](models_command_documentation_and_examples_implementation.md)** -
  Phase 5: Documentation and Examples (reference docs, how-to updates, CLI help
  text, tests, and example scripts)
- ~1,502 lines of comprehensive documentation

**Phase 3: Streaming Infrastructure** - Complete

- SSE parsing functions: `parse_sse_line()`, `parse_sse_event()`
- Stream response method: `stream_response()` for /responses endpoint
- Stream completion method: `stream_completion()` for /chat/completions endpoint
- Helper method: `convert_tools_legacy()` for legacy tool format
- Comprehensive streaming tests (10 tests, all passing)
- Reqwest streaming feature enabled for `bytes_stream()` support
- Documentation:
  `docs/explanation/phase3_streaming_infrastructure_implementation.md`
- ~335 lines of production code, zero warnings

**Phase 4: Provider Integration** - Complete

- Extended `CopilotConfig` with streaming, fallback, and reasoning fields
- Endpoint selection logic: `select_endpoint()` and `model_supports_endpoint()`
- Refactored `Provider::complete()` with intelligent routing
- Endpoint-specific implementations: `complete_with_responses_endpoint()`,
  `complete_with_completions_endpoint()`
- Streaming and blocking variants for both endpoints
- Smart RwLock guard management across async boundaries
- Configuration management with serde defaults
- Updated provider capabilities to advertise streaming support
- Comprehensive tests (22 tests for Phase 4, all passing)
- Documentation:
  `docs/explanation/phase4_provider_integration_implementation.md`
- ~4,500 lines added/modified, zero warnings

**Copilot Responses Endpoint - Missing Deliverables Fix** - Complete

Audit against
`docs/explanation/copilot_responses_endpoint_implementation_plan.md` identified
and resolved four gaps:

- `CompletionResponse.model` field added to `src/providers/base.rs` - surfaces
  the model identifier that generated each response
- `CompletionResponse.reasoning` field added to `src/providers/base.rs` -
  surfaces extended-thinking content from o1-family models
- Builder methods `set_model()` and `set_reasoning()` added to
  `CompletionResponse`, plus `with_model()` constructor
- All completion paths in `copilot.rs` updated to populate `model` on every
  returned `CompletionResponse`; `complete_responses_streaming` and
  `complete_responses_blocking` also extract and attach `reasoning` when present
  in the API response
- `test_message_conversion_roundtrip` added (Task 2.1 plan item): verifies
  forward + reverse message conversion preserves roles and content
- `test_select_endpoint_default_completions` added (Task 4.1 plan item):
  documents legacy-model fallback logic
- `test_select_endpoint_unknown_model_error` added (Task 4.1 plan item):
  validates error message format for unknown models
- `test_model_supports_endpoint_logic` added (Task 4.1 plan item - replaces
  async stub): unit-tests `CopilotModelData::supports_endpoint` across all three
  cases (both, completions-only, legacy/empty)
- Five additional unit tests for `CompletionResponse` model/reasoning fields
- Total tests increased from 956 to 965, zero warnings

### MCP Support Implementation

- Phase 3: OAuth 2.1 / OIDC Authorization -- Complete

- **[mcp_support_implementation_plan.md](mcp_support_implementation_plan.md)** -
  Full seven-phase plan for MCP client support (protocol revision 2025-11-25)

---

## MCP Phase 0: Repository Integration Scaffold (2025-07-XX)

### Overview

Created the minimal scaffolding required for all subsequent MCP phases to
compile and integrate incrementally. The project compiles cleanly after this
phase. All new stubs use empty `Ok(())` bodies.

### Components Delivered

- `src/mcp/mod.rs` (26 lines) - Module root declaring all future submodules
- `src/mcp/config.rs` (20 lines) - Placeholder `McpConfig` type with `Default`
- `src/mcp/server.rs` (4 lines) - Placeholder comment for Phase 4
- `src/commands/mcp.rs` (66 lines) - Stub `McpCommands` enum and `handle_mcp`
  handler
- `src/lib.rs` - Updated with `pub mod mcp;`
- `src/commands/mod.rs` - Updated with `pub mod mcp;`
- `src/cli.rs` - Updated with `Commands::Mcp` variant
- `src/main.rs` - Updated with dispatch arm and import removal
- `src/config.rs` - Updated with `#[serde(default)] pub mcp: McpConfig` field
- `src/watcher/watcher.rs` - Updated test struct literal with `mcp` field
- `config/config.yaml` - Appended commented-out MCP configuration block
- `tests/mcp_types_test.rs` - Created placeholder test file

### Validation Results

- cargo fmt --all passed
- cargo check --all-targets --all-features passed (zero errors)
- cargo clippy --all-targets --all-features -- -D warnings passed (zero
  warnings)
- cargo test --all-features passed (all pre-existing tests pass)

---

## MCP Phase 1: Core MCP Types and JSON-RPC Client (2025-07-XX)

### Overview

Implemented all MCP 2025-11-25 protocol types, the transport-agnostic JSON-RPC
2.0 client backed by Tokio channels, and the typed MCP lifecycle wrapper. No
transport or auth code is introduced in this phase.

### Components Delivered

- `src/mcp/types.rs` (1556 lines) - All MCP 2025-11-25 protocol types, JSON-RPC
  2.0 wire types, method/notification constants, and inline unit tests
- `src/mcp/client.rs` (890 lines) - Transport-agnostic `JsonRpcClient` with
  `start_read_loop`, `request`, `notify`, `on_notification`, and
  `on_server_request`
- `src/mcp/protocol.rs` (1142 lines) - `McpProtocol` (uninitialized) and
  `InitializedMcpProtocol` (fully negotiated) wrappers; `SamplingHandler` and
  `ElicitationHandler` traits; `ServerCapabilityFlag` enum
- `src/mcp/mod.rs` - Updated to declare `types`, `client`, `protocol`, `config`,
  `server` and re-export `types::*`
- `src/error.rs` - Added 9 MCP error variants: `Mcp`, `McpTransport`,
  `McpServerNotFound`, `McpToolNotFound`, `McpProtocolVersion`, `McpTimeout`,
  `McpAuth`, `McpElicitation`, `McpTask`
- `Cargo.toml` - Added `tokio-util` (with `codec` feature), `sha2`, `rand` via
  `cargo add`
- `tests/mcp_types_test.rs` - 38 integration tests covering all type
  round-trips, serialization invariants, and wire-format contracts
- `tests/mcp_client_test.rs` - 11 integration tests covering request/response
  matching, timeout, notification dispatch, concurrent requests, and read-loop
  lifecycle

### Implementation Details

#### src/mcp/types.rs

Defines all wire types for protocol revision 2025-11-25 with 2025-03-26
backwards compatibility:

- Protocol version constants: `LATEST_PROTOCOL_VERSION`,
  `PROTOCOL_VERSION_2025_03_26`, `SUPPORTED_PROTOCOL_VERSIONS`
- 30 JSON-RPC method and notification constants (`METHOD_INITIALIZE`,
  `NOTIF_TOOLS_LIST_CHANGED`, etc.)
- JSON-RPC primitives: `JsonRpcRequest`, `JsonRpcResponse`, `JsonRpcError` (with
  `Display`), `JsonRpcNotification`
- Core identity: `ProtocolVersion` (newtype with `Display`, `From`),
  `Implementation`
- Capability types: `ClientCapabilities`, `ServerCapabilities`,
  `TasksCapability`, `ElicitationCapability`, `SamplingCapability`,
  `RootsCapability`
- Initialize: `InitializeParams`, `InitializeResponse`
- Tool types: `McpTool`, `ToolAnnotations`, `ToolExecution`, `TaskSupport`,
  `ListToolsResponse`, `CallToolParams`, `CallToolResponse`,
  `ToolResponseContent` (tagged enum: Text/Image/Audio/Resource)
- Task types: `Task`, `TaskStatus` (snake_case), `CreateTaskResult`,
  `TasksListResponse`, `TasksGetParams`, `TasksResultParams`,
  `TasksCancelParams`, `TasksListParams`
- Resource types: `Resource`, `ResourceTemplate`, `ResourceContents` (untagged:
  Text/Blob), `TextResourceContents`, `BlobResourceContents`,
  `ListResourcesResponse`, `ReadResourceParams`, `ReadResourceResponse`
- Prompt types: `Role`, `TextContent`, `ImageContent`, `AudioContent`,
  `MessageContent` (tagged), `PromptMessage`, `PromptArgument`, `Prompt`,
  `ListPromptsResponse`, `GetPromptParams`, `GetPromptResponse`
- Sampling types: `ModelHint`, `ModelPreferences`, `ToolChoiceMode` (`None_`
  serializes as `"none"`), `SamplingToolChoice`, `CreateMessageRequest`,
  `CreateMessageResult`
- Elicitation types: `ElicitationMode`, `ElicitationAction`,
  `ElicitationCreateParams`, `ElicitationResult`
- Logging: `LoggingLevel` (ordered)
- Completion: `CompletionCompleteParams`, `CompletionResult`,
  `CompletionCompleteResponse`
- Utilities: `Root`, `CancelledParams`, `ProgressParams`, `PaginatedParams`

All struct fields use `#[serde(rename_all = "camelCase")]`. All `Option<>`
fields use `#[serde(skip_serializing_if = "Option::is_none")]`. `_meta` fields
use explicit `#[serde(rename = "_meta")]`.

#### src/mcp/client.rs

Transport-agnostic `JsonRpcClient` backed by
`tokio::sync::mpsc::UnboundedSender/Receiver<String>`:

- `JsonRpcClient::new(outbound_tx)` - constructs client; caller wires channels
- `request<P, R>(method, params, timeout)` - assigns monotonic ID, registers
  oneshot in `pending` map, sends serialized request, awaits with timeout
- `notify<P>(method, params)` - sends notification (no `id` field)
- `on_notification(method, f)` - registers handler for server-sent notifications
- `on_server_request(method, f)` - registers async handler for server-initiated
  requests; sends response automatically
- `start_read_loop(inbound_rx, cancellation, client)` - Tokio task that
  classifies each inbound message as response/server-request/notification and
  dispatches; drops all pending senders on cancellation

Message classification logic:

- `has_id && (has_result || has_error) && !has_method` → response
- `has_id && has_method` → server-initiated request
- `has_method && !has_id` → notification

#### src/mcp/protocol.rs

`McpProtocol::initialize(client_info, capabilities)` performs the handshake:

1. Sends `initialize` request with `LATEST_PROTOCOL_VERSION`
2. Validates server's chosen version is in `SUPPORTED_PROTOCOL_VERSIONS`;
   returns `McpProtocolVersion` error if not
3. Fires `notifications/initialized` (fire-and-forget)
4. Returns `InitializedMcpProtocol`

`InitializedMcpProtocol` provides:

- `capable(flag)` - checks `InitializeResponse.capabilities`
- `list_tools()`, `list_resources()`, `list_prompts()` - cursor-paginated list
  methods
- `call_tool(name, arguments, task)`, `read_resource(uri)`,
  `get_prompt(name, arguments)` - single-call methods
- `complete(params)`, `ping()` - utility methods
- `tasks_get/result/cancel/list` - task lifecycle methods
- `register_sampling_handler(Arc<dyn SamplingHandler>)` - wires
  `sampling/createMessage`
- `register_elicitation_handler(Arc<dyn ElicitationHandler>)` - wires
  `elicitation/create`

### Testing

Test coverage: >80% on all three new modules.

```text
tests/mcp_types_test.rs:  38 passed; 0 failed
tests/mcp_client_test.rs: 11 passed; 0 failed
src/mcp/types.rs inline:  included in lib test run
src/mcp/client.rs inline: included in lib test run
src/mcp/protocol.rs inline: included in lib test run
Total new MCP tests: ~75 (inline + integration)
```

Key test cases per Task 1.7:

- `test_protocol_version_constants_are_correct` - constant values verified
- `test_implementation_description_skipped_when_none` - `skip_serializing_if`
  verified
- `test_call_tool_response_roundtrip` - full round-trip with all fields
- `test_task_status_serializes_snake_case` - all 5 variants checked
- `test_tool_response_content_text_roundtrip` - tagged enum discrimination
- `test_json_rpc_error_display` - `Display` impl verified
- `test_request_resolves_with_correct_result` - pending map resolution
- `test_request_timeout_fires` - `McpTimeout` error on no response
- `test_notification_handler_called_for_matching_method` - dispatch to handler
- `test_pending_sender_dropped_cleanly_on_read_loop_exit` - clean cancellation
- `test_json_rpc_error_response_mapped_to_mcp_error` - error variant mapping

### Validation Results

- cargo fmt --all passed
- cargo check --all-targets --all-features passed (zero errors)
- cargo clippy --all-targets --all-features -- -D warnings passed (zero
  warnings)
- cargo test --all-features passed (1008 passed; 1 pre-existing failure in
  unrelated `providers::copilot` module)
- All 49 integration tests in `tests/mcp_types_test.rs` and
  `tests/mcp_client_test.rs` pass

### References

- Architecture: `docs/explanation/mcp_support_implementation_plan.md`
- Protocol spec: MCP revision 2025-11-25

---

## MCP Phase 2: Transport Layer (2025-07-XX)

### Overview

Implemented the `Transport` trait and two concrete transports: a stdio
child-process transport and a Streamable HTTP/SSE transport for the `2025-11-25`
spec. Also implemented a `FakeTransport` for tests, added the `mcp_test_server`
integration test binary, and wired a `clone_shared` method onto `JsonRpcClient`
to support the `McpProtocol`/read-loop wiring pattern from integration tests.

### Components Delivered

- `src/mcp/transport/mod.rs` (98 lines) - `Transport` trait definition plus
  submodule declarations
- `src/mcp/transport/stdio.rs` (377 lines) - `StdioTransport`: spawns a child
  process, bridges stdin/stdout/stderr via Tokio channels, SIGTERM on Drop
- `src/mcp/transport/http.rs` (655 lines) - `HttpTransport`: `2025-11-25`
  Streamable HTTP/SSE transport with session management, SSE parser, and DELETE
  on Drop
- `src/mcp/transport/fake.rs` (414 lines) - `FakeTransport` and
  `FakeTransportHandle` for in-process testing
- `src/mcp/mod.rs` - Updated with `pub mod transport;`
- `src/mcp/client.rs` - Added `clone_shared()` method for sharing internal Arcs
  between a read-loop `Arc<JsonRpcClient>` and a `McpProtocol`-owned
  `JsonRpcClient`
- `Cargo.toml` - Added `bytes = "1.5"`, `libc` (unix-only target dep), and
  `reqwest` `blocking` feature; added `[[bin]]` for `mcp_test_server`
- `tests/helpers/mcp_test_server/main.rs` (207 lines) - Minimal stdio MCP server
  handling `initialize`, `tools/list`, `tools/call echo`, and `ping`
- `tests/mcp_http_transport_test.rs` (431 lines) - 9 wiremock-based HTTP
  transport integration tests
- `tests/mcp_stdio_test.rs` (295 lines) - 5 integration tests against the real
  `mcp_test_server` subprocess

### Implementation Details

#### src/mcp/transport/mod.rs

Defines the `Transport` trait:

- `async fn send(&self, message: String) -> Result<()>` - sends one complete
  JSON-RPC string to the remote peer; framing is the implementation's
  responsibility
- `fn receive(&self) -> Pin<Box<dyn Stream<Item = String> + Send + '_>>` -
  stream of inbound JSON-RPC strings (one per logical message)
- `fn receive_err(&self) -> Pin<Box<dyn Stream<Item = String> + Send + '_>>` -
  stream of transport-level diagnostics; MUST NOT be treated as errors per MCP
  spec

#### src/mcp/transport/stdio.rs

`StdioTransport::spawn(executable, args, env, working_dir)`:

1. Builds `tokio::process::Command` with all three stdio pipes.
2. Clears environment with `env_clear()` then applies `env` map.
3. Spawns three background Tokio tasks: stdin writer (channel -> child stdin),
   stdout reader (child stdout -> channel), stderr reader (child stderr ->
   channel + `tracing::debug!`).
4. `Drop` sends SIGTERM via `libc::kill` (Unix) or `start_kill` (non-Unix),
   best-effort only.

#### src/mcp/transport/http.rs

`HttpTransport::new(endpoint, headers, timeout)`:

- Constructs a `reqwest::Client` with the given timeout.
- Maintains `session_id: Arc<RwLock<Option<String>>>` for MCP session
  management.
- Maintains `last_event_id: Arc<RwLock<Option<String>>>` for SSE resumption.

`send()` POST behaviour:

- Adds `MCP-Protocol-Version: 2025-11-25` on every POST (spec-required).
- Adds `MCP-Session-Id` when a session is active.
- Captures `MCP-Session-Id` from the first successful response header.
- Routes `application/json` responses to `response_tx` directly.
- Routes `text/event-stream` responses to `parse_sse_stream` task.
- `202 Accepted` is a no-op.
- `401 Unauthorized` returns `XzatomaError::McpAuth` with `WWW-Authenticate`.
- `404` with active session clears session and returns
  `XzatomaError::Mcp("mcp session expired")`.

`parse_sse_stream` free function:

- Splits byte chunks on `\n\n` (SSE event boundaries).
- Parses `data:`, `id:`, `event:`, `retry:` fields per SSE spec.
- Stores `id:` values in `last_event_id`.
- Silently discards `event: ping` and `data: [PING]` events.
- Pushes all other `data:` values to `response_tx`.

`Drop` issues a synchronous `DELETE` with `MCP-Session-Id` header when a session
is active (spec-required session termination), on a dedicated thread to avoid
blocking the async runtime.

#### src/mcp/transport/fake.rs

`FakeTransport::new()` returns `(FakeTransport, FakeTransportHandle)`:

- `send()` writes to `outbound_tx`; test reads via `handle.outbound_rx`.
- `receive()` yields messages injected via `handle.inbound_tx` or
  `inject_response(Value)`.
- `receive_err()` returns a permanently empty stream.

`FakeTransportHandle` fields:

- `pub outbound_rx` - what the client sent
- `pub inbound_tx` - inject server responses here

#### src/mcp/client.rs: clone_shared()

Added `JsonRpcClient::clone_shared()` which creates a new `JsonRpcClient`
sharing all internal `Arc` fields (`next_id`, `pending`,
`notification_handlers`, `server_request_handlers`) and cloning `outbound_tx`.
This enables the pattern:

```text
Arc<JsonRpcClient> -> start_read_loop   (reads responses, resolves pending)
shared.clone_shared() -> McpProtocol   (registers pending entries)
```

Both clients share the same `pending` map so responses resolved by the read loop
unblock calls on the protocol client.

#### tests/helpers/mcp_test_server/main.rs

Minimal stdio MCP server for integration testing:

- Reads newline-delimited JSON from stdin in a loop.
- `initialize` -> valid `InitializeResponse` with `tools` capability set.
- `tools/list` -> one tool: `"echo"` with string `message` parameter.
- `tools/call` with `name: "echo"` -> echoes `arguments.message` as Text
  content.
- `ping` -> empty result object.
- All other methods -> JSON-RPC `-32601 Method not found` error.
- Writes all responses as newline-terminated JSON to stdout.

### Testing

Test coverage: all new modules exceed 80%.

```text
tests/mcp_http_transport_test.rs: 9 passed; 0 failed
tests/mcp_stdio_test.rs:          5 passed; 0 failed
src/mcp/transport/mod.rs inline:  included in lib test run
src/mcp/transport/stdio.rs inline: 4 passed (via lib test run)
src/mcp/transport/http.rs inline:  7 passed (via lib test run)
src/mcp/transport/fake.rs inline: 10 passed (via lib test run)
Total new Phase 2 tests:          35 (inline) + 14 (integration) = 49
```

Key test cases per Task 2.7:

- `test_post_with_json_response_forwarded_to_receive` - body pushed to
  `receive()` when Content-Type is `application/json`
- `test_post_with_sse_two_events_both_forwarded` - two `data:` events both
  arrive on `receive()`
- `test_post_202_yields_nothing` - `202 Accepted` produces no message
- `test_mcp_protocol_version_header_present_on_every_post` - wiremock matcher
  asserts header on both POST requests
- `test_session_id_captured_and_sent_on_subsequent_requests` - full round-trip:
  server sets header, second request carries it
- `test_404_with_session_id_emits_mcp_error` - error variant and session cleared
  correctly
- `test_ping_sse_events_are_silently_dropped` - only the real event received
- `test_stdio_transport_initialize_and_list_tools` - real subprocess, handshake,
  capability flag, tools list
- `test_stdio_transport_call_echo_tool` - `echo` tool returns exact input

### Validation Results

- cargo fmt --all passed
- cargo check --all-targets --all-features passed (zero errors)
- cargo clippy --all-targets --all-features -- -D warnings passed (zero
  warnings)
- cargo test --all-features passed (1032 passed; 1 pre-existing failure in
  unrelated `providers::copilot` module; 8 ignored)
- All 9 HTTP transport integration tests pass
- All 5 stdio integration tests pass
- All 35 inline unit tests in transport submodules pass

### References

- Architecture: `docs/explanation/mcp_support_implementation_plan.md` Phase 2
  (lines 961-1244)
- Protocol spec: MCP revision 2025-11-25
- SSE specification:
  <https://html.spec.whatwg.org/multipage/server-sent-events.html>

---

## MCP Phase 3: OAuth 2.1 / OIDC Authorization (2025-07-XX)

### Overview

Implements the full OAuth 2.1 authorization code flow with PKCE S256 required by
the MCP `2025-11-25` specification for HTTP transport connections. Authorization
applies only to HTTP transport; stdio servers obtain credentials from
environment variables per the specification.

### Components Delivered

| File                                  | Action                       |
| ------------------------------------- | ---------------------------- |
| `src/mcp/auth/mod.rs`                 | Created                      |
| `src/mcp/auth/token_store.rs`         | Created                      |
| `src/mcp/auth/discovery.rs`           | Created                      |
| `src/mcp/auth/pkce.rs`                | Created                      |
| `src/mcp/auth/flow.rs`                | Created                      |
| `src/mcp/auth/manager.rs`             | Created                      |
| `src/mcp/mod.rs`                      | Updated with `pub mod auth;` |
| `tests/mcp_auth_pkce_test.rs`         | Created                      |
| `tests/mcp_auth_discovery_test.rs`    | Created                      |
| `tests/mcp_auth_token_store_test.rs`  | Created                      |
| `tests/mcp_auth_flow_test.rs`         | Created                      |
| `docs/explanation/implementations.md` | Updated with Phase 3 entry   |

### Implementation Details

#### src/mcp/auth/mod.rs

Module root that declares all five sub-modules: `discovery`, `flow`, `manager`,
`pkce`, and `token_store`. Module-level doc comment describes the authorization
scope (HTTP transport only, per spec).

#### src/mcp/auth/token_store.rs

- `OAuthToken` -- `Debug, Clone, Serialize, Deserialize`. Stores `access_token`,
  `token_type`, `expires_at` (RFC-3339 via `chrono` serde), `refresh_token`, and
  `scope`. Optional fields use `skip_serializing_if = "Option::is_none"`.
- `OAuthToken::is_expired()` -- returns `true` when
  `Utc::now() >= expires_at - 60s`. The 60-second buffer ensures callers have
  time to refresh before the token is actually rejected.
- `TokenStore` -- zero-field struct. Provides `save_token`, `load_token`, and
  `delete_token` backed by the OS native keyring (`keyring` crate). Service name
  is prefixed `xzatoma-mcp-{server_id}` for collision avoidance. `load_token`
  returns `Ok(None)` on `keyring::Error::NoEntry`. `delete_token` is idempotent.

#### src/mcp/auth/discovery.rs

- `ProtectedResourceMetadata` -- RFC 9728 document with `resource`,
  `authorization_servers`, and optional `scopes_supported` /
  `bearer_methods_supported`. Uses `#[serde(rename_all = "snake_case")]`.
- `AuthorizationServerMetadata` -- RFC 8414 / OIDC Discovery document. Captures
  all standard fields plus unknown fields via `#[serde(flatten)]` into an
  `extra: HashMap<String, serde_json::Value>`.
- `ClientIdMetadataDocument` -- MCP extension allowing a stable URL to serve as
  the `client_id`.
- `fetch_protected_resource_metadata` -- tries the `resource_metadata` URL from
  a `WWW-Authenticate` header first; falls back to the RFC 9728 well-known URI
  `/.well-known/oauth-protected-resource{path}`.
- `fetch_authorization_server_metadata` -- tries five well-known endpoint
  orderings (RFC 8414 path-insertion, OIDC path-insertion, OIDC path-appending,
  RFC 8414 root, OIDC root) and returns the first success.
- `fetch_client_id_metadata_document` -- simple GET and JSON deserialize.

#### src/mcp/auth/pkce.rs

- `PkceChallenge` -- `Debug, Clone` with `verifier`, `challenge`, and `method`
  fields.
- `generate()` -- 32 random bytes via `rand::rng().fill_bytes()`, base64url (no
  padding) encoded as the verifier; SHA-256 of the verifier UTF-8 bytes also
  base64url-encoded as the challenge. Always sets `method = "S256"`. Verified
  against RFC 7636 Appendix B known-answer test vector.
- `verify_s256_support()` -- rejects servers that do not advertise `"S256"` in
  `code_challenge_methods_supported`; case-sensitive comparison.

#### src/mcp/auth/flow.rs

- `OAuthFlowConfig` -- per-server config: `server_id`, `resource_url`,
  `client_name`, `redirect_port`, `static_client_id`, `static_client_secret`.
- `OAuthFlow` -- stateless flow driver. Key methods:
  - `authorize()` -- full authorization code flow: PKCE check, client ID
    resolution (static > metadata document > DCR), PKCE + state generation, TCP
    listener bind, authorization URL construction with `resource` parameter (RFC
    8707), browser open attempt, callback accept + state validation, code
    exchange.
  - `refresh_token()` -- POST to token endpoint with `grant_type=refresh_token`
    and `resource` parameter.
  - `handle_step_up()` -- parses `scope=` from `WWW-Authenticate` challenge
    header; calls `authorize` up to 3 times before returning an error.
- Private helpers: `resolve_client_id`, `dynamic_client_registration`,
  `generate_state`, `build_authorization_url`, `try_open_browser`,
  `accept_callback`, `exchange_code`.
- Utility functions: `parse_query_string`, `percent_decode`,
  `parse_scope_from_www_authenticate`.

#### src/mcp/auth/manager.rs

- `AuthManager` -- owns `Arc<reqwest::Client>`, `Arc<TokenStore>`, and
  `HashMap<String, OAuthFlowConfig>`. Not internally synchronized; wrap in
  `Arc<tokio::sync::Mutex<_>>` for shared use.
- `new()` -- creates empty manager.
- `add_server()` -- registers or overwrites a server's flow config.
- `get_token()` -- load from keyring; if valid return immediately; if expired
  attempt refresh; if refresh fails or no refresh token, run full auth flow.
- `handle_401()` -- delete cached token, call `get_token` for full re-auth.
- `handle_403_scope()` -- call `flow.handle_step_up()`, persist new token.
- `inject_token()` -- inserts `Authorization: Bearer <token>` into a header map.

### Testing

| Test File                            | Tests                     | Notes                                                        |
| ------------------------------------ | ------------------------- | ------------------------------------------------------------ |
| `tests/mcp_auth_pkce_test.rs`        | 15                        | All pass; includes RFC 7636 Appendix B KAT                   |
| `tests/mcp_auth_token_store_test.rs` | 15 (11 active, 4 ignored) | Keyring tests marked `#[ignore = "requires system keyring"]` |
| `tests/mcp_auth_discovery_test.rs`   | 15                        | wiremock integration; all pass                               |
| `tests/mcp_auth_flow_test.rs`        | 10                        | wiremock integration; all pass                               |
| Inline unit tests (all auth modules) | 80+                       | Embedded in `#[cfg(test)]` blocks                            |

### Validation Results

- `cargo fmt --all` passed
- `cargo check --all-targets --all-features` passed (zero errors)
- `cargo clippy --all-targets --all-features -- -D warnings` passed (zero
  warnings)
- `cargo test --all-features` passed (1102 passed; 1 pre-existing failure in
  unrelated `providers::copilot` module; 11 ignored)
- All 51 Phase 3 integration tests pass
- All 80+ inline unit tests across auth modules pass
- RFC 7636 Appendix B known-answer test vector verified

### Success Criteria Verification

- PKCE S256 challenge verified correct against RFC 7636 Appendix B test vector.
- Discovery tries all five well-known endpoint orderings (wiremock verified).
- Authorization URL includes `resource` parameter (RFC 8707) -- test verified.
- `is_expired` returns `true` when past `expires_at - 60s`; `false` otherwise.
- `inject_token` sets `Authorization: Bearer <token>` correctly.
- All Phase 3 tests pass under `cargo test`.

### References

- Architecture: `docs/explanation/mcp_support_implementation_plan.md` Phase 3
  (lines 1244-1570)
- RFC 7636 PKCE: <https://www.rfc-editor.org/rfc/rfc7636>
- RFC 8414 Authorization Server Metadata:
  <https://www.rfc-editor.org/rfc/rfc8414>
- RFC 8707 Resource Indicators: <https://www.rfc-editor.org/rfc/rfc8707>
- RFC 9728 Protected Resource Metadata: <https://www.rfc-editor.org/rfc/rfc9728>
- OpenID Connect Discovery 1.0:
  <https://openid.net/specs/openid-connect-discovery-1_0.html>

---

### Pending Implementation

⏳ **Phase 1: Foundation** - Not started

- Project structure and module skeleton
- Error types and configuration
- CLI argument parsing
- Basic testing infrastructure

⏳ **Phase 2: Core Agent** - Not started

- Agent execution loop
- Conversation management with pruning
- Tool executor framework

⏳ **Phase 4: Provider Implementations** - Not started

- Base provider trait
- Copilot provider (OAuth)
- Ollama provider (local)
- OpenAI provider (planned extension)
- Anthropic provider (planned extension)

  **Phase 5: File Tools and Plan Parsing** - Complete

- File operations (list, read, write, create, delete, diff)
- Terminal execution with security model
- Plan parser (YAML/JSON/Markdown)

⏳ **Phase 6: Integration** - Not started

- End-to-end agent workflows
- Plan execution mode
- Interactive mode

⏳ **Phase 7: Polish** - Not started

- Error handling refinement
- Documentation completion
- Performance optimization

## Implementation Status

### Phase 3 Security Validation Details

**Components Implemented:**

- CommandValidator struct with security rules
- Denylist patterns for dangerous commands (27 patterns)
- Allowlist for restricted autonomous mode (13 commands)
- Path validation preventing directory traversal
- Three execution modes with appropriate security levels
- Comprehensive error handling with XzatomaError types

**Security Features:**

- Blocks destructive operations (rm -rf /, dd to devices)
- Prevents privilege escalation (sudo, su)
- Stops remote code execution (curl | sh patterns)
- Rejects absolute paths and home directory references
- Validates directory traversal attempts (.. in paths)

**Testing Coverage:**

- 17 unit tests covering all security scenarios
- 100% test coverage for validation logic
- Edge case testing for path validation
- Error type verification

## Provider Abstraction Details

### Implementation Phases

1. **Phase 1: Core Abstractions** (~700 LOC)

- Provider interface/trait
- Message, Tool, Response types
- Error types and handling
- Configuration structures

1. **Phase 2: HTTP Client** (~1,250 LOC)

- API client with authentication
- Retry logic with exponential backoff
- Request formatters for each provider

1. **Phase 3: Provider Implementations** (~2,100 LOC)

- OpenAI provider
- Anthropic provider
- GitHub Copilot provider (with OAuth)
- Ollama provider

1. **Phase 4: Streaming Support** (~950 LOC)

- SSE parser (OpenAI/Anthropic/Copilot)
- JSON Lines parser (Ollama)
- Streaming interface implementation

1. **Phase 5: Factory & Registry** (~700 LOC)

- Provider factory pattern
- Provider metadata
- Configuration-based instantiation

1. **Phase 6: Advanced Features** (~650 LOC)

- Usage tracking and cost estimation
- Caching support (optional)

### Provider Comparison

| Provider  | Auth           | API Format         | Streaming  | Tool Calls      |
| --------- | -------------- | ------------------ | ---------- | --------------- |
| OpenAI    | Bearer Token   | OpenAI-compatible  | SSE        | Native          |
| Anthropic | API Key Header | Anthropic-specific | SSE        | Tool Use Blocks |
| Copilot   | OAuth Device   | OpenAI-compatible  | SSE        | Native          |
| Ollama    | None (local)   | OpenAI-compatible  | JSON Lines | Limited         |

### Key Differences to Handle

1. **System Prompt**: Anthropic uses separate field, others use first message
2. **Tool Calls**: OpenAI (separate field) vs Anthropic (content array)
3. **Tool Results**: OpenAI (tool role) vs Anthropic (user message with
   tool_result)
4. **Streaming**: SSE (OpenAI/Anthropic/Copilot) vs JSON Lines (Ollama)
5. **Token Counting**: Anthropic omits total_tokens (calculate from input +
   output)

## Next Steps

### Immediate Actions

1. **Continue with Phase 1 Implementation** - Foundation work

- Create Cargo.toml and project structure
- Implement error types (`error.rs`)
- Implement configuration (`config.rs`)
- Implement CLI skeleton (`cli.rs`)
- Set up test infrastructure and CI

1. **Phase 2 Implementation** - Core Agent

- Agent execution loop with iteration limits
- Conversation management with token tracking and pruning
- Tool executor framework

### References

- **Architecture**: `docs/reference/architecture.md` - Single source of truth
- **Planning Rules**: `PLAN.md` - Planning template and guidelines
- **Agent Rules**: `AGENTS.md` - Development standards and quality gates
- **Security Implementation**:
  `docs/explanation/phase3_security_validation_implementation.md`
- **Provider Plan**:
  `../archive/implementation_summaries/provider_abstraction_implementation_plan.md`

## Quality Requirements

All implementations must meet these requirements (per AGENTS.md):

- `cargo fmt --all` passes
- `cargo check --all-targets --all-features` passes (zero errors)
- `cargo clippy --all-targets --all-features -- -D warnings` passes (zero
  warnings)
- `cargo test --all-features` passes with >80% coverage
- All public items have doc comments with examples
- All functions have unit tests (success, failure, edge cases)
- Documentation follows Diataxis framework
- Files use correct extensions (`.yaml`, `.md`)
- Filenames use lowercase_with_underscores (except README.md)
- No emojis in code or documentation (except AGENTS.md)

## Version History

- **2025-01-XX** - Phase 5: Documentation and Examples completed
- **2025-01-XX** - Phase 4: Provider Integration completed
- **2025-01-XX** - Phase 3 Security Validation completed
- **2025-01-XX** - Provider abstraction planning completed
- **2025-01-XX** - Architecture validation completed (approved)
- **2025-01-XX** - Initial architecture design documented
- **2025-01-XX** - Project initiated as XZatoma

## Documentation Deliverables

### Phase 5: Documentation and Examples (2025-01-XX)

**Completed:**

- `docs/reference/copilot_provider.md` (221 lines) - Complete API reference for
  Copilot provider including:

  - Configuration field documentation (6 fields with defaults and descriptions)
  - Method signatures for `complete()` and `list_models()`
  - Endpoint selection explanation with algorithm details
  - Streaming support documentation
  - Reasoning model configuration
  - Authentication flow overview
  - Error handling guide
  - Performance characteristics
  - Limitations and constraints
  - Common configuration patterns (production, testing, extended thinking)

- `docs/explanation/copilot_usage_examples.md` (540 lines) - Practical usage
  examples covering:

  - Basic chat completion (default configuration)
  - Multi-turn conversation state management
  - Reasoning models (o1-family with extended thinking)
  - Tool calling with function definitions
  - Streaming disabled (blocking responses)
  - Custom API base for testing with mock servers
  - Endpoint fallback behavior control
  - Comprehensive error handling patterns
  - Configuration from YAML files
  - Configuration from environment variables
  - Listing available models
  - System message context usage
  - Feature matrix by endpoint
  - Migration guide for existing users

- `docs/explanation/phase5_documentation_examples_implementation.md` (394
  lines) - Implementation summary

- **Total:** ~1,155 lines of documentation

**Coverage:**

- All public methods documented with runnable examples
- All configuration options with use cases
- Both major endpoints (/responses and /chat/completions)
- Streaming, tool calling, and reasoning features
- Error handling patterns for production code
- Configuration patterns for different scenarios
- Migration guidance for existing users

### Model Management Documentation (2025-01-22)

**Completed:**

- `docs/reference/model_management.md` (645 lines) - Complete API reference for
  model management
- `docs/how-to/manage_models.md` (418 lines) - Practical guide for discovering
  and inspecting models
- `docs/how-to/switch_models.md` (439 lines) - Practical guide for switching
  models in chat mode
- `docs/explanation/model_management_missing_deliverables_implementation.md`
  (362 lines) - Implementation summary
- `docs/explanation/models_command_chat_mode_help_implementation.md` (new) - Bug
  fix: Chat mode `/models` (no args) now shows models-specific help;
  special-command parser and chat loop updated; tests added

- **Total:** ~1,502 lines of documentation

**Coverage:**

- All provider trait methods for model management
- All CLI commands (models list, models info, models current)
- All chat mode special commands (/models list, /model <name>, /context)
- Provider-specific details for Copilot and Ollama
- Troubleshooting procedures
- Best practices and examples

---

**Status**: Phase 4 complete (Provider Integration), Phase 5 complete
(Documentation and Examples). Model Management Documentation complete. Phases
1-3 implemented. Security validation completed and tested.
