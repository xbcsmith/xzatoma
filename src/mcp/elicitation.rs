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
//! - [`ElicitationMode::Url`] -- validates the URL against an `https` scheme
//!   allowlist and then asks the user to explicitly confirm opening the full
//!   URL before it is handed to the system browser. The browser is invoked
//!   ONLY after an affirmative confirmation. In headless contexts, or when the
//!   user declines, the request fails closed and nothing is opened.
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
//!
//! Two independent gates protect this path:
//!
//! 1. Only `https` URLs are ever eligible. The requested URL is checked
//!    against a scheme allowlist ([`is_allowed_elicitation_url`]) before
//!    anything else, so a malicious MCP server cannot make the host open
//!    `file://`, `vscode://`, `smb://`, or other non-`https` handlers.
//! 2. Even for an allowed `https` URL, the user must explicitly confirm
//!    opening the full URL. Confirmation is routed through the
//!    [`UrlOpenConfirmer`] abstraction: production uses [`StdinUrlConfirmer`],
//!    while tests inject a fake responder so neither a real browser nor real
//!    stdin is ever touched. If confirmation is refused -- including every
//!    headless/non-interactive session -- the request fails closed and the
//!    browser opener is never invoked.

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
// URL-open confirmation abstraction
// ---------------------------------------------------------------------------

/// Abstraction over the user-facing confirmation shown before an MCP URL
/// elicitation is opened in the system browser.
///
/// Implementors receive the full URL that a connected MCP server asked the
/// host to open and return `true` only when the user has explicitly approved
/// opening it. Returning `false` -- the fail-closed default for any refusal,
/// ambiguous answer, or non-interactive situation -- means the URL is NOT
/// opened.
///
/// Production code uses [`StdinUrlConfirmer`]. Tests inject a fake responder so
/// that no real browser is launched and no real stdin is read.
pub trait UrlOpenConfirmer {
    /// Ask the user whether the full `url` may be opened in the system browser.
    ///
    /// # Arguments
    ///
    /// * `url` - The complete URL the MCP server requested be opened.
    ///
    /// # Returns
    ///
    /// `true` to open the URL, `false` to fail closed and refuse.
    fn confirm_open_url(&self, url: &str) -> bool;
}

/// Production [`UrlOpenConfirmer`] that prompts on stderr and reads a single
/// confirmation line from stdin.
///
/// Displays the full URL and returns `true` only when the user answers `y` or
/// `yes` (case-insensitive). Any other answer, an empty line, EOF, or an I/O
/// error is treated as a refusal (fail closed), so the browser is never opened
/// without an explicit affirmative response.
///
/// # Safety
///
/// Reads from the real stdin. Never use this in tests -- inject a fake
/// [`UrlOpenConfirmer`] instead so no real stdin is read.
#[derive(Debug, Default)]
pub struct StdinUrlConfirmer;

impl UrlOpenConfirmer for StdinUrlConfirmer {
    /// Prompt on stderr and read one confirmation line from stdin.
    ///
    /// Fails closed (returns `false`) on EOF, empty input, an I/O error, or any
    /// answer other than `y`/`yes`.
    fn confirm_open_url(&self, url: &str) -> bool {
        eprintln!(
            "\nAn MCP server is requesting that this URL be opened in your browser:\n  {}",
            url
        );
        eprint!("Open this URL in your default browser? [y/N]: ");
        if let Err(error) = std::io::stderr().flush() {
            tracing::warn!(error = %error, "Failed to flush MCP URL confirmation prompt");
        }

        let stdin = std::io::stdin();
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                // EOF: no interactive input available. Fail closed.
                tracing::warn!(
                    "No confirmation input available for MCP URL elicitation; refusing to open"
                );
                false
            }
            Ok(_) => {
                let answer = line.trim().to_lowercase();
                matches!(answer.as_str(), "y" | "yes")
            }
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "Failed to read MCP URL confirmation; refusing to open"
                );
                false
            }
        }
    }
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
/// - **URL mode** when `headless == false`: validates the URL against the
///   `https` scheme allowlist, asks the user to confirm opening the full URL,
///   and calls `browser_opener` ONLY after an affirmative confirmation. Always
///   returns `Cancel` because the handler cannot await an async browser OAuth
///   redirect callback.
/// - **URL mode** when the user declines the confirmation: returns `Decline`
///   and never calls `browser_opener`.
/// - **URL mode** when `headless == true`: logs a warning and returns `Cancel`
///   immediately, without prompting and without calling `browser_opener`.
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
            if let Err(error) = std::io::stderr().flush() {
                tracing::warn!(field = %field, error = %error, "Failed to flush MCP elicitation prompt");
            }

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
    /// Convenience wrapper that delegates to
    /// [`handle_url_with_confirmer`][Self::handle_url_with_confirmer] using the
    /// production [`StdinUrlConfirmer`]. This is the entry point used by the
    /// [`ElicitationHandler`] trait dispatch.
    ///
    /// In headless contexts, logs a warning and returns
    /// [`ElicitationAction::Cancel`] immediately without prompting and without
    /// calling `browser_opener`.
    ///
    /// The requested URL is validated with [`is_allowed_elicitation_url`]
    /// before anything is opened: only a parseable `https` URL is permitted.
    /// Any other scheme (for example `file://`, `vscode://`, `smb://`, or
    /// plain `http`) is rejected with a `tracing::warn!` and the request is
    /// cancelled WITHOUT prompting or calling `browser_opener`. This prevents a
    /// malicious MCP server from making the host open arbitrary local handlers
    /// or filesystem paths.
    ///
    /// For an accepted `https` URL, the user is asked to confirm opening the
    /// full URL. Only after an affirmative confirmation is
    /// `self.browser_opener` called. A declined confirmation returns
    /// [`ElicitationAction::Decline`] and never opens the URL. On success the
    /// handler still returns `Cancel` because it cannot await an async browser
    /// OAuth redirect callback.
    ///
    /// # Arguments
    ///
    /// * `params` - The elicitation parameters, including the `url` to open.
    fn handle_url(&self, params: ElicitationCreateParams) -> Result<ElicitationResult> {
        self.handle_url_with_confirmer(params, &StdinUrlConfirmer)
    }

    /// Handle a URL-mode elicitation request using an injectable confirmer.
    ///
    /// This is the testable core of URL-mode handling. The `confirmer`
    /// abstraction decides whether the (already scheme-validated) URL may be
    /// opened, allowing tests to drive "approve" versus "decline" without
    /// touching the real OS opener or real stdin.
    ///
    /// Fail-closed guarantees:
    ///
    /// - Headless/non-interactive sessions return `Cancel` before the
    ///   confirmer is consulted and never open anything.
    /// - A non-`https` scheme returns `Cancel` before the confirmer is
    ///   consulted.
    /// - A declined confirmation returns `Decline` and never calls
    ///   `browser_opener`.
    ///
    /// # Arguments
    ///
    /// * `params` - The elicitation parameters, including the `url` to open.
    /// * `confirmer` - Abstraction that confirms whether the full URL may be
    ///   opened.
    fn handle_url_with_confirmer(
        &self,
        params: ElicitationCreateParams,
        confirmer: &dyn UrlOpenConfirmer,
    ) -> Result<ElicitationResult> {
        // Headless / non-interactive: fail closed. Never prompt, never open.
        if self.headless {
            tracing::warn!(
                "MCP URL elicitation received in headless context; refusing to open (fail closed)"
            );
            return Ok(ElicitationResult {
                action: ElicitationAction::Cancel,
                content: None,
            });
        }

        let url = params.url.as_deref().unwrap_or("(no URL provided)");

        // Scheme allowlist: only https URLs may be opened. Reject anything
        // else before prompting or touching the browser opener.
        if !is_allowed_elicitation_url(url) {
            tracing::warn!(
                url = %url,
                "MCP URL elicitation rejected: only https URLs are allowed; refusing to open"
            );
            return Ok(ElicitationResult {
                action: ElicitationAction::Cancel,
                content: None,
            });
        }

        // Require explicit user confirmation of the FULL url before opening.
        // Any refusal (including non-interactive input) fails closed.
        if !confirmer.confirm_open_url(url) {
            tracing::warn!(
                url = %url,
                "User declined to open MCP URL elicitation; not opening (fail closed)"
            );
            return Ok(ElicitationResult {
                action: ElicitationAction::Decline,
                content: None,
            });
        }

        // Confirmed. Delegate to the injected browser opener. In production
        // this spawns `open` / `xdg-open`. In tests this is a fake opener so no
        // subprocess is ever spawned and no network request is made.
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
        // callback.  A notification-based flow is needed to support this.
        tracing::warn!(
            "MCP URL elicitation returning Cancel; URL OAuth redirect handling \
             requires a notification-based async callback flow that is not active \
             in this session"
        );
        Ok(ElicitationResult {
            action: ElicitationAction::Cancel,
            content: None,
        })
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Return `true` only when `raw` parses as a URL whose scheme is `https`.
///
/// This is the scheme allowlist used by [`XzatomaElicitationHandler::handle_url`]
/// to decide whether a URL requested by an MCP server may be opened in the
/// system browser. Any non-`https` scheme (for example `file`, `vscode`,
/// `smb`, or plain `http`), or any string that does not parse as a URL, is
/// rejected.
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::elicitation::is_allowed_elicitation_url;
///
/// assert!(is_allowed_elicitation_url("https://example.com/auth"));
/// assert!(!is_allowed_elicitation_url("http://example.com"));
/// assert!(!is_allowed_elicitation_url("file:///etc/passwd"));
/// assert!(!is_allowed_elicitation_url("(no URL provided)"));
/// ```
pub fn is_allowed_elicitation_url(raw: &str) -> bool {
    match url::Url::parse(raw) {
        Ok(parsed) => parsed.scheme() == "https",
        Err(_) => false,
    }
}

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
    // Fake confirmers and recording opener (drive approve/decline without
    // touching the real OS opener or real stdin)
    // -----------------------------------------------------------------------

    use std::cell::RefCell;

    /// Confirmer that always approves and records every URL it is asked about.
    struct RecordingApprovingConfirmer {
        seen: RefCell<Vec<String>>,
    }

    impl RecordingApprovingConfirmer {
        fn new() -> Self {
            Self {
                seen: RefCell::new(Vec::new()),
            }
        }
    }

    impl UrlOpenConfirmer for RecordingApprovingConfirmer {
        fn confirm_open_url(&self, url: &str) -> bool {
            self.seen.borrow_mut().push(url.to_string());
            true
        }
    }

    /// Confirmer that always declines and records every URL it is asked about.
    struct RecordingDecliningConfirmer {
        seen: RefCell<Vec<String>>,
    }

    impl RecordingDecliningConfirmer {
        fn new() -> Self {
            Self {
                seen: RefCell::new(Vec::new()),
            }
        }
    }

    impl UrlOpenConfirmer for RecordingDecliningConfirmer {
        fn confirm_open_url(&self, url: &str) -> bool {
            self.seen.borrow_mut().push(url.to_string());
            false
        }
    }

    /// Confirmer that must never be consulted; panics if called.
    ///
    /// Used to prove that headless/fail-closed paths never prompt.
    struct PanickingConfirmer;

    impl UrlOpenConfirmer for PanickingConfirmer {
        fn confirm_open_url(&self, _url: &str) -> bool {
            panic!("confirmer must not be consulted in this context");
        }
    }

    // Per-thread record of URLs handed to the recording browser opener. Each
    // `#[tokio::test]` runs on its own OS thread, so this is isolated per test.
    thread_local! {
        static OPENED_URLS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    /// Browser opener that records the URL it received instead of spawning a
    /// subprocess. Returns `true` (open "succeeded").
    fn recording_browser_opener(url: &str) -> bool {
        OPENED_URLS.with(|c| c.borrow_mut().push(url.to_string()));
        true
    }

    /// Drain and return the URLs recorded by [`recording_browser_opener`].
    fn take_opened_urls() -> Vec<String> {
        OPENED_URLS.with(|c| std::mem::take(&mut *c.borrow_mut()))
    }

    /// Browser opener that must never be invoked; panics if called.
    fn panicking_browser_opener(_url: &str) -> bool {
        panic!("browser opener must not be called in this context");
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
    // URL mode -- confirmation gate (non-headless)
    //
    // These tests drive the injectable UrlOpenConfirmer directly so no real
    // browser opens and no real stdin is read.
    // -----------------------------------------------------------------------

    /// Build a handler with an explicit browser opener for confirmation tests.
    fn make_handler_with_opener(
        execution_mode: ExecutionMode,
        headless: bool,
        browser_opener: BrowserOpenerFn,
    ) -> XzatomaElicitationHandler {
        XzatomaElicitationHandler {
            execution_mode,
            headless,
            browser_opener,
        }
    }

    #[tokio::test]
    async fn test_url_mode_approved_confirmation_opens_full_url() {
        let _ = take_opened_urls(); // ensure a clean per-thread recorder
        let full_url = "https://auth.test.invalid/authorize?client_id=abc&state=xyz";
        let handler =
            make_handler_with_opener(ExecutionMode::Interactive, false, recording_browser_opener);
        let confirmer = RecordingApprovingConfirmer::new();
        let params = url_params(Some(full_url));

        let result = handler
            .handle_url_with_confirmer(params, &confirmer)
            .unwrap();

        // The confirmation surface saw the FULL url.
        assert_eq!(
            confirmer.seen.borrow().as_slice(),
            [full_url.to_string()],
            "confirmer must be asked about the full url"
        );
        // Opening proceeded only after approval, with the FULL url.
        assert_eq!(
            take_opened_urls(),
            vec![full_url.to_string()],
            "browser opener must receive the full url after approval"
        );
        // Still Cancel: the handler cannot await the OAuth callback.
        assert_eq!(result.action, ElicitationAction::Cancel);
        assert!(result.content.is_none());
    }

    #[tokio::test]
    async fn test_url_mode_declined_confirmation_does_not_open() {
        let full_url = "https://auth.test.invalid/authorize?client_id=abc";
        // panicking_browser_opener guarantees the URL is never opened.
        let handler =
            make_handler_with_opener(ExecutionMode::Interactive, false, panicking_browser_opener);
        let confirmer = RecordingDecliningConfirmer::new();
        let params = url_params(Some(full_url));

        let result = handler
            .handle_url_with_confirmer(params, &confirmer)
            .unwrap();

        // The confirmation surface saw the FULL url but declined.
        assert_eq!(
            confirmer.seen.borrow().as_slice(),
            [full_url.to_string()],
            "confirmer must be asked about the full url"
        );
        // A declined confirmation fails closed with Decline and never opens.
        assert_eq!(
            result.action,
            ElicitationAction::Decline,
            "declined confirmation must return Decline"
        );
        assert!(result.content.is_none());
    }

    #[tokio::test]
    async fn test_url_mode_headless_fails_closed_without_confirming_or_opening() {
        // Both the confirmer and the opener panic if consulted; headless must
        // return before either is reached.
        let handler =
            make_handler_with_opener(ExecutionMode::Interactive, true, panicking_browser_opener);
        let confirmer = PanickingConfirmer;
        let params = url_params(Some("https://auth.test.invalid/authorize"));

        let result = handler
            .handle_url_with_confirmer(params, &confirmer)
            .unwrap();

        assert_eq!(
            result.action,
            ElicitationAction::Cancel,
            "headless URL mode must fail closed with Cancel"
        );
        assert!(result.content.is_none());
    }

    #[tokio::test]
    async fn test_url_mode_non_https_rejected_before_confirming() {
        // A non-https scheme must be rejected before the confirmer or opener is
        // consulted; both panic if reached.
        let handler =
            make_handler_with_opener(ExecutionMode::Interactive, false, panicking_browser_opener);
        let confirmer = PanickingConfirmer;
        let params = url_params(Some("file:///etc/passwd"));

        let result = handler
            .handle_url_with_confirmer(params, &confirmer)
            .unwrap();

        assert_eq!(
            result.action,
            ElicitationAction::Cancel,
            "non-https URL must be cancelled without prompting or opening"
        );
        assert!(result.content.is_none());
    }

    // -----------------------------------------------------------------------
    // URL scheme allowlist (is_allowed_elicitation_url)
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_allowed_elicitation_url_accepts_https() {
        assert!(is_allowed_elicitation_url("https://example.com/auth"));
    }

    #[test]
    fn test_is_allowed_elicitation_url_rejects_file_scheme() {
        assert!(!is_allowed_elicitation_url("file:///etc/passwd"));
    }

    #[test]
    fn test_is_allowed_elicitation_url_rejects_vscode_scheme() {
        assert!(!is_allowed_elicitation_url("vscode://x"));
    }

    #[test]
    fn test_is_allowed_elicitation_url_rejects_smb_scheme() {
        assert!(!is_allowed_elicitation_url("smb://host/share"));
    }

    #[test]
    fn test_is_allowed_elicitation_url_rejects_http_scheme() {
        assert!(!is_allowed_elicitation_url("http://example.com"));
    }

    #[test]
    fn test_is_allowed_elicitation_url_rejects_unparseable() {
        assert!(!is_allowed_elicitation_url("(no URL provided)"));
    }

    #[tokio::test]
    async fn test_url_mode_non_https_scheme_returns_cancel_without_opening() {
        let handler = make_handler(ExecutionMode::Interactive, false);
        // A non-https scheme must be rejected before the browser opener runs.
        let params = url_params(Some("file:///etc/passwd"));

        let result = handler.create_elicitation(params).await.unwrap();

        assert_eq!(
            result.action,
            ElicitationAction::Cancel,
            "non-https URL must be cancelled without opening"
        );
        assert!(result.content.is_none());
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
