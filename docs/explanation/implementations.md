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

- **[agent_skills_phase1_implementation.md](agent_skills_phase1_implementation.md)** -
  Phase 1: Skill Discovery and Parsing Foundation - added `skills` module
  foundation, `SkillsConfig`, deterministic skill discovery roots and precedence
  rules, `SKILL.md` parsing and validation, diagnostics for invalid and shadowed
  skills, catalog foundation, tests, and configuration examples

- **[agent_skills_phase2_implementation.md](agent_skills_phase2_implementation.md)** -
  Phase 2: Skill Catalog Disclosure and Prompt Integration - added disclosure
  rendering, visibility filtering, trust-gated catalog disclosure, startup
  prompt integration for chat and run flows, disclosure tests, and
  implementation documentation

- **[agent_skills_phase3_implementation.md](agent_skills_phase3_implementation.md)** -
  Phase 3: Skill Activation and Active-Skill Registry - added
  `ActiveSkillRegistry`, `ActiveSkill`, and `activate_skill` tool foundation,
  activation deduplication, active-skill prompt injection support, runtime
  registration helpers, and activation/tool integration tests

- **[agent_skills_phase4_implementation.md](agent_skills_phase4_implementation.md)** -
  Phase 4: Skill Resource Access, Trust Enforcement, and Lifecycle Management -
  added persistent trust store support, trust-backed disclosure filtering,
  resource resolution and enumeration constrained to the skill root,
  session-local active skill lifecycle behavior, and trust/resource/lifecycle
  test coverage

- **[agent_skills_implementation.md](agent_skills_implementation.md)** - Phase
  5: Configuration, CLI, Documentation, and Hardening - completed the `skills`
  CLI command surface, hardening for skills configuration and trust operations,
  configuration examples, Diataxis documentation set, and integration coverage
  for config and CLI behavior

- **[acp_phase1_implementation.md](acp_phase1_implementation.md)** - Phase 1:
  ACP Domain Model and Core Abstractions - added the ACP module foundation,
  protocol-facing manifest/message/run/session/event types, ACP-specific
  validation and lifecycle abstractions, crate-level ACP error integration,
  message-to-ACP adapters, and core contract test coverage

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

**Zed Session Mode Phase 1: Embedded Context Capability and Rich Content Block
Handling** - Complete

- Advertised `embedded_context: true` in `PromptCapabilities` within
  `handle_initialize()` so Zed enables `ContentBlock::Resource` blocks and shows
  `#diagnostics`, `#git-diff`, `#rules`, `#thread`, and `#fetch` in the mention
  completion menu.
- Replaced the unconditional `ContentBlock::Resource` image path with an
  explicit three-way dispatch on the inner `EmbeddedResourceResource` variant:
  `TextResourceContents` (non-image) produces a formatted text part with a URI
  context header; `TextResourceContents` (image MIME) falls back to URI
  conversion; `BlobResourceContents` continues on the image path.
- Replaced the hard `Provider` error for non-image `ContentBlock::ResourceLink`
  blocks with a `[Reference: <name> (<uri>)]` text placeholder so directory
  mentions and file stubs no longer terminate prompt conversion.
- Added 8 new unit tests in `src/acp/prompt_input.rs` covering all new dispatch
  paths and edge cases (diagnostics MIME type, absent MIME type, blob stays on
  image path, placeholder format, order preservation).
- Updated
  `test_handle_initialize_advertises_text_and_vision_prompt_capabilities` to
  assert `embedded_context == true`.
- Added
  `test_initialize_request_prompt_capabilities_include_embedded_context_over_protocol`
  to verify the capability is advertised over the full protocol stack.
- Documentation: `docs/explanation/zed_session_mode_phase1_implementation.md`
- All 2076 unit tests pass, zero warnings, `cargo fmt` clean.

**Zed Session Mode Phase 2: Zed-Forwarded MCP Server Integration** - Complete

- Added private `sanitize_mcp_server_id` helper that lowercases a Zed server
  name and replaces every character outside `[a-z0-9_-]` with an underscore,
  truncated to 64 characters, with an error on empty result.
- Added private `convert_acp_mcp_server` helper that converts all three
  `acp::McpServer` variants (`Stdio`, `Http`, `Sse`) to `McpServerConfig`:
  `Stdio` maps to `McpServerTransportConfig::Stdio`; `Http` and `Sse` both map
  to `McpServerTransportConfig::Http`. A wildcard arm handles future
  `#[non_exhaustive]` variants gracefully.
- Extended `create_session` to iterate `request.mcp_servers`, convert each
  server, deduplicate by id against already-connected servers, connect via
  `McpClientManager::connect`, and register tools via `register_mcp_tools`.
  Individual failures are logged and skipped; they do not abort session
  creation. The entire block is skipped when `env.mcp_manager` is `None` (MCP
  disabled in config) to preserve global MCP policy.
- Added 6 new unit tests in `src/acp/stdio.rs` covering Stdio config production,
  Http config production, name sanitization, empty name rejection, invalid URL
  rejection, and session creation with an empty `mcp_servers` list.
- Documentation: `docs/explanation/zed_session_mode_phase2_implementation.md`
- All 2082 unit tests pass, zero warnings, `cargo fmt` clean.

**Zed Session Mode Phase 3: Runtime Execution Mode Enforcement on Mode
Change** - Complete

- Added `pub fn tools_mut(&mut self) -> &mut ToolRegistry` accessor to `Agent`
  in `src/agent/core.rs` so the ACP stdio layer can replace individual tools in
  the registry without bypassing the agent abstraction.
- Extended `set_session_mode` in `src/acp/stdio.rs` to construct a new
  `TerminalTool` carrying the updated `ExecutionMode` and `SafetyMode` and
  register it under the `"terminal"` key in the agent's tool registry
  immediately after rebuilding transient system messages. The replacement takes
  effect on the next prompt turn within the session.
- Added `use crate::tools::terminal::{CommandValidator, TerminalTool};` import
  to `src/acp/stdio.rs`.
- Added 4 new unit tests: `test_agent_tools_mut_returns_mutable_registry` in
  `src/agent/core.rs` verifies the accessor registers tools correctly;
  `test_set_session_mode_updates_terminal_tool_execution_mode_to_full_autonomous`,
  `test_set_session_mode_full_autonomous_to_planning_restricts_terminal`, and
  `test_set_session_mode_does_not_change_terminal_for_unknown_mode` in
  `src/acp/stdio.rs` verify correct behavior over the full protocol stack.
- Documentation: `docs/explanation/zed_session_mode_phase3_implementation.md`
- All 2086 unit tests pass, zero warnings, `cargo fmt` clean.

### MCP Support Implementation

- Phase 3: OAuth 2.1 / OIDC Authorization -- Complete
- Phase 4: MCP Client Lifecycle and Server Manager -- Complete
- Phase 5A: Tool Bridge, Resources, and Prompts -- Complete
- Phase 5B: Sampling, Elicitation, and Command Integration -- Complete

- **[mcp_support_implementation_plan.md](mcp_support_implementation_plan.md)** -
  Full seven-phase plan for MCP client support (protocol revision 2025-11-25)

### Demo Scaffolding Initiative -- Complete

Seven self-contained demo directories delivered across five phases. Every demo
is independently portable, uses Ollama as the provider, writes all generated
state under `tmp/`, and passes the 13-point isolation audit.

- **Phase 1** - Demo framework: directory contract, sandboxing rules, model
  contract, top-level `demos/README.md`
- **Phase 2** - Core demos: `chat`, `run`, `vision`
- **Phase 3** - Advanced feature demos: `skills`, `mcp`, `subagents`
- **Phase 4** - Watcher demo + cross-demo isolation audit (all 7 demos, 13
  checks)
- **Phase 5** - Validation matrix, implementation summary, index update

Documentation: **[demo_implementation.md](demo_implementation.md)** - canonical
implementation summary with validation matrix (7 demos x 13 checks, all true)

### Zed ACP Agent Command -- Complete

Seven-phase implementation of `xzatoma agent`, the ACP stdio subprocess mode for
Zed and other ACP-compatible IDE clients.

- **Phase 1** - Dependency, CLI, and stdio safety foundation
- **Phase 2** - ACP stdio handshake and session creation
- **Phase 3** - Text and vision input model
- **Phase 4** - Session persistence, resume, and model advertisement
- **Phase 5** - Zed IDE tooling and runtime controls
- **Phase 6** - Prompt execution, streaming updates, queueing, and cancellation
- **Phase 7** - Documentation, demo, and quality gates

Documentation:
**[zed_acp_agent_command_implementation.md](zed_acp_agent_command_implementation.md)**
-- canonical implementation summary covering transport architecture, session
lifecycle, prompt queuing, evented execution, cancellation, and follow-up work.

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

## MCP Phase 4: Client Lifecycle and Server Manager (2025-07-XX)

### Overview

Phase 4 implements the full MCP server configuration types, the `McpConfig`
top-level configuration struct, and the `McpClientManager` which manages the
complete lifecycle of all connected MCP servers. It also wires `McpConfig` into
`src/config.rs` including environment variable overrides and validation, and
introduces a `TaskManager` placeholder for Phase 6.

### Components Delivered

| File                        | Action                                             |
| --------------------------- | -------------------------------------------------- |
| `src/mcp/server.rs`         | Replaced stub with full implementation             |
| `src/mcp/config.rs`         | Replaced stub with full implementation             |
| `src/mcp/manager.rs`        | Created                                            |
| `src/mcp/task_manager.rs`   | Created (Phase 6 placeholder)                      |
| `src/mcp/mod.rs`            | Added `pub mod manager` and `pub mod task_manager` |
| `src/config.rs`             | `apply_env_vars` and `validate` updated            |
| `tests/mcp_manager_test.rs` | Created                                            |
| `tests/mcp_config_test.rs`  | Created                                            |

### Implementation Details

#### src/mcp/server.rs

Defines three public types:

- `OAuthServerConfig` -- optional OAuth 2.1 overrides for a single HTTP server
  (client_id, client_secret, redirect_port, metadata_url). All fields `Option<>`
  with `#[serde(default)]`.

- `McpServerTransportConfig` -- tagged enum
  (`#[serde(tag = "type", rename_all = "lowercase")]`) with two variants:

  - `Stdio { executable, args, env, working_dir }` -- launch a subprocess and
    communicate over stdin/stdout.
  - `Http { endpoint, headers, timeout_seconds, oauth }` -- Streamable HTTP/SSE
    transport with optional OAuth.

- `McpServerConfig` -- full per-server descriptor with `id` (validated against
  `^[a-z0-9_-]{1,64}$`), `transport`, `enabled`, `timeout_seconds`, and five
  capability-enable flags (`tools_enabled`, `resources_enabled`,
  `prompts_enabled`, `sampling_enabled`, `elicitation_enabled`).

  `McpServerConfig::validate` enforces:

  1. ID matches regex.
  2. Stdio: `executable` is non-empty.
  3. Http: endpoint scheme is `"http"` or `"https"`.

#### src/mcp/config.rs

Defines `McpConfig` with `#[serde(default)]` at the struct level so that config
files that omit the `mcp:` key continue to deserialise without error.

Fields:

- `servers: Vec<McpServerConfig>` -- list of servers to connect to.
- `request_timeout_seconds: u64` -- default `30`.
- `auto_connect: bool` -- default `true`.
- `expose_resources_tool: bool` -- default `true`.
- `expose_prompts_tool: bool` -- default `true`.

`McpConfig::validate` checks for duplicate server IDs (returns
`XzatomaError::Config("duplicate MCP server id: ...")`) and calls
`McpServerConfig::validate` for each entry.

#### src/config.rs

`apply_env_vars` extended with two new overrides:

```text
XZATOMA_MCP_REQUEST_TIMEOUT  -> mcp.request_timeout_seconds (u64)
XZATOMA_MCP_AUTO_CONNECT     -> mcp.auto_connect (true|1|yes => true, else false)
```

`validate` extended to call `self.mcp.validate()` before returning `Ok(())`.

#### src/mcp/manager.rs

Core of Phase 4. Provides:

- `McpServerState` -- `Debug, Clone, PartialEq` enum with variants
  `Disconnected`, `Connecting`, `Connected`, `Failed(String)`.

- `McpServerEntry` -- all-`pub` struct holding: `config`, `protocol`
  (`Option<Arc<InitializedMcpProtocol>>`), `tools` (cached `Vec<McpTool>`),
  `state`, `auth_manager` (`Option<Arc<AuthManager>>`), `server_metadata`
  (`Option<AuthorizationServerMetadata>`), `read_loop_handle`, and
  `cancellation`.

- `McpClientManager` -- owns a `HashMap<String, McpServerEntry>`, a shared
  `reqwest::Client`, a shared `TokenStore`, and an `Arc<Mutex<TaskManager>>`
  (Phase 6 placeholder).

  Public methods:

  - `new(http_client, token_store) -> Self`
  - `connect_all(config) -> Result<()>` -- connects all enabled servers; logs
    individual failures without propagating.
  - `connect(config) -> Result<()>` -- full lifecycle: transport spawn, channel
    wiring, read loop start, initialize handshake, tool list fetch. Supports
    stdio and HTTP (with OAuth token injection).
  - `disconnect(id) -> Result<()>` -- cancels the read loop, drops the protocol,
    transitions to `Disconnected`.
  - `reconnect(id) -> Result<()>` -- disconnect then connect.
  - `refresh_tools(id) -> Result<()>` -- re-issues `tools/list` and updates the
    cache.
  - `connected_servers() -> Vec<&McpServerEntry>`
  - `get_tools_for_registry() -> Vec<(String, Vec<McpTool>)>`
  - `call_tool(server_id, tool_name, arguments) -> Result<CallToolResponse>` --
    includes single 401 retry via `AuthManager::handle_401`.
  - `call_tool_as_task(server_id, tool_name, arguments, ttl)` -- uses
    `TaskParams`; Phase 6 will add task polling.
  - `list_resources(server_id) -> Result<Vec<Resource>>`
  - `read_resource(server_id, uri) -> Result<String>` -- text returned directly;
    blobs prefixed with `"[base64 <mime>] "`.
  - `list_prompts(server_id) -> Result<Vec<Prompt>>`
  - `get_prompt(server_id, name, arguments) -> Result<GetPromptResponse>`
  - `insert_entry_for_test(id, entry)` -- test helper for integration tests.

- `xzatoma_client_capabilities() -> ClientCapabilities` -- canonical
  capabilities advertised to all servers: sampling, elicitation (form + url),
  roots (list_changed: true), tasks (list + cancel + requests).

#### src/mcp/task_manager.rs

Phase 6 placeholder. Provides `TaskManager` (Default-constructible) with:

- `register_task(server_id, task_id, ttl)` -- records a new in-flight task.
- `update_task_state(server_id, task_id, state)` -- updates lifecycle state.
- `remove_task(server_id, task_id)` -- removes a completed task.
- `task_state(server_id, task_id) -> Option<&TaskLifecycleState>` -- queries
  current state.
- `active_task_count() -> usize`

### Testing

#### tests/mcp_config_test.rs (25 tests)

- `test_server_id_rejects_uppercase` -- `"MyServer"` fails validate.
- `test_server_id_rejects_spaces` -- `"my server"` fails validate.
- `test_server_id_rejects_too_long` -- 65-char id fails validate.
- `test_server_id_accepts_valid` -- `"my-server_01"` passes validate.
- `test_config_yaml_without_mcp_key_loads_default` -- empty YAML gives
  `McpConfig::default()` with no servers.
- `test_duplicate_server_ids_fail_mcp_config_validate` -- two servers with same
  id returns `Err`.
- `test_env_var_xzatoma_mcp_request_timeout_applies` -- env var sets field.
- `test_env_var_xzatoma_mcp_auto_connect_applies_false` -- `"false"` sets
  `auto_connect` to `false`.
- Plus 17 additional tests for boundary values, serde round-trips, OAuth YAML
  parsing, and invalid-scheme/empty-executable validation.

  All env-var tests use `#[serial]` from `serial_test` to prevent concurrent
  interference via shared process-wide env vars.

#### tests/mcp_manager_test.rs (24 tests)

- `test_connect_succeeds_with_fake_transport_and_valid_initialize_response` --
  wired protocol transitions to Connected state correctly.
- `test_connect_fails_with_protocol_version_mismatch` -- injecting
  `"protocolVersion": "1999-01-01"` causes `McpProtocolVersion` error.
- `test_refresh_tools_updates_cached_tool_list` -- second `list_tools` call with
  two-tool response updates the cache from 1 to 2.
- `test_refresh_tools_via_manager_updates_cached_tool_list` -- full manager
  `refresh_tools` path updates `get_tools_for_registry`.
- `test_call_tool_returns_not_found_for_unknown_tool_name` -- empty tool cache
  returns `McpToolNotFound`.
- `test_call_tool_succeeds_for_known_tool` -- known tool is forwarded to the
  protocol layer and the injected response is returned.
- `test_401_triggers_reauth_classification` -- `McpAuth` error is classified
  correctly by the retry guard; `McpTransport` is not.
- `test_disconnect_transitions_state_to_disconnected` -- state transitions and
  protocol is cleared.
- Plus 16 additional tests for state equality, empty registries, capability
  values, connect_all skip-disabled behaviour, and not-found errors.

### Validation Results

All Phase 4 success criteria are met:

- `connect` completes `initialize` then `tools/list` using wired channels.
- `Config` loaded from YAML with no `mcp:` key produces `McpConfig::default()`.
- Duplicate server IDs caught by `McpConfig::validate`.
- `XZATOMA_MCP_REQUEST_TIMEOUT` env var applies to
  `mcp.request_timeout_seconds`.
- `XZATOMA_MCP_AUTO_CONNECT` env var applies to `mcp.auto_connect`.
- 401 during `call_tool` triggers a single retry via `auth_manager`.
- All Phase 4 tests pass under `cargo test --all-features`.

Quality gate results:

- `cargo fmt --all` -- pass
- `cargo check --all-targets --all-features` -- pass (zero errors)
- `cargo clippy --all-targets --all-features -- -D warnings` -- pass (zero
  warnings)
- `cargo test --all-features` -- 1171 Phase 4 tests pass; 1 pre-existing failure
  in `providers::copilot` (unrelated to Phase 4)

### References

- Implementation plan: `docs/explanation/mcp_support_implementation_plan.md`
  Phase 4 (lines 1570-1909)
- MCP protocol specification revision 2025-11-25

---

## MCP Phase 5A: Tool Bridge, Resources, and Prompts (2025-07-XX)

### Overview

Phase 5A implements the bridge layer between MCP servers and Xzatoma's internal
`ToolRegistry`. It introduces three `ToolExecutor` adapters, a unified
auto-approval policy, and the `register_mcp_tools` helper used by both the `run`
and `chat` commands.

### Components Delivered

| File                                                         | Action                                                      |
| ------------------------------------------------------------ | ----------------------------------------------------------- |
| `src/mcp/approval.rs`                                        | Created -- auto-approval policy function                    |
| `src/mcp/tool_bridge.rs`                                     | Created -- three ToolExecutor adapters                      |
| `src/mcp/mod.rs`                                             | Updated with `pub mod approval;` and `pub mod tool_bridge;` |
| `tests/mcp_tool_bridge_test.rs`                              | Created -- 18 integration tests                             |
| `docs/explanation/mcp_phase5a_tool_bridge_implementation.md` | Created                                                     |

### Implementation Details

#### `src/mcp/approval.rs`

Single authoritative source for MCP tool auto-approval policy. Exports one
function:

- `should_auto_approve(execution_mode, headless) -> bool` -- returns `true` when
  `headless == true` OR `execution_mode == FullAutonomous`. All other
  combinations require a confirmation prompt. No other module may embed inline
  approval checks.

#### `src/mcp/tool_bridge.rs`

Three `ToolExecutor` implementations and a registration helper:

- `McpToolExecutor` -- wraps a single `tools/call` tool. Registry name is always
  `format!("{}__{}", server_id, tool_name)` (double underscore). Routes through
  `call_tool_as_task` when `task_support == Some(TaskSupport::Required)`.
  Appends `structured_content` after a `"---"` delimiter in output.
- `McpResourceToolExecutor` -- exposes `resources/read` as
  `"mcp_read_resource"`. Accepts `server_id` and `uri` arguments.
- `McpPromptToolExecutor` -- exposes `prompts/get` as `"mcp_get_prompt"`.
  Accepts `server_id`, `prompt_name`, and optional `arguments`. Formats response
  messages as `"[ROLE]\ncontent"` blocks separated by blank lines.
- `register_mcp_tools(registry, manager, execution_mode, headless)` -- iterates
  all connected servers and registers one `McpToolExecutor` per tool, plus
  always registers `mcp_read_resource` and `mcp_get_prompt`. Emits
  `tracing::warn!` before overwriting an existing registry entry.

### Validation Results

All Phase 5A quality gates pass:

- `cargo fmt --all` -- pass
- `cargo check --all-targets --all-features` -- pass (zero errors)
- `cargo clippy --all-targets --all-features -- -D warnings` -- pass (zero
  warnings)
- `cargo test --all-features --lib mcp::approval` -- 8 passed
- `cargo test --all-features --lib mcp::tool_bridge` -- 16 passed
- `cargo test --all-features --test mcp_tool_bridge_test` -- 18 passed

The single pre-existing failure
(`providers::copilot::tests::test_copilot_config_defaults`) is unrelated to
Phase 5A.

### References

- Implementation plan: `docs/explanation/mcp_support_implementation_plan.md`
  Phase 5A (lines 1909-2152)
- Implementation summary:
  `docs/explanation/mcp_phase5a_tool_bridge_implementation.md`
- MCP protocol specification revision 2025-11-25

---

## MCP Phase 5B: Sampling, Elicitation, and Command Integration (2025-07-XX)

### Overview

Phase 5B completes the MCP client integration by wiring two server-initiated
callback handlers into Xzatoma's execution model, connecting the
`McpClientManager` to both the `run` and `chat` command flows, and providing
full integration test coverage including an end-to-end test that exercises the
complete path from `ToolRegistry::get` through a live MCP server subprocess and
back.

### Components Delivered

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
| `docs/explanation/phase5b_sampling_elicitation_implementation.md` | Created -- full implementation summary                      |

### Implementation Details

#### `src/mcp/sampling.rs`

`XzatomaSamplingHandler` implements `SamplingHandler` and handles
`sampling/createMessage` requests from connected MCP servers:

1. Delegates approval to `should_auto_approve(execution_mode, headless)`. When
   approval is required it prompts on stderr and reads one line from stdin. Any
   answer other than `"y"` or `"yes"` returns
   `Err(XzatomaError::McpElicitation("user rejected sampling request"))`.
2. Converts `CreateMessageRequest.messages` to provider `Message` values.
   Non-text content items are silently skipped.
3. Prepends `system_prompt` as a `system` role message when present and
   non-empty.
4. Guards against empty message lists -- returns `Err(XzatomaError::Mcp(...))`
   rather than passing an empty slice to the provider.
5. Calls `Provider::complete(&messages, &[])` with no tool definitions.
6. Maps the `CompletionResponse` to `CreateMessageResult` with
   `role: Assistant`, `stop_reason: "toolUse"` or `"endTurn"`, and `model` from
   the response (falling back to `"unknown"`).

`Debug` is implemented manually because `Arc<dyn Provider>` does not implement
`Debug` in the general case.

#### `src/mcp/elicitation.rs`

`XzatomaElicitationHandler` implements `ElicitationHandler`:

- **Form mode**: non-interactive contexts (`headless == true` or
  `execution_mode == FullAutonomous`) cancel immediately. Interactive contexts
  prompt for each field from `requested_schema["properties"]` (sorted
  alphabetically; falls back to a single `"value"` field). Typing `"decline"`
  returns `Decline`; completing all fields returns `Accept` with a JSON object.
- **URL mode**: headless contexts cancel immediately. Non-headless contexts
  print the URL and attempt to open it with `open`/`xdg-open`; always returns
  `Cancel` because the handler cannot await the OAuth redirect synchronously.

The `extract_field_names` helper extracts sorted property names from a JSON
Schema `"properties"` object, falling back to `["value"]` when the schema is
absent or empty.

#### `src/commands/mod.rs` -- Run and Chat Commands

Both `run_plan_with_options` and `run_chat` now build an `McpClientManager` when
`config.mcp.auto_connect == true` and `config.mcp.servers` is non-empty. The
manager `Arc<RwLock<McpClientManager>>` is kept alive for the entire function
duration. Per-server failures are logged at `warn` level without aborting.
`register_mcp_tools` is called with `headless: true` for run and
`headless: false` for chat.

### Testing

#### `tests/mcp_sampling_test.rs` (6 tests)

- `test_full_autonomous_mode_skips_user_prompt_and_calls_provider`
- `test_headless_mode_skips_user_prompt_and_calls_provider`
- `test_interactive_mode_with_user_rejection_returns_mcp_elicitation_error`
- `test_stop_reason_is_end_turn_for_plain_text_response`
- `test_result_model_field_not_empty`
- `test_multiple_messages_all_forwarded_to_provider`

#### `tests/mcp_elicitation_test.rs` (12 tests)

- `test_form_mode_headless_returns_cancel`
- `test_form_mode_full_autonomous_returns_cancel`
- `test_url_mode_headless_returns_cancel`
- `test_form_mode_headless_and_full_autonomous_returns_cancel`
- `test_form_mode_restricted_autonomous_headless_returns_cancel`
- `test_url_mode_full_autonomous_headless_returns_cancel`
- `test_url_mode_no_url_headless_returns_cancel`
- `test_url_mode_non_headless_returns_cancel_after_display`
- `test_mode_none_defaults_to_form_and_full_autonomous_returns_cancel`
- `test_mode_none_defaults_to_form_and_headless_returns_cancel`
- `test_handler_is_idempotent_for_cancel_path`
- `test_form_mode_headless_with_rich_schema_still_returns_cancel`

#### `tests/mcp_tool_execution_test.rs` (4 tests)

End-to-end tests spawning the `mcp_test_server` subprocess via
`McpClientManager::connect`:

- `test_end_to_end_tool_call_via_registry` -- full path from
  `registry.get("test_server__echo").execute({"message":"hello"})` to subprocess
  and back; asserts output is `"hello"`
- `test_end_to_end_sequential_echo_calls_via_registry` -- four sequential calls
- `test_end_to_end_tool_definition_is_well_formed` -- validates definition
  structure
- `test_end_to_end_registry_contains_namespaced_tool_name` -- validates
  `tool_names()` output

### Validation Results

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

### References

- Implementation plan: `docs/explanation/mcp_support_implementation_plan.md`
  Phase 5B (lines 2152-2426)
- Implementation summary:
  `docs/explanation/phase5b_sampling_elicitation_implementation.md`
- MCP protocol specification revision 2025-11-25

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

## Demo Scaffolding Initiative

### Overview

The demo scaffolding initiative delivered seven self-contained, independently
portable demo directories under `demos/`. All demos use Ollama as the only
provider and enforce strict isolation: every generated file is written under the
demo-local `tmp/` directory.

### Validation Matrix

| Demo      | Dir | README | Config | Ollama | Model | Setup | Run | Reset | Gitignore | Output | Self-Contained | Sandbox |
| --------- | --- | ------ | ------ | ------ | ----- | ----- | --- | ----- | --------- | ------ | -------------- | ------- |
| chat      | ok  | ok     | ok     | ok     | ok    | ok    | ok  | ok    | ok        | ok     | ok             | ok      |
| run       | ok  | ok     | ok     | ok     | ok    | ok    | ok  | ok    | ok        | ok     | ok             | ok      |
| skills    | ok  | ok     | ok     | ok     | ok    | ok    | ok  | ok    | ok        | ok     | ok             | ok      |
| mcp       | ok  | ok     | ok     | ok     | ok    | ok    | ok  | ok    | ok        | ok     | ok             | ok      |
| subagents | ok  | ok     | ok     | ok     | ok    | ok    | ok  | ok    | ok        | ok     | ok             | ok      |
| vision    | ok  | ok     | ok     | ok     | ok    | ok    | ok  | ok    | ok        | ok     | ok             | ok      |
| watcher   | ok  | ok     | ok     | ok     | ok    | ok    | ok  | ok    | ok        | ok     | ok             | ok      |

Column key: Dir = directory exists, README = all 10 required sections present,
Config = demo-local config.yaml, Ollama = provider is ollama, Model = correct
model for demo type, Setup/Run/Reset = scripts exist and use DEMO_DIR pattern,
Gitignore = tmp/.gitignore present, Output = tmp/output/ present, Self-Contained
= DEMO_DIR pattern + --config flag, Sandbox = Sandbox Boundaries section in
README.

### Components Delivered

- **demos/chat/** - Interactive conversation demo; granite4:3b; planning mode
- **demos/run/** - Autonomous plan execution demo; granite4:3b; plan file +
  prompt modes
- **demos/vision/** - Image understanding demo; granite3.2-vision:2b;
  Python-generated sample PNG
- **demos/skills/** - Skill discovery and activation demo; granite4:3b; three
  demo-local skills
- **demos/mcp/** - Model Context Protocol demo; granite4:3b; stdio MCP server
  via npx
- **demos/subagents/** - Nested agent delegation demo; granite4:3b; three
  parallel subagents
- **demos/watcher/** - Event-driven plan execution demo; granite4:3b; generic
  Kafka backend

### Validation Results

- 7 demos created
- 13 isolation checks per demo
- 91 / 91 checks passed (all true)
- All per-demo READMEs complete (10 sections each, 70 / 70 section checks
  passed)
- All Markdown files pass markdownlint and prettier
- `cargo fmt`, `cargo check`, `cargo clippy -D warnings` all clean

### References

- **[demo_implementation_plan.md](demo_implementation_plan.md)** - phased
  implementation plan (five phases)
- **[demo_implementation.md](demo_implementation.md)** - canonical
  implementation summary with full validation matrix
- **[demos/README.md](../../demos/README.md)** - top-level demos index and
  quickstart guide

---

## Version History

- **2025-07-XX** - Zed Session Mode Phase 2: Zed-Forwarded MCP Server
  Integration completed
- **2025-07-XX** - Zed Session Mode Phase 1: Embedded Context Capability and
  Rich Content Block Handling completed
- **2025-07-XX** - Demo Scaffolding Initiative completed (7 demos, 5 phases, all
  validation checks passing)
- **2025-07-XX** - MCP Phase 4: Client Lifecycle and Server Manager completed
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
