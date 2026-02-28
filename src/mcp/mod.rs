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
//! - `types`     -- All MCP 2025-11-25 protocol types and JSON-RPC primitives
//! - `client`    -- Transport-agnostic async JSON-RPC 2.0 client
//! - `protocol`  -- Typed MCP lifecycle wrapper over `JsonRpcClient`
//! - `transport` -- `Transport` trait and concrete implementations (stdio, HTTP,
//!   fake)
//! - `config`    -- MCP client configuration structures
//! - `server`    -- Per-server connection descriptors (Phase 4)
#![allow(dead_code)]
#![allow(unused_imports)]

pub mod client;
pub mod config;
pub mod protocol;
pub mod server;
pub mod transport;
pub mod types;

pub use types::*;
