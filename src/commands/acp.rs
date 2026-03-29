/// ACP subcommand handler.
///
/// This module implements the `acp` CLI subcommands for:
///
/// - starting the ACP HTTP server
/// - printing effective ACP configuration
/// - listing active or recent ACP runs from persistent storage
/// - validating ACP configuration and optional manifest documents
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
///         AcpCommand::Config,
///         config,
///     )
///     .await
/// }
/// ```
use std::fs;
use std::path::Path;

use crate::acp::manifest::AcpAgentManifest;
use crate::acp::server::run_server;
use crate::cli::AcpCommand;
use crate::config::{AcpCompatibilityMode, Config};
use crate::error::{Result, XzatomaError};
use crate::storage::SqliteStorage;

/// Handles ACP subcommands.
///
/// # Arguments
///
/// * `command` - The ACP subcommand to execute
/// * `config` - Application configuration
///
/// # Errors
///
/// Returns an error if ACP configuration is invalid, manifest validation fails,
/// storage access fails, or the ACP HTTP server fails to start.
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
///         AcpCommand::Validate { manifest: None },
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
        AcpCommand::Config => {
            config.validate()?;
            print_effective_config(&config)?;
            Ok(())
        }
        AcpCommand::Runs { session_id, limit } => {
            config.validate()?;
            list_recent_runs(&session_id, limit)?;
            Ok(())
        }
        AcpCommand::Validate { manifest } => {
            config.validate()?;
            validate_acp_manifest_and_config(&config, manifest.as_deref())?;
            Ok(())
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

/// Prints the effective ACP configuration as pretty JSON.
///
/// # Arguments
///
/// * `config` - Effective application configuration
///
/// # Errors
///
/// Returns an error if serialization fails.
///
/// # Examples
///
/// ```
/// use xzatoma::commands::acp::print_effective_config;
/// use xzatoma::Config;
///
/// print_effective_config(&Config::default())?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn print_effective_config(config: &Config) -> Result<()> {
    let rendered = serde_json::to_string_pretty(&config.acp)?;
    println!("{rendered}");
    Ok(())
}

/// Lists active or recent ACP runs from persistent storage.
///
/// # Arguments
///
/// * `session_id` - Optional session filter
/// * `limit` - Maximum number of rows to print
///
/// # Errors
///
/// Returns an error if storage access fails or if `limit` is zero.
///
/// # Examples
///
/// ```
/// use xzatoma::commands::acp::list_recent_runs;
///
/// let _ = list_recent_runs(&None, 10);
/// ```
pub fn list_recent_runs(session_id: &Option<String>, limit: usize) -> Result<()> {
    if limit == 0 {
        return Err(XzatomaError::Config(
            "ACP run listing limit must be greater than 0".to_string(),
        ));
    }

    let storage = SqliteStorage::new()?;
    let runs = match session_id {
        Some(session) => storage.list_acp_runs_for_session(session)?,
        None => load_all_runs(&storage)?,
    };

    println!("run_id\tsession_id\tstate\tcreated_at\tupdated_at");

    for run in runs.into_iter().rev().take(limit) {
        println!(
            "{}\t{}\t{}\t{}\t{}",
            run.run_id, run.session_id, run.state, run.created_at, run.updated_at
        );
    }

    Ok(())
}

/// Validates ACP configuration and an optional manifest file.
///
/// # Arguments
///
/// * `config` - Effective application configuration
/// * `manifest_path` - Optional manifest path
///
/// # Errors
///
/// Returns an error if configuration validation fails, the manifest file cannot
/// be read, the manifest cannot be parsed, or manifest validation fails.
///
/// # Examples
///
/// ```
/// use xzatoma::commands::acp::validate_acp_manifest_and_config;
/// use xzatoma::Config;
///
/// validate_acp_manifest_and_config(&Config::default(), None)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn validate_acp_manifest_and_config(
    config: &Config,
    manifest_path: Option<&Path>,
) -> Result<()> {
    config.validate()?;

    if let Some(path) = manifest_path {
        let manifest = load_manifest(path)?;
        manifest.validate()?;
        println!("ACP manifest validation succeeded: {}", path.display());
    } else {
        println!("ACP configuration validation succeeded");
    }

    println!(
        "ACP compatibility mode: {}",
        match config.acp.compatibility_mode {
            AcpCompatibilityMode::Versioned => "versioned",
            AcpCompatibilityMode::RootCompatible => "root_compatible",
        }
    );

    Ok(())
}

/// Loads an ACP manifest from JSON or YAML.
///
/// # Arguments
///
/// * `path` - Manifest file path
///
/// # Returns
///
/// Returns the parsed ACP manifest.
///
/// # Errors
///
/// Returns an error if the file cannot be read, parsed, or is an unsupported
/// format.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use xzatoma::commands::acp::load_manifest;
///
/// let _ = load_manifest(Path::new("manifest.json"));
/// ```
pub fn load_manifest(path: &Path) -> Result<AcpAgentManifest> {
    let contents = fs::read_to_string(path).map_err(|error| {
        XzatomaError::Config(format!(
            "Failed to read ACP manifest '{}': {}",
            path.display(),
            error
        ))
    })?;

    match path.extension().and_then(|value| value.to_str()) {
        Some("json") => serde_json::from_str(&contents).map_err(Into::into),
        Some("yaml") => serde_yaml::from_str(&contents).map_err(Into::into),
        Some(other) => Err(XzatomaError::Config(format!(
            "Unsupported ACP manifest extension '{}'; expected .json or .yaml",
            other
        ))),
        None => Err(XzatomaError::Config(format!(
            "ACP manifest '{}' must have a .json or .yaml extension",
            path.display()
        ))),
    }
}

/// Loads all persisted ACP runs from storage.
///
/// # Arguments
///
/// * `storage` - Initialized SQLite storage
///
/// # Returns
///
/// Returns stored ACP runs ordered by creation time.
///
/// # Errors
///
/// Returns an error if storage access fails.
///
/// # Examples
///
/// ```
/// use xzatoma::commands::acp::load_all_runs;
/// use xzatoma::storage::SqliteStorage;
///
/// let storage = SqliteStorage::new()?;
/// let _runs = load_all_runs(&storage)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn load_all_runs(storage: &SqliteStorage) -> Result<Vec<crate::storage::PublicStoredAcpRun>> {
    let connection = rusqlite::Connection::open(storage.database_path())
        .map_err(|error| XzatomaError::Storage(error.to_string()))?;

    let mut statement = connection
        .prepare(
            "SELECT run_id, session_id, conversation_id, mode, state, created_at, updated_at, completed_at, failure_reason, cancellation_reason, await_kind, await_detail, input_json, output_json
             FROM acp_runs
             ORDER BY created_at ASC",
        )
        .map_err(|error| XzatomaError::Storage(error.to_string()))?;

    let rows = statement
        .query_map([], |row| {
            let created_at_text: String = row.get(5)?;
            let updated_at_text: String = row.get(6)?;
            let completed_at_text: Option<String> = row.get(7)?;

            let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_text)
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        5,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?
                .with_timezone(&chrono::Utc);

            let updated_at = chrono::DateTime::parse_from_rfc3339(&updated_at_text)
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        6,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?
                .with_timezone(&chrono::Utc);

            let completed_at = match completed_at_text {
                Some(value) => Some(
                    chrono::DateTime::parse_from_rfc3339(&value)
                        .map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                7,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?
                        .with_timezone(&chrono::Utc),
                ),
                None => None,
            };

            Ok(crate::storage::PublicStoredAcpRun {
                run_id: row.get(0)?,
                session_id: row.get(1)?,
                conversation_id: row.get(2)?,
                mode: row.get(3)?,
                state: row.get(4)?,
                created_at,
                updated_at,
                completed_at,
                failure_reason: row.get(8)?,
                cancellation_reason: row.get(9)?,
                await_kind: row.get(10)?,
                await_detail: row.get(11)?,
                input_json: row.get(12)?,
                output_json: row.get(13)?,
                metadata: std::collections::BTreeMap::new(),
            })
        })
        .map_err(|error| XzatomaError::Storage(error.to_string()))?;

    let mut runs = Vec::new();
    for row in rows {
        runs.push(row.map_err(|error| XzatomaError::Storage(error.to_string()))?);
    }

    Ok(runs)
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

    #[test]
    fn test_load_manifest_rejects_unsupported_extension() {
        let path = Path::new("manifest.txt");
        let result = load_manifest(path);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_acp_manifest_and_config_without_manifest_succeeds() {
        let config = Config::default();
        let result = validate_acp_manifest_and_config(&config, None);
        assert!(result.is_ok());
    }
}
