//! Integration tests for Phase 5B: Elicitation Handler
//!
//! Covers Task 5B.5 requirements:
//!
//! - `test_form_mode_headless_returns_cancel`
//! - `test_form_mode_full_autonomous_returns_cancel`
//! - `test_url_mode_headless_returns_cancel`
//!
//! Additional coverage for all non-interactive cancellation paths.
//!
//! # Network isolation guarantee
//!
//! Every handler in this file is built with `noop_browser_opener`.  No
//! subprocess is ever spawned and no network request is ever made -- not
//! even in the non-headless URL tests.  The `noop_browser_opener` function
//! is a pure no-op exported from `xzatoma::mcp::elicitation`.

use xzatoma::config::ExecutionMode;
use xzatoma::mcp::elicitation::{noop_browser_opener, XzatomaElicitationHandler};
use xzatoma::mcp::protocol::ElicitationHandler;
use xzatoma::mcp::types::{ElicitationAction, ElicitationCreateParams, ElicitationMode};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a handler with the given execution mode and headless flag.
///
/// Always uses [`noop_browser_opener`] -- no subprocess is ever spawned and
/// no network request is ever made, even in non-headless URL-mode tests.
fn make_handler(execution_mode: ExecutionMode, headless: bool) -> XzatomaElicitationHandler {
    XzatomaElicitationHandler {
        execution_mode,
        headless,
        browser_opener: noop_browser_opener,
    }
}

/// Build `ElicitationCreateParams` for Form mode.
fn form_params(message: &str, schema: Option<serde_json::Value>) -> ElicitationCreateParams {
    ElicitationCreateParams {
        mode: Some(ElicitationMode::Form),
        message: message.to_string(),
        requested_schema: schema,
        url: None,
        elicitation_id: None,
    }
}

/// Build `ElicitationCreateParams` for URL mode.
fn url_params(url: Option<&str>) -> ElicitationCreateParams {
    ElicitationCreateParams {
        mode: Some(ElicitationMode::Url),
        message: String::new(),
        requested_schema: None,
        url: url.map(|s| s.to_string()),
        elicitation_id: None,
    }
}

// ---------------------------------------------------------------------------
// Task 5B.5 required tests: Form mode -- headless / FullAutonomous
// ---------------------------------------------------------------------------

/// Form mode with `headless: true` must return `Cancel` without prompting.
///
/// The handler must not attempt to read from stdin. If it does, the test
/// will block indefinitely (no stdin in the test harness).
#[tokio::test]
async fn test_form_mode_headless_returns_cancel() {
    let handler = make_handler(ExecutionMode::Interactive, true);
    let params = form_params("Please provide your name and email", None);

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        handler.create_elicitation(params),
    )
    .await
    .expect("create_elicitation timed out -- did it block on stdin?")
    .expect("create_elicitation returned an error");

    assert_eq!(
        result.action,
        ElicitationAction::Cancel,
        "headless=true, form mode must return Cancel"
    );
    assert!(
        result.content.is_none(),
        "Cancel result must have no content"
    );
}

/// Form mode with `execution_mode: FullAutonomous` (and `headless: false`)
/// must return `Cancel` without prompting.
#[tokio::test]
async fn test_form_mode_full_autonomous_returns_cancel() {
    let handler = make_handler(ExecutionMode::FullAutonomous, false);
    let params = form_params("Enter your credentials", None);

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        handler.create_elicitation(params),
    )
    .await
    .expect("create_elicitation timed out in FullAutonomous form mode")
    .expect("create_elicitation returned an error");

    assert_eq!(
        result.action,
        ElicitationAction::Cancel,
        "FullAutonomous + headless=false must return Cancel for form mode"
    );
    assert!(
        result.content.is_none(),
        "Cancel result must have no content"
    );
}

// ---------------------------------------------------------------------------
// Task 5B.5 required test: URL mode -- headless
// ---------------------------------------------------------------------------

/// URL mode with `headless: true` must return `Cancel` immediately.
#[tokio::test]
async fn test_url_mode_headless_returns_cancel() {
    let handler = make_handler(ExecutionMode::Interactive, true);
    let params = url_params(Some("https://auth.test.invalid/oauth/authorize"));

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        handler.create_elicitation(params),
    )
    .await
    .expect("create_elicitation timed out in headless URL mode")
    .expect("create_elicitation returned an error");

    assert_eq!(
        result.action,
        ElicitationAction::Cancel,
        "headless=true must return Cancel for URL mode"
    );
    assert!(
        result.content.is_none(),
        "Cancel result must have no content"
    );
}

// ---------------------------------------------------------------------------
// Additional coverage: all cancellation combinations
// ---------------------------------------------------------------------------

/// Both `headless=true` AND `FullAutonomous` must return `Cancel`.
#[tokio::test]
async fn test_form_mode_headless_and_full_autonomous_returns_cancel() {
    let handler = make_handler(ExecutionMode::FullAutonomous, true);
    let params = form_params("", None);

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        handler.create_elicitation(params),
    )
    .await
    .expect("create_elicitation timed out")
    .expect("create_elicitation returned an error");

    assert_eq!(result.action, ElicitationAction::Cancel);
    assert!(result.content.is_none());
}

/// `RestrictedAutonomous` mode with `headless=true` must return `Cancel`.
#[tokio::test]
async fn test_form_mode_restricted_autonomous_headless_returns_cancel() {
    let handler = make_handler(ExecutionMode::RestrictedAutonomous, true);
    let params = form_params("Provide API key", None);

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        handler.create_elicitation(params),
    )
    .await
    .expect("create_elicitation timed out")
    .expect("create_elicitation returned an error");

    assert_eq!(
        result.action,
        ElicitationAction::Cancel,
        "RestrictedAutonomous + headless=true must return Cancel"
    );
}

/// URL mode with `FullAutonomous` and `headless=true` must return `Cancel`.
#[tokio::test]
async fn test_url_mode_full_autonomous_headless_returns_cancel() {
    let handler = make_handler(ExecutionMode::FullAutonomous, true);
    let params = url_params(Some("https://auth.test.invalid/login"));

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        handler.create_elicitation(params),
    )
    .await
    .expect("create_elicitation timed out")
    .expect("create_elicitation returned an error");

    assert_eq!(result.action, ElicitationAction::Cancel);
    assert!(result.content.is_none());
}

/// URL mode with no URL provided and `headless=true` must return `Cancel`.
#[tokio::test]
async fn test_url_mode_no_url_headless_returns_cancel() {
    let handler = make_handler(ExecutionMode::Interactive, true);
    let params = url_params(None);

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        handler.create_elicitation(params),
    )
    .await
    .expect("create_elicitation timed out")
    .expect("create_elicitation returned an error");

    assert_eq!(
        result.action,
        ElicitationAction::Cancel,
        "URL mode with no URL and headless=true must return Cancel"
    );
}

// ---------------------------------------------------------------------------
// URL mode -- non-headless: noop_browser_opener, returns Cancel
// ---------------------------------------------------------------------------

/// URL mode in a non-headless context must still return `Cancel` because
/// the handler cannot await a browser OAuth callback synchronously.
///
/// `browser_opener` is set to `noop_browser_opener` in `make_handler`, so
/// no subprocess is spawned and no network request is made -- even though
/// `headless` is `false` and the handler does call `browser_opener`.
#[tokio::test]
async fn test_url_mode_non_headless_returns_cancel_after_display() {
    let handler = make_handler(ExecutionMode::Interactive, false);
    // .invalid TLD (RFC 2606) -- guaranteed to never resolve.
    // noop_browser_opener ensures this string is never passed to open/xdg-open.
    let params = url_params(Some("https://auth.test.invalid/authorize?code=abc123"));

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        handler.create_elicitation(params),
    )
    .await
    .expect("create_elicitation timed out")
    .expect("create_elicitation returned an error");

    assert_eq!(
        result.action,
        ElicitationAction::Cancel,
        "URL mode must return Cancel -- cannot await browser callback"
    );
    assert!(result.content.is_none());
}

// ---------------------------------------------------------------------------
// Default mode (None) falls back to Form
// ---------------------------------------------------------------------------

/// When `mode` is `None`, the handler must treat it as `Form` mode.
/// With `FullAutonomous`, that means `Cancel`.
#[tokio::test]
async fn test_mode_none_defaults_to_form_and_full_autonomous_returns_cancel() {
    let handler = make_handler(ExecutionMode::FullAutonomous, false);

    let params = ElicitationCreateParams {
        mode: None,
        message: "Enter details".to_string(),
        requested_schema: None,
        url: None,
        elicitation_id: None,
    };

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        handler.create_elicitation(params),
    )
    .await
    .expect("create_elicitation timed out")
    .expect("create_elicitation returned an error");

    assert_eq!(
        result.action,
        ElicitationAction::Cancel,
        "None mode defaults to Form; FullAutonomous must return Cancel"
    );
}

/// When `mode` is `None` and `headless=true`, the handler must also cancel.
#[tokio::test]
async fn test_mode_none_defaults_to_form_and_headless_returns_cancel() {
    let handler = make_handler(ExecutionMode::Interactive, true);

    let params = ElicitationCreateParams {
        mode: None,
        message: "Provide input".to_string(),
        requested_schema: None,
        url: None,
        elicitation_id: None,
    };

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        handler.create_elicitation(params),
    )
    .await
    .expect("create_elicitation timed out")
    .expect("create_elicitation returned an error");

    assert_eq!(result.action, ElicitationAction::Cancel);
    assert!(result.content.is_none());
}

// ---------------------------------------------------------------------------
// Idempotence: calling the same handler multiple times
// ---------------------------------------------------------------------------

/// Calling `create_elicitation` multiple times on the same handler must
/// produce the same result each time (no state mutation between calls).
#[tokio::test]
async fn test_handler_is_idempotent_for_cancel_path() {
    let handler = make_handler(ExecutionMode::FullAutonomous, false);

    for i in 0..3 {
        let params = form_params(
            &format!("Request number {}", i),
            Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "field_a": {"type": "string"},
                    "field_b": {"type": "integer"}
                }
            })),
        );

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            handler.create_elicitation(params),
        )
        .await
        .expect("create_elicitation timed out")
        .expect("create_elicitation returned an error");

        assert_eq!(
            result.action,
            ElicitationAction::Cancel,
            "call #{} must return Cancel",
            i
        );
    }
}

// ---------------------------------------------------------------------------
// Schema with properties in headless context (cancel, no schema parsing)
// ---------------------------------------------------------------------------

/// A rich `requested_schema` must not affect the Cancel outcome in headless
/// contexts.
#[tokio::test]
async fn test_form_mode_headless_with_rich_schema_still_returns_cancel() {
    let handler = make_handler(ExecutionMode::Interactive, true);

    let params = form_params(
        "Fill in user details",
        Some(serde_json::json!({
            "type": "object",
            "properties": {
                "username": {"type": "string", "minLength": 3},
                "email": {"type": "string", "format": "email"},
                "age": {"type": "integer", "minimum": 0}
            },
            "required": ["username", "email"]
        })),
    );

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        handler.create_elicitation(params),
    )
    .await
    .expect("create_elicitation timed out")
    .expect("create_elicitation returned an error");

    assert_eq!(
        result.action,
        ElicitationAction::Cancel,
        "headless=true must return Cancel regardless of schema complexity"
    );
    assert!(result.content.is_none());
}
