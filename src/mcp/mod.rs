//! MCP (Model Context Protocol) client support for Xzatoma
//!
//! This module provides MCP client functionality, enabling the agent to connect
//! to external MCP servers and consume their tools, resources, prompts, sampling,
//! elicitation, and task capabilities.
//!
//! The implementation targets protocol revision **2025-11-25** with **2025-03-26**
//! as a backwards-compatibility fallback.
//!
//! # Module Layout
//!
//! - `approval`     -- MCP tool auto-approval policy (single authoritative source)
//! - `auth`         -- OAuth 2.1 / OIDC authorization for HTTP transport
//! - `client`       -- Transport-agnostic async JSON-RPC 2.0 client
//! - `config`       -- MCP client configuration structures
//! - `elicitation`  -- Elicitation handler for structured user input collection
//! - `manager`      -- Client lifecycle and server manager
//! - `protocol`     -- Typed MCP lifecycle wrapper over `JsonRpcClient`
//! - `sampling`     -- Sampling handler forwarding LLM inference to the Provider
//! - `server`       -- Per-server connection descriptors
//! - `tool_bridge`  -- ToolExecutor adapters for MCP tools, resources, and prompts
//! - `transport`    -- `Transport` trait and concrete implementations (stdio, HTTP,
//!   fake)
//! - `types`        -- All MCP 2025-11-25 protocol types and JSON-RPC primitives

pub mod approval;
pub mod auth;
pub mod client;
pub mod config;
pub mod elicitation;
pub mod manager;
pub mod protocol;
pub mod sampling;
pub mod server;
pub mod tool_bridge;
pub mod transport;
pub mod types;
