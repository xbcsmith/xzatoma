//! Structured logging setup for watcher
//!
//! Provides JSON-formatted and human-readable logging with optional file output.
//! Integrates with the tracing ecosystem for structured event logging.

use crate::config::WatcherLoggingConfig;
use anyhow::Result;
use std::fs::OpenOptions;
use std::sync::Arc;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize watcher logging based on configuration.
///
/// Sets up structured logging with support for both JSON and human-readable formats,
/// with optional file output in addition to STDOUT.
///
/// # Arguments
///
/// * `config` - Logging configuration
///
/// # Returns
///
/// Returns success or error if logging initialization fails
///
/// # Examples
///
/// ```no_run
/// use xzatoma::config::WatcherLoggingConfig;
/// use xzatoma::watcher::logging::init_watcher_logging;
///
/// let config = WatcherLoggingConfig {
///     level: "info".to_string(),
///     json_format: true,
///     file_path: None,
///     include_payload: false,
/// };
///
/// let result = init_watcher_logging(&config);
/// assert!(result.is_ok());
/// ```
pub fn init_watcher_logging(config: &WatcherLoggingConfig) -> Result<()> {
    let env_filter =
        EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new(&config.level))?;

    let registry = tracing_subscriber::registry().with(env_filter);

    if config.json_format {
        // JSON formatting
        let stdout_layer = fmt::layer()
            .json()
            .with_current_span(true)
            .with_span_list(true);

        if let Some(file_path) = &config.file_path {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(file_path)?;

            let file_layer = fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(true)
                .with_writer(Arc::new(file));

            registry.with(stdout_layer).with(file_layer).init();
        } else {
            registry.with(stdout_layer).init();
        }
    } else {
        // Human-readable formatting
        let stdout_layer = fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            .with_level(true);

        if let Some(file_path) = &config.file_path {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(file_path)?;

            let file_layer = fmt::layer()
                .with_target(true)
                .with_thread_ids(true)
                .with_level(true)
                .with_writer(Arc::new(file));

            registry.with(stdout_layer).with(file_layer).init();
        } else {
            registry.with(stdout_layer).init();
        }
    }

    Ok(())
}

/// Create structured log fields for an event.
///
/// # Examples
///
/// ```ignore
/// let event = /* CloudEventMessage */;
/// let span = event_fields!(event);
/// ```
#[macro_export]
macro_rules! event_fields {
    ($event:expr) => {
        tracing::info_span!(
            "event",
            event_id = %$event.id,
            event_type = %$event.event_type,
            source = %$event.source,
            platform_id = %$event.platform_id,
            package = %$event.package,
            success = %$event.success
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_logging_config_default() {
        let config = WatcherLoggingConfig::default();
        assert_eq!(config.level, "info");
        assert!(config.json_format);
        assert_eq!(config.file_path, None);
        assert!(!config.include_payload);
    }

    #[test]
    fn test_logging_config_json_format() {
        let config = WatcherLoggingConfig {
            level: "info".to_string(),
            json_format: true,
            file_path: None,
            include_payload: true,
        };

        assert!(config.json_format);
        assert!(config.include_payload);
        assert_eq!(config.level, "info");
    }

    #[test]
    fn test_logging_config_text_format() {
        let config = WatcherLoggingConfig {
            level: "warn".to_string(),
            json_format: false,
            file_path: None,
            include_payload: false,
        };

        assert!(!config.json_format);
        assert_eq!(config.level, "warn");
    }

    #[test]
    fn test_logging_config_with_file_path() {
        let config = WatcherLoggingConfig {
            level: "debug".to_string(),
            json_format: true,
            file_path: Some(PathBuf::from("/tmp/test.log")),
            include_payload: false,
        };

        assert_eq!(config.file_path, Some(PathBuf::from("/tmp/test.log")));
    }

    #[test]
    fn test_logging_config_trace_level() {
        let config = WatcherLoggingConfig {
            level: "trace".to_string(),
            json_format: true,
            file_path: None,
            include_payload: true,
        };

        assert_eq!(config.level, "trace");
        assert!(config.include_payload);
    }

    #[test]
    fn test_logging_config_error_level() {
        let config = WatcherLoggingConfig {
            level: "error".to_string(),
            json_format: false,
            file_path: None,
            include_payload: false,
        };

        assert_eq!(config.level, "error");
        assert!(!config.json_format);
    }
}
