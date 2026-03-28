/// ACP subcommand handler.
///
/// This module implements the `acp` CLI subcommand for starting the ACP HTTP
/// server in a dedicated mode that is separate from normal CLI task execution.
///
/// Phase 2 focuses on the ACP discovery surface, so this command currently
/// starts the HTTP server that exposes discovery endpoints such as `/ping`,
/// `/agents`, and `/agents/{name}` according to the configured ACP route
/// strategy.
///
/// # Examples
///
/// ```
/// use xzatoma::cli::AcpCommand;
/// use xzatoma::commands::acp::handle_acp;
/// use xzatoma::Config;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let config = Config::default();
///     handle_acp(
///         AcpCommand::Serve {
///             host: None,
///             port: None,
///             base_path: None,
///             root_compatible: false,
///         },
///         config,
///     )
///     .await
/// }
/// ```
use crate::acp::server::run_server;
use crate::cli::AcpCommand;
use crate::config::{AcpCompatibilityMode, Config};
use crate::error::Result;

/// Handles ACP subcommands.
///
/// # Arguments
///
/// * `command` - The ACP subcommand to execute
/// * `config` - Application configuration
///
/// # Errors
///
/// Returns an error if ACP server configuration is invalid or if the ACP HTTP
/// server fails to start or serve requests.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::cli::AcpCommand;
/// use xzatoma::commands::acp::handle_acp;
/// use xzatoma::Config;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let config = Config::default();
///     handle_acp(
///         AcpCommand::Serve {
///             host: Some("127.0.0.1".to_string()),
///             port: Some(8765),
///             base_path: Some("/api/v1/acp".to_string()),
///             root_compatible: false,
///         },
///         config,
///     )
///     .await
/// }
/// ```
pub async fn handle_acp(command: AcpCommand, mut config: Config) -> Result<()> {
    match command {
        AcpCommand::Serve {
            host,
            port,
            base_path,
            root_compatible,
        } => {
            apply_serve_overrides(&mut config, host, port, base_path, root_compatible);
            config.acp.enabled = true;
            config.validate()?;
            run_server(config).await
        }
    }
}

/// Applies CLI overrides for the ACP `serve` subcommand.
///
/// # Arguments
///
/// * `config` - Mutable application configuration
/// * `host` - Optional ACP bind host override
/// * `port` - Optional ACP bind port override
/// * `base_path` - Optional ACP versioned base path override
/// * `root_compatible` - Whether to enable ACP root-compatible routing
///
/// # Examples
///
/// ```
/// use xzatoma::commands::acp::apply_serve_overrides;
/// use xzatoma::Config;
///
/// let mut config = Config::default();
/// apply_serve_overrides(
///     &mut config,
///     Some("0.0.0.0".to_string()),
///     Some(9000),
///     Some("/acp".to_string()),
///     true,
/// );
///
/// assert_eq!(config.acp.host, "0.0.0.0");
/// assert_eq!(config.acp.port, 9000);
/// assert!(matches!(
///     config.acp.compatibility_mode,
///     xzatoma::config::AcpCompatibilityMode::RootCompatible
/// ));
/// ```
pub fn apply_serve_overrides(
    config: &mut Config,
    host: Option<String>,
    port: Option<u16>,
    base_path: Option<String>,
    root_compatible: bool,
) {
    if let Some(host) = host {
        config.acp.host = host;
    }

    if let Some(port) = port {
        config.acp.port = port;
    }

    if let Some(base_path) = base_path {
        config.acp.base_path = base_path;
    }

    if root_compatible {
        config.acp.compatibility_mode = AcpCompatibilityMode::RootCompatible;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_serve_overrides_updates_host_port_and_base_path() {
        let mut config = Config::default();

        apply_serve_overrides(
            &mut config,
            Some("0.0.0.0".to_string()),
            Some(9000),
            Some("/acp".to_string()),
            false,
        );

        assert_eq!(config.acp.host, "0.0.0.0");
        assert_eq!(config.acp.port, 9000);
        assert_eq!(config.acp.base_path, "/acp");
        assert_eq!(
            config.acp.compatibility_mode,
            AcpCompatibilityMode::Versioned
        );
    }

    #[test]
    fn test_apply_serve_overrides_enables_root_compatible_mode() {
        let mut config = Config::default();

        apply_serve_overrides(&mut config, None, None, None, true);

        assert_eq!(
            config.acp.compatibility_mode,
            AcpCompatibilityMode::RootCompatible
        );
    }

    #[test]
    fn test_apply_serve_overrides_keeps_existing_values_when_none() {
        let mut config = Config::default();
        let original_host = config.acp.host.clone();
        let original_port = config.acp.port;
        let original_base_path = config.acp.base_path.clone();

        apply_serve_overrides(&mut config, None, None, None, false);

        assert_eq!(config.acp.host, original_host);
        assert_eq!(config.acp.port, original_port);
        assert_eq!(config.acp.base_path, original_base_path);
    }
}
