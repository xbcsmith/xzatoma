//! MCP client configuration types
//!
//! This module defines configuration structures for MCP client connections.
//! Populated fully in Phase 4.

/// MCP client configuration
///
/// Holds all configuration for MCP server connections. Defaults to an empty
/// configuration so that existing YAML files that omit the `mcp:` key
/// continue to deserialize without error.
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::config::McpConfig;
///
/// let cfg = McpConfig::default();
/// ```
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct McpConfig {}
