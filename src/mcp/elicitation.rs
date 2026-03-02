//! MCP elicitation handler -- collects structured user input on behalf of a
//! connected MCP server.
//!
//! When an MCP server sends an `elicitation/create` request it is asking the
//! client (Xzatoma) to collect input from the user. This module implements the
//! [`ElicitationHandler`][crate::mcp::protocol::ElicitationHandler] trait with
//! two modes:
//!
//! - [`ElicitationMode::Form`] -- prompts the user for each field declared in
//!   `requested_schema`. In headless or `FullAutonomous` contexts the request
//!   is cancelled immediately without user interaction.
//! - [`ElicitationMode::Url`] -- displays the URL to the user and attempts to
//!   open it in the default browser. In headless contexts the request is
//!   cancelled immediately.
//!
//! # Cancellation Policy
//!
//! Both modes return [`ElicitationResult`] with
//! [`ElicitationAction::Cancel`] and `content: None` when the context is
//! non-interactive (headless or `FullAutonomous` for form mode). This keeps
//! the agent moving without blocking on user input that can never arrive.
//!
//! # Approval Policy
//!
//! This module does NOT use [`crate::mcp::approval::should_auto_approve`] --
//! elicitation is inherently user-facing and has its own context rules that
//! differ from tool-call approval.

use std::collections::HashMap;
use std::io::{BufRead, Write};

use crate::config::ExecutionMode;
use crate::error::Result;
use crate::mcp::client::BoxFuture;
use crate::mcp::protocol::ElicitationHandler;
use crate::mcp::types::{
    ElicitationAction, ElicitationCreateParams, ElicitationMode, ElicitationResult,
};

// ---------------------------------------------------------------------------
// XzatomaElicitationHandler
// ---------------------------------------------------------------------------

/// MCP elicitation handler for Xzatoma.
///
/// Handles `elicitation/create` requests from connected MCP servers. Behaviour
/// varies by mode:
///
/// - **Form mode** in an interactive, non-`FullAutonomous` context: prints the
///   server's message, iterates over fields declared in `requested_schema`, and
///   reads one line per field from stdin using `rustyline`. The user may type
///   `"decline"` at any field prompt to decline the elicitation entirely.
/// - **Form mode** when `headless == true` or
///   `execution_mode == FullAutonomous`: logs a warning and returns
///   `Cancel` immediately.
/// - **URL mode** when `headless == false`: prints the URL to stderr and
///   attempts to open it with the platform's default browser command. Returns
///   `Cancel` after displaying (the client cannot wait for an async browser
///   callback in this synchronous handler).
/// - **URL mode** when `headless == true`: logs a warning and returns `Cancel`
///   immediately.
///
/// # Examples
///
/// ```
/// use xzatoma::config::ExecutionMode;
/// use xzatoma::mcp::elicitation::XzatomaElicitationHandler;
/// use xzatoma::mcp::protocol::ElicitationHandler;
/// use xzatoma::mcp::types::{
///     ElicitationAction, ElicitationCreateParams, ElicitationMode,
/// };
///
/// # #[tokio::main]
/// # async fn main() {
/// let handler = XzatomaElicitationHandler {
///     execution_mode: ExecutionMode::FullAutonomous,
///     headless: false,
/// };
///
/// let params = ElicitationCreateParams {
///     mode: Some(ElicitationMode::Form),
///     message: Some("Please provide your name".to_string()),
///     requested_schema: None,
///     url: None,
///     elicitation_id: None,
/// };
///
/// // FullAutonomous => Cancel without prompting.
/// let result = handler.create_elicitation(params).await.unwrap();
/// assert_eq!(result.action, ElicitationAction::Cancel);
/// # }
/// ```
#[derive(Debug)]
pub struct XzatomaElicitationHandler {
    /// Agent execution mode; `FullAutonomous` suppresses form elicitation.
    pub execution_mode: ExecutionMode,
    /// Whether the agent is running headless (non-interactive).
    pub headless: bool,
}

impl ElicitationHandler for XzatomaElicitationHandler {
    /// Handle an `elicitation/create` request from a connected MCP server.
    ///
    /// Dispatches to form or URL handling based on `params.mode`.
    ///
    /// # Arguments
    ///
    /// * `params` - The elicitation parameters sent by the MCP server.
    ///
    /// # Returns
    ///
    /// Returns [`ElicitationResult`] with one of:
    /// - `Accept` + collected field values (form mode, interactive context).
    /// - `Decline` if the user typed `"decline"` at any prompt.
    /// - `Cancel` in all headless, `FullAutonomous`, or URL-mode contexts.
    ///
    /// # Errors
    ///
    /// Currently infallible for cancellation paths. Returns an error only if
    /// stdin I/O fails in form mode.
    fn create_elicitation<'a>(
        &'a self,
        params: ElicitationCreateParams,
    ) -> BoxFuture<'a, Result<ElicitationResult>> {
        Box::pin(async move {
            let mode = params.mode.clone().unwrap_or(ElicitationMode::Form);

            match mode {
                ElicitationMode::Form => self.handle_form(params),
                ElicitationMode::Url => self.handle_url(params),
            }
        })
    }
}

impl XzatomaElicitationHandler {
    /// Handle a Form-mode elicitation request.
    ///
    /// In headless or `FullAutonomous` contexts, logs a warning and returns
    /// [`ElicitationAction::Cancel`] immediately. Otherwise, interactively
    /// prompts the user for each field declared in `requested_schema`.
    ///
    /// Typing `"decline"` at any field prompt returns
    /// [`ElicitationAction::Decline`] with no content.
    ///
    /// # Arguments
    ///
    /// * `params` - The elicitation parameters, including `message` and
    ///   `requested_schema`.
    fn handle_form(&self, params: ElicitationCreateParams) -> Result<ElicitationResult> {
        // Non-interactive contexts: cancel immediately.
        if self.headless || self.execution_mode == ExecutionMode::FullAutonomous {
            tracing::warn!(
                "MCP elicitation request received in non-interactive context; cancelling"
            );
            return Ok(ElicitationResult {
                action: ElicitationAction::Cancel,
                content: None,
            });
        }

        // Print the server's message.
        if !params.message.is_empty() {
            eprintln!("\nMCP server elicitation: {}", params.message);
        } else {
            eprintln!("\nMCP server is requesting structured input.");
        }
        eprintln!("(Type 'decline' at any prompt to decline the request.)\n");

        // Collect field names from the schema's "properties" object.
        // If no schema is provided, present a single free-form "value" field.
        let fields: Vec<String> = extract_field_names(params.requested_schema.as_ref());

        let stdin = std::io::stdin();
        let mut collected: HashMap<String, serde_json::Value> = HashMap::new();

        for field in &fields {
            eprint!("  {}: ", field);
            let _ = std::io::stderr().flush();

            let mut line = String::new();
            stdin.lock().read_line(&mut line).map_err(|e| {
                crate::error::XzatomaError::Tool(format!(
                    "elicitation stdin read error for field '{}': {}",
                    field, e
                ))
            })?;

            let trimmed = line.trim().to_string();

            if trimmed.to_lowercase() == "decline" {
                return Ok(ElicitationResult {
                    action: ElicitationAction::Decline,
                    content: None,
                });
            }

            collected.insert(field.clone(), serde_json::Value::String(trimmed));
        }

        Ok(ElicitationResult {
            action: ElicitationAction::Accept,
            content: Some(serde_json::Value::Object(
                collected.into_iter().collect::<serde_json::Map<_, _>>(),
            )),
        })
    }

    /// Handle a URL-mode elicitation request.
    ///
    /// In headless contexts, logs a warning and returns
    /// [`ElicitationAction::Cancel`] immediately. Otherwise, prints the URL to
    /// stderr and attempts to open it in the platform default browser. Returns
    /// `Cancel` after displaying since the handler cannot await an async
    /// browser callback.
    ///
    /// # Arguments
    ///
    /// * `params` - The elicitation parameters, including the `url` to open.
    fn handle_url(&self, params: ElicitationCreateParams) -> Result<ElicitationResult> {
        if self.headless {
            tracing::warn!("MCP URL elicitation received in headless context; cancelling");
            return Ok(ElicitationResult {
                action: ElicitationAction::Cancel,
                content: None,
            });
        }

        let url = params.url.as_deref().unwrap_or("(no URL provided)");
        eprintln!("MCP server requests authorization at: {}", url);

        // Attempt to open the URL in the default browser.
        // We try common platform commands: `open` (macOS), `xdg-open` (Linux).
        // Failure to launch is non-fatal -- we still return Cancel because we
        // cannot await the OAuth redirect callback synchronously.
        let opened = try_open_browser(url);
        if opened {
            tracing::info!(url = %url, "Opened URL in default browser for MCP elicitation");
        } else {
            tracing::warn!(
                url = %url,
                "Failed to open browser automatically; user must visit the URL manually"
            );
        }

        // Return Cancel: the synchronous handler cannot await the OAuth
        // callback. Phase 6 will wire up the notification-based flow.
        Ok(ElicitationResult {
            action: ElicitationAction::Cancel,
            content: None,
        })
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Extract field names from a JSON Schema `"properties"` object.
///
/// Given a schema like `{ "type": "object", "properties": { "name": {...},
/// "email": {...} } }`, returns `["email", "name"]` (sorted for determinism).
///
/// Returns `["value"]` when the schema is `None`, `null`, or has no
/// `"properties"` key.
fn extract_field_names(schema: Option<&serde_json::Value>) -> Vec<String> {
    let properties = schema
        .and_then(|s| s.get("properties"))
        .and_then(|p| p.as_object());

    match properties {
        Some(props) if !props.is_empty() => {
            let mut names: Vec<String> = props.keys().cloned().collect();
            // Sort for deterministic prompt ordering.
            names.sort();
            names
        }
        _ => vec!["value".to_string()],
    }
}

/// Attempt to open a URL in the platform's default browser.
///
/// Tries `open` (macOS) then `xdg-open` (Linux). Returns `true` if a command
/// was successfully spawned (not necessarily that the browser opened), `false`
/// if both attempts fail.
fn try_open_browser(url: &str) -> bool {
    // macOS
    if std::process::Command::new("open")
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
    {
        return true;
    }

    // Linux
    std::process::Command::new("xdg-open")
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::types::{ElicitationAction, ElicitationCreateParams, ElicitationMode};

    fn make_handler(execution_mode: ExecutionMode, headless: bool) -> XzatomaElicitationHandler {
        XzatomaElicitationHandler {
            execution_mode,
            headless,
        }
    }

    fn form_params(message: &str, schema: Option<serde_json::Value>) -> ElicitationCreateParams {
        ElicitationCreateParams {
            mode: Some(ElicitationMode::Form),
            message: message.to_string(),
            requested_schema: schema,
            url: None,
            elicitation_id: None,
        }
    }

    fn url_params(url: Option<&str>) -> ElicitationCreateParams {
        ElicitationCreateParams {
            mode: Some(ElicitationMode::Url),
            message: String::new(),
            requested_schema: None,
            url: url.map(|s| s.to_string()),
            elicitation_id: None,
        }
    }

    // -----------------------------------------------------------------------
    // Form mode -- headless/FullAutonomous cancellation
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_form_mode_headless_returns_cancel() {
        let handler = make_handler(ExecutionMode::Interactive, true);
        let params = form_params("Please fill in the form", None);

        let result = handler.create_elicitation(params).await.unwrap();

        assert_eq!(
            result.action,
            ElicitationAction::Cancel,
            "headless=true must return Cancel for form mode"
        );
        assert!(result.content.is_none());
    }

    #[tokio::test]
    async fn test_form_mode_full_autonomous_returns_cancel() {
        let handler = make_handler(ExecutionMode::FullAutonomous, false);
        let params = form_params("Please fill in the form", None);

        let result = handler.create_elicitation(params).await.unwrap();

        assert_eq!(
            result.action,
            ElicitationAction::Cancel,
            "FullAutonomous, headless=false must return Cancel for form mode"
        );
        assert!(result.content.is_none());
    }

    #[tokio::test]
    async fn test_form_mode_headless_and_full_autonomous_returns_cancel() {
        let handler = make_handler(ExecutionMode::FullAutonomous, true);
        let params = form_params("", None);

        let result = handler.create_elicitation(params).await.unwrap();

        assert_eq!(result.action, ElicitationAction::Cancel);
    }

    #[tokio::test]
    async fn test_form_mode_restricted_autonomous_headless_returns_cancel() {
        let handler = make_handler(ExecutionMode::RestrictedAutonomous, true);
        let params = form_params("Provide values", None);

        let result = handler.create_elicitation(params).await.unwrap();

        assert_eq!(
            result.action,
            ElicitationAction::Cancel,
            "RestrictedAutonomous + headless must return Cancel"
        );
    }

    // -----------------------------------------------------------------------
    // URL mode -- headless cancellation
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_url_mode_headless_returns_cancel() {
        let handler = make_handler(ExecutionMode::Interactive, true);
        let params = url_params(Some("https://example.com/auth"));

        let result = handler.create_elicitation(params).await.unwrap();

        assert_eq!(
            result.action,
            ElicitationAction::Cancel,
            "headless=true must return Cancel for URL mode"
        );
        assert!(result.content.is_none());
    }

    #[tokio::test]
    async fn test_url_mode_full_autonomous_headless_returns_cancel() {
        let handler = make_handler(ExecutionMode::FullAutonomous, true);
        let params = url_params(Some("https://example.com/oauth"));

        let result = handler.create_elicitation(params).await.unwrap();

        assert_eq!(result.action, ElicitationAction::Cancel);
    }

    #[tokio::test]
    async fn test_url_mode_no_url_provided_headless_returns_cancel() {
        let handler = make_handler(ExecutionMode::Interactive, true);
        let params = url_params(None);

        let result = handler.create_elicitation(params).await.unwrap();

        assert_eq!(result.action, ElicitationAction::Cancel);
    }

    // -----------------------------------------------------------------------
    // URL mode -- non-headless returns Cancel (browser attempt, no callback)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_url_mode_non_headless_returns_cancel_after_display() {
        // Non-headless URL mode displays the URL and returns Cancel because
        // the handler cannot await an OAuth callback synchronously.
        let handler = make_handler(ExecutionMode::Interactive, false);
        let params = url_params(Some("https://example.com/authorize"));

        let result = handler.create_elicitation(params).await.unwrap();

        // The handler always returns Cancel for URL mode; it cannot wait for
        // the browser OAuth redirect.
        assert_eq!(
            result.action,
            ElicitationAction::Cancel,
            "URL mode must return Cancel (cannot await browser callback)"
        );
    }

    // -----------------------------------------------------------------------
    // Default mode (None) falls back to Form
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_default_mode_none_treated_as_form() {
        let handler = make_handler(ExecutionMode::FullAutonomous, false);

        let params = ElicitationCreateParams {
            mode: None, // <-- not specified
            message: "Enter details".to_string(),
            requested_schema: None,
            url: None,
            elicitation_id: None,
        };

        // FullAutonomous -> form mode -> Cancel
        let result = handler.create_elicitation(params).await.unwrap();
        assert_eq!(result.action, ElicitationAction::Cancel);
    }

    // -----------------------------------------------------------------------
    // extract_field_names helper
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_field_names_returns_value_when_no_schema() {
        let names = extract_field_names(None);
        assert_eq!(names, vec!["value"]);
    }

    #[test]
    fn test_extract_field_names_returns_value_for_null_schema() {
        let names = extract_field_names(Some(&serde_json::Value::Null));
        assert_eq!(names, vec!["value"]);
    }

    #[test]
    fn test_extract_field_names_returns_value_for_schema_without_properties() {
        let schema = serde_json::json!({"type": "object"});
        let names = extract_field_names(Some(&schema));
        assert_eq!(names, vec!["value"]);
    }

    #[test]
    fn test_extract_field_names_extracts_and_sorts_property_names() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "zebra": {"type": "string"},
                "apple": {"type": "string"},
                "mango": {"type": "string"}
            }
        });
        let names = extract_field_names(Some(&schema));
        assert_eq!(names, vec!["apple", "mango", "zebra"]);
    }

    #[test]
    fn test_extract_field_names_single_field() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "email": {"type": "string", "format": "email"}
            }
        });
        let names = extract_field_names(Some(&schema));
        assert_eq!(names, vec!["email"]);
    }

    #[test]
    fn test_extract_field_names_empty_properties_returns_value_fallback() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {}
        });
        let names = extract_field_names(Some(&schema));
        assert_eq!(names, vec!["value"]);
    }
}
