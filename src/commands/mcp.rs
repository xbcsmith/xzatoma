//! MCP subcommand handler
//!
//! This module implements the `mcp` CLI subcommand, providing management
//! operations for MCP (Model Context Protocol) server connections.
//! Full implementation is delivered in Phase 6.

use crate::config::Config;
use crate::error::Result;

/// MCP subcommand variants
///
/// Enumerates all operations available under the `xzatoma mcp` command.
/// Additional variants are added in Phase 6 once the full MCP lifecycle
/// is implemented.
#[derive(Debug, Clone, clap::Subcommand)]
pub enum McpCommands {
    /// List configured MCP servers
    List,
}

/// Handle MCP subcommands
///
/// Dispatches the given `McpCommands` variant to the appropriate handler.
/// Currently prints a stub message; full logic is wired in Phase 6.
///
/// # Arguments
///
/// * `command` - The MCP subcommand to execute
/// * `_config` - Application configuration (used in Phase 6)
///
/// # Errors
///
/// Returns an error if the subcommand handler fails.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::commands::mcp::{handle_mcp, McpCommands};
/// use xzatoma::Config;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let config = Config::default();
///     handle_mcp(McpCommands::List, config).await
/// }
/// ```
pub async fn handle_mcp(command: McpCommands, _config: Config) -> Result<()> {
    match command {
        McpCommands::List => {
            println!("MCP support not yet implemented.");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_handle_mcp_list_prints_stub_message() {
        let config = Config::default();
        let result = handle_mcp(McpCommands::List, config).await;
        assert!(result.is_ok());
    }
}
