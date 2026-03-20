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
//!
//! # Browser Opening
//!
//! The URL mode calls the `browser_opener` function pointer stored on the
//! handler to attempt to open a URL in the system browser.  In production
//! this is set to [`open_browser`], which spawns `open` (macOS) or
//! `xdg-open` (Linux).  In tests it must be set to [`noop_browser_opener`]
//! so that no subprocess is ever spawned and no network request is made.

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
// Browser opener function type and built-in implementations
// ---------------------------------------------------------------------------

/// Signature for a browser-opener function stored on
/// [`XzatomaElicitationHandler`].
///
/// The function receives the URL string to open and returns `true` if the
/// attempt succeeded (a process was spawned or the URL was handled), `false`
/// otherwise.  It must never block and must never make network requests.
pub type BrowserOpenerFn = fn(&str) -> bool;

/// Production browser opener: spawns `open` (macOS) or `xdg-open` (Linux).
///
/// Returns `true` if a subprocess was successfully spawned (not necessarily
/// that the browser actually opened the URL).  Returns `false` if both
/// platform commands fail to spawn.
///
/// # Safety
///
/// Spawns an OS subprocess.  Never call this in tests -- use
/// [`noop_browser_opener`] instead.
pub fn open_browser(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
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
    }
    #[cfg(target_os = "linux")]
    {
        if std::process::Command::new("xdg-open")
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .is_ok()
        {
            return true;
        }
    }
    // Suppress unused-variable warning on platforms with no opener.
    let _ = url;
    false
}

/// No-op browser opener for use in tests.
///
/// Never spawns a subprocess, never opens a browser, never makes a network
/// request.  Always returns `false` (no browser was opened).
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::elicitation::noop_browser_opener;
///
/// // Safe to call in any context -- pure no-op.
/// let result = noop_browser_opener("https://auth.test.invalid/oauth");
/// assert!(!result);
/// ```
pub fn noop_browser_opener(_url: &str) -> bool {
    false
}

// ---------------------------------------------------------------------------
// XzatomaElicitationHandler
// ---------------------------------------------------------------------------

/// MCP elicitation handler for Xzatoma.
///
/// Handles `elicitation/create` requests from connected MCP servers.
/// Behaviour varies by mode:
///
/// - **Form mode** in an interactive, non-`FullAutonomous` context: prints the
///   server's message, iterates over fields declared in `requested_schema`, and
///   reads one line per field from stdin.  The user may type `"decline"` at
///   any field prompt to decline the elicitation entirely.
/// - **Form mode** when `headless == true` or
///   `execution_mode == FullAutonomous`: logs a warning and returns `Cancel`
///   immediately without touching stdin.
/// - **URL mode** when `headless == false`: prints the URL to stderr and calls
///   `browser_opener` to attempt to open it.  Always returns `Cancel` because
///   the handler cannot await an async browser OAuth redirect callback.
/// - **URL mode** when `headless == true`: logs a warning and returns `Cancel`
///   immediately without calling `browser_opener`.
///
/// # Browser Opener Injection
///
/// The `browser_opener` field is a plain function pointer
/// (`fn(&str) -> bool`).  Production callers set it to [`open_browser`].
/// Tests must set it to [`noop_browser_opener`] so no subprocess is ever
/// spawned.
///
/// ```
/// use xzatoma::config::ExecutionMode;
/// use xzatoma::mcp::elicitation::{
///     XzatomaElicitationHandler, noop_browser_opener, open_browser,
/// };
///
/// // Production handler:
/// let _prod = XzatomaElicitationHandler {
///     execution_mode: ExecutionMode::Interactive,
///     headless: false,
///     browser_opener: open_browser,
/// };
///
/// // Test handler -- never spawns a browser:
/// let _test = XzatomaElicitationHandler {
///     execution_mode: ExecutionMode::Interactive,
///     headless: false,
///     browser_opener: noop_browser_opener,
/// };
/// ```
pub struct XzatomaElicitationHandler {
    /// Agent execution mode; `FullAutonomous` suppresses form elicitation.
    pub execution_mode: ExecutionMode,
    /// Whether the agent is running headless (non-interactive).
    pub headless: bool,
    /// Function used to open a URL in the system browser.
    ///
    /// Set to [`open_browser`] in production.
    /// Set to [`noop_browser_opener`] in tests to prevent subprocess spawning.
    pub browser_opener: BrowserOpenerFn,
}

impl std::fmt::Debug for XzatomaElicitationHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XzatomaElicitationHandler")
            .field("execution_mode", &self.execution_mode)
            .field("headless", &self.headless)
            // function pointers do not have a meaningful Debug representation
            .field("browser_opener", &"<fn>")
            .finish()
    }
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
    /// Currently infallible for cancellation paths.  Returns an error only if
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
    /// [`ElicitationAction::Cancel`] immediately without touching stdin.
    /// Otherwise, interactively prompts the user for each field declared in
    /// `requested_schema`.
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
    /// [`ElicitationAction::Cancel`] immediately without calling
    /// `browser_opener`.  Otherwise, prints the URL to stderr and calls
    /// `self.browser_opener` to attempt to open it in the system browser.
    /// Always returns `Cancel` because the handler cannot await an async
    /// browser OAuth redirect callback.
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

        // Delegate to the injected browser opener.  In production this spawns
        // `open` / `xdg-open`.  In tests this is always `noop_browser_opener`
        // so no subprocess is ever spawned and no network request is made.
        let opened = (self.browser_opener)(url);
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
/// `"properties"` key, or when the properties map is empty.
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

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::types::{ElicitationAction, ElicitationCreateParams, ElicitationMode};

    /// Build a handler with the given mode and headless flag.
    ///
    /// Always uses [`noop_browser_opener`] -- no subprocess is ever spawned.
    fn make_handler(execution_mode: ExecutionMode, headless: bool) -> XzatomaElicitationHandler {
        XzatomaElicitationHandler {
            execution_mode,
            headless,
            // Tests must never use the real browser opener.
            browser_opener: noop_browser_opener,
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
    // URL mode -- headless cancellation (noop_browser_opener: no subprocess)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_url_mode_headless_returns_cancel() {
        let handler = make_handler(ExecutionMode::Interactive, true);
        // headless=true: handle_url returns Cancel before calling browser_opener.
        let params = url_params(Some("https://auth.test.invalid/auth"));

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
        // headless=true: handle_url returns Cancel before calling browser_opener.
        let params = url_params(Some("https://auth.test.invalid/oauth"));

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
    // URL mode -- non-headless: browser_opener is noop, returns Cancel
    //
    // headless=false causes handle_url to call self.browser_opener, which is
    // noop_browser_opener here.  No subprocess is spawned, no network request
    // is made.  The handler still returns Cancel because it cannot await the
    // OAuth callback synchronously.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_url_mode_non_headless_returns_cancel_after_display() {
        let handler = make_handler(ExecutionMode::Interactive, false);
        // browser_opener is noop_browser_opener -- zero subprocesses spawned.
        let params = url_params(Some("https://auth.test.invalid/authorize"));

        let result = handler.create_elicitation(params).await.unwrap();

        assert_eq!(
            result.action,
            ElicitationAction::Cancel,
            "URL mode must return Cancel -- cannot await browser callback"
        );
    }

    // -----------------------------------------------------------------------
    // Default mode (None) falls back to Form
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_default_mode_none_treated_as_form() {
        let handler = make_handler(ExecutionMode::FullAutonomous, false);

        let params = ElicitationCreateParams {
            mode: None,
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
    // noop_browser_opener: verify it is truly a no-op
    // -----------------------------------------------------------------------

    #[test]
    fn test_noop_browser_opener_returns_false_and_spawns_nothing() {
        // Calling noop_browser_opener must return false and must not
        // spawn any subprocess.  This test verifies the return value only;
        // subprocess absence is guaranteed by the function's implementation.
        let result = noop_browser_opener("https://auth.test.invalid/should-not-open");
        assert!(
            !result,
            "noop_browser_opener must return false (no browser opened)"
        );
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
