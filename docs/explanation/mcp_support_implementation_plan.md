# MCP Client Support Implementation Plan

## Overview

This plan adds Model Context Protocol (MCP) client support to Xzatoma,
enabling the agent to connect to external MCP servers and consume their tools,
resources, prompts, sampling, elicitation, and task capabilities. The
implementation targets protocol revision **2025-11-25** with **2025-03-26** as
a backwards-compatibility fallback. The design uses Tokio-native primitives
throughout and integrates with Xzatoma's existing `ToolExecutor` trait,
`ToolRegistry`, and `ExecutionMode` enum without introducing new abstraction
layers or foreign dependencies.

The feature is delivered in seven phases:

- **Phase 0**: Repository integration scaffold -- module, CLI, command wiring,
  and test harness registration so all subsequent phases compile and integrate
  incrementally
- **Phase 1**: Core MCP types and JSON-RPC 2.0 client -- protocol foundation
- **Phase 2**: Transport layer -- stdio child-process and Streamable HTTP with
  SSE and session management
- **Phase 3**: OAuth 2.1 / OIDC authorization layer for HTTP servers
- **Phase 4**: MCP client lifecycle, server manager, and config integration
- **Phase 5A**: Tool bridge -- MCP tools, resources, and prompts wired into the
  existing `ToolRegistry` via the `ToolExecutor` trait
- **Phase 5B**: Sampling and elicitation callbacks wired into run and chat flows
- **Phase 6**: Task manager, CLI commands, and documentation

---

## Current State Analysis

### Existing Infrastructure

| Item                    | Location                                      | Relevant Detail                                                                                                                                                                              |
| ----------------------- | --------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ToolExecutor` trait    | `src/tools/mod.rs`                            | `fn tool_definition() -> serde_json::Value`; `async fn execute(args: Value) -> Result<ToolResult>`; no `requires_confirmation` or `summarize_action` methods                                 |
| `ToolRegistry`          | `src/tools/mod.rs`                            | `register(name: impl Into<String>, executor: Arc<dyn ToolExecutor>)` -- silently overwrites duplicates (uses `HashMap::insert`); keyed by `String`                                           |
| `ToolResult`            | `src/tools/mod.rs`                            | `success(output)`, `error(message)`, `with_metadata`, `truncate_if_needed`; no `failure_with_output` method                                                                                  |
| `Agent`                 | `src/agent/core.rs`                           | Autonomous execution loop; calls `tools.all_definitions()` and `ToolRegistry::get` during iteration                                                                                          |
| `Provider` trait        | `src/providers/base.rs`                       | `async fn complete(messages, tool_definitions) -> Result<CompletionResponse>`                                                                                                                |
| `Config`                | `src/config.rs`                               | YAML + env vars; top-level fields: `provider`, `agent`, `watcher`; no `working_dir` field at top level                                                                                       |
| `ExecutionMode`         | `src/config.rs`                               | Enum: `Interactive`, `RestrictedAutonomous` (default), `FullAutonomous`; lives in `src/config.rs`, NOT a `TerminalMode` type alias                                                           |
| `run_plan_with_options` | `src/commands/mod.rs` (inline `pub mod run`)  | Headless plan/prompt execution; `allow_dangerous: bool` escalates to `FullAutonomous`; working dir derived from `std::env::current_dir()`                                                    |
| `run_chat`              | `src/commands/mod.rs` (inline `pub mod chat`) | Interactive chat; calls `build_tools_for_mode`, constructs `Agent`                                                                                                                           |
| `XzatomaError`          | `src/error.rs`                                | `thiserror`-based enum; `Result<T>` is `anyhow::Result<T>`; no `ToolNotAvailable` variant exists                                                                                             |
| CLI                     | `src/cli.rs`                                  | `clap`; `Commands` enum: `Chat`, `Run`, `Watch`, `Auth`, `Models`, `History`, `Replay`                                                                                                       |
| Command dispatch        | `src/main.rs`                                 | `match cli.command { ... }` pattern; new `Mcp` arm required                                                                                                                                  |
| Command module          | `src/commands/mod.rs`                         | Declares `pub mod chat_mode; pub mod special_commands; pub mod models; pub mod history; pub mod replay;` plus inlined `pub mod chat`, `pub mod run`, `pub mod auth`, `pub mod watch`         |
| `ToolRegistryBuilder`   | `src/tools/registry_builder.rs`               | Builds tool sets per `ChatMode` + `SafetyMode`; `build_for_chat`, `build_for_planning`, `build_for_write`                                                                                    |
| `keyring` crate         | `Cargo.toml`                                  | Already present at `2.3`                                                                                                                                                                     |
| `reqwest` crate         | `Cargo.toml`                                  | Already present; `features = ["json", "stream", "rustls-tls"]`                                                                                                                               |
| `base64` crate          | `Cargo.toml`                                  | Already present at `0.22`                                                                                                                                                                    |
| `ulid` crate            | `Cargo.toml`                                  | Already present at `1.1` with serde feature                                                                                                                                                  |
| `wiremock` crate        | `Cargo.toml` `[dev-dependencies]`             | Present at `0.5`                                                                                                                                                                             |
| `prettytable-rs` crate  | `Cargo.toml`                                  | Already present at `0.10.0`                                                                                                                                                                  |
| `rustyline` crate       | `Cargo.toml`                                  | Already present at `13.0`                                                                                                                                                                    |
| `regex` crate           | `Cargo.toml`                                  | Already present at `1.10`                                                                                                                                                                    |
| Test harness            | `tests/`                                      | Individual integration test files (e.g., `tests/integration_advanced_exec.rs`); `tests/common/mod.rs` for shared helpers; NO `tests/unit/mod.rs` or `tests/integration_tests.rs` files exist |
| Config example          | `config/config.yaml`                          | Live config file at `config/config.yaml`; no `config.example.yaml` exists                                                                                                                    |

### Identified Issues

The following issues MUST be addressed by this plan:

1. No JSON-RPC 2.0 client exists; must be written from scratch.
2. No transport primitives for stdio process management or Streamable HTTP/SSE
   under Tokio.
3. `ToolRegistry::register` silently overwrites on duplicate name. MCP server
   tool namespacing (`<server_id>__<tool_name>`) must be enforced to prevent
   unintentional collisions with existing tools and between servers. The
   existing `register` signature takes `(name: impl Into<String>, executor:
Arc<dyn ToolExecutor>)` -- MCP tools must use the namespaced name as the
   first argument.
4. `XzatomaError` has no MCP-specific variants; additions required. There is
   no existing `ToolNotAvailable` or `ConfigurationValidation` variant to
   insert after -- new MCP variants are appended to the enum.
5. `Config` has no `mcp:` section; the field must be added with
   `#[serde(default)]` to preserve backward compatibility with all existing
   YAML files that omit the key.
6. No `tokio-util` dependency is present; required for codec framing of
   newline-delimited JSON on stdio transport.
7. No `sha2` or `rand` dependencies are present; required for PKCE S256
   in Phase 3.
8. The `ToolExecutor` trait has no `requires_confirmation` or
   `summarize_action` methods. MCP tools needing confirmation behavior must
   implement the check internally within their `execute` method (prompting
   before the actual call) rather than via a trait method.
9. `ToolResult` has no `failure_with_output` constructor; the MCP tool bridge
   must use `ToolResult::error(message)` and embed the error text in the
   output string.
10. No OAuth 2.1 / OIDC flow exists anywhere in the codebase.
11. The test structure is flat -- there are no `tests/unit/` or
    `tests/integration/` subdirectories and no `tests/unit_tests.rs` or
    `tests/integration_tests.rs` umbrella files. New MCP tests are added as
    standalone test files in `tests/` with the naming convention
    `tests/mcp_<feature>.rs`.
12. There is no `config.example.yaml`; the example config is
    `config/config.yaml`. New MCP configuration blocks are appended to
    `config/config.yaml` as commented-out YAML.
13. `src/commands/mod.rs` uses both standalone `pub mod` declarations (for
    `chat_mode`, `special_commands`, `models`, `history`, `replay`) and inline
    `pub mod` blocks (for `chat`, `run`, `auth`, `watch`). A new `mcp`
    command handler must be added as a standalone file `src/commands/mcp.rs`
    with a corresponding `pub mod mcp;` declaration in `src/commands/mod.rs`.
14. No `tests/helpers/` directory exists; the minimal MCP test server binary
    required for integration tests must be created as a `[[bin]]` target in
    `Cargo.toml`.
15. The MCP tool namespacing separator MUST be `__` (double underscore) rather
    than `_` (single underscore) because MCP server IDs and tool names can
    both contain single underscores, making `_` ambiguous as a separator.
16. The `ExecutionMode` enum is defined in `src/config.rs` and is named
    `ExecutionMode`, not `TerminalMode`. All references in MCP code must use
    `crate::config::ExecutionMode`.

---

## Implementation Phases

### Cross-Cutting Repository Integration Requirements

These tasks apply across ALL phases. An implementing agent MUST complete the
relevant item from this list in each phase before declaring that phase done.

| Requirement              | Action Required                                                           | When                                |
| ------------------------ | ------------------------------------------------------------------------- | ----------------------------------- |
| Module wiring            | Add `pub mod mcp;` to `src/lib.rs`                                        | Phase 0                             |
| Command wiring           | Add `pub mod mcp;` to `src/commands/mod.rs`                               | Phase 0                             |
| CLI wiring               | Add `Mcp` variant to `Commands` enum in `src/cli.rs`                      | Phase 0                             |
| Main dispatch            | Add `Commands::Mcp { command } => ...` arm to `src/main.rs`               | Phase 0                             |
| Config wiring            | Add `#[serde(default)] pub mcp: McpConfig` to `Config` in `src/config.rs` | Phase 0                             |
| Test files               | Add new `tests/mcp_<feature>.rs` files for each phase                     | Each phase with new tests           |
| Implementations tracking | Append entry to `docs/explanation/implementations.md`                     | End of each phase that changes code |

---

### Phase 0: Repository Integration Scaffold

**Goal:** Create the minimal scaffolding so all subsequent phases can add code
without large merge conflicts. The project MUST compile cleanly after this
phase. All new stubs use `todo!()` or empty `Ok(())` bodies.

#### Task 0.1: Create Module Skeleton

Create the following files with stub content. Every file must have a module-
level doc comment, `#![allow(dead_code)]`, and `#![allow(unused_imports)]`.

**File: `src/mcp/mod.rs`** -- declare all future submodules:

```text
//! MCP (Model Context Protocol) client support for Xzatoma
#![allow(dead_code)]
#![allow(unused_imports)]

pub mod config;
pub mod server;
```

**File: `src/mcp/config.rs`** -- placeholder `McpConfig` type:

```text
//! MCP client configuration types

/// MCP client configuration (populated in Phase 4)
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct McpConfig {}
```

**File: `src/mcp/server.rs`** -- placeholder comment:

```text
//! MCP server configuration types (populated in Phase 4)
```

#### Task 0.2: Wire Module into Library Root

In `src/lib.rs`, add the following line in the `pub mod` block. Insert it
after `pub mod watcher;` (which is currently the last `pub mod` line):

```text
pub mod mcp;
```

#### Task 0.3: Wire Config Field

In `src/config.rs`, add the following import at the top of the file (after the
existing `use` statements):

```text
use crate::mcp::config::McpConfig;
```

Add the following field to `pub struct Config` after the existing
`pub watcher: WatcherConfig` field:

```text
/// MCP client configuration
#[serde(default)]
pub mcp: McpConfig,
```

No changes to `Config::validate`, `Config::apply_env_vars`, or `Config::load`
are required at this phase -- those updates are done in Phase 4.

#### Task 0.4: Create Command Handler Stub

Create `src/commands/mcp.rs`:

```text
//! MCP subcommand handler

use crate::config::Config;
use crate::error::Result;

/// MCP subcommand variants (populated in Phase 6)
#[derive(Debug, Clone, clap::Subcommand)]
pub enum McpCommands {
    /// List configured MCP servers
    List,
}

/// Handle MCP subcommands
///
/// # Errors
///
/// Returns an error if the subcommand fails
pub async fn handle_mcp(command: McpCommands, _config: Config) -> Result<()> {
    match command {
        McpCommands::List => {
            println!("MCP support not yet implemented.");
            Ok(())
        }
    }
}
```

#### Task 0.5: Wire Command into `src/commands/mod.rs`

Add the following line to the `pub mod` declarations block (after `pub mod
replay;`):

```text
pub mod mcp;
```

#### Task 0.6: Add CLI Variant to `src/cli.rs`

Add the following variant to `pub enum Commands` (after the `Replay` variant):

```text
/// MCP server management commands
Mcp {
    /// MCP subcommand to execute
    #[command(subcommand)]
    command: crate::commands::mcp::McpCommands,
},
```

#### Task 0.7: Add Dispatch Arm to `src/main.rs`

Add the following import to `src/main.rs` (alongside existing `use xzatoma::*`
imports):

```text
use xzatoma::commands::mcp::McpCommands;
```

Add the following arm to the `match cli.command` block (after `Commands::Replay`):

```text
Commands::Mcp { command } => {
    tracing::info!("Starting MCP command");
    commands::mcp::handle_mcp(command, config).await?;
    Ok(())
}
```

#### Task 0.8: Update `config/config.yaml`

Append the following commented-out block to `config/config.yaml`:

```text
# MCP (Model Context Protocol) client configuration
# mcp:
#   auto_connect: true
#   request_timeout_seconds: 30
#   expose_resources_tool: true
#   expose_prompts_tool: true
#   servers:
#     - id: "my_stdio_server"
#       transport:
#         type: "stdio"
#         executable: "/usr/local/bin/my-mcp-server"
#         args: []
#         env: {}
#     - id: "my_http_server"
#       transport:
#         type: "http"
#         endpoint: "https://mcp.example.com/mcp"
#         oauth:
#           redirect_port: 8765
```

#### Task 0.9: Create Test Placeholder

Create `tests/mcp_types_test.rs` with a placeholder test:

```text
//! MCP types unit tests (placeholder; populated in Phase 1)

#[test]
fn placeholder_mcp_types() {
    // Populated in Phase 1
}
```

#### Task 0.10: Deliverables

| File                      | Status                                         |
| ------------------------- | ---------------------------------------------- |
| `src/mcp/mod.rs`          | Created (scaffold)                             |
| `src/mcp/config.rs`       | Created (placeholder `McpConfig`)              |
| `src/mcp/server.rs`       | Created (placeholder comment)                  |
| `src/lib.rs`              | Updated with `pub mod mcp;`                    |
| `src/config.rs`           | Updated with `mcp: McpConfig` field and import |
| `src/commands/mcp.rs`     | Created (stub handler + `McpCommands`)         |
| `src/commands/mod.rs`     | Updated with `pub mod mcp;`                    |
| `src/cli.rs`              | Updated with `Commands::Mcp` variant           |
| `src/main.rs`             | Updated with dispatch arm and import           |
| `config/config.yaml`      | Updated with commented MCP block               |
| `tests/mcp_types_test.rs` | Created (placeholder)                          |

#### Task 0.11: Success Criteria

- `cargo check --all-targets --all-features` passes with zero errors.
- `cargo clippy --all-targets --all-features -- -D warnings` passes with zero
  warnings.
- `xzatoma mcp list` parses correctly and prints the stub message.
- All pre-existing tests pass without modification.
- `cargo test` discovers `tests/mcp_types_test.rs`.

---

### Phase 1: Core MCP Types and JSON-RPC Client

**Goal:** Implement all MCP 2025-11-25 protocol types, the transport-agnostic
JSON-RPC 2.0 client, and the typed MCP lifecycle wrapper. No transport or auth
code is introduced in this phase -- those come in Phases 2 and 3.

#### Task 1.1: Add Required Dependencies

Run the following commands from the repository root:

```text
cargo add tokio-util --features codec
cargo add sha2
cargo add rand
```

Verify that the following are already present in `Cargo.toml` (no action
required for these):

| Crate       | Already Present  | Used For                              |
| ----------- | ---------------- | ------------------------------------- |
| `base64`    | yes (`0.22`)     | Provider streaming; MCP blob encoding |
| `ulid`      | yes (`1.1`)      | Session IDs                           |
| `reqwest`   | yes (`0.11`)     | HTTP client                           |
| `keyring`   | yes (`2.3`)      | Auth token storage                    |
| `wiremock`  | yes (`0.5`, dev) | HTTP mock tests                       |
| `futures`   | yes (`0.3`)      | Async stream utilities                |
| `regex`     | yes (`1.10`)     | ID validation                         |
| `rustyline` | yes (`13.0`)     | Interactive prompts (elicitation)     |

#### Task 1.2: Add MCP Error Variants to `src/error.rs`

Add the following variants to `pub enum XzatomaError`. Insert them at the end
of the enum (before the closing `}`), after the existing
`MessageConversionError` variant:

```text
/// General MCP protocol error
#[error("MCP error: {0}")]
Mcp(String),

/// MCP transport-level I/O failure
#[error("MCP transport error: {0}")]
McpTransport(String),

/// Named MCP server not found in config or registry
#[error("MCP server not found: {0}")]
McpServerNotFound(String),

/// Tool not found on the specified MCP server
#[error("MCP tool not found: server={server}, tool={tool}")]
McpToolNotFound {
    /// Server identifier
    server: String,
    /// Tool name
    tool: String,
},

/// MCP protocol version negotiation failure
#[error("MCP protocol version mismatch: expected one of {expected:?}, got {got}")]
McpProtocolVersion {
    /// List of accepted versions
    expected: Vec<String>,
    /// Version the server returned
    got: String,
},

/// MCP request timed out
#[error("MCP timeout: server={server}, method={method}")]
McpTimeout {
    /// Server identifier
    server: String,
    /// JSON-RPC method that timed out
    method: String,
},

/// OAuth / OIDC authorization error for an MCP HTTP server
#[error("MCP auth error: {0}")]
McpAuth(String),

/// MCP elicitation error or user decline/cancel
#[error("MCP elicitation error: {0}")]
McpElicitation(String),

/// MCP task lifecycle error
#[error("MCP task error: {0}")]
McpTask(String),
```

Add corresponding unit tests in `src/error.rs` in the existing `mod tests`
block:

```text
#[test]
fn test_mcp_error_variants() {
    let e = XzatomaError::Mcp("protocol violation".to_string());
    assert!(e.to_string().contains("MCP error"));

    let e = XzatomaError::McpToolNotFound {
        server: "my_server".to_string(),
        tool: "search".to_string(),
    };
    assert!(e.to_string().contains("my_server"));
    assert!(e.to_string().contains("search"));

    let e = XzatomaError::McpProtocolVersion {
        expected: vec!["2025-11-25".to_string()],
        got: "2024-01-01".to_string(),
    };
    assert!(e.to_string().contains("2024-01-01"));

    let e = XzatomaError::McpTimeout {
        server: "s1".to_string(),
        method: "tools/list".to_string(),
    };
    assert!(e.to_string().contains("tools/list"));
}
```

#### Task 1.3: Create `src/mcp/types.rs`

This file defines ALL MCP 2025-11-25 protocol types. Every type must derive
`Debug, Clone, serde::Serialize, serde::Deserialize` unless noted otherwise.
All struct fields use `#[serde(rename_all = "camelCase")]` at the struct level
unless the field name is already camelCase or noted otherwise. All
`Option<>` fields use `#[serde(skip_serializing_if = "Option::is_none")]`.

**Protocol version constants** (at module top level):

```text
pub const LATEST_PROTOCOL_VERSION: &str = "2025-11-25";
pub const PROTOCOL_VERSION_2025_03_26: &str = "2025-03-26";
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] =
    &[LATEST_PROTOCOL_VERSION, PROTOCOL_VERSION_2025_03_26];
```

**Core identity types:**

- `struct ProtocolVersion(pub String)` -- newtype; implements `Display`,
  `From<String>`, `From<&str>`, `PartialEq`, `Eq`
- `struct Implementation { pub name: String, pub version: String, pub
description: Option<String> }` -- `description` new in `2025-11-25`

**Capability types:**

- `struct TasksCapability` -- all fields `Option<serde_json::Value>` matching
  spec shape for `tasks.list`, `tasks.cancel`, `tasks.requests.*`
- `struct ElicitationCapability { pub form: Option<serde_json::Value>, pub
url: Option<serde_json::Value> }`
- `struct SamplingCapability { pub tools: Option<serde_json::Value>, pub
context: Option<serde_json::Value> }`
- `struct RootsCapability { pub list_changed: Option<bool> }`
- `struct ClientCapabilities { pub experimental: Option<serde_json::Value>,
pub sampling: Option<SamplingCapability>, pub roots: Option<RootsCapability>,
pub elicitation: Option<ElicitationCapability>, pub tasks:
Option<TasksCapability> }`
- `struct ServerCapabilities { pub experimental: Option<serde_json::Value>,
pub logging: Option<serde_json::Value>, pub completions:
Option<serde_json::Value>, pub prompts: Option<serde_json::Value>, pub
resources: Option<serde_json::Value>, pub tools: Option<serde_json::Value>,
pub tasks: Option<serde_json::Value> }`

**Initialize types:**

- `struct InitializeParams { pub protocol_version: String, pub capabilities:
ClientCapabilities, pub client_info: Implementation }`
- `struct InitializeResponse { pub protocol_version: String, pub capabilities:
ServerCapabilities, pub server_info: Implementation, pub instructions:
Option<String> }`

**Tool types:**

- `enum TaskSupport { Required, Optional, Forbidden }` -- serialize as
  `"required"`, `"optional"`, `"forbidden"` via
  `#[serde(rename_all = "lowercase")]`
- `struct ToolExecution { pub task_support: Option<TaskSupport> }`
- `struct ToolAnnotations { pub title: Option<String>, pub read_only_hint:
Option<bool>, pub destructive_hint: Option<bool>, pub idempotent_hint:
Option<bool>, pub open_world_hint: Option<bool> }`
- `struct McpTool { pub name: String, pub title: Option<String>, pub
description: Option<String>, pub input_schema: serde_json::Value, pub
output_schema: Option<serde_json::Value>, pub annotations:
Option<ToolAnnotations>, pub execution: Option<ToolExecution> }` -- named
  `McpTool` to avoid a naming collision with `crate::tools::Tool`
- `struct ListToolsResponse { pub tools: Vec<McpTool>, pub next_cursor:
Option<String>, pub meta: Option<serde_json::Value> }`
- `struct CallToolParams { pub name: String, pub arguments:
Option<serde_json::Value>, #[serde(rename = "_meta")] pub meta:
Option<serde_json::Value>, pub task: Option<TaskParams> }`
- `struct CallToolResponse { pub content: Vec<ToolResponseContent>, pub
is_error: Option<bool>, #[serde(rename = "_meta")] pub meta:
Option<serde_json::Value>, pub structured_content: Option<serde_json::Value> }`
- `struct TaskParams { pub ttl: Option<u64> }`

**Tool response content:**

- `enum ToolResponseContent` -- `#[serde(tag = "type", rename_all = "lowercase")]`:
  - `Text { text: String }`
  - `Image { data: String, mime_type: String }`
  - `Audio { data: String, mime_type: String }`
  - `Resource { resource: ResourceContents }`

**Task types:**

- `enum TaskStatus` -- `#[serde(rename_all = "snake_case")]`: `Working`,
  `InputRequired`, `Completed`, `Failed`, `Cancelled`
- `struct Task { pub task_id: String, pub status: TaskStatus, pub
status_message: Option<String>, pub created_at: Option<String>, pub
last_updated_at: Option<String>, pub ttl: Option<u64>, pub poll_interval:
Option<u64> }`
- `struct CreateTaskResult { pub task: Task }`
- `struct TasksListResponse { pub tasks: Vec<Task>, pub next_cursor:
Option<String> }`
- `struct TasksGetParams { pub task_id: String }`
- `struct TasksResultParams { pub task_id: String }`
- `struct TasksCancelParams { pub task_id: String }`
- `struct TasksListParams { pub cursor: Option<String> }`

**Resource types:**

- `struct TextResourceContents { pub uri: String, pub mime_type: Option<String>,
pub text: String }`
- `struct BlobResourceContents { pub uri: String, pub mime_type: Option<String>,
pub blob: String }` -- `blob` is base64-encoded bytes
- `enum ResourceContents` -- `#[serde(untagged)]`: `Text(TextResourceContents)`,
  `Blob(BlobResourceContents)`
- `struct Resource { pub uri: String, pub name: String, pub description:
Option<String>, pub mime_type: Option<String> }`
- `struct ResourceTemplate { pub uri_template: String, pub name: String, pub
description: Option<String>, pub mime_type: Option<String> }`
- `struct ListResourcesResponse { pub resources: Vec<Resource>, pub
next_cursor: Option<String> }`
- `struct ReadResourceParams { pub uri: String }`
- `struct ReadResourceResponse { pub contents: Vec<ResourceContents> }`

**Prompt types:**

- `enum Role` -- `#[serde(rename_all = "lowercase")]`: `User`, `Assistant`
- `struct TextContent { pub text: String, pub annotations:
Option<serde_json::Value> }`
- `struct ImageContent { pub data: String, pub mime_type: String, pub
annotations: Option<serde_json::Value> }`
- `struct AudioContent { pub data: String, pub mime_type: String, pub
annotations: Option<serde_json::Value> }`
- `enum MessageContent` -- `#[serde(tag = "type", rename_all = "lowercase")]`:
  `Text(TextContent)`, `Image(ImageContent)`, `Audio(AudioContent)`,
  `Resource { resource: ResourceContents, annotations: Option<serde_json::Value> }`
- `struct PromptMessage { pub role: Role, pub content: MessageContent }`
- `struct PromptArgument { pub name: String, pub description: Option<String>,
pub required: Option<bool> }`
- `struct Prompt { pub name: String, pub description: Option<String>, pub
arguments: Option<Vec<PromptArgument>> }`
- `struct ListPromptsResponse { pub prompts: Vec<Prompt>, pub next_cursor:
Option<String> }`
- `struct GetPromptParams { pub name: String, pub arguments:
Option<std::collections::HashMap<String, String>>, #[serde(rename = "_meta")]
pub meta: Option<serde_json::Value> }`
- `struct GetPromptResponse { pub description: Option<String>, pub messages:
Vec<PromptMessage>, #[serde(rename = "_meta")] pub meta:
Option<serde_json::Value> }`

**Sampling types:**

- `struct ModelHint { pub name: Option<String> }`
- `struct ModelPreferences { pub hints: Option<Vec<ModelHint>>, pub
cost_priority: Option<f64>, pub speed_priority: Option<f64>, pub
intelligence_priority: Option<f64> }`
- `enum ToolChoiceMode` -- `#[serde(rename_all = "lowercase")]`: `Auto`,
  `Required`, `None_` (serialized as `"none"` via `#[serde(rename = "none")]`)
- `struct SamplingToolChoice { pub mode: ToolChoiceMode }`
- `struct CreateMessageRequest { pub messages: Vec<PromptMessage>, pub
model_preferences: Option<ModelPreferences>, pub system_prompt: Option<String>,
pub include_context: Option<String>, pub temperature: Option<f64>, pub
max_tokens: u32, pub stop_sequences: Option<Vec<String>>, pub metadata:
Option<serde_json::Value>, pub tools: Option<Vec<serde_json::Value>>, pub
tool_choice: Option<SamplingToolChoice> }`
- `struct CreateMessageResult { pub role: Role, pub content: MessageContent,
pub model: String, pub stop_reason: Option<String> }`

**Elicitation types:**

- `enum ElicitationMode` -- `#[serde(rename_all = "lowercase")]`: `Form`, `Url`
- `enum ElicitationAction` -- `#[serde(rename_all = "lowercase")]`: `Accept`,
  `Decline`, `Cancel`
- `struct ElicitationCreateParams { pub mode: Option<ElicitationMode>, pub
message: String, pub requested_schema: Option<serde_json::Value>, pub url:
Option<String>, pub elicitation_id: Option<String> }`
- `struct ElicitationResult { pub action: ElicitationAction, pub content:
Option<serde_json::Value> }`

**Logging:**

- `enum LoggingLevel` -- `#[serde(rename_all = "lowercase")]`: `Debug`, `Info`,
  `Notice`, `Warning`, `Error`, `Critical`, `Alert`, `Emergency`

**Completion types:**

- `struct CompletionCompleteParams { pub r#ref: serde_json::Value, pub
argument: serde_json::Value }`
- `struct CompletionResult { pub values: Vec<String>, pub total: Option<u32>,
pub has_more: Option<bool> }`
- `struct CompletionCompleteResponse { pub completion: CompletionResult }`

**Other types:**

- `struct Root { pub uri: String, pub name: Option<String> }`
- `struct CancelledParams { pub request_id: serde_json::Value, pub reason:
Option<String> }`
- `struct ProgressParams { pub progress_token: serde_json::Value, pub progress:
f64, pub message: Option<String>, pub total: Option<f64>,
# [serde(rename = "_meta")] pub meta: Option<serde_json::Value> }`
- `struct PaginatedParams { pub cursor: Option<String> }`

**JSON-RPC types** (used by `src/mcp/client.rs`):

- `struct JsonRpcRequest { pub jsonrpc: String, pub id:
Option<serde_json::Value>, pub method: String, pub params:
Option<serde_json::Value> }` -- `jsonrpc` is always `"2.0"`
- `struct JsonRpcResponse { pub jsonrpc: String, pub id:
Option<serde_json::Value>, pub result: Option<serde_json::Value>, pub error:
Option<JsonRpcError> }`
- `struct JsonRpcError { pub code: i64, pub message: String, pub data:
Option<serde_json::Value> }` -- implements `Display` as
  `"JSON-RPC error {code}: {message}"`
- `struct JsonRpcNotification { pub jsonrpc: String, pub method: String,
pub params: Option<serde_json::Value> }`

**Request/notification method constants** -- define as `pub const &str` at
module level, one per MCP method:

```text
pub const METHOD_INITIALIZE: &str = "initialize";
pub const METHOD_INITIALIZED: &str = "notifications/initialized";
pub const METHOD_PING: &str = "ping";
pub const METHOD_TOOLS_LIST: &str = "tools/list";
pub const METHOD_TOOLS_CALL: &str = "tools/call";
pub const METHOD_RESOURCES_LIST: &str = "resources/list";
pub const METHOD_RESOURCES_READ: &str = "resources/read";
pub const METHOD_RESOURCES_SUBSCRIBE: &str = "resources/subscribe";
pub const METHOD_RESOURCES_UNSUBSCRIBE: &str = "resources/unsubscribe";
pub const METHOD_RESOURCES_TEMPLATES_LIST: &str = "resources/templates/list";
pub const METHOD_PROMPTS_LIST: &str = "prompts/list";
pub const METHOD_PROMPTS_GET: &str = "prompts/get";
pub const METHOD_COMPLETION_COMPLETE: &str = "completion/complete";
pub const METHOD_LOGGING_SET_LEVEL: &str = "logging/setLevel";
pub const METHOD_SAMPLING_CREATE_MESSAGE: &str = "sampling/createMessage";
pub const METHOD_ELICITATION_CREATE: &str = "elicitation/create";
pub const METHOD_TASKS_GET: &str = "tasks/get";
pub const METHOD_TASKS_RESULT: &str = "tasks/result";
pub const METHOD_TASKS_CANCEL: &str = "tasks/cancel";
pub const METHOD_TASKS_LIST: &str = "tasks/list";
pub const NOTIF_TOOLS_LIST_CHANGED: &str = "notifications/tools/listChanged";
pub const NOTIF_RESOURCES_LIST_CHANGED: &str = "notifications/resources/listChanged";
pub const NOTIF_RESOURCES_UPDATED: &str = "notifications/resources/updated";
pub const NOTIF_PROMPTS_LIST_CHANGED: &str = "notifications/prompts/listChanged";
pub const NOTIF_TASKS_STATUS: &str = "notifications/tasks/status";
pub const NOTIF_PROGRESS: &str = "notifications/progress";
pub const NOTIF_CANCELLED: &str = "notifications/cancelled";
pub const NOTIF_ROOTS_LIST_CHANGED: &str = "notifications/roots/listChanged";
```

#### Task 1.4: Update `src/mcp/mod.rs`

Replace the Phase 0 content with:

```text
//! MCP (Model Context Protocol) client support for Xzatoma
#![allow(dead_code)]
#![allow(unused_imports)]

pub mod types;
pub mod client;
pub mod protocol;
pub mod config;
pub mod server;

pub use types::*;
```

#### Task 1.5: Create `src/mcp/client.rs`

Implement a transport-agnostic async JSON-RPC 2.0 client backed by Tokio
channels.

**`pub const DEFAULT_REQUEST_TIMEOUT: std::time::Duration =
std::time::Duration::from_secs(30);`**

**`pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn
std::future::Future<Output = T> + Send + 'a>>;`**

**`pub struct JsonRpcClient`** fields (all `pub(crate)`):

- `next_id: Arc<std::sync::atomic::AtomicU64>`
- `pending: Arc<tokio::sync::Mutex<std::collections::HashMap<u64,
tokio::sync::oneshot::Sender<Result<serde_json::Value,
crate::mcp::types::JsonRpcError>>>>>`
- `outbound_tx: tokio::sync::mpsc::UnboundedSender<String>`
- `notification_handlers: Arc<tokio::sync::Mutex<std::collections::HashMap<String,
Box<dyn Fn(serde_json::Value) + Send + Sync + 'static>>>>`
- `server_request_handlers: Arc<tokio::sync::Mutex<std::collections::HashMap<String,
  Box<dyn Fn(serde_json::Value) -> BoxFuture<'static, serde_json::Value>
  - Send + Sync + 'static>>>>`

**`impl JsonRpcClient`** methods:

- `pub fn new(outbound_tx: tokio::sync::mpsc::UnboundedSender<String>) -> Self`
  -- creates a new client; the caller is responsible for wiring the read loop

- `pub async fn request<P, R>(&self, method: &str, params: P,
timeout: Option<std::time::Duration>) -> crate::error::Result<R>`
  where `P: serde::Serialize + Send`, `R: serde::de::DeserializeOwned`:

  1. Increment `next_id` with `fetch_add(1, Ordering::SeqCst)` to get `id`
  2. Create `oneshot::channel()`; store `tx` in `pending` keyed by `id`
  3. Serialize `JsonRpcRequest { jsonrpc: "2.0".into(), id:
Some(serde_json::json!(id)), method: method.into(),
params: Some(serde_json::to_value(params)?) }` to a JSON string
  4. Send the string on `outbound_tx`; map `SendError` to
     `XzatomaError::McpTransport`
  5. Await `rx` with `tokio::time::timeout(timeout.unwrap_or(DEFAULT_REQUEST_TIMEOUT), rx)`;
     map timeout to `XzatomaError::McpTimeout { server: "(unknown)".into(),
method: method.to_string() }`
  6. Map `JsonRpcError` in the response to
     `XzatomaError::Mcp(error.message)`
  7. Deserialize `result` to `R`; map deserialization error to
     `XzatomaError::Serialization`

- `pub fn notify<P: serde::Serialize + Send>(&self, method: &str,
params: P) -> crate::error::Result<()>` -- serializes a notification (no
  `id` field) and sends on `outbound_tx`

- `pub fn on_notification(&self, method: impl Into<String>, f: impl
Fn(serde_json::Value) + Send + Sync + 'static)` -- registers a notification
  handler by method name

- `pub fn on_server_request(&self, method: impl Into<String>, f: impl
Fn(serde_json::Value) -> BoxFuture<'static, serde_json::Value> + Send +
Sync + 'static)` -- registers a handler for server-initiated requests

**`pub fn start_read_loop`** -- takes
`inbound_rx: tokio::sync::mpsc::UnboundedReceiver<String>`,
`cancellation: tokio_util::sync::CancellationToken`,
`client: Arc<JsonRpcClient>`. Returns `tokio::task::JoinHandle<()>`. The loop:

1. Uses `tokio::select!` over the receiver and the cancellation token
2. On each received JSON string:
   a. Deserializes to `serde_json::Value`
   b. If value has `"id"` AND (`"result"` OR `"error"`): it is a response;
   extract the `u64` ID from the `"id"` field, look up in `pending`, send
   via the oneshot sender
   c. If value has `"id"` AND `"method"`: it is a server-initiated request;
   look up `method` in `server_request_handlers`, call the handler
   asynchronously, send the return value as a `JsonRpcResponse`; if no
   handler found, send a JSON-RPC `-32601 Method not found` error response
   d. If value has `"method"` but no `"id"`: it is a notification; look up
   `method` in `notification_handlers` and call the handler
3. On cancellation: drop all pending senders (receivers get a closed-channel
   error) and exit

#### Task 1.6: Create `src/mcp/protocol.rs`

Typed wrapper over `JsonRpcClient` providing the full MCP lifecycle.

**`pub struct McpProtocol`** -- wraps an uninitialized `JsonRpcClient`:

- `pub fn new(client: JsonRpcClient) -> Self`

- `pub async fn initialize(self, client_info: Implementation,
capabilities: ClientCapabilities) ->
crate::error::Result<InitializedMcpProtocol>`:
  1. Call `client.request(METHOD_INITIALIZE, InitializeParams {
protocol_version: LATEST_PROTOCOL_VERSION.into(), capabilities,
client_info }, None)` to get `InitializeResponse`
  2. Check `response.protocol_version` is in `SUPPORTED_PROTOCOL_VERSIONS`;
     if not, return `XzatomaError::McpProtocolVersion { expected:
SUPPORTED_PROTOCOL_VERSIONS.iter().map(|s| s.to_string()).collect(),
got: response.protocol_version }`
  3. Call `client.notify(METHOD_INITIALIZED, serde_json::json!({}))` --
     fire-and-forget; ignore errors
  4. Return `Ok(InitializedMcpProtocol { client,
initialize_response: response })`

**`pub struct InitializedMcpProtocol`** fields:

- `pub client: JsonRpcClient`
- `pub initialize_response: InitializeResponse`

**`impl InitializedMcpProtocol`** -- all methods `pub async fn`:

- `pub fn capable(&self, capability: ServerCapabilityFlag) -> bool` -- checks
  the corresponding field in `self.initialize_response.capabilities`
- `pub async fn list_tools(&self) -> crate::error::Result<Vec<McpTool>>` --
  cursor-paginates `METHOD_TOOLS_LIST` until `next_cursor` is `None`; returns
  accumulated `tools`
- `pub async fn call_tool(&self, name: &str,
arguments: Option<serde_json::Value>, task: Option<TaskParams>) ->
crate::error::Result<CallToolResponse>` -- calls `METHOD_TOOLS_CALL`
- `pub async fn list_resources(&self) -> crate::error::Result<Vec<Resource>>`
  -- cursor-paginates `METHOD_RESOURCES_LIST`
- `pub async fn read_resource(&self, uri: &str) ->
crate::error::Result<Vec<ResourceContents>>` -- calls `METHOD_RESOURCES_READ`
- `pub async fn list_prompts(&self) -> crate::error::Result<Vec<Prompt>>` --
  cursor-paginates `METHOD_PROMPTS_LIST`
- `pub async fn get_prompt(&self, name: &str,
arguments: Option<std::collections::HashMap<String, String>>) ->
crate::error::Result<GetPromptResponse>` -- calls `METHOD_PROMPTS_GET`
- `pub async fn complete(&self, params: CompletionCompleteParams) ->
crate::error::Result<CompletionCompleteResponse>` -- calls
  `METHOD_COMPLETION_COMPLETE`
- `pub async fn ping(&self) -> crate::error::Result<()>` -- calls
  `METHOD_PING`; discards result value
- `pub async fn tasks_get(&self, task_id: &str) ->
crate::error::Result<Task>` -- calls `METHOD_TASKS_GET`
- `pub async fn tasks_result(&self, task_id: &str) ->
crate::error::Result<CallToolResponse>` -- calls `METHOD_TASKS_RESULT`
- `pub async fn tasks_cancel(&self, task_id: &str) ->
crate::error::Result<Task>` -- calls `METHOD_TASKS_CANCEL`
- `pub async fn tasks_list(&self, cursor: Option<&str>) ->
crate::error::Result<TasksListResponse>` -- calls `METHOD_TASKS_LIST`
- `pub fn register_sampling_handler(&self, handler: Arc<dyn SamplingHandler>)`
  -- installs an `on_server_request` callback for
  `METHOD_SAMPLING_CREATE_MESSAGE`
- `pub fn register_elicitation_handler(&self, handler:
Arc<dyn ElicitationHandler>)` -- installs an `on_server_request` callback
  for `METHOD_ELICITATION_CREATE`

**`pub enum ServerCapabilityFlag`** -- variants: `Tools`, `Resources`,
`Prompts`, `Logging`, `Completions`, `Tasks`, `Experimental`.

**`pub trait SamplingHandler: Send + Sync`**:

```text
fn create_message<'a>(
    &'a self,
    params: CreateMessageRequest,
) -> crate::mcp::client::BoxFuture<'a, crate::error::Result<CreateMessageResult>>;
```

**`pub trait ElicitationHandler: Send + Sync`**:

```text
fn create_elicitation<'a>(
    &'a self,
    params: ElicitationCreateParams,
) -> crate::mcp::client::BoxFuture<'a, crate::error::Result<ElicitationResult>>;
```

#### Task 1.7: Testing Requirements

Replace the Phase 0 placeholder in `tests/mcp_types_test.rs` with real tests:

- `test_protocol_version_constants_are_correct` -- assert
  `LATEST_PROTOCOL_VERSION == "2025-11-25"` and
  `PROTOCOL_VERSION_2025_03_26 == "2025-03-26"`.
- `test_implementation_description_skipped_when_none` -- serialize an
  `Implementation` with `description: None`; assert the resulting JSON string
  does not contain the key `"description"`.
- `test_call_tool_response_roundtrip` -- construct a `CallToolResponse` with
  all fields populated; serialize to `serde_json::Value`; deserialize back;
  assert equality.
- `test_task_status_serializes_snake_case` -- assert
  `serde_json::to_string(&TaskStatus::InputRequired).unwrap() ==
"\"input_required\""`.
- `test_tool_response_content_text_roundtrip` -- round-trip
  `ToolResponseContent::Text { text: "hello".into() }`.
- `test_json_rpc_error_display` -- `JsonRpcError { code: -32600,
message: "Invalid Request".into(), data: None }` displays as
  `"JSON-RPC error -32600: Invalid Request"`.

Create `tests/mcp_client_test.rs`:

- `test_request_resolves_with_correct_result` -- create a `JsonRpcClient`
  with an in-process channel; manually inject a well-formed response JSON
  string on the inbound channel; assert `request()` returns the expected
  deserialized value.
- `test_request_timeout_fires` -- do NOT send a response; call `request()`
  with `Some(Duration::from_millis(50))`; assert the result is
  `XzatomaError::McpTimeout`.
- `test_notification_handler_called_for_matching_method` -- register a
  handler for `"notifications/tools/listChanged"`; inject a matching
  notification; assert the handler was called exactly once.
- `test_pending_sender_dropped_cleanly_on_read_loop_exit` -- cancel the
  `CancellationToken`; await the `JoinHandle`; verify any awaiting `request()`
  call returns an error rather than hanging.
- `test_json_rpc_error_response_mapped_to_mcp_error` -- inject a response
  containing an `error` field; assert the result from `request()` is
  `Err(XzatomaError::Mcp(...))`.

#### Task 1.8: Deliverables

| File                                  | Action                                                          |
| ------------------------------------- | --------------------------------------------------------------- |
| `src/mcp/types.rs`                    | Created                                                         |
| `src/mcp/client.rs`                   | Created                                                         |
| `src/mcp/protocol.rs`                 | Created                                                         |
| `src/mcp/mod.rs`                      | Updated with `pub mod types; pub mod client; pub mod protocol;` |
| `src/error.rs`                        | Updated with MCP error variants and tests                       |
| `Cargo.toml`                          | Updated via `cargo add` (tokio-util, sha2, rand)                |
| `tests/mcp_types_test.rs`             | Replaced placeholder with real tests                            |
| `tests/mcp_client_test.rs`            | Created                                                         |
| `docs/explanation/implementations.md` | Updated with Phase 1 entry                                      |

#### Task 1.9: Success Criteria

- All MCP types round-trip through `serde_json::to_value` / `from_value`
  without data loss.
- `JsonRpcClient::request` correctly matches responses to pending senders.
- `McpProtocol::initialize` returns `XzatomaError::McpProtocolVersion` when
  the server returns a version not in `SUPPORTED_PROTOCOL_VERSIONS`.
- All Phase 1 tests pass under `cargo test`.
- `cargo clippy --all-targets --all-features -- -D warnings` passes with zero
  warnings.
- Greater than 80% line coverage on `src/mcp/types.rs`, `src/mcp/client.rs`,
  and `src/mcp/protocol.rs`.

---

### Phase 2: Transport Layer

**Goal:** Implement the `Transport` trait and two concrete transports: a stdio
child-process transport and a Streamable HTTP/SSE transport for the
`2025-11-25` spec. Also implement a `FakeTransport` for tests. Replace any
inline transport stub in `client.rs` with imports from this module.

#### Task 2.1: Define `Transport` Trait in `src/mcp/transport.rs`

```text
//! MCP transport abstraction

use crate::error::Result;
use futures::Stream;
use std::pin::Pin;

/// Abstraction over MCP transport implementations
///
/// Implementations exist for stdio (child process) and Streamable HTTP.
/// A FakeTransport is provided for tests.
#[async_trait::async_trait]
pub trait Transport: Send + Sync + std::fmt::Debug {
    /// Send a complete JSON-RPC message string
    async fn send(&self, message: String) -> Result<()>;

    /// Returns a stream of inbound JSON-RPC message strings (one per item,
    /// newlines stripped)
    fn receive(&self) -> Pin<Box<dyn Stream<Item = String> + Send + '_>>;

    /// Returns a stream of transport-level diagnostic strings (e.g., stderr
    /// from a child process)
    fn receive_err(&self) -> Pin<Box<dyn Stream<Item = String> + Send + '_>>;
}
```

Update `src/mcp/mod.rs` to add `pub mod transport;`.

#### Task 2.2: Create `src/mcp/transport/mod.rs`

```text
//! MCP transport implementations

pub mod stdio;
pub mod http;

#[cfg(test)]
pub mod fake;
```

The `Transport` trait definition stays in `src/mcp/transport.rs`. The canonical
import path is `crate::mcp::transport::Transport`.

#### Task 2.3: Implement `src/mcp/transport/stdio.rs`

**`pub struct StdioTransport`** fields:

- `stdin_tx: tokio::sync::mpsc::UnboundedSender<String>`
- `stdout_rx: Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<String>>>`
- `stderr_rx: Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<String>>>`
- `child: Arc<tokio::sync::Mutex<tokio::process::Child>>`

**`impl StdioTransport`**:

- `pub fn spawn(executable: std::path::PathBuf, args: Vec<String>,
env: std::collections::HashMap<String, String>,
working_dir: Option<std::path::PathBuf>) ->
crate::error::Result<Self>`:
  1. Build a `tokio::process::Command` with `stdin(Stdio::piped())`,
     `stdout(Stdio::piped())`, `stderr(Stdio::piped())`; apply `env` via
     `.env_clear()` then `.envs(env)`; apply `working_dir` with `.current_dir`
     if `Some`
  2. Call `.spawn()` to get a `tokio::process::Child`; map `std::io::Error`
     to `XzatomaError::McpTransport`
  3. Take `child.stdin`, `child.stdout`, `child.stderr` (all `Some` because
     `Stdio::piped()` was used)
  4. Spawn a Tokio task: wrap stdout in `tokio::io::BufReader`; iterate with
     `AsyncBufReadExt::lines()`; send each line on `stdout_tx`
  5. Spawn a second Tokio task for stderr: same pattern; each line is:
     a. Sent on `stderr_tx`
     b. Logged via `tracing::debug!(target: "xzatoma::mcp::transport::stdio",
"mcp server stderr: {}", line)` -- per MCP spec, stderr is diagnostic
     only and MUST NOT be treated as an error condition
  6. Return `Self`

**`impl Transport for StdioTransport`**:

- `send`: write `format!("{}\n", message)` to stdin using
  `tokio::io::AsyncWriteExt`
- `receive`: converts `stdout_rx` into a `futures::stream::unfold` stream
- `receive_err`: converts `stderr_rx` into a `futures::stream::unfold` stream

**`impl Drop for StdioTransport`**: sends SIGTERM to the child process on Unix
via `libc::kill(pid, libc::SIGTERM)`; on non-Unix calls
`child.try_wait()` and `child.start_kill()`. Do NOT block in `Drop` -- the
kill is best-effort.

Note: Add `libc` as a dependency only for Unix targets:
`cargo add libc --target cfg(unix)`.

#### Task 2.4: Implement `src/mcp/transport/http.rs`

Implements the `2025-11-25` Streamable HTTP transport specification.

**`pub struct HttpTransport`** fields:

- `http_client: Arc<reqwest::Client>`
- `endpoint: url::Url`
- `session_id: Arc<tokio::sync::RwLock<Option<String>>>`
- `protocol_version: String` -- always `"2025-11-25"` at construction time
- `headers: std::collections::HashMap<String, String>` -- static headers;
  auth token injected here by the auth layer before construction
- `response_tx: tokio::sync::mpsc::UnboundedSender<String>`
- `response_rx: Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<String>>>`
- `error_tx: tokio::sync::mpsc::UnboundedSender<String>`
- `error_rx: Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<String>>>`
- `last_event_id: Arc<tokio::sync::RwLock<Option<String>>>` -- for SSE
  resumption via `Last-Event-ID` header

**`impl HttpTransport`**:

- `pub fn new(endpoint: url::Url,
headers: std::collections::HashMap<String, String>,
timeout: std::time::Duration) -> Self` -- constructs the transport; builds
  `reqwest::Client` with `.timeout(timeout)`

**`impl Transport for HttpTransport`** -- `send`:

Every outbound message is an HTTP POST to `self.endpoint` with these mandatory
headers (in addition to any in `self.headers`):

- `Content-Type: application/json`
- `Accept: application/json, text/event-stream`
- `MCP-Protocol-Version: 2025-11-25` -- REQUIRED by spec on EVERY POST
- `MCP-Session-Id: <id>` -- only when `session_id` is `Some`
- `Last-Event-ID: <id>` -- only when reconnecting AND `last_event_id` is `Some`

Response handling by `Content-Type` of the HTTP response:

- `application/json`: read body to string; push to `response_tx`
- `text/event-stream`: spawn a Tokio task running `parse_sse_stream`
- `202 Accepted` (notification ACK): no-op
- `401 Unauthorized`: extract `WWW-Authenticate` header value; return
  `Err(XzatomaError::McpAuth(header_value))`
- `404` when `session_id` is `Some`: clear `session_id`; return
  `Err(XzatomaError::Mcp("mcp session expired".into()))`

Session ID capture: after a successful `initialize` POST, extract the
`MCP-Session-Id` response header value (if present) and store in
`self.session_id`.

**`fn parse_sse_stream`** -- free function taking an
`impl futures::Stream<Item = reqwest::Result<bytes::Bytes>>`, `response_tx`,
and `last_event_id`. For each byte chunk:

1. Split on `\n\n` (SSE event boundaries)
2. For each event, parse `data:`, `id:`, `event:`, `retry:` fields
3. If `id:` is present: store in `last_event_id`
4. If `data: [PING]` (case-insensitive) or `event: ping`: discard silently
5. All other `data:` values: push to `response_tx`

**`pub async fn open_get_stream(&self) -> crate::error::Result<()>`** -- issues
an HTTP GET to `self.endpoint` with `Accept: text/event-stream` and all
session headers; spawns a Tokio task running `parse_sse_stream`; returns
immediately.

**`impl Drop for HttpTransport`**: if `session_id` is `Some`, issues a
synchronous HTTP DELETE to the endpoint with the `MCP-Session-Id` header using
`reqwest::blocking::Client`. This is spec-required session termination.

#### Task 2.5: Implement `src/mcp/transport/fake.rs` (`#[cfg(test)]`)

In-process fake transport for unit and integration tests.

**`pub struct FakeTransport`** fields:

- `inbound_tx: tokio::sync::mpsc::UnboundedSender<String>`
- `inbound_rx: Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<String>>>`
- `outbound_tx: tokio::sync::mpsc::UnboundedSender<String>`
- `outbound_rx: Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<String>>>`

**`impl FakeTransport`**:

- `pub fn new() -> (Self, FakeTransportHandle)` -- returns the transport and
  a `FakeTransportHandle` holding the other channel ends
- `pub fn inject_response(&self, response: serde_json::Value)` -- serializes
  and pushes to `inbound_tx`

**`pub struct FakeTransportHandle`**:

- `pub outbound_rx: tokio::sync::mpsc::UnboundedReceiver<String>` -- the test
  reads what the client sent from here
- `pub inbound_tx: tokio::sync::mpsc::UnboundedSender<String>` -- the test
  sends server responses here

**`impl Transport for FakeTransport`**: `send` writes to `outbound_tx`;
`receive` drains `inbound_rx`; `receive_err` returns an empty stream.

#### Task 2.6: Add Test Binary for Integration Tests

Add a `[[bin]]` entry to `Cargo.toml` for the test MCP server. This binary is
only built during testing and is not part of the main release:

```text
[[bin]]
name = "mcp_test_server"
path = "tests/helpers/mcp_test_server/main.rs"
required-features = []
```

Create `tests/helpers/mcp_test_server/main.rs`. This binary must:

1. Read newline-delimited JSON from stdin in a loop
2. Handle `initialize` -- respond with a valid `InitializeResponse` using
   `protocol_version: "2025-11-25"` and non-empty `ServerCapabilities` where
   `tools: Some(serde_json::json!({}))` is set
3. Handle `tools/list` -- respond with one tool: name `"echo"`, description
   `"Echoes input"`, `input_schema: { "type": "object", "properties": {
"message": { "type": "string" } } }`
4. Handle `tools/call` with `name: "echo"` -- respond with
   `CallToolResponse { content: [ToolResponseContent::Text { text:
args["message"].as_str().unwrap_or("") }], is_error: None, ... }`
5. Write all responses to stdout as newline-delimited JSON

#### Task 2.7: Testing Requirements

Create `tests/mcp_http_transport_test.rs` using `wiremock`:

- `test_post_with_json_response_forwarded_to_receive` -- mock server returns
  `application/json` body; assert `receive()` yields the body.
- `test_post_with_sse_two_events_both_forwarded` -- mock server returns
  `text/event-stream` with two `data:` events; assert both are received.
- `test_post_202_yields_nothing` -- mock returns `202 Accepted`; assert
  `receive()` yields nothing within `Duration::from_millis(100)`.
- `test_mcp_protocol_version_header_present_on_every_post` -- assert
  `MCP-Protocol-Version: 2025-11-25` is in every captured POST.
- `test_session_id_captured_and_sent_on_subsequent_requests` -- first response
  includes `MCP-Session-Id: test-session-1` header; assert the second request
  includes `MCP-Session-Id: test-session-1`.
- `test_404_with_session_id_emits_mcp_error` -- mock returns `404` after
  session is set; assert `XzatomaError::Mcp("mcp session expired")`.
- `test_ping_sse_events_are_silently_dropped` -- SSE stream with `event: ping`
  followed by a real `data:` event; assert only the real event is received.

Create `tests/mcp_stdio_test.rs` for integration tests against the test server
binary:

- `test_stdio_transport_initialize_and_list_tools` -- spawn the test server
  binary using `StdioTransport::spawn`; build a `JsonRpcClient`; call
  `McpProtocol::initialize`; assert
  `InitializedMcpProtocol::capable(ServerCapabilityFlag::Tools)` is `true`;
  call `list_tools()`; assert the result contains the `"echo"` tool.
- `test_stdio_transport_call_echo_tool` -- call
  `call_tool("echo", Some(json!({"message": "hello"})), None)`; assert the
  response content text is `"hello"`.

#### Task 2.8: Deliverables

| File                                    | Action                                                             |
| --------------------------------------- | ------------------------------------------------------------------ |
| `src/mcp/transport.rs`                  | Created (trait definition)                                         |
| `src/mcp/transport/mod.rs`              | Created                                                            |
| `src/mcp/transport/stdio.rs`            | Created                                                            |
| `src/mcp/transport/http.rs`             | Created                                                            |
| `src/mcp/transport/fake.rs`             | Created (`cfg(test)`)                                              |
| `src/mcp/mod.rs`                        | Updated with `pub mod transport;`                                  |
| `Cargo.toml`                            | Updated with `[[bin]]` for `mcp_test_server` and `libc` dependency |
| `tests/helpers/mcp_test_server/main.rs` | Created                                                            |
| `tests/mcp_http_transport_test.rs`      | Created                                                            |
| `tests/mcp_stdio_test.rs`               | Created                                                            |
| `docs/explanation/implementations.md`   | Updated with Phase 2 entry                                         |

#### Task 2.9: Success Criteria

- `StdioTransport` drives `initialize` then `tools/list` with the real test
  server subprocess.
- `HttpTransport` includes `MCP-Protocol-Version: 2025-11-25` on every POST
  (verified by unit test).
- SSE streams with multiple events all reach `receive()`.
- Session ID round-trips correctly across multiple requests.
- All Phase 2 tests pass under `cargo test`.

---

### Phase 3: OAuth 2.1 / OIDC Authorization

**Goal:** Implement the full authorization flow required by the `2025-11-25`
specification. Authorization applies only to HTTP transport; stdio servers
obtain credentials from environment variables (per spec).

#### Task 3.1: Create `src/mcp/auth/mod.rs`

```text
//! MCP OAuth 2.1 / OIDC authorization

pub mod discovery;
pub mod flow;
pub mod manager;
pub mod pkce;
pub mod token_store;
```

Update `src/mcp/mod.rs` to add `pub mod auth;`.

#### Task 3.2: Implement `src/mcp/auth/token_store.rs`

**`pub struct OAuthToken`** -- `Debug, Clone, Serialize, Deserialize`:

- `pub access_token: String`
- `pub token_type: String`
- `pub expires_at: Option<chrono::DateTime<chrono::Utc>>` -- RFC-3339 via
  `chrono` serde feature
- `pub refresh_token: Option<String>`
- `pub scope: Option<String>`

**`impl OAuthToken`**:

- `pub fn is_expired(&self) -> bool` -- returns `true` if `expires_at` is
  `Some` and `Utc::now() >= (expires_at - chrono::Duration::seconds(60))`;
  the 60-second buffer allows early refresh before actual expiry

**`pub struct TokenStore;`** (zero-field struct; `keyring` is stateless)

**`impl TokenStore`**:

- `fn service_name(server_id: &str) -> String` -- returns
  `format!("xzatoma-mcp-{}", server_id)` (private helper)
- `pub fn save_token(&self, server_id: &str, token: &OAuthToken) ->
crate::error::Result<()>` -- serialize `token` to JSON via
  `serde_json::to_string`; store in keyring with
  `keyring::Entry::new(&service_name, server_id)?.set_password(&json_str)`
- `pub fn load_token(&self, server_id: &str) ->
crate::error::Result<Option<OAuthToken>>` -- load from keyring; if
  `keyring::Error::NoEntry`, return `Ok(None)`; otherwise deserialize
- `pub fn delete_token(&self, server_id: &str) -> crate::error::Result<()>`
  -- delete from keyring; if `NoEntry`, return `Ok(())`

#### Task 3.3: Implement `src/mcp/auth/discovery.rs`

**`pub struct ProtectedResourceMetadata`** -- `Debug, Clone, Serialize,
Deserialize`, `#[serde(rename_all = "snake_case")]`:

- `pub resource: String`
- `pub authorization_servers: Vec<String>`
- `pub scopes_supported: Option<Vec<String>>`
- `pub bearer_methods_supported: Option<Vec<String>>`

**`pub struct AuthorizationServerMetadata`** -- `Debug, Clone, Serialize,
Deserialize`, `#[serde(rename_all = "snake_case")]`:

- `pub issuer: String`
- `pub authorization_endpoint: String`
- `pub token_endpoint: String`
- `pub registration_endpoint: Option<String>`
- `pub scopes_supported: Option<Vec<String>>`
- `pub response_types_supported: Vec<String>`
- `pub grant_types_supported: Option<Vec<String>>`
- `pub code_challenge_methods_supported: Option<Vec<String>>`
- `pub client_id_metadata_document_supported: Option<bool>`
- `#[serde(flatten)] pub extra: std::collections::HashMap<String, serde_json::Value>`

**`pub struct ClientIdMetadataDocument`** -- `Debug, Clone, Serialize,
Deserialize`:

- `pub client_id: String`
- `pub client_name: String`
- `pub client_uri: Option<String>`
- `pub redirect_uris: Vec<String>`
- `pub grant_types: Option<Vec<String>>`
- `pub response_types: Option<Vec<String>>`
- `pub token_endpoint_auth_method: Option<String>`

**`pub async fn fetch_protected_resource_metadata(http: &reqwest::Client,
resource_url: &url::Url, www_authenticate: Option<&str>) ->
crate::error::Result<ProtectedResourceMetadata>`**:

1. If `www_authenticate` contains `resource_metadata=`, parse the URL from
   that attribute and GET it
2. Otherwise, construct the well-known URI per RFC 9728:
   `https://<host>/.well-known/oauth-protected-resource<path>` where `<path>`
   is the URL path of `resource_url`
3. Return `XzatomaError::McpAuth(...)` if neither succeeds

**`pub async fn fetch_authorization_server_metadata(http: &reqwest::Client,
issuer: &url::Url) -> crate::error::Result<AuthorizationServerMetadata>`**:

Try the following five endpoint orderings in order; return the first success:

1. `/.well-known/oauth-authorization-server/<path>` (path insertion)
2. `/.well-known/openid-configuration/<path>` (path insertion)
3. `<issuer>/.well-known/openid-configuration` (path appending)
4. `/.well-known/oauth-authorization-server`
5. `/.well-known/openid-configuration`

Return `XzatomaError::McpAuth("authorization server metadata not found")` if
all five fail.

**`pub async fn fetch_client_id_metadata_document(http: &reqwest::Client,
client_id_url: &url::Url) ->
crate::error::Result<ClientIdMetadataDocument>`** -- GET the URL and
deserialize the response body.

#### Task 3.4: Implement `src/mcp/auth/pkce.rs`

**`pub struct PkceChallenge`** -- `Debug, Clone`:

- `pub verifier: String` -- base64url-encoded (no padding), 43 chars for 32
  random bytes
- `pub challenge: String` -- base64url-encoded SHA-256 of the verifier
- `pub method: String` -- always `"S256"`

**`pub fn generate() -> crate::error::Result<PkceChallenge>`**:

1. Generate 32 cryptographically random bytes using `rand::thread_rng()` and
   `rand::RngCore::fill_bytes`
2. Base64url-encode (no padding) with
   `base64::engine::general_purpose::URL_SAFE_NO_PAD` to produce `verifier`
3. Compute `sha2::Sha256` digest of the UTF-8 bytes of the verifier string
   (per RFC 7636 section 4.2)
4. Base64url-encode (no padding) the digest bytes to produce `challenge`
5. Return `PkceChallenge { verifier, challenge, method: "S256".into() }`

**`pub fn verify_s256_support(metadata: &AuthorizationServerMetadata) ->
crate::error::Result<()>`**:

- If `metadata.code_challenge_methods_supported` is `None` OR does not
  contain `"S256"`, return
  `Err(XzatomaError::McpAuth("PKCE S256 not supported by authorization server".into()))`
- Otherwise return `Ok(())`

#### Task 3.5: Implement `src/mcp/auth/flow.rs`

**`pub struct OAuthFlowConfig`** -- `Debug, Clone`:

- `pub server_id: String`
- `pub resource_url: url::Url`
- `pub client_name: String` -- default `"Xzatoma"`
- `pub redirect_port: u16` -- `0` means OS-assigned
- `pub static_client_id: Option<String>`
- `pub static_client_secret: Option<String>`

**`pub struct OAuthFlow`** fields:

- `http: Arc<reqwest::Client>`
- `config: OAuthFlowConfig`

**`impl OAuthFlow`**:

**`pub async fn authorize(&self, server_metadata:
&AuthorizationServerMetadata, scope: Option<&str>) ->
crate::error::Result<OAuthToken>`**:

1. Call `verify_s256_support(server_metadata)`; propagate any error
2. Determine `client_id` via the following priority:
   a. If `config.static_client_id` is `Some`: use it
   b. Else if `server_metadata.client_id_metadata_document_supported ==
Some(true)`: use the local metadata endpoint URL string as `client_id`
   c. Else if `server_metadata.registration_endpoint` is `Some`: perform
   Dynamic Client Registration (RFC 7591) by POSTing registration data and
   extracting `client_id`
   d. Otherwise: return
   `Err(XzatomaError::McpAuth("no viable client registration mechanism".into()))`
3. Call `pkce::generate()` to get `PkceChallenge`
4. Generate `state`: 16 random bytes, base64url-encoded (no padding)
5. Bind `http://127.0.0.1:<redirect_port>/callback` using
   `tokio::net::TcpListener::bind`; if port is `0`, call `.local_addr()`
6. Construct the authorization URL with query parameters: `response_type=code`,
   `client_id`, `redirect_uri`, `scope` (if `Some`), `state`,
   `code_challenge`, `code_challenge_method=S256`,
   `resource=<config.resource_url>` (RFC 8707)
7. Print to stderr:
   `"Open the following URL in your browser to authorize Xzatoma:\n{url}"`
8. On macOS attempt `std::process::Command::new("open").arg(&url).spawn()`;
   on Linux attempt `xdg-open`; ignore spawn errors
9. Accept one TCP connection; read the HTTP GET request line; parse `code` and
   `state` query parameters from the path
10. Validate `state` matches the value from step 4; if not, return
    `Err(XzatomaError::McpAuth("state mismatch in OAuth callback".into()))`
11. Write a plain `HTTP/1.1 200 OK` response with body
    `"Authorization successful. You may close this tab."`
12. Exchange `code` for tokens: POST to `server_metadata.token_endpoint` with
    `Content-Type: application/x-www-form-urlencoded` body containing
    `grant_type=authorization_code`, `code`, `redirect_uri`, `client_id`,
    `code_verifier`, `resource`
13. Parse response JSON to `OAuthToken`; convert `expires_in` seconds to
    `expires_at: Some(Utc::now() + chrono::Duration::seconds(expires_in))`
14. Return `Ok(token)`

**`pub async fn refresh_token(&self, server_metadata:
&AuthorizationServerMetadata, refresh_token: &str, scope: Option<&str>) ->
crate::error::Result<OAuthToken>`** -- POST to token endpoint with
`grant_type=refresh_token` body. Parse and return `OAuthToken`.

**`pub async fn handle_step_up(&self, server_metadata:
&AuthorizationServerMetadata, www_authenticate: &str,
current_token: &OAuthToken) -> crate::error::Result<OAuthToken>`** -- parse
`scope=` value from `Bearer error="insufficient_scope"` challenge; call
`authorize` with the new scope. Limited to 3 attempts total; return
`XzatomaError::McpAuth("step-up authorization loop limit reached")` if
exceeded.

#### Task 3.6: Implement `src/mcp/auth/manager.rs`

**`pub struct AuthManager`** fields:

- `http: Arc<reqwest::Client>`
- `token_store: Arc<TokenStore>`
- `flow_configs: std::collections::HashMap<String, OAuthFlowConfig>`

**`impl AuthManager`**:

- `pub fn new(http: Arc<reqwest::Client>, token_store: Arc<TokenStore>) -> Self`
- `pub fn add_server(&mut self, server_id: String, config: OAuthFlowConfig)`
- `pub async fn get_token(&self, server_id: &str,
server_metadata: &AuthorizationServerMetadata) ->
crate::error::Result<String>`:
  1. `token_store.load_token(server_id)?`
  2. If `Some(token)` and `!token.is_expired()`: return `token.access_token`
  3. If `Some(token)` and `token.is_expired()` and
     `token.refresh_token.is_some()`: call `flow.refresh_token()`; if
     success: save updated token; return `access_token`; if refresh fails:
     fall through to full auth
  4. Call `flow.authorize()`; save token; return `access_token`
- `pub async fn handle_401(&self, server_id: &str, www_authenticate: &str,
server_metadata: &AuthorizationServerMetadata) ->
crate::error::Result<String>` -- delete existing token; call `get_token`
  to trigger full re-auth
- `pub async fn handle_403_scope(&self, server_id: &str,
www_authenticate: &str, server_metadata: &AuthorizationServerMetadata,
current_token: &OAuthToken) -> crate::error::Result<String>` -- call
  `flow.handle_step_up()`; save token; return `access_token`
- `pub fn inject_token(headers: &mut std::collections::HashMap<String, String>,
token: &str)` -- `headers.insert("Authorization".into(),
format!("Bearer {}", token))`

#### Task 3.7: Testing Requirements

Create `tests/mcp_auth_pkce_test.rs`:

- `test_generate_produces_correct_verifier_length` -- call `generate()`; assert
  `verifier.len() == 43`.
- `test_challenge_is_correct_s256_of_verifier` -- compute expected challenge
  manually; assert equality.
- `test_method_is_always_s256` -- assert `challenge.method == "S256"`.
- `test_verify_s256_support_rejects_when_absent` -- metadata with
  `code_challenge_methods_supported: Some(vec!["plain".into()])`; assert
  `Err(XzatomaError::McpAuth(...))`.
- `test_verify_s256_support_accepts_when_present` -- metadata with
  `code_challenge_methods_supported: Some(vec!["S256".into()])`; assert `Ok(())`.

Create `tests/mcp_auth_discovery_test.rs` using `wiremock`:

- `test_fetch_protected_resource_metadata_from_www_authenticate_header` --
  `wiremock` serves the metadata at a URL embedded in the header; assert
  correct parsing.
- `test_fetch_protected_resource_metadata_falls_back_to_well_known` -- no
  header; `wiremock` serves at well-known URI; assert success.
- `test_fetch_authorization_server_metadata_tries_five_orderings` -- set up
  `wiremock` to return `404` for the first four orderings and `200` for the
  fifth; assert the function succeeds.

Create `tests/mcp_auth_token_store_test.rs`:

Note: Tests that interact with the OS keychain MUST be marked `#[ignore]`
with the comment `"requires system keyring"`.

- `test_oauth_token_is_expired_when_past_expiry` -- `expires_at` is 1 second
  in the past; assert `is_expired() == true`.
- `test_oauth_token_not_expired_when_future_expiry` -- `expires_at` is 1 hour
  in the future; assert `is_expired() == false`.
- `test_oauth_token_not_expired_when_no_expiry` -- `expires_at: None`; assert
  `is_expired() == false`.
- `test_token_roundtrip_through_json` -- serialize and deserialize
  `OAuthToken`; assert all fields preserved.

Create `tests/mcp_auth_flow_test.rs` using `wiremock`:

- `test_full_pkce_exchange_sends_correct_verifier` -- set up a `wiremock` mock
  OAuth server; run the token exchange portion of `authorize`; assert the
  `code_verifier` in the token endpoint POST body matches the verifier from
  `generate()`.

#### Task 3.8: Deliverables

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

#### Task 3.9: Success Criteria

- PKCE `S256` challenge verified correct against a known test vector.
- Discovery tries all five well-known endpoint orderings (unit test verified).
- Authorization URL includes `resource` parameter (RFC 8707).
- `is_expired` returns `true` when past `expires_at - 60s`; `false` otherwise.
- `inject_token` sets `Authorization: Bearer <token>` correctly.
- All Phase 3 tests pass under `cargo test`.

---

### Phase 4: MCP Client Lifecycle and Server Manager

**Goal:** Implement the full server configuration types, the `McpConfig`
struct, and the `McpClientManager` which manages the lifecycle of all connected
MCP servers. Wire `McpConfig` fully into `src/config.rs` including env vars
and validation.

#### Task 4.1: Replace `src/mcp/server.rs` Stub

Define these types in `src/mcp/server.rs`:

**`pub struct OAuthServerConfig`** -- `Debug, Clone, Default, Serialize,
Deserialize`; all fields `Option<>` and `#[serde(default)]`:

- `pub client_id: Option<String>`
- `pub client_secret: Option<String>`
- `pub redirect_port: Option<u16>` -- default OS-assigned (`0`)
- `pub metadata_url: Option<String>` -- override well-known discovery URL

**`pub enum McpServerTransportConfig`** -- `Debug, Clone, Serialize,
Deserialize`, `#[serde(tag = "type", rename_all = "lowercase")]`:

- `Stdio { executable: String, #[serde(default)] args: Vec<String>,
# [serde(default)] env: std::collections::HashMap<String, String>,
working_dir: Option<String> }`
- `Http { endpoint: url::Url, #[serde(default)] headers:
std::collections::HashMap<String, String>, timeout_seconds: Option<u64>,
oauth: Option<OAuthServerConfig> }`

**`pub struct McpServerConfig`** -- `Debug, Clone, Serialize, Deserialize`:

- `pub id: String` -- validated as matching regex `^[a-z0-9_-]{1,64}$`
- `pub transport: McpServerTransportConfig`
- `#[serde(default = "default_true")] pub enabled: bool`
- `#[serde(default = "default_timeout")] pub timeout_seconds: u64` -- default `30`
- `#[serde(default = "default_true")] pub tools_enabled: bool`
- `#[serde(default)] pub resources_enabled: bool` -- default `false`
- `#[serde(default)] pub prompts_enabled: bool` -- default `false`
- `#[serde(default)] pub sampling_enabled: bool` -- default `false`
- `#[serde(default = "default_true")] pub elicitation_enabled: bool`

**`impl McpServerConfig`**:

- `pub fn validate(&self) -> crate::error::Result<()>`:
  1. Validate `id` matches `^[a-z0-9_-]{1,64}$` using `regex::Regex`; return
     `XzatomaError::Config(...)` if invalid
  2. For `Stdio` variant: check `executable` is non-empty; return
     `XzatomaError::Config(...)` if empty
  3. For `Http` variant: check `endpoint.scheme()` is `"https"` or `"http"`;
     return `XzatomaError::Config(...)` for any other scheme

Private module-level helpers:

```text
fn default_true() -> bool { true }
fn default_timeout() -> u64 { 30 }
```

#### Task 4.2: Replace `src/mcp/config.rs` Stub

**`pub struct McpConfig`** -- `Debug, Clone, Serialize, Deserialize`,
`#[serde(default)]` on the struct:

- `#[serde(default)] pub servers: Vec<McpServerConfig>`
- `#[serde(default = "default_request_timeout")] pub request_timeout_seconds: u64`
  -- default `30`
- `#[serde(default = "default_true")] pub auto_connect: bool`
- `#[serde(default = "default_true")] pub expose_resources_tool: bool`
- `#[serde(default = "default_true")] pub expose_prompts_tool: bool`

Private module-level helpers:

```text
fn default_request_timeout() -> u64 { 30 }
fn default_true() -> bool { true }
```

**`impl Default for McpConfig`**:

```text
fn default() -> Self {
    Self {
        servers: Vec::new(),
        request_timeout_seconds: 30,
        auto_connect: true,
        expose_resources_tool: true,
        expose_prompts_tool: true,
    }
}
```

**`impl McpConfig`**:

- `pub fn validate(&self) -> crate::error::Result<()>`:
  1. Collect all `server.id` values; if any duplicate exists, return
     `XzatomaError::Config(format!("duplicate MCP server id: {}", id))`
  2. Call `server.validate()` for each server; propagate errors

#### Task 4.3: Update `src/config.rs` Fully

These changes build on the Phase 0 stub that added the `mcp` field.

**Add to `Config::apply_env_vars`** (inside the existing `fn apply_env_vars`
function, after the existing env var handling):

```text
if let Ok(val) = std::env::var("XZATOMA_MCP_REQUEST_TIMEOUT") {
    if let Ok(n) = val.parse::<u64>() {
        self.mcp.request_timeout_seconds = n;
    }
}
if let Ok(val) = std::env::var("XZATOMA_MCP_AUTO_CONNECT") {
    self.mcp.auto_connect =
        matches!(val.to_lowercase().as_str(), "true" | "1" | "yes");
}
```

**Add to `Config::validate`** (append before the final `Ok(())` return of the
validate function):

```text
self.mcp.validate()?;
```

#### Task 4.4: Create `src/mcp/manager.rs`

**`pub enum McpServerState`** -- `Debug, Clone, PartialEq`:

- `Disconnected`
- `Connecting`
- `Connected`
- `Failed(String)`

**`pub struct McpServerEntry`** fields:

- `pub config: McpServerConfig`
- `pub protocol: Option<Arc<InitializedMcpProtocol>>`
- `pub tools: Vec<McpTool>`
- `pub state: McpServerState`
- `pub auth_manager: Option<Arc<AuthManager>>`
- `pub server_metadata: Option<AuthorizationServerMetadata>`
- `pub read_loop_handle: Option<tokio::task::JoinHandle<()>>`

**`pub struct McpClientManager`** fields:

- `servers: std::collections::HashMap<String, McpServerEntry>`
- `http_client: Arc<reqwest::Client>`
- `token_store: Arc<TokenStore>`
- `task_manager: Arc<std::sync::Mutex<crate::mcp::task_manager::TaskManager>>`
  (populated in Phase 6; use `Default::default()` until then)

**`impl McpClientManager`**:

- `pub fn new(http_client: Arc<reqwest::Client>, token_store: Arc<TokenStore>)
-> Self`

- `pub async fn connect(&mut self, config: McpServerConfig) ->
crate::error::Result<()>`:

  1. Set entry state to `Connecting`
  2. Build transport based on `config.transport` variant:
     - `Stdio`: call `StdioTransport::spawn(...)`
     - `Http` without OAuth: call `HttpTransport::new(endpoint, headers, timeout)`
     - `Http` with OAuth: run auth discovery; call `auth_manager.get_token()`;
       call `inject_token` on `headers`; then call `HttpTransport::new`
  3. Create `(outbound_tx, outbound_rx) = mpsc::unbounded_channel::<String>()`
  4. Create `(inbound_tx, inbound_rx) = mpsc::unbounded_channel::<String>()`
  5. Spawn a Tokio task that:
     a. Polls `transport.receive()` and forwards each message to `inbound_tx`
     b. Polls `outbound_rx` and calls `transport.send(msg)` for each message
  6. Create `JsonRpcClient::new(outbound_tx)`
  7. Start read loop: `JsonRpcClient::start_read_loop(inbound_rx,
cancellation_token.clone(), Arc::new(client))`; store handle
  8. Build `xzatoma_client_capabilities()` (see Task 4.5)
  9. Call `McpProtocol::new(client).initialize(client_info, capabilities).await`
  10. Install sampling handler if `config.sampling_enabled`: create
      `XzatomaSamplingHandler` stub (implemented in Phase 5B); register via
      `register_sampling_handler`
  11. Install elicitation handler if `config.elicitation_enabled`: create
      `XzatomaElicitationHandler` stub; register via
      `register_elicitation_handler`
  12. If `config.tools_enabled`: call `protocol.list_tools()`, store in
      `entry.tools`
  13. Set entry state to `Connected`; store `protocol` in `entry.protocol`

- `pub async fn disconnect(&mut self, id: &str) -> crate::error::Result<()>`
  -- abort the `read_loop_handle` (call `.abort()`); drop the `protocol`;
  set state to `Disconnected`

- `pub async fn reconnect(&mut self, id: &str) -> crate::error::Result<()>`
  -- call `disconnect(id)`; get the config from the entry; call `connect(config)`

- `pub async fn refresh_tools(&mut self, id: &str) ->
crate::error::Result<()>` -- get the protocol for `id`; call
  `list_tools()`; update `entry.tools`

- `pub fn connected_servers(&self) -> Vec<&McpServerEntry>` -- return entries
  where `state == McpServerState::Connected`

- `pub fn get_tools_for_registry(&self) ->
Vec<(String, Vec<McpTool>)>` -- for each connected server with
  `tools_enabled`, return `(server_id, tools.clone())`

- `pub async fn call_tool(&self, server_id: &str, tool_name: &str,
arguments: Option<serde_json::Value>) ->
crate::error::Result<CallToolResponse>`:

  1. Look up `server_id` in `servers`; return `XzatomaError::McpServerNotFound`
     if absent
  2. Check `entry.tools` contains a tool with `name == tool_name`; return
     `XzatomaError::McpToolNotFound` if absent
  3. Call `protocol.call_tool(tool_name, arguments, None).await`
  4. If the result is `Err(XzatomaError::McpAuth(_))` AND `auth_manager` is
     `Some`: call `auth_manager.handle_401()` to get a new token; inject it
     into the transport headers; retry `call_tool` once; return the retry
     result regardless

- `pub async fn call_tool_as_task(&self, server_id: &str, tool_name: &str,
arguments: Option<serde_json::Value>, ttl: Option<u64>) ->
crate::error::Result<CallToolResponse>` -- call
  `protocol.call_tool(tool_name, arguments, Some(TaskParams { ttl }))`; if
  the response `meta` indicates a task was created, delegate to
  `TaskManager::wait_for_completion`; otherwise return the response directly

- `pub async fn list_resources(&self, server_id: &str) ->
crate::error::Result<Vec<Resource>>` -- delegates to
  `protocol.list_resources()`

- `pub async fn read_resource(&self, server_id: &str, uri: &str) ->
crate::error::Result<String>` -- call `protocol.read_resource(uri)`;
  for `TextResourceContents` return `text`; for `BlobResourceContents`
  return the blob string prefixed with `"[base64 <mime_type>] "`

- `pub async fn list_prompts(&self, server_id: &str) ->
crate::error::Result<Vec<Prompt>>` -- delegates to
  `protocol.list_prompts()`

- `pub async fn get_prompt(&self, server_id: &str, name: &str,
arguments: std::collections::HashMap<String, String>) ->
crate::error::Result<GetPromptResponse>` -- delegates to
  `protocol.get_prompt(name, Some(arguments))`

#### Task 4.5: Canonical `ClientCapabilities` Value

Define the following function in `src/mcp/manager.rs`:

```text
/// Returns the ClientCapabilities that Xzatoma advertises to MCP servers
pub fn xzatoma_client_capabilities() -> ClientCapabilities {
    ClientCapabilities {
        sampling: Some(SamplingCapability {
            tools: Some(serde_json::json!({})),
            context: None,
        }),
        elicitation: Some(ElicitationCapability {
            form: Some(serde_json::json!({})),
            url: Some(serde_json::json!({})),
        }),
        roots: Some(RootsCapability {
            list_changed: Some(true),
        }),
        tasks: Some(TasksCapability { /* all sub-fields Some(json!({})) */ }),
        experimental: None,
    }
}
```

#### Task 4.6: Update `src/mcp/mod.rs`

Add:

```text
pub mod manager;
```

#### Task 4.7: Testing Requirements

Create `tests/mcp_manager_test.rs`:

- `test_connect_succeeds_with_fake_transport_and_valid_initialize_response` --
  inject a valid `InitializeResponse` via `FakeTransport`; assert `connect`
  returns `Ok(())`; assert `entry.state == McpServerState::Connected`.
- `test_connect_fails_with_protocol_version_mismatch` -- inject a response
  with `protocol_version: "1999-01-01"`; assert
  `Err(XzatomaError::McpProtocolVersion { ... })`.
- `test_refresh_tools_updates_cached_tool_list` -- first `list_tools` returns
  `[tool_a]`; after `refresh_tools`, inject a response returning
  `[tool_a, tool_b]`; assert `entry.tools.len() == 2`.
- `test_call_tool_returns_not_found_for_unknown_tool_name` -- `entry.tools`
  is empty; assert `call_tool("server", "missing_tool", None)` returns
  `Err(XzatomaError::McpToolNotFound { ... })`.
- `test_401_triggers_reauth_and_single_retry` -- first `call_tool` returns
  `Err(XzatomaError::McpAuth(_))`; assert `auth_manager.handle_401` is called;
  assert second `call_tool` attempt is made.

Create `tests/mcp_config_test.rs`:

- `test_server_id_rejects_uppercase` -- `id: "MyServer"` fails `validate`.
- `test_server_id_rejects_spaces` -- `id: "my server"` fails `validate`.
- `test_server_id_rejects_too_long` -- 65-char id fails `validate`.
- `test_server_id_accepts_valid` -- `id: "my-server_01"` passes `validate`.
- `test_config_yaml_without_mcp_key_loads_default` -- parse a YAML string
  that has no `mcp:` key; assert `Config::load` succeeds and
  `config.mcp.servers.is_empty()`.
- `test_duplicate_server_ids_fail_mcp_config_validate` -- two servers with
  the same id; assert `McpConfig::validate` returns `Err`.
- `test_env_var_xzatoma_mcp_request_timeout_applies` -- set
  `XZATOMA_MCP_REQUEST_TIMEOUT`; load config; assert
  `config.mcp.request_timeout_seconds` equals the env value.
- `test_env_var_xzatoma_mcp_auto_connect_applies` -- set
  `XZATOMA_MCP_AUTO_CONNECT=false`; load config; assert
  `config.mcp.auto_connect == false`.

#### Task 4.8: Deliverables

| File                                  | Action                                  |
| ------------------------------------- | --------------------------------------- |
| `src/mcp/server.rs`                   | Replaced with full implementation       |
| `src/mcp/config.rs`                   | Replaced with full implementation       |
| `src/mcp/manager.rs`                  | Created                                 |
| `src/mcp/mod.rs`                      | Updated with `pub mod manager;`         |
| `src/config.rs`                       | `apply_env_vars` and `validate` updated |
| `tests/mcp_manager_test.rs`           | Created                                 |
| `tests/mcp_config_test.rs`            | Created                                 |
| `docs/explanation/implementations.md` | Updated with Phase 4 entry              |

#### Task 4.9: Success Criteria

- `connect` completes `initialize` then `tools/list` using `FakeTransport`.
- `Config` loaded from YAML with no `mcp:` key produces `McpConfig::default()`.
- Duplicate server IDs caught by `McpConfig::validate`.
- `XZATOMA_MCP_REQUEST_TIMEOUT` env var applies to
  `mcp.request_timeout_seconds`.
- `XZATOMA_MCP_AUTO_CONNECT` env var applies to `mcp.auto_connect`.
- 401 during `call_tool` triggers a single retry via `auth_manager`.
- All Phase 4 tests pass under `cargo test`.

---

### Phase 5A: Tool Bridge, Resources, and Prompts

**Goal:** Implement the approval policy, the `McpToolExecutor`,
`McpResourceToolExecutor`, and `McpPromptToolExecutor` adapters that implement
the `ToolExecutor` trait, and the `register_mcp_tools` helper used in both run
and chat commands.

#### Task 5A.1: Create `src/mcp/approval.rs`

This module is the single authoritative source of the MCP tool auto-approval
policy. No inline policy checks are permitted elsewhere.

````text
//! MCP tool auto-approval policy

use crate::config::ExecutionMode;

/// Returns `true` if MCP tool calls should be auto-approved without prompting
/// the user.
///
/// Auto-approval applies when:
/// - `execution_mode == ExecutionMode::FullAutonomous`: the user has
///   explicitly opted into unrestricted autonomous operation, OR
/// - `headless == true`: the run command is always non-interactive; all
///   tool calls within a plan must proceed without blocking for user input.
///
/// All other modes (`Interactive`, `RestrictedAutonomous`) require presenting
/// a confirmation prompt for each MCP tool invocation.
///
/// # Examples
///
/// ```
/// use xzatoma::config::ExecutionMode;
/// use xzatoma::mcp::approval::should_auto_approve;
///
/// assert!(should_auto_approve(ExecutionMode::FullAutonomous, false));
/// assert!(should_auto_approve(ExecutionMode::Interactive, true));
/// assert!(!should_auto_approve(ExecutionMode::Interactive, false));
/// ```
pub fn should_auto_approve(execution_mode: ExecutionMode, headless: bool) -> bool {
    headless || execution_mode == ExecutionMode::FullAutonomous
}
````

Add `pub mod approval;` to `src/mcp/mod.rs`.

#### Task 5A.2: Create `src/mcp/tool_bridge.rs`

**Namespacing rule:** the registry name for any MCP tool MUST be
`format!("{}__{}", server_id, tool_name)` (double underscore separator).
This is REQUIRED because both `server_id` and `tool_name` may independently
contain single underscores, making a single-underscore separator ambiguous.

**`pub struct McpToolExecutor`** -- implements `crate::tools::ToolExecutor`;
`#[derive(Debug)]`:

- `server_id: String`
- `tool_name: String` -- original name from the MCP server
- `registry_name: String` -- `format!("{}__{}", server_id, tool_name)`
- `description: String`
- `input_schema: serde_json::Value`
- `task_support: Option<TaskSupport>`
- `manager: Arc<tokio::sync::RwLock<McpClientManager>>`
- `execution_mode: ExecutionMode`
- `headless: bool`

**`impl crate::tools::ToolExecutor for McpToolExecutor`**:

- `fn tool_definition(&self) -> serde_json::Value`:

  Returns a JSON value following the `{ "name", "description", "parameters" }`
  format used by Xzatoma's existing tools (see `src/tools/mod.rs`):

  ```text
  serde_json::json!({
      "name": self.registry_name,
      "description": self.description,
      "parameters": self.input_schema,
  })
  ```

- `async fn execute(&self, args: serde_json::Value) ->
crate::error::Result<crate::tools::ToolResult>`:

  1. If `!should_auto_approve(self.execution_mode, self.headless)`:
     - Print to stderr:
       `"MCP tool call: {}/{} with args: {}. Allow? [y/N] "` using
       `eprint!`
     - Read one line from stdin using `std::io::BufRead::read_line`
     - If the trimmed line is not `"y"` or `"yes"` (case-insensitive):
       return `Ok(ToolResult::error(format!("User rejected MCP tool call: {}", self.registry_name)))`
  2. Acquire a read lock on `self.manager`
  3. If `task_support == Some(TaskSupport::Required)`: call
     `manager.call_tool_as_task(&self.server_id, &self.tool_name,
Some(args), None).await`
  4. Otherwise: call
     `manager.call_tool(&self.server_id, &self.tool_name,
Some(args)).await`
  5. If `response.is_error == Some(true)`: extract text from content items;
     return `Ok(ToolResult::error(text_content))`
  6. Build `text_content`: join all `ToolResponseContent::Text { text }`
     items with `"\n"`
  7. If `response.structured_content` is `Some(v)`: append
     `format!("\n---\n{}", serde_json::to_string_pretty(&v)?)` to
     `text_content`
  8. Return `Ok(ToolResult::success(text_content))`

**`pub fn register_mcp_tools(registry: &mut ToolRegistry,
manager: Arc<tokio::sync::RwLock<McpClientManager>>,
execution_mode: ExecutionMode, headless: bool) ->
crate::error::Result<usize>`**:

```text
1. Acquire a read lock on manager
2. Call manager.get_tools_for_registry()
3. Drop the read lock
4. For each (server_id, tools) pair:
   a. For each McpTool in tools:
      - Construct McpToolExecutor with:
          registry_name = format!("{}__{}", server_id, tool.name)
          description = tool.description.unwrap_or_default()
          input_schema = tool.input_schema.clone()
          task_support = tool.execution.as_ref().and_then(|e| e.task_support.clone())
      - Call registry.register(registry_name.clone(), Arc::new(executor))
        NOTE: ToolRegistry::register silently overwrites duplicates.
        Log a warning via tracing::warn! if the registry name was already
        present (check with registry.get(&registry_name).is_some() BEFORE
        calling register)
5. Return total count of registered tools
```

**`pub struct McpResourceToolExecutor`** -- implements `ToolExecutor`;
`#[derive(Debug)]`:

- `manager: Arc<tokio::sync::RwLock<McpClientManager>>`
- `execution_mode: ExecutionMode`
- `headless: bool`

**`impl ToolExecutor for McpResourceToolExecutor`**:

- `fn tool_definition(&self) -> serde_json::Value`:

  ```text
  serde_json::json!({
      "name": "mcp_read_resource",
      "description": "Read a resource from a connected MCP server by URI",
      "parameters": {
          "type": "object",
          "properties": {
              "server_id": {
                  "type": "string",
                  "description": "MCP server identifier"
              },
              "uri": {
                  "type": "string",
                  "description": "Resource URI to read"
              }
          },
          "required": ["server_id", "uri"]
      }
  })
  ```

- `async fn execute(&self, args: serde_json::Value) ->
crate::error::Result<ToolResult>` -- extract `server_id` and `uri` from
  `args`; check `should_auto_approve` and prompt if needed; call
  `manager.read_resource(server_id, uri).await`; return
  `ToolResult::success(content)`

**`pub struct McpPromptToolExecutor`** -- same structure as
`McpResourceToolExecutor` with different schema and execution.

**`impl ToolExecutor for McpPromptToolExecutor`**:

- `fn tool_definition(&self) -> serde_json::Value` -- schema with fields
  `server_id` (string), `prompt_name` (string), `arguments` (object);
  `required: ["server_id", "prompt_name"]`; tool name `"mcp_get_prompt"`

- `async fn execute(&self, args: serde_json::Value) ->
crate::error::Result<ToolResult>` -- extract `server_id`, `prompt_name`,
  `arguments` (default to empty map); call
  `manager.get_prompt(server_id, prompt_name, arguments).await`; format
  `GetPromptResponse.messages` as `"[<role>]\n<content_text>"` blocks
  separated by blank lines; return as `ToolResult::success`

Add `pub mod tool_bridge;` and `pub mod approval;` to `src/mcp/mod.rs`.

#### Task 5A.3: Testing Requirements

Create `tests/mcp_tool_bridge_test.rs`:

- `test_registry_name_uses_double_underscore_separator` -- construct an
  `McpToolExecutor` with `server_id: "my_server"` and
  `tool_name: "search_items"`; assert
  `registry_name == "my_server__search_items"`.
- `test_tool_definition_format_matches_xzatoma_convention` -- assert the
  returned JSON has top-level keys `"name"`, `"description"`, and
  `"parameters"`.
- `test_execute_returns_success_for_text_response` -- backed by `FakeTransport`;
  call `execute`; assert the result is `Ok` and contains the expected text.
- `test_execute_maps_is_error_to_error_result` -- response has
  `is_error: Some(true)`; assert `ToolResult.success == false`.
- `test_structured_content_appended_after_delimiter` -- response has
  `structured_content: Some(json!({"key": "value"}))`; assert the output
  contains `"---"` followed by the JSON.
- `test_should_auto_approve_false_in_full_autonomous_mode` --
  `execution_mode: ExecutionMode::FullAutonomous`, `headless: false`;
  assert `should_auto_approve` returns `true` (no prompt needed).
- `test_should_auto_approve_true_when_headless` --
  `execution_mode: ExecutionMode::Interactive`, `headless: true`;
  assert `should_auto_approve` returns `true`.
- `test_should_auto_approve_false_in_interactive_mode` --
  `execution_mode: ExecutionMode::Interactive`, `headless: false`;
  assert `should_auto_approve` returns `false` (prompt required).
- `test_task_support_required_routes_to_call_tool_as_task` -- set
  `task_support: Some(TaskSupport::Required)`; assert `call_tool_as_task`
  path is taken (verify via the JSON captured on
  `FakeTransportHandle.outbound_rx`).

#### Task 5A.4: Deliverables

| File                                  | Action                                                      |
| ------------------------------------- | ----------------------------------------------------------- |
| `src/mcp/approval.rs`                 | Created                                                     |
| `src/mcp/tool_bridge.rs`              | Created                                                     |
| `src/mcp/mod.rs`                      | Updated with `pub mod approval;` and `pub mod tool_bridge;` |
| `tests/mcp_tool_bridge_test.rs`       | Created                                                     |
| `docs/explanation/implementations.md` | Updated with Phase 5A entry                                 |

#### Task 5A.5: Success Criteria

- `registry_name` is always `<server_id>__<tool_name>` (double underscore).
- `tool_definition()` returns the `{ "name", "description", "parameters" }`
  format matching Xzatoma's existing tool convention.
- `should_auto_approve` returns `true` in `FullAutonomous` and headless
  contexts; `false` in `Interactive` and `RestrictedAutonomous` non-headless.
- `structured_content` is appended after a `"---"` delimiter.
- Confirmation prompt is shown when `should_auto_approve` is `false` and
  user input is read from stdin.
- All Phase 5A tests pass under `cargo test`.

---

### Phase 5B: Sampling, Elicitation, and Command Integration

**Goal:** Implement sampling and elicitation callbacks, wire the
`McpClientManager` into both the run and chat commands, and add an integration
test exercising the full tool call flow end-to-end.

#### Task 5B.1: Create `src/mcp/sampling.rs`

**`pub struct XzatomaSamplingHandler`** -- `Debug`:

- `provider: Arc<dyn crate::providers::Provider>`
- `execution_mode: ExecutionMode`
- `headless: bool`

**`impl SamplingHandler for XzatomaSamplingHandler`**:

`fn create_message<'a>(&'a self, params: CreateMessageRequest) ->
BoxFuture<'a, crate::error::Result<CreateMessageResult>>`:

```text
Box::pin(async move {
    // Step 1: Interactive confirmation check
    if !should_auto_approve(self.execution_mode, self.headless) {
        eprintln!(
            "MCP server requests LLM sampling. System prompt: {:?}. \
             Max tokens: {}. Allow? [y/N] ",
            params.system_prompt, params.max_tokens
        );
        let mut line = String::new();
        std::io::BufRead::read_line(
            &mut std::io::stdin().lock(), &mut line
        )?;
        if !matches!(line.trim().to_lowercase().as_str(), "y" | "yes") {
            return Err(XzatomaError::McpElicitation(
                "user rejected sampling request".into(),
            ));
        }
    }

    // Step 2: Convert CreateMessageRequest.messages to provider Message format.
    // Map Role::User -> role string "user"
    // Map Role::Assistant -> role string "assistant"
    // Map MessageContent::Text to the text string

    // Step 3: Prepend system_prompt as a system message if present

    // Step 4: Call self.provider.complete(&messages, &[])
    //         (no tool definitions for sampling)

    // Step 5: Map the CompletionResponse to CreateMessageResult
    //         stop_reason: if response has tool calls set "toolUse", else "endTurn"
})
```

Add `pub mod sampling;` to `src/mcp/mod.rs`.

#### Task 5B.2: Create `src/mcp/elicitation.rs`

**`pub struct XzatomaElicitationHandler`** -- `Debug`:

- `execution_mode: ExecutionMode`
- `headless: bool`

**`impl ElicitationHandler for XzatomaElicitationHandler`**:

`fn create_elicitation<'a>(&'a self, params: ElicitationCreateParams) ->
BoxFuture<'a, crate::error::Result<ElicitationResult>>`:

```text
Box::pin(async move {
    match params.mode.unwrap_or(ElicitationMode::Form) {
        ElicitationMode::Form => {
            if self.headless
                || self.execution_mode == ExecutionMode::FullAutonomous
            {
                tracing::warn!(
                    "MCP elicitation request received in non-interactive \
                     context; cancelling"
                );
                return Ok(ElicitationResult {
                    action: ElicitationAction::Cancel,
                    content: None,
                });
            }
            // Print message and prompt for each field in requested_schema
            // Collect values using rustyline DefaultEditor
            // Validate against requested_schema (check required fields)
            // Return ElicitationResult { action: Accept, content: Some(collected) }
            // If user types "decline": return { action: Decline, content: None }
        }
        ElicitationMode::Url => {
            if self.headless {
                tracing::warn!(
                    "MCP URL elicitation received in headless context; cancelling"
                );
                return Ok(ElicitationResult {
                    action: ElicitationAction::Cancel,
                    content: None,
                });
            }
            let url = params.url.as_deref().unwrap_or("(no URL provided)");
            eprintln!("MCP server requests authorization at: {}", url);
            // Attempt to open in browser on macOS/Linux
            // Wait for notification via registered handler (30s timeout)
            // Return Accept when notification arrives, or Cancel on timeout
        }
    }
})
```

Add `pub mod elicitation;` to `src/mcp/mod.rs`.

#### Task 5B.3: Wire MCP into `src/commands/mod.rs` -- Run Command

Locate the `pub mod run` inline module in `src/commands/mod.rs`. Inside
`run_plan_with_options`, add the following block immediately after the
`Config` is loaded and before the `ToolRegistryBuilder` is constructed:

```text
// Build MCP client manager if auto_connect is enabled and servers are configured
let mcp_manager = if config.mcp.auto_connect && !config.mcp.servers.is_empty() {
    use crate::mcp::manager::McpClientManager;
    use crate::mcp::auth::token_store::TokenStore;
    use tokio::sync::RwLock;

    let http_client = Arc::new(reqwest::Client::new());
    let token_store = Arc::new(TokenStore);
    let mut manager = McpClientManager::new(http_client, token_store);

    for server_config in config.mcp.servers.iter().filter(|s| s.enabled) {
        if let Err(e) = manager.connect(server_config.clone()).await {
            tracing::warn!(
                "Failed to connect to MCP server {}: {}",
                server_config.id, e
            );
        }
    }

    Some(Arc::new(RwLock::new(manager)))
} else {
    None
};
```

After the `ToolRegistryBuilder::build()` call creates the `registry`, add:

```text
if let Some(ref manager) = mcp_manager {
    use crate::mcp::tool_bridge::register_mcp_tools;
    use crate::mcp::protocol::ServerCapabilityFlag;

    let execution_mode = config.agent.terminal.default_mode;
    let count = register_mcp_tools(
        &mut registry,
        Arc::clone(manager),
        execution_mode,
        true, // headless: run command is always headless
    )?;
    tracing::info!("Registered {} MCP tools", count);

    // Register expose_resources_tool if any server has resources capability
    if config.mcp.expose_resources_tool {
        let reader = manager.read().await;
        let has_resources = reader.connected_servers().iter().any(|e| {
            e.protocol.as_ref()
                .map_or(false, |p| p.capable(ServerCapabilityFlag::Resources))
        });
        drop(reader);
        if has_resources {
            registry.register(
                "mcp_read_resource",
                Arc::new(crate::mcp::tool_bridge::McpResourceToolExecutor {
                    manager: Arc::clone(manager),
                    execution_mode,
                    headless: true,
                }),
            );
        }
    }

    // Register expose_prompts_tool if any server has prompts capability
    if config.mcp.expose_prompts_tool {
        let reader = manager.read().await;
        let has_prompts = reader.connected_servers().iter().any(|e| {
            e.protocol.as_ref()
                .map_or(false, |p| p.capable(ServerCapabilityFlag::Prompts))
        });
        drop(reader);
        if has_prompts {
            registry.register(
                "mcp_get_prompt",
                Arc::new(crate::mcp::tool_bridge::McpPromptToolExecutor {
                    manager: Arc::clone(manager),
                    execution_mode,
                    headless: true,
                }),
            );
        }
    }
}
```

#### Task 5B.4: Wire MCP into `src/commands/mod.rs` -- Chat Command

Apply the same MCP manager construction and tool registration as in Task 5B.3,
inside the `run_chat` function in the inline `pub mod chat` block, with these
two differences:

- Pass the `execution_mode` from `config.agent.terminal.default_mode`
- Use `headless: false` everywhere (chat is interactive)
- The MCP manager `Arc<RwLock<McpClientManager>>` must be kept alive for the
  entire duration of `run_chat` so `McpToolExecutor` instances can call back
  to it; do not drop it early

#### Task 5B.5: Testing Requirements

Create `tests/mcp_sampling_test.rs`:

- `test_full_autonomous_mode_skips_user_prompt_and_calls_provider` -- use a
  mock provider; assert `create_message` calls the provider without any stdin
  prompt.
- `test_interactive_mode_with_user_rejection_returns_mcp_elicitation_error` --
  mock stdin returning `"n"`; assert
  `Err(XzatomaError::McpElicitation("user rejected sampling request"))`.

Create `tests/mcp_elicitation_test.rs`:

- `test_form_mode_headless_returns_cancel` -- `headless: true`; assert
  `ElicitationResult { action: ElicitationAction::Cancel, content: None }`.
- `test_form_mode_full_autonomous_returns_cancel` --
  `execution_mode: ExecutionMode::FullAutonomous`, `headless: false`; assert
  `Cancel`.
- `test_url_mode_headless_returns_cancel` -- `headless: true`; assert `Cancel`.

Create `tests/mcp_tool_execution_test.rs` for end-to-end integration:

- `test_end_to_end_tool_call_via_registry` -- spawn the test MCP server from
  Phase 2; build a `McpClientManager`; connect to the server using server ID
  `"test_server"`; call `register_mcp_tools`; call
  `registry.get("test_server__echo").unwrap().execute(json!({"message":
"hello"})).await`; assert result output is `"hello"`.

#### Task 5B.6: Deliverables

| File                                  | Action                                                      |
| ------------------------------------- | ----------------------------------------------------------- |
| `src/mcp/sampling.rs`                 | Created                                                     |
| `src/mcp/elicitation.rs`              | Created                                                     |
| `src/mcp/mod.rs`                      | Updated with `pub mod sampling;` and `pub mod elicitation;` |
| `src/commands/mod.rs`                 | Updated run and chat flows with MCP manager wiring          |
| `tests/mcp_sampling_test.rs`          | Created                                                     |
| `tests/mcp_elicitation_test.rs`       | Created                                                     |
| `tests/mcp_tool_execution_test.rs`    | Created                                                     |
| `docs/explanation/implementations.md` | Updated with Phase 5B entry                                 |

#### Task 5B.7: Success Criteria

- MCP tools are visible and callable in both `xzatoma run` and `xzatoma chat`
  via the normal `ToolRegistry::get` + `execute` dispatch path.
- `should_auto_approve` returns `true` in `FullAutonomous` mode and in the
  headless `run` command; the `execute` method skips the confirmation prompt.
- `should_auto_approve` returns `false` in `Interactive` and
  `RestrictedAutonomous` non-headless contexts; the `execute` method prompts
  the user before calling the MCP server.
- Sampling requests are forwarded to Xzatoma's configured `Provider`.
- Elicitation form mode prompts in interactive contexts; returns `Cancel`
  cleanly in headless and `FullAutonomous` contexts.
- Elicitation URL mode displays the URL; returns `Cancel` in headless.
- End-to-end integration test passes: `execute` flows from `ToolRegistry`
  through `McpToolExecutor::execute` to the test server subprocess and back.
- All Phase 5B tests pass under `cargo test`.

---

### Phase 6: Task Manager, CLI Commands, and Documentation

**Goal:** Implement client-side task polling, expand the `xzatoma mcp`
subcommands to their full set, update all documentation, and add a final entry
to `implementations.md`.

#### Task 6.1: Create `src/mcp/task_manager.rs`

**`pub struct TaskEntry`** -- `Debug, Clone`:

- `pub task_id: String`
- `pub server_id: String`
- `pub tool_name: String`
- `pub status: TaskStatus`
- `pub created_at: chrono::DateTime<chrono::Utc>` -- logged in RFC-3339 format
- `pub poll_interval_ms: u64` -- default `5000`

**`pub struct TaskManager`** fields:

- `tasks: std::collections::HashMap<String, TaskEntry>`

**`impl TaskManager`**:

- `pub fn new() -> Self`
- `pub fn upsert(&mut self, entry: TaskEntry)` -- insert or replace by `task_id`
- `pub fn get(&self, task_id: &str) -> Option<&TaskEntry>`
- `pub fn update_status(&mut self, task_id: &str, status: TaskStatus)` --
  updates the `status` field of the entry if found; no-op if not found

**`pub async fn wait_for_completion(protocol: &InitializedMcpProtocol,
task_id: &str, poll_interval: std::time::Duration,
cancellation: tokio_util::sync::CancellationToken) ->
crate::error::Result<CallToolResponse>`**:

```text
loop {
    tokio::select! {
        _ = cancellation.cancelled() => {
            protocol.tasks_cancel(task_id).await.ok();
            return Err(XzatomaError::McpTask("task was cancelled".into()));
        }
        _ = tokio::time::sleep(poll_interval) => {
            let task = protocol.tasks_get(task_id).await?;
            tracing::debug!(
                target: "xzatoma::mcp::task_manager",
                task_id = %task_id,
                status = ?task.status,
                last_updated = %task.last_updated_at.as_deref().unwrap_or("unknown"),
                "task poll result"
            );
            match task.status {
                TaskStatus::Completed => {
                    return protocol.tasks_result(task_id).await;
                }
                TaskStatus::Failed => {
                    return Err(XzatomaError::McpTask(
                        task.status_message
                            .unwrap_or_else(|| "task failed".into()),
                    ));
                }
                TaskStatus::Cancelled => {
                    return Err(XzatomaError::McpTask(
                        "task was cancelled by server".into(),
                    ));
                }
                TaskStatus::InputRequired => {
                    // Per spec: call tasks_result immediately; the result may
                    // contain nested elicitation or sampling requests handled
                    // by registered protocol handlers. Resume polling after.
                    let _ = protocol.tasks_result(task_id).await.ok();
                }
                TaskStatus::Working => {
                    // Respect server-provided poll_interval for next sleep
                    if let Some(server_interval) = task.poll_interval {
                        tokio::time::sleep(
                            std::time::Duration::from_millis(server_interval)
                        ).await;
                    }
                }
            }
        }
    }
}
```

All `last_updated_at` and `created_at` timestamps MUST be logged in RFC-3339
format (they are `chrono::DateTime<Utc>` values formatted via `Display`).

Add `pub mod task_manager;` to `src/mcp/mod.rs`.

#### Task 6.2: Register Task Notification Handler in `src/mcp/manager.rs`

Add a `task_manager: Arc<std::sync::Mutex<TaskManager>>` field to
`McpClientManager` (initialized as `Arc::new(Mutex::new(TaskManager::new()))`
in `McpClientManager::new`).

In `McpClientManager::connect`, after the protocol is initialized, add:

```text
if protocol.capable(ServerCapabilityFlag::Tasks) {
    let task_mgr = Arc::clone(&self.task_manager);
    protocol.client.on_notification(NOTIF_TASKS_STATUS, move |params| {
        if let (Some(task_id), Some(status)) = (
            params.get("taskId").and_then(|v| v.as_str()),
            params.get("status").and_then(|v| {
                serde_json::from_value::<TaskStatus>(v.clone()).ok()
            }),
        ) {
            if let Ok(mut mgr) = task_mgr.lock() {
                mgr.update_status(task_id, status);
            }
        }
    });
}
```

This is informational only; `tasks_get` polling in `wait_for_completion` is
always authoritative.

#### Task 6.3: Expand CLI Commands in `src/commands/mcp.rs`

Replace the Phase 0 stub `McpCommands` enum with the full set:

**`pub enum McpCommands`** -- `Debug, Clone, clap::Subcommand`:

- `List` -- print a table of configured servers. Columns: `ID`, `Transport`,
  `State`, `Tool Count`. Use `prettytable-rs` (already in `Cargo.toml`).
- `Tools { #[arg(short, long)] server: Option<String> }` -- list tools from
  one or all connected servers. Columns: `Server`, `Registry Name`,
  `Description`, `Task Support`.
- `Resources { server: String }` -- list resources from a specific server.
- `Prompts { server: String }` -- list prompts from a specific server.
- `Connect { server: String }` -- connect a single named server by ID.
- `Disconnect { server: String }` -- disconnect a single named server by ID.
- `Ping { server: String }` -- send `ping` to a server; print round-trip time
  in milliseconds.
- `Auth { server: String }` -- trigger interactive OAuth authorization for an
  HTTP server; save the token to keyring. Print an error if the server uses
  stdio transport.
- `Tasks { server: String }` -- list non-terminal tasks on a server.

**`pub async fn handle_mcp(command: McpCommands, config: Config) ->
crate::error::Result<()>`** -- build `McpClientManager`, connect enabled
servers when `config.mcp.auto_connect` is `true`, then match on `command`:

- `List` -- iterate `connected_servers()`; print table with ID, transport
  type (`"stdio"` or `"http"`), state string, and tool count
- `Tools { server }` -- list tools from specified server or all servers;
  print table with `registry_name` (double-underscore namespaced) and
  task support value
- `Resources { server }` -- call `manager.list_resources(&server)`; print list
- `Prompts { server }` -- call `manager.list_prompts(&server)`; print list
- `Connect { server }` -- look up server config by ID from
  `config.mcp.servers`; call `manager.connect(config)`; print success or error
- `Disconnect { server }` -- call `manager.disconnect(&server)`; print result
- `Ping { server }` -- record `std::time::Instant::now()` before and after
  `protocol.ping()`; print `"Ping {server}: {elapsed_ms}ms"`
- `Auth { server }` -- validate that the server uses HTTP transport; if not,
  return `Err(XzatomaError::Config("auth is only supported for HTTP servers".into()))`;
  otherwise call `auth_manager.get_token()` to trigger the full OAuth flow;
  print `"Authorization successful for {server}"`
- `Tasks { server }` -- retrieve tasks where `status` is `Working` or
  `InputRequired`; print table with `task_id`, `tool_name`, `status`,
  `created_at` (RFC-3339)

#### Task 6.4: Update `src/cli.rs`

Verify that the `Commands::Mcp` variant added in Phase 0 still compiles now
that `McpCommands` has been replaced with the full enum. No structural change
to `src/cli.rs` is needed if Phase 0 was completed correctly.

Add CLI tests to the existing `mod tests` block in `src/cli.rs`:

- `test_cli_parse_mcp_list` -- parse `["mcp", "list"]`; assert
  `Commands::Mcp { command: McpCommands::List }`.
- `test_cli_parse_mcp_tools` -- parse `["mcp", "tools"]`; assert
  `Commands::Mcp { command: McpCommands::Tools { server: None } }`.
- `test_cli_parse_mcp_ping` -- parse `["mcp", "ping", "my-server"]`; assert
  `Commands::Mcp { command: McpCommands::Ping { server: "my-server".into() } }`.

#### Task 6.5: Documentation Updates

**`docs/reference/architecture.md`** -- add a new `## MCP Subsystem` section.
Include:

- Module dependency diagram (text-based):

  ```text
  manager -> protocol -> client -> transport
  manager -> auth
  manager -> tool_bridge -> ToolRegistry
  ```

- Auth flow: "HTTP transport triggers discovery on 401; token stored in keyring
  via `TokenStore`; refreshed automatically before expiry via `AuthManager`"
- Tool namespacing: "`<server_id>__<tool_name>` (double underscore) registered
  in `ToolRegistry` as `Arc<dyn ToolExecutor>`"
- Auto-approval policy table:

  | Context                    | `headless` | `execution_mode`       | `should_auto_approve` |
  | -------------------------- | ---------- | ---------------------- | --------------------- |
  | `xzatoma run`              | `true`     | any                    | `true`                |
  | `xzatoma chat` full-auto   | `false`    | `FullAutonomous`       | `true`                |
  | `xzatoma chat` interactive | `false`    | `Interactive`          | `false`               |
  | `xzatoma chat` restricted  | `false`    | `RestrictedAutonomous` | `false`               |

**`docs/reference/configuration.md`** -- add a `## MCP Configuration` section:

- Full `mcp:` YAML schema with all fields, types, and defaults
- Environment variable table: `XZATOMA_MCP_REQUEST_TIMEOUT` (integer seconds),
  `XZATOMA_MCP_AUTO_CONNECT` (boolean: `true`, `1`, `yes`)
- Note: "YAML files without an `mcp:` key load unchanged; all fields default"
- Per-server field reference for both `stdio` and `http` transport variants
- `oauth:` sub-section reference

**`docs/how-to/commands.md`** -- add an `## xzatoma mcp` section with usage
examples for all nine subcommands: `list`, `tools`, `resources`, `prompts`,
`connect`, `disconnect`, `ping`, `auth`, `tasks`.

**`README.md`** -- add a `## MCP Support` section:

- A minimal `mcp:` YAML block showing one stdio server and one HTTP server
- `xzatoma mcp` command reference (one line per subcommand)
- OAuth authorization walkthrough for HTTP servers
- Note on `FullAutonomous` and headless auto-approval behavior

#### Task 6.6: Update `docs/explanation/implementations.md`

Append an entry for each phase (0 through 6) if not already added during
earlier phases. Follow the existing entry format in `implementations.md`:

```text
## MCP Client Support - Phase 6 - [Date]

**Files Changed:**

- `src/mcp/task_manager.rs` - Client-side task polling and lifecycle
- `src/mcp/manager.rs` - Task notification handler registration
- `src/mcp/mod.rs` - `pub mod task_manager` added
- `src/commands/mcp.rs` - Full MCP CLI command handler
- `src/cli.rs` - New CLI tests for mcp subcommands
- `docs/reference/architecture.md` - MCP subsystem section
- `docs/reference/configuration.md` - MCP configuration reference
- `docs/how-to/commands.md` - xzatoma mcp command usage
- `README.md` - MCP support overview section

**Summary:** Completed the MCP client implementation with client-side task
polling, task status notification handling, the full set of xzatoma mcp CLI
subcommands, and all documentation updates.

**Testing:** Added mcp_task_manager_test and mcp_commands_test; all prior
MCP tests continue to pass.
```

#### Task 6.7: Testing Requirements

Create `tests/mcp_task_manager_test.rs`:

- `test_wait_for_completion_resolves_after_completed_status` -- inject
  `TaskStatus::Working` on first poll, `TaskStatus::Completed` on second;
  assert `wait_for_completion` calls `tasks_result` and returns `Ok`.
- `test_wait_for_completion_returns_error_on_failed_status` -- inject
  `TaskStatus::Failed` with `status_message: Some("out of memory")`; assert
  `Err(XzatomaError::McpTask("out of memory"))`.
- `test_input_required_triggers_tasks_result_call` -- inject
  `TaskStatus::InputRequired` on first poll; assert `tasks_result` is called
  before polling resumes.
- `test_cancellation_token_triggers_tasks_cancel_and_returns_error` -- cancel
  the `CancellationToken` before the first poll fires; assert `tasks_cancel`
  is called; assert `Err(XzatomaError::McpTask("task was cancelled"))`.
- `test_server_provided_poll_interval_is_respected` -- inject a `Task` with
  `poll_interval: Some(50)`; assert the next sleep is at least 50ms.
- `test_task_manager_upsert_and_update_status` -- `upsert` a `TaskEntry`
  with `TaskStatus::Working`; call `update_status` to `TaskStatus::Completed`;
  assert `get` returns the updated status.

Create `tests/mcp_commands_test.rs`:

- `test_handle_mcp_list_prints_server_state_table` -- mock manager with one
  connected server; capture stdout; assert output contains the server ID and
  `"Connected"`.
- `test_handle_mcp_tools_prints_tool_list_with_task_support_column` -- mock
  manager with one tool that has `task_support: Some(TaskSupport::Required)`;
  assert output contains `"Required"`.
- `test_handle_mcp_ping_prints_latency_on_success` -- mock protocol returns
  `Ok(())` for `ping`; assert output contains `"ms"`.
- `test_handle_mcp_auth_returns_error_for_stdio_server` -- server uses stdio
  transport; assert `handle_mcp(Auth { server: "my-server".into() }, config)`
  returns `Err` with a message indicating auth requires HTTP transport.

#### Task 6.8: Deliverables

| File                                  | Action                                              |
| ------------------------------------- | --------------------------------------------------- |
| `src/mcp/task_manager.rs`             | Created                                             |
| `src/mcp/manager.rs`                  | Updated (task manager field + notification handler) |
| `src/mcp/mod.rs`                      | Updated with `pub mod task_manager;`                |
| `src/commands/mcp.rs`                 | Replaced stub with full handler                     |
| `src/cli.rs`                          | New MCP CLI tests added                             |
| `docs/reference/architecture.md`      | Updated with MCP subsystem section                  |
| `docs/reference/configuration.md`     | Updated with MCP configuration reference            |
| `docs/how-to/commands.md`             | Updated with `xzatoma mcp` usage                    |
| `README.md`                           | Updated with MCP support section                    |
| `tests/mcp_task_manager_test.rs`      | Created                                             |
| `tests/mcp_commands_test.rs`          | Created                                             |
| `docs/explanation/implementations.md` | Updated with all phase entries                      |

#### Task 6.9: Success Criteria

- `xzatoma mcp list` shows configured servers with ID, transport type, state,
  and tool count.
- `xzatoma mcp tools` lists all registered MCP tools with double-underscore
  namespaced registry names and task support indication.
- `xzatoma mcp auth <server>` completes the OAuth flow and saves the token
  to keyring; returns an error for stdio servers.
- `xzatoma mcp ping <server>` prints round-trip time in milliseconds.
- Tools with `task_support: Some(TaskSupport::Required)` route through
  `wait_for_completion` and poll correctly until a terminal status.
- `notifications/tasks/status` notifications update the in-memory
  `TaskManager` via the registered notification handler.
- All quality gates pass:

  ```text
  cargo fmt --all
  cargo check --all-targets --all-features
  cargo clippy --all-targets --all-features -- -D warnings
  cargo test
  ```

- Greater than 80% line coverage across all new `src/mcp/` modules.

---

## Validation Matrix (Mandatory)

Run ALL of the following commands before declaring the implementation complete.
Every command must exit with code `0`.

```text
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

---

## Module Structure (Final)

```text
src/
└── mcp/
    ├── mod.rs            -- re-exports and pub mod declarations
    ├── types.rs          -- all MCP 2025-11-25 protocol types + JSON-RPC types
    ├── client.rs         -- JSON-RPC 2.0 client (Tokio channels) + BoxFuture
    ├── protocol.rs       -- typed MCP lifecycle + SamplingHandler/ElicitationHandler traits
    ├── transport.rs      -- Transport trait definition
    ├── transport/
    │   ├── mod.rs        -- pub mod declarations
    │   ├── stdio.rs      -- stdio child-process transport
    │   ├── http.rs       -- Streamable HTTP transport (2025-11-25 spec)
    │   └── fake.rs       -- in-process fake transport (cfg(test))
    ├── auth/
    │   ├── mod.rs        -- pub mod declarations
    │   ├── token_store.rs -- keyring-backed OAuth token persistence
    │   ├── discovery.rs  -- Protected Resource Metadata + AS metadata discovery
    │   ├── pkce.rs       -- PKCE S256 challenge generation and verification
    │   ├── flow.rs       -- full OAuth 2.1 authorization code + refresh + step-up
    │   └── manager.rs    -- per-server auth coordinator
    ├── server.rs         -- McpServerConfig, McpServerTransportConfig, OAuthServerConfig
    ├── config.rs         -- McpConfig
    ├── manager.rs        -- McpClientManager + McpServerEntry + McpServerState
    ├── approval.rs       -- should_auto_approve() (single source of truth)
    ├── tool_bridge.rs    -- McpToolExecutor, McpResourceToolExecutor,
    │                        McpPromptToolExecutor, register_mcp_tools
    ├── sampling.rs       -- XzatomaSamplingHandler
    ├── elicitation.rs    -- XzatomaElicitationHandler
    └── task_manager.rs   -- TaskManager, TaskEntry, wait_for_completion

src/commands/
├── mod.rs                -- inline pub mod chat, run, auth, watch; plus new mcp wiring
└── mcp.rs                -- McpCommands enum and handle_mcp handler

tests/
├── common/
│   └── mod.rs            -- shared test helpers (existing)
├── helpers/
│   └── mcp_test_server/
│       └── main.rs       -- minimal MCP server binary for integration tests
├── mcp_types_test.rs
├── mcp_client_test.rs
├── mcp_http_transport_test.rs
├── mcp_stdio_test.rs
├── mcp_auth_pkce_test.rs
├── mcp_auth_discovery_test.rs
├── mcp_auth_token_store_test.rs
├── mcp_auth_flow_test.rs
├── mcp_manager_test.rs
├── mcp_config_test.rs
├── mcp_tool_bridge_test.rs
├── mcp_sampling_test.rs
├── mcp_elicitation_test.rs
├── mcp_tool_execution_test.rs
├── mcp_task_manager_test.rs
└── mcp_commands_test.rs
```

---

## Key Design Decisions

| Decision                       | Choice                                              | Rationale                                                                                     |
| ------------------------------ | --------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| Runtime                        | Tokio (existing)                                    | Xzatoma already uses Tokio; no new async runtime introduced                                   |
| Protocol version               | `2025-11-25` primary, `2025-03-26` fallback         | Latest spec with backward compatibility                                                       |
| Tool namespacing separator     | `__` (double underscore)                            | Single `_` is ambiguous; both server IDs and tool names may contain `_`                       |
| Transport abstraction          | Own `Transport` trait (Tokio channels)              | Tokio-native; testable via `FakeTransport`; no external framework required                    |
| Tool adapter interface         | `ToolExecutor` trait (existing)                     | Reuses existing interface; MCP tools are indistinguishable from native tools at the call site |
| `tool_definition()` format     | `{ "name", "description", "parameters" }` JSON      | Matches Xzatoma's existing tool convention used by all other `ToolExecutor` implementations   |
| `ToolRegistry::register` API   | `register(name, Arc<dyn ToolExecutor>)`             | Existing signature; no error on duplicate (silent overwrite); log warning before registering  |
| Confirmation model             | Inline prompt in `execute()` (not a trait method)   | `ToolExecutor` has no `requires_confirmation` method; confirmation is internal to `execute`   |
| `ExecutionMode` type name      | `crate::config::ExecutionMode`                      | Actual type in `src/config.rs`; NOT `TerminalMode`                                            |
| `ToolResult` error constructor | `ToolResult::error(message)`                        | Actual method; there is no `failure_with_output` method in Xzatoma's `ToolResult`             |
| PKCE                           | `S256` only; hard fail if server lacks support      | Required by OAuth 2.1 and MCP spec; `plain` is insecure                                       |
| OAuth token storage            | `keyring` crate (already present at `2.3`)          | Consistent with existing auth credential storage                                              |
| AS discovery                   | Tries all 5 well-known orderings per spec           | Required for interoperability with OAuth 2.0 and OIDC servers                                 |
| Auto-approve policy            | Single `should_auto_approve()` in `approval.rs`     | One authoritative source; no inline policy checks scattered across the codebase               |
| Auto-approve conditions        | `FullAutonomous` mode OR headless `run`             | User intent is explicit in both cases                                                         |
| Elicitation in headless        | Returns `Cancel`                                    | Cannot block headless execution; server handles cancel gracefully                             |
| Elicitation in FullAutonomous  | Form returns `Cancel`; URL returns `Cancel`         | No terminal interaction available in autonomous mode                                          |
| Sampling                       | Forwarded to Xzatoma's configured `Provider`        | Reuses existing provider abstraction; no new model plumbing                                   |
| Task completion                | Poll-based; notification as informational hint only | Spec requires poll; notifications are optional and not authoritative                          |
| HTTP mandatory header          | `MCP-Protocol-Version: 2025-11-25` on all POSTs     | Required by spec; must be present on every request                                            |
| Config backward compatibility  | All MCP fields `#[serde(default)]`                  | Existing `config/config.yaml` files load unchanged                                            |
| Test server binary             | `[[bin]]` target in `Cargo.toml`                    | Required for integration tests; builds with the workspace; no separate crate needed           |
| Test file naming               | `tests/mcp_<feature>_test.rs` flat layout           | Matches existing test file structure in Xzatoma (no `tests/unit/` or `tests/integration/`)    |
| Config file location           | `config/config.yaml`                                | Actual location; no `config.example.yaml` exists                                              |
| Timestamps                     | RFC-3339 format via `chrono`                        | Per PLAN.md general rules                                                                     |
| Unique IDs                     | `ulid` crate (already present at `1.1`)             | Per PLAN.md preference; lexicographically sortable; collision-resistant                       |

---

## Dependencies to Add

All dependencies are added via `cargo add` per AGENTS.md rules. Never manually
edit version numbers in `Cargo.toml`.

Run these commands from the repository root at the start of Phase 1:

```text
cargo add tokio-util --features codec
cargo add sha2
cargo add rand
```

Also add `libc` for the stdio transport's Unix SIGTERM handling at the start
of Phase 2:

```text
cargo add libc --target cfg(unix)
```

The following are already present in `Cargo.toml` and require no action:

| Crate            | Present Version | Already Used For                           |
| ---------------- | --------------- | ------------------------------------------ |
| `base64`         | `0.22`          | Provider streaming; MCP blob encoding      |
| `ulid`           | `1.1`           | Conversation IDs; MCP session IDs          |
| `reqwest`        | `0.11`          | HTTP client for providers                  |
| `keyring`        | `2.3`           | Auth credential storage                    |
| `chrono`         | `0.4`           | Timestamps throughout                      |
| `url`            | `2.5`           | URL parsing                                |
| `wiremock`       | `0.5` (dev)     | HTTP mock tests                            |
| `futures`        | `0.3`           | Async stream utilities                     |
| `regex`          | `1.10`          | Pattern matching (used in validation)      |
| `rustyline`      | `13.0`          | Interactive readline (used in elicitation) |
| `prettytable-rs` | `0.10`          | Table output in CLI commands               |
