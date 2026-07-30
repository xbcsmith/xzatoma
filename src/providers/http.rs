//! Shared HTTP error-construction helpers for provider implementations.
//!
//! The GitHub Copilot, Ollama, and OpenAI providers all translate non-success
//! HTTP responses into [`XzatomaError`] values. Before this module existed,
//! each provider duplicated the same three steps: capture the status, read and
//! redact the response body, and build the appropriate error variant. The
//! helpers here centralize those steps so every provider produces byte-identical
//! error strings and variants while sharing a single implementation.
//!
//! Two families of helpers are provided because the providers deliberately use
//! different error variants:
//!
//! * [`api_error`] builds the string-based [`XzatomaError::Authentication`] /
//!   [`XzatomaError::Provider`] pair used by the Copilot provider.
//! * [`provider_http_status`], [`redacted_body`], and [`check_response`] build
//!   the structured [`XzatomaError::ProviderHttpStatus`] variant used by the
//!   Ollama and OpenAI providers.

use crate::error::{Result, XzatomaError};

/// Build a string-based provider error from a non-success HTTP response.
///
/// The `body` is redacted with [`crate::security::redact_sensitive_text`] and
/// embedded in a message of the form `"{provider_label} returned error
/// {status}: {body}"`. When `status` is `401 Unauthorized` and
/// `unauthorized_hint` is `Some`, the hint is appended after a period and the
/// error is returned as [`XzatomaError::Authentication`]; otherwise the error is
/// returned as [`XzatomaError::Provider`].
///
/// # Arguments
///
/// * `provider_label` - Human-readable provider name used in the message.
/// * `status` - The HTTP status code received.
/// * `body` - The raw (pre-redaction) response body text.
/// * `unauthorized_hint` - Optional re-authentication hint appended on `401`.
///
/// # Returns
///
/// An [`XzatomaError::Authentication`] on `401` with a hint, otherwise an
/// [`XzatomaError::Provider`].
///
/// # Examples
///
/// ```
/// use xzatoma::error::XzatomaError;
/// use xzatoma::providers::http::api_error;
///
/// let hint = "Token may have expired; please re-authenticate";
/// let err = api_error(
///     "Copilot",
///     reqwest::StatusCode::UNAUTHORIZED,
///     "unauthorized",
///     Some(hint),
/// );
/// assert!(matches!(err, XzatomaError::Authentication(_)));
///
/// let err = api_error(
///     "Copilot",
///     reqwest::StatusCode::INTERNAL_SERVER_ERROR,
///     "boom",
///     Some(hint),
/// );
/// assert!(matches!(err, XzatomaError::Provider(_)));
/// ```
pub fn api_error(
    provider_label: &str,
    status: reqwest::StatusCode,
    body: &str,
    unauthorized_hint: Option<&str>,
) -> XzatomaError {
    let body = crate::security::redact_sensitive_text(body);
    let base = format!("{} returned error {}: {}", provider_label, status, body);
    if status == reqwest::StatusCode::UNAUTHORIZED
        && let Some(hint) = unauthorized_hint
    {
        return XzatomaError::Authentication(format!("{}. {}", base, hint));
    }
    XzatomaError::Provider(base)
}

/// Build a structured [`XzatomaError::ProviderHttpStatus`] error.
///
/// This centralizes the struct-literal construction that the Ollama and OpenAI
/// providers repeat at each non-success response site. The caller is
/// responsible for having already redacted `response` where appropriate.
///
/// # Arguments
///
/// * `provider` - Provider name such as `openai` or `ollama`.
/// * `endpoint` - Endpoint category such as `api/chat` or `models`.
/// * `status` - The HTTP status code received.
/// * `response` - The redacted and bounded response body or context.
///
/// # Returns
///
/// An [`XzatomaError::ProviderHttpStatus`] carrying the supplied fields.
///
/// # Examples
///
/// ```
/// use xzatoma::error::XzatomaError;
/// use xzatoma::providers::http::provider_http_status;
///
/// let err = provider_http_status(
///     "ollama",
///     "api/chat",
///     reqwest::StatusCode::BAD_GATEWAY,
///     "upstream failure",
/// );
/// assert!(matches!(err, XzatomaError::ProviderHttpStatus { .. }));
/// ```
pub fn provider_http_status(
    provider: &str,
    endpoint: &str,
    status: reqwest::StatusCode,
    response: impl Into<String>,
) -> XzatomaError {
    XzatomaError::ProviderHttpStatus {
        provider: provider.to_string(),
        endpoint: endpoint.to_string(),
        status,
        response: response.into(),
    }
}

/// Read a response body and redact any sensitive text from it.
///
/// Consumes `response`, reading the body as text (falling back to an empty
/// string on read failure) and returning the redacted result. This mirrors the
/// `redact_sensitive_text(&response.text().await.unwrap_or_default())` idiom
/// duplicated across the providers.
///
/// # Arguments
///
/// * `response` - The HTTP response whose body should be read and redacted.
///
/// # Returns
///
/// The redacted response body as a `String`.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::http::redacted_body;
///
/// // Reading a live response body is only possible with a real connection,
/// // so this example only demonstrates the call shape.
/// async fn demo(response: reqwest::Response) -> String {
///     redacted_body(response).await
/// }
/// ```
pub async fn redacted_body(response: reqwest::Response) -> String {
    crate::security::redact_sensitive_text(&response.text().await.unwrap_or_default())
}

/// Return the response unchanged on success, or a structured error otherwise.
///
/// On a non-success status the body is read and redacted via [`redacted_body`]
/// and an [`XzatomaError::ProviderHttpStatus`] is returned via
/// [`provider_http_status`]. Use this to collapse the common
/// `if !status.is_success() { read body; redact; build error }` block. Sites
/// that additionally log the body or perform token-refresh flows should not use
/// this helper; they should call [`redacted_body`] and [`provider_http_status`]
/// directly to preserve their extra behavior.
///
/// # Arguments
///
/// * `response` - The HTTP response to inspect.
/// * `provider` - Provider name such as `openai` or `ollama`.
/// * `endpoint` - Endpoint category such as `api/chat` or `models`.
///
/// # Errors
///
/// Returns [`XzatomaError::ProviderHttpStatus`] when the response status is not
/// a success.
///
/// # Examples
///
/// ```
/// use xzatoma::error::Result;
/// use xzatoma::providers::http::check_response;
///
/// // Inspecting a live response requires a real connection, so this example
/// // only demonstrates the call shape.
/// async fn demo(response: reqwest::Response) -> Result<reqwest::Response> {
///     check_response(response, "ollama", "api/chat").await
/// }
/// ```
pub async fn check_response(
    response: reqwest::Response,
    provider: &str,
    endpoint: &str,
) -> Result<reqwest::Response> {
    let status = response.status();
    if status.is_success() {
        Ok(response)
    } else {
        let body = redacted_body(response).await;
        Err(provider_http_status(provider, endpoint, status, body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_error_unauthorized_with_hint_returns_authentication() {
        let err = api_error(
            "Copilot",
            reqwest::StatusCode::UNAUTHORIZED,
            "unauthorized request",
            Some("please re-authenticate"),
        );
        assert!(matches!(err, XzatomaError::Authentication(_)));
        let message = err.to_string();
        assert!(message.contains("Copilot returned error"));
        assert!(message.contains("unauthorized request"));
        assert!(message.contains("please re-authenticate"));
    }

    #[test]
    fn test_api_error_unauthorized_without_hint_returns_provider() {
        let err = api_error(
            "Copilot",
            reqwest::StatusCode::UNAUTHORIZED,
            "unauthorized request",
            None,
        );
        assert!(matches!(err, XzatomaError::Provider(_)));
        assert!(!err.to_string().contains("please re-authenticate"));
    }

    #[test]
    fn test_api_error_non_401_returns_provider() {
        let err = api_error(
            "Copilot",
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "boom",
            Some("please re-authenticate"),
        );
        assert!(matches!(err, XzatomaError::Provider(_)));
        assert!(!err.to_string().contains("please re-authenticate"));
    }

    #[test]
    fn test_api_error_redacts_body() {
        let err = api_error(
            "Copilot",
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "Authorization: Bearer sk-secret-value",
            None,
        );
        let message = err.to_string();
        assert!(!message.contains("sk-secret-value"));
    }

    #[test]
    fn test_provider_http_status_builds_expected_variant() {
        let err = provider_http_status(
            "ollama",
            "api/chat",
            reqwest::StatusCode::BAD_GATEWAY,
            "upstream failure",
        );
        match err {
            XzatomaError::ProviderHttpStatus {
                provider,
                endpoint,
                status,
                response,
            } => {
                assert_eq!(provider, "ollama");
                assert_eq!(endpoint, "api/chat");
                assert_eq!(status, reqwest::StatusCode::BAD_GATEWAY);
                assert_eq!(response, "upstream failure");
            }
            other => panic!("expected ProviderHttpStatus, got {:?}", other),
        }
    }
}
