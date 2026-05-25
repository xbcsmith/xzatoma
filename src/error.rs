/// Error types for XZatoma.
///
/// This module defines the shared crate-wide error enum and the primary
/// `Result<T>` alias used across the project. It also centralizes conversions
/// from module-local error types into [`XzatomaError`] so callers can use `?`
/// without losing typed error information.
///
/// # Examples
///
/// ```
/// use xzatoma::error::{Result, XzatomaError};
///
/// fn example() -> Result<()> {
///     Err(XzatomaError::Tool("example failure".to_string()))
/// }
///
/// assert!(matches!(example(), Err(XzatomaError::Tool(_))));
/// ```
use thiserror::Error;

/// Main error type for XZatoma operations.
///
/// This enum encompasses all possible errors that can occur during
/// agent execution, configuration loading, provider interactions,
/// tool execution, and security validations.
#[derive(Error, Debug)]
pub enum XzatomaError {
    /// Configuration-related errors
    #[error("Configuration error: {0}")]
    Config(String),

    /// Provider-related errors (API calls, authentication, etc.)
    #[error("Provider error: {0}")]
    Provider(String),

    /// Provider HTTP request failed before a response was received.
    #[error("Provider HTTP request failed: provider={provider}, endpoint={endpoint}: {source}")]
    ProviderHttpRequest {
        /// Provider name such as `openai`, `ollama`, or `copilot`.
        provider: String,
        /// Endpoint category such as `models` or `chat/completions`.
        endpoint: String,
        /// Underlying HTTP client error.
        #[source]
        source: anyhow::Error,
    },

    /// Provider returned a non-success HTTP status.
    #[error("Provider HTTP status error: provider={provider}, endpoint={endpoint}, status={status}, response={response}")]
    ProviderHttpStatus {
        /// Provider name such as `openai`, `ollama`, or `copilot`.
        provider: String,
        /// Endpoint category such as `models` or `chat/completions`.
        endpoint: String,
        /// HTTP response status code.
        status: reqwest::StatusCode,
        /// Redacted and bounded response body or context.
        response: String,
    },

    /// Provider response body could not be decoded.
    #[error("Provider response parse failed: provider={provider}, endpoint={endpoint}: {source}")]
    ProviderResponseParse {
        /// Provider name such as `openai`, `ollama`, or `copilot`.
        provider: String,
        /// Endpoint category such as `models` or `chat/completions`.
        endpoint: String,
        /// Underlying decode error.
        #[source]
        source: anyhow::Error,
    },

    /// Tool execution errors
    #[error("Tool execution error: {0}")]
    Tool(String),

    /// Watcher-related errors
    #[error("Watcher error: {0}")]
    Watcher(String),

    /// Watcher failure with operation context and source chain.
    #[error("Watcher error during {operation}: {source}")]
    WatcherFailure {
        /// Watcher operation being performed.
        operation: String,
        /// Underlying watcher error.
        #[source]
        source: anyhow::Error,
    },

    /// Command execution errors
    #[error("Command error: {0}")]
    Command(String),

    /// Fetch-related errors (HTTP fetch, SSRF, timeouts, rate limits)
    #[error("Fetch error: {0}")]
    Fetch(String),

    /// Mention parsing errors (invalid mention syntax)
    #[error("Mention parsing error: {0}")]
    MentionParse(String),

    /// File loading errors (read errors, size, binary)
    #[error("File load error: {0}")]
    FileLoad(String),

    /// Search/Grep related errors
    #[error("Search error: {0}")]
    Search(String),

    /// Rate limit exceeded for an operation
    #[error("Rate limit exceeded: limit={limit}, {message}")]
    RateLimitExceeded {
        /// The configured limit that was exceeded
        limit: u32,
        /// Additional message explaining the failure
        message: String,
    },

    /// Agent exceeded maximum iteration limit
    #[error("Agent exceeded maximum iterations: limit={limit}, {message}")]
    MaxIterationsExceeded {
        /// The configured iteration limit
        limit: usize,
        /// Additional context about the failure
        message: String,
    },

    /// Command is considered dangerous and requires confirmation
    #[error("Dangerous command detected: {0}")]
    DangerousCommand(String),

    /// Command requires user confirmation but cannot proceed
    #[error("Command requires confirmation: {0}")]
    CommandRequiresConfirmation(String),

    /// Path validation failed (outside working directory)
    #[error("Path validation failed: {0}")]
    PathOutsideWorkingDirectory(String),

    /// Streaming not supported by provider
    #[error("Streaming is not supported by this provider")]
    StreamingNotSupported,

    /// Missing credentials for provider
    #[error("Missing credentials for provider: {0}")]
    MissingCredentials(String),

    /// Authentication errors (e.g., 401 Unauthorized)
    #[error("Authentication error: {0}")]
    Authentication(String),

    /// IO errors
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization errors
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// YAML parsing errors
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// HTTP request errors
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// Regex parsing and compilation errors
    #[error("Regex error: {0}")]
    Regex(#[from] regex::Error),

    /// Tracing filter parsing errors
    #[error("Tracing filter error: {0}")]
    TracingFilter(#[from] tracing_subscriber::filter::ParseError),

    /// Keyring/credential storage errors
    #[error("Keyring error: {0}")]
    Keyring(#[from] keyring::Error),

    /// Conversation storage errors (database operations)
    #[error("Storage error: {0}")]
    Storage(String),

    /// Storage database open failure.
    #[error("Storage database open failed at {path}: {source}")]
    StorageDatabaseOpen {
        /// Database path that failed to open.
        path: String,
        /// Underlying open error.
        #[source]
        source: anyhow::Error,
    },

    /// Storage migration or schema initialization failure.
    #[error("Storage migration failed during {operation}: {source}")]
    StorageMigration {
        /// Migration or schema operation being performed.
        operation: String,
        /// Underlying migration error.
        #[source]
        source: anyhow::Error,
    },

    /// Storage query or statement execution failure.
    #[error("Storage query failed during {operation}: {source}")]
    StorageQuery {
        /// Query operation being performed.
        operation: String,
        /// Underlying query error.
        #[source]
        source: anyhow::Error,
    },

    /// Storage row decoding failure.
    #[error("Storage row decode failed during {operation}: {source}")]
    StorageRowDecode {
        /// Row decoding operation being performed.
        operation: String,
        /// Underlying row decoding error.
        #[source]
        source: anyhow::Error,
    },

    /// Storage serialization or deserialization failure.
    #[error("Storage serialization failed during {operation}: {source}")]
    StorageSerialization {
        /// Serialization operation being performed.
        operation: String,
        /// Underlying serialization error.
        #[source]
        source: anyhow::Error,
    },

    /// Storage persistence path failure.
    #[error("Storage persistence path failed at {path}: {source}")]
    StoragePersistencePath {
        /// Filesystem path or directory purpose that failed.
        path: String,
        /// Underlying path error.
        #[source]
        source: anyhow::Error,
    },

    /// Resource quota exceeded
    #[error("Resource quota exceeded: {0}")]
    QuotaExceeded(String),

    /// Internal runtime error
    #[error("Internal error: {0}")]
    Internal(String),

    /// Runtime operation exceeded its configured timeout.
    #[error("Runtime timeout during {operation}: elapsed {elapsed_seconds}s exceeded limit {timeout_seconds}s")]
    RuntimeTimeout {
        /// Runtime operation that timed out.
        operation: String,
        /// Configured timeout in seconds.
        timeout_seconds: u64,
        /// Observed elapsed time in seconds.
        elapsed_seconds: u64,
    },

    /// Model does not support the requested endpoint
    #[error("Model {0} does not support endpoint {1}")]
    UnsupportedEndpoint(String, String),

    /// Failed to parse Server-Sent Events data
    #[error("Failed to parse SSE event: {0}")]
    SseParseError(String),

    /// Stream was interrupted before completion
    #[error("Stream interrupted: {0}")]
    StreamInterrupted(String),

    /// Response format does not match expected structure
    #[error("Invalid response format: {0}")]
    InvalidResponseFormat(String),

    /// Both completions and responses endpoints failed
    #[error("Endpoint fallback failed - both completions and responses returned errors")]
    EndpointFallbackFailed,

    /// Message format conversion failed
    #[error("Message conversion failed: {0}")]
    MessageConversionError(String),

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

    /// ACP protocol error (consolidated from multiple variants)
    #[error("ACP error: {0}")]
    Acp(#[from] crate::acp::error::AcpError),

    /// Execution was cancelled via a cancellation token.
    #[error("Execution cancelled")]
    Cancelled,
}

/// Result type alias for XZatoma operations.
///
/// This is the primary result type used throughout the codebase,
/// using `XzatomaError` as the error type for precise error handling.
///
/// # Examples
///
/// ```
/// use xzatoma::error::{Result, XzatomaError};
///
/// fn example() -> Result<()> {
///     Ok(())
/// }
///
/// assert!(example().is_ok());
/// assert!(matches!(
///     Err::<(), _>(XzatomaError::Config("bad config".to_string())),
///     Err(XzatomaError::Config(_))
/// ));
/// ```
pub type Result<T> = std::result::Result<T, XzatomaError>;

/// Parses tool arguments into a strongly typed input structure.
///
/// This helper centralizes tool argument deserialization so all tool entry
/// points report invalid JSON parameters consistently through
/// [`XzatomaError::Tool`].
///
/// # Type Parameters
///
/// * `T` - The deserialized tool input type
///
/// # Arguments
///
/// * `args` - Raw JSON tool arguments
///
/// # Returns
///
/// Returns the deserialized tool input.
///
/// # Errors
///
/// Returns [`XzatomaError::Tool`] when the provided JSON does not match the
/// expected input schema.
///
/// # Examples
///
/// ```
/// use serde::Deserialize;
/// use serde_json::json;
/// use xzatoma::error::parse_tool_args;
///
/// #[derive(Debug, Deserialize, PartialEq)]
/// struct ExampleArgs {
///     path: String,
/// }
///
/// let parsed: ExampleArgs = parse_tool_args(json!({ "path": "src/main.rs" })).unwrap();
/// assert_eq!(
///     parsed,
///     ExampleArgs {
///         path: "src/main.rs".to_string(),
///     }
/// );
/// ```
pub fn parse_tool_args<T>(args: serde_json::Value) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(args)
        .map_err(|error| XzatomaError::Tool(format!("Invalid tool parameters: {}", error)))
}

// From implementations for module-local error types

/// Converts `ChatModeParseError` to `XzatomaError::Config`
impl From<crate::chat_mode::ChatModeParseError> for XzatomaError {
    fn from(err: crate::chat_mode::ChatModeParseError) -> Self {
        XzatomaError::Config(err.to_string())
    }
}

/// Converts `SafetyModeParseError` to `XzatomaError::Config`
impl From<crate::chat_mode::SafetyModeParseError> for XzatomaError {
    fn from(err: crate::chat_mode::SafetyModeParseError) -> Self {
        XzatomaError::Config(err.to_string())
    }
}

/// Converts `PromptInputError` to `XzatomaError::Provider`
impl From<crate::providers::PromptInputError> for XzatomaError {
    fn from(err: crate::providers::PromptInputError) -> Self {
        XzatomaError::Provider(err.to_string())
    }
}

/// Converts `ImagePromptError` to `XzatomaError::Provider`
impl From<crate::providers::ImagePromptError> for XzatomaError {
    fn from(err: crate::providers::ImagePromptError) -> Self {
        XzatomaError::Provider(err.to_string())
    }
}

/// Converts `AcpValidationError` to `XzatomaError::Acp`
impl From<crate::acp::error::AcpValidationError> for XzatomaError {
    fn from(err: crate::acp::error::AcpValidationError) -> Self {
        crate::acp::error::AcpError::from(err).into()
    }
}

/// Converts `FileMetadataError` to `XzatomaError::Tool`
impl From<crate::tools::file_metadata::FileMetadataError> for XzatomaError {
    fn from(err: crate::tools::file_metadata::FileMetadataError) -> Self {
        XzatomaError::Tool(err.to_string())
    }
}

/// Converts `FileUtilsError` to `XzatomaError::Tool`
impl From<crate::tools::file_utils::FileUtilsError> for XzatomaError {
    fn from(err: crate::tools::file_utils::FileUtilsError) -> Self {
        XzatomaError::Tool(err.to_string())
    }
}

/// Converts `CommandError` to `XzatomaError::Command`
impl From<crate::commands::special_commands::CommandError> for XzatomaError {
    fn from(err: crate::commands::special_commands::CommandError) -> Self {
        XzatomaError::Command(err.to_string())
    }
}

/// Converts `ReadlineError` to `XzatomaError::Command`
impl From<rustyline::error::ReadlineError> for XzatomaError {
    fn from(err: rustyline::error::ReadlineError) -> Self {
        XzatomaError::Command(format!("readline error: {}", err))
    }
}

/// Converts `GenericWatcherError` to `XzatomaError::Watcher`
impl From<crate::watcher::generic::watcher::GenericWatcherError> for XzatomaError {
    fn from(err: crate::watcher::generic::watcher::GenericWatcherError) -> Self {
        XzatomaError::Watcher(err.to_string())
    }
}

/// Converts `WatcherError` to `XzatomaError::WatcherFailure`
impl From<crate::watcher::xzepr::watcher::WatcherError> for XzatomaError {
    fn from(err: crate::watcher::xzepr::watcher::WatcherError) -> Self {
        XzatomaError::WatcherFailure {
            operation: err.operation().to_string(),
            source: err.into(),
        }
    }
}

/// Converts `PlanExtractionError` to `XzatomaError::WatcherFailure`
impl From<crate::watcher::xzepr::plan_extractor::PlanExtractionError> for XzatomaError {
    fn from(err: crate::watcher::xzepr::plan_extractor::PlanExtractionError) -> Self {
        XzatomaError::WatcherFailure {
            operation: "plan extraction".to_string(),
            source: err.into(),
        }
    }
}

/// Converts `ConsumerError` to `XzatomaError::Watcher`
impl From<crate::watcher::xzepr::consumer::kafka::ConsumerError> for XzatomaError {
    fn from(err: crate::watcher::xzepr::consumer::kafka::ConsumerError) -> Self {
        XzatomaError::Watcher(err.to_string())
    }
}

/// Converts `ClientError` to `XzatomaError::Provider`
impl From<crate::watcher::xzepr::consumer::client::ClientError> for XzatomaError {
    fn from(err: crate::watcher::xzepr::consumer::client::ClientError) -> Self {
        XzatomaError::Provider(err.to_string())
    }
}

/// Converts watcher `ConfigError` to `XzatomaError::Config`
impl From<crate::watcher::xzepr::consumer::config::ConfigError> for XzatomaError {
    fn from(err: crate::watcher::xzepr::consumer::config::ConfigError) -> Self {
        XzatomaError::Config(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tool_args_with_valid_input() {
        #[derive(Debug, serde::Deserialize, PartialEq)]
        struct ExampleArgs {
            path: String,
            recursive: bool,
        }

        let parsed: ExampleArgs = parse_tool_args(serde_json::json!({
            "path": "src/main.rs",
            "recursive": true
        }))
        .unwrap();

        assert_eq!(
            parsed,
            ExampleArgs {
                path: "src/main.rs".to_string(),
                recursive: true,
            }
        );
    }

    #[test]
    fn test_parse_tool_args_with_invalid_input_returns_tool_error() {
        #[derive(Debug, serde::Deserialize)]
        struct ExampleArgs {
            _path: String,
        }

        let error = parse_tool_args::<ExampleArgs>(serde_json::json!({
            "recursive": true
        }))
        .unwrap_err();

        assert!(matches!(error, XzatomaError::Tool(_)));
        assert!(error.to_string().contains("Invalid tool parameters"));
    }

    #[test]
    fn test_from_acp_validation_error_converts_to_acp_variant() {
        let error = XzatomaError::from(crate::acp::error::AcpValidationError::new(
            "run.id",
            "run id cannot be empty",
        ));

        assert!(matches!(
            error,
            XzatomaError::Acp(crate::acp::error::AcpError::Validation(_))
        ));
    }

    #[test]
    fn test_from_chat_mode_parse_error_converts_to_config() {
        let error =
            XzatomaError::from(crate::chat_mode::ChatMode::parse_str("review").unwrap_err());
        assert!(matches!(error, XzatomaError::Config(_)));
        assert!(error.to_string().contains("unknown chat mode 'review'"));
    }

    #[test]
    fn test_from_prompt_input_error_converts_to_provider() {
        let error = XzatomaError::from(crate::providers::PromptInputError::Empty);
        assert!(matches!(error, XzatomaError::Provider(_)));
        assert!(error.to_string().contains("prompt input must contain"));
    }

    #[test]
    fn test_runtime_timeout_display() {
        let error = XzatomaError::RuntimeTimeout {
            operation: "agent execution".to_string(),
            timeout_seconds: 1,
            elapsed_seconds: 2,
        };
        assert!(error.to_string().contains("Runtime timeout"));
        assert!(error.to_string().contains("agent execution"));
    }

    #[test]
    fn test_provider_http_status_display_includes_endpoint() {
        let error = XzatomaError::ProviderHttpStatus {
            provider: "openai".to_string(),
            endpoint: "models".to_string(),
            status: reqwest::StatusCode::BAD_REQUEST,
            response: "bad request".to_string(),
        };
        let message = error.to_string();
        assert!(message.contains("provider=openai"));
        assert!(message.contains("endpoint=models"));
        assert!(message.contains("400 Bad Request"));
    }

    #[test]
    fn test_config_error_display() {
        let error = XzatomaError::Config("invalid format".to_string());
        assert_eq!(error.to_string(), "Configuration error: invalid format");
    }

    #[test]
    fn test_provider_error_display() {
        let error = XzatomaError::Provider("API timeout".to_string());
        assert_eq!(error.to_string(), "Provider error: API timeout");
    }

    #[test]
    fn test_tool_error_display() {
        let error = XzatomaError::Tool("file not found".to_string());
        assert_eq!(error.to_string(), "Tool execution error: file not found");
    }

    #[test]
    fn test_watcher_error_display() {
        let error = XzatomaError::Watcher("consumer failed".to_string());
        assert_eq!(error.to_string(), "Watcher error: consumer failed");
    }

    #[test]
    fn test_command_error_display() {
        let error = XzatomaError::Command("unknown command".to_string());
        assert_eq!(error.to_string(), "Command error: unknown command");
    }

    #[test]
    fn test_max_iterations_error_display() {
        let error = XzatomaError::MaxIterationsExceeded {
            limit: 50,
            message: "stuck in loop".to_string(),
        };
        assert!(error.to_string().contains("limit=50"));
        assert!(error.to_string().contains("stuck in loop"));
    }

    #[test]
    fn test_dangerous_command_error_display() {
        let error = XzatomaError::DangerousCommand("rm -rf /".to_string());
        assert_eq!(error.to_string(), "Dangerous command detected: rm -rf /");
    }

    #[test]
    fn test_command_confirmation_error_display() {
        let error = XzatomaError::CommandRequiresConfirmation("sudo apt install".to_string());
        assert_eq!(
            error.to_string(),
            "Command requires confirmation: sudo apt install"
        );
    }

    #[test]
    fn test_path_validation_error_display() {
        let error = XzatomaError::PathOutsideWorkingDirectory("/etc/passwd".to_string());
        assert_eq!(error.to_string(), "Path validation failed: /etc/passwd");
    }

    #[test]
    fn test_streaming_not_supported_error() {
        let error = XzatomaError::StreamingNotSupported;
        assert_eq!(
            error.to_string(),
            "Streaming is not supported by this provider"
        );
    }

    #[test]
    fn test_missing_credentials_error_display() {
        let error = XzatomaError::MissingCredentials("github_copilot".to_string());
        assert_eq!(
            error.to_string(),
            "Missing credentials for provider: github_copilot"
        );
    }

    #[test]
    fn test_authentication_error_display() {
        let error = XzatomaError::Authentication("token expired".to_string());
        assert_eq!(error.to_string(), "Authentication error: token expired");
    }

    #[test]
    fn test_io_error_conversion() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let error: XzatomaError = io_error.into();
        assert!(matches!(error, XzatomaError::Io(_)));
    }

    #[test]
    fn test_json_error_conversion() {
        let json_str = "{invalid json}";
        let json_error = serde_json::from_str::<serde_json::Value>(json_str).unwrap_err();
        let error: XzatomaError = json_error.into();
        assert!(matches!(error, XzatomaError::Serialization(_)));
    }

    #[test]
    fn test_yaml_error_conversion() {
        let yaml_str = "invalid: : yaml";
        let yaml_error = serde_yaml::from_str::<serde_yaml::Value>(yaml_str).unwrap_err();
        let error: XzatomaError = yaml_error.into();
        assert!(matches!(error, XzatomaError::Yaml(_)));
    }

    #[test]
    fn test_fetch_error_display() {
        let error = XzatomaError::Fetch("timeout".to_string());
        assert_eq!(error.to_string(), "Fetch error: timeout");
    }

    #[test]
    fn test_mention_parse_error_display() {
        let error = XzatomaError::MentionParse("invalid syntax".to_string());
        assert_eq!(error.to_string(), "Mention parsing error: invalid syntax");
    }

    #[test]
    fn test_file_load_error_display() {
        let error = XzatomaError::FileLoad("not found".to_string());
        assert_eq!(error.to_string(), "File load error: not found");
    }

    #[test]
    fn test_search_error_display() {
        let error = XzatomaError::Search("grep failed".to_string());
        assert_eq!(error.to_string(), "Search error: grep failed");
    }

    #[test]
    fn test_rate_limit_exceeded_display() {
        let error = XzatomaError::RateLimitExceeded {
            limit: 10,
            message: "Too many requests".to_string(),
        };
        let s = error.to_string();
        assert!(s.contains("limit=10"));
        assert!(s.contains("Too many requests"));
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<XzatomaError>();
    }

    #[test]
    fn test_storage_error_display() {
        let error = XzatomaError::Storage("database connection failed".to_string());
        assert_eq!(
            error.to_string(),
            "Storage error: database connection failed"
        );
    }

    #[test]
    fn test_internal_error_display() {
        let error = XzatomaError::Internal("poisoned lock".to_string());
        assert_eq!(error.to_string(), "Internal error: poisoned lock");
    }

    #[test]
    fn test_unsupported_endpoint_error() {
        let err =
            XzatomaError::UnsupportedEndpoint("gpt-3.5-turbo".to_string(), "responses".to_string());
        let msg = err.to_string();
        assert!(msg.contains("gpt-3.5-turbo"));
        assert!(msg.contains("responses"));
        assert!(msg.contains("does not support"));
    }

    #[test]
    fn test_sse_parse_error() {
        let err = XzatomaError::SseParseError("Invalid JSON in event".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Failed to parse SSE event"));
        assert!(msg.contains("Invalid JSON"));
    }

    #[test]
    fn test_stream_interrupted_error() {
        let err = XzatomaError::StreamInterrupted("Connection reset".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Stream interrupted"));
        assert!(msg.contains("Connection reset"));
    }

    #[test]
    fn test_invalid_response_format_error() {
        let err = XzatomaError::InvalidResponseFormat("Missing required field".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Invalid response format"));
        assert!(msg.contains("Missing required field"));
    }

    #[test]
    fn test_endpoint_fallback_failed_error() {
        let err = XzatomaError::EndpointFallbackFailed;
        let msg = err.to_string();
        assert!(msg.contains("Endpoint fallback failed"));
        assert!(msg.contains("both completions and responses"));
    }

    #[test]
    fn test_message_conversion_error() {
        let err = XzatomaError::MessageConversionError("Invalid role".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Message conversion failed"));
        assert!(msg.contains("Invalid role"));
    }

    #[test]
    fn test_mcp_error_variants() {
        let e = XzatomaError::Mcp("protocol violation".to_string());
        assert!(e.to_string().contains("MCP error"));

        let e = XzatomaError::McpTransport("connection reset".to_string());
        assert!(e.to_string().contains("MCP transport error"));

        let e = XzatomaError::McpServerNotFound("my_server".to_string());
        assert!(e.to_string().contains("my_server"));

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

        let e = XzatomaError::McpAuth("token expired".to_string());
        assert!(e.to_string().contains("MCP auth error"));

        let e = XzatomaError::McpElicitation("user cancelled".to_string());
        assert!(e.to_string().contains("MCP elicitation error"));

        let e = XzatomaError::McpTask("task failed".to_string());
        assert!(e.to_string().contains("MCP task error"));
    }

    #[test]
    fn test_error_propagation() {
        fn failing_function() -> Result<()> {
            Err(XzatomaError::SseParseError("Test error".to_string()))
        }

        let result = failing_function();
        assert!(result.is_err());
    }

    #[test]
    fn test_acp_error_conversion() {
        use crate::acp::error::AcpError;

        let acp_err = AcpError::validation("invalid payload");
        let error: XzatomaError = acp_err.into();
        assert!(matches!(error, XzatomaError::Acp(_)));
        assert!(error.to_string().contains("ACP error"));
    }

    #[test]
    fn test_cancelled_error_display() {
        let error = XzatomaError::Cancelled;
        assert_eq!(error.to_string(), "Execution cancelled");
    }
}
