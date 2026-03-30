//! MCP subcommand handler
//!
//! This module implements the `mcp` CLI subcommand, providing management
//! operations for MCP (Model Context Protocol) server connections including
//! listing configured servers and their connection status.

use crate::config::Config;
use crate::error::Result;
use crate::mcp::manager::{build_mcp_manager_from_config, McpServerState};
use crate::mcp::server::McpServerTransportConfig;

/// MCP subcommand variants
///
/// Enumerates all operations available under the `xzatoma mcp` command.
#[derive(Debug, Clone, clap::Subcommand)]
pub enum McpCommands {
    /// List configured MCP servers
    List,
}

/// Handle MCP subcommands
///
/// Dispatches the given [`McpCommands`] variant to the appropriate handler.
///
/// # Arguments
///
/// * `command` - The MCP subcommand to execute
/// * `config` - Application configuration containing MCP server definitions
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
pub async fn handle_mcp(command: McpCommands, config: Config) -> Result<()> {
    match command {
        McpCommands::List => handle_list(config).await,
    }
}

/// List all configured MCP servers and their connection status.
///
/// When no servers are configured, prints a short informational message and
/// returns. When `auto_connect` is enabled, establishes connections to all
/// enabled servers and reports their live state (Connected, Disconnected, or
/// Failed) along with the number of advertised tools. When `auto_connect` is
/// disabled, lists the configured servers without attempting to connect.
///
/// # Arguments
///
/// * `config` - Application configuration containing MCP server definitions
///
/// # Errors
///
/// Returns an error if building the MCP manager fails for reasons other than
/// individual server connection failures (which are reported inline).
async fn handle_list(config: Config) -> Result<()> {
    if config.mcp.servers.is_empty() {
        println!("No MCP servers configured.");
        return Ok(());
    }

    println!("Configured MCP servers ({}):\n", config.mcp.servers.len());

    if config.mcp.auto_connect {
        // Build the manager, which connects to all enabled servers.
        let manager_opt = build_mcp_manager_from_config(&config).await?;

        match manager_opt {
            Some(manager_arc) => {
                let manager = manager_arc.read().await;
                let connected = manager.connected_servers();

                // Build a lookup set of connected server IDs for quick access.
                let connected_map: std::collections::HashMap<
                    &str,
                    &crate::mcp::manager::McpServerEntry,
                > = connected
                    .iter()
                    .map(|entry| (entry.config.id.as_str(), *entry))
                    .collect();

                for server_cfg in &config.mcp.servers {
                    let transport_label = transport_type_label(&server_cfg.transport);
                    let enabled_label = if server_cfg.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    };

                    if let Some(entry) = connected_map.get(server_cfg.id.as_str()) {
                        let state_label = format_server_state(&entry.state);
                        let tool_count = entry.tools.len();
                        println!(
                            "  - {} ({}, {}, {}, {} tools)",
                            server_cfg.id, transport_label, enabled_label, state_label, tool_count
                        );
                    } else {
                        // Server exists in config but was not connected (e.g. disabled or failed
                        // before being registered in the manager).
                        let state_label = if server_cfg.enabled {
                            "Disconnected".to_string()
                        } else {
                            "Skipped (disabled)".to_string()
                        };
                        println!(
                            "  - {} ({}, {}, {}, 0 tools)",
                            server_cfg.id, transport_label, enabled_label, state_label
                        );
                    }
                }
            }
            None => {
                // build_mcp_manager_from_config returned None even though we checked
                // auto_connect -- this can happen if all servers are disabled.
                print_servers_without_status(&config);
            }
        }
    } else {
        println!("  auto_connect is disabled; showing configuration only.\n");
        print_servers_without_status(&config);
    }

    Ok(())
}

/// Print configured servers without attempting any connections.
///
/// Used when `auto_connect` is disabled or when no manager could be built.
fn print_servers_without_status(config: &Config) {
    for server_cfg in &config.mcp.servers {
        let transport_label = transport_type_label(&server_cfg.transport);
        let enabled_label = if server_cfg.enabled {
            "enabled"
        } else {
            "disabled"
        };
        println!(
            "  - {} ({}, {})",
            server_cfg.id, transport_label, enabled_label
        );
    }
}

/// Return a human-readable label for the transport type.
fn transport_type_label(transport: &McpServerTransportConfig) -> &'static str {
    match transport {
        McpServerTransportConfig::Stdio { .. } => "stdio",
        McpServerTransportConfig::Http { .. } => "http",
    }
}

/// Format an [`McpServerState`] as a human-readable string.
fn format_server_state(state: &McpServerState) -> String {
    if *state == McpServerState::Connected {
        "Connected".to_string()
    } else if *state == McpServerState::Disconnected {
        "Disconnected".to_string()
    } else if *state == McpServerState::Connecting {
        "Connecting".to_string()
    } else {
        // McpServerState::Failed(msg)
        match state {
            McpServerState::Failed(msg) => format!("Failed ({})", msg),
            // Covered above, but the compiler needs this arm.
            _ => "Unknown".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_handle_mcp_list_with_default_config_returns_ok() {
        // Default config has no MCP servers, so this should succeed
        // and print "No MCP servers configured."
        let config = Config::default();
        let result = handle_mcp(McpCommands::List, config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_mcp_list_with_empty_servers_returns_ok() {
        let mut config = Config::default();
        config.mcp.servers = vec![];
        config.mcp.auto_connect = false;
        let result = handle_mcp(McpCommands::List, config).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_transport_type_label_stdio() {
        let transport = McpServerTransportConfig::Stdio {
            executable: "npx".to_string(),
            args: vec![],
            env: std::collections::HashMap::new(),
            working_dir: None,
        };
        assert_eq!(transport_type_label(&transport), "stdio");
    }

    #[test]
    fn test_transport_type_label_http() {
        let transport = McpServerTransportConfig::Http {
            endpoint: url::Url::parse("https://example.com/mcp").expect("test URL must be valid"),
            headers: std::collections::HashMap::new(),
            timeout_seconds: None,
            oauth: None,
        };
        assert_eq!(transport_type_label(&transport), "http");
    }

    #[test]
    fn test_format_server_state_connected() {
        assert_eq!(format_server_state(&McpServerState::Connected), "Connected");
    }

    #[test]
    fn test_format_server_state_disconnected() {
        assert_eq!(
            format_server_state(&McpServerState::Disconnected),
            "Disconnected"
        );
    }

    #[test]
    fn test_format_server_state_connecting() {
        assert_eq!(
            format_server_state(&McpServerState::Connecting),
            "Connecting"
        );
    }

    #[test]
    fn test_format_server_state_failed() {
        let state = McpServerState::Failed("connection refused".to_string());
        assert_eq!(format_server_state(&state), "Failed (connection refused)");
    }

    #[tokio::test]
    async fn test_handle_list_with_servers_and_auto_connect_disabled() {
        let mut config = Config::default();
        config.mcp.auto_connect = false;
        config
            .mcp
            .servers
            .push(crate::mcp::server::McpServerConfig {
                id: "test-server".to_string(),
                transport: McpServerTransportConfig::Stdio {
                    executable: "echo".to_string(),
                    args: vec![],
                    env: std::collections::HashMap::new(),
                    working_dir: None,
                },
                enabled: true,
                timeout_seconds: 30,
                tools_enabled: true,
                resources_enabled: false,
                prompts_enabled: false,
                sampling_enabled: false,
                elicitation_enabled: true,
            });
        // Should not attempt any connections, just list config.
        let result = handle_mcp(McpCommands::List, config).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_servers_without_status_shows_disabled() {
        let mut config = Config::default();
        config
            .mcp
            .servers
            .push(crate::mcp::server::McpServerConfig {
                id: "disabled-srv".to_string(),
                transport: McpServerTransportConfig::Stdio {
                    executable: "echo".to_string(),
                    args: vec![],
                    env: std::collections::HashMap::new(),
                    working_dir: None,
                },
                enabled: false,
                timeout_seconds: 30,
                tools_enabled: true,
                resources_enabled: false,
                prompts_enabled: false,
                sampling_enabled: false,
                elicitation_enabled: true,
            });
        // This should not panic; it only prints to stdout.
        print_servers_without_status(&config);
    }
}
