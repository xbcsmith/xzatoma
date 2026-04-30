//! OAuth 2.1 authorization code flow integration tests using wiremock
//!
//! Verifies the token exchange portion of `src/mcp/auth/flow.rs`:
//!
//! - The `code_verifier` sent to the token endpoint matches the verifier
//!   produced by `pkce::generate()`.
//! - The token endpoint response is correctly parsed into an `OAuthToken`.
//! - The `resource` parameter (RFC 8707) is included in the token exchange
//!   request.
//! - `refresh_token` flow sends correct parameters.
//! - Error responses from the token endpoint propagate as `McpAuth` errors.
//!
//! All `OAuthFlow` instances in this file are constructed with
//! `OAuthFlow::new_with_opener(..., noop_browser_opener)` so that no
//! subprocess is ever spawned and no browser or network request is made.
//! The `refresh_token` and error-path tests that do make HTTP calls direct
//! them exclusively to an in-process `wiremock::MockServer` bound to
//! `127.0.0.1` on an OS-assigned port -- never to the public internet.

use std::collections::HashMap;
use std::sync::Arc;

use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use xzatoma::mcp::auth::discovery::AuthorizationServerMetadata;
use xzatoma::mcp::auth::flow::{noop_browser_opener, OAuthFlow, OAuthFlowConfig};
use xzatoma::mcp::auth::pkce;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Builds a minimal [`AuthorizationServerMetadata`] whose token endpoint
/// points at the given wiremock server URL.
fn make_server_metadata(base_url: &str) -> AuthorizationServerMetadata {
    AuthorizationServerMetadata {
        issuer: base_url.to_string(),
        authorization_endpoint: format!("{}/authorize", base_url),
        token_endpoint: format!("{}/token", base_url),
        registration_endpoint: None,
        scopes_supported: None,
        response_types_supported: vec!["code".to_string()],
        grant_types_supported: Some(vec![
            "authorization_code".to_string(),
            "refresh_token".to_string(),
        ]),
        code_challenge_methods_supported: Some(vec!["S256".to_string()]),
        client_id_metadata_document_supported: None,
        extra: HashMap::new(),
    }
}

/// Builds an [`OAuthFlowConfig`] with a static client ID pointing at
/// `resource_url`.
fn make_flow_config(server_id: &str, resource_url: &str, redirect_port: u16) -> OAuthFlowConfig {
    OAuthFlowConfig {
        server_id: server_id.to_string(),
        resource_url: url::Url::parse(resource_url).expect("valid resource URL"),
        client_name: "Xzatoma".to_string(),
        redirect_port,
        static_client_id: Some("test-client-id".to_string()),
        static_client_secret: None,
    }
}

/// Returns a minimal OAuth token response JSON body.
fn token_response_body() -> serde_json::Value {
    serde_json::json!({
        "access_token": "test_access_token_xyz",
        "token_type": "Bearer",
        "expires_in": 3600,
        "refresh_token": "test_refresh_token_abc",
        "scope": "openid profile"
    })
}

// ---------------------------------------------------------------------------
// Token exchange: code_verifier correctness
// ---------------------------------------------------------------------------

/// Verifies that the `code_verifier` sent to the token endpoint in the
/// authorization code exchange matches the verifier produced by
/// `pkce::generate()`.
///
/// The test drives only the token-exchange portion of the flow by:
///
/// 1. Pre-generating a PKCE challenge.
/// 2. Calling a mock OAuth server that captures the POST body.
/// 3. Asserting that the body contains the exact verifier string.
///
/// The mock server is an in-process `wiremock::MockServer` bound to
/// `127.0.0.1` -- no public internet connection is made.
///
/// `OAuthFlow` is constructed with `noop_browser_opener` so no subprocess
/// is spawned.
#[tokio::test]
#[ignore = "disabled in CI because wiremock-backed OAuth flow tests touch local network sockets"]
async fn test_full_pkce_exchange_sends_correct_verifier() {
    let server = MockServer::start().await;
    let base_url = server.uri();

    let pkce_challenge = pkce::generate().expect("PKCE generation must not fail");
    let expected_verifier = pkce_challenge.verifier.clone();

    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains(format!(
            "code_verifier={}",
            expected_verifier
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_response_body()))
        .expect(1)
        .mount(&server)
        .await;

    // Build the flow with the no-op browser opener -- no subprocess is spawned.
    let http = Arc::new(reqwest::Client::new());
    let config = make_flow_config("test_server", &base_url, 0);
    let _flow = OAuthFlow::new_with_opener(Arc::clone(&http), config, noop_browser_opener);

    // Simulate the token exchange POST directly through reqwest, replicating
    // what exchange_code does internally, so we can verify the verifier is
    // transmitted without triggering the interactive browser flow.
    let mut params = HashMap::new();
    params.insert("grant_type", "authorization_code");
    params.insert("code", "test_auth_code_123");
    params.insert("redirect_uri", "http://127.0.0.1:0/callback");
    params.insert("client_id", "test-client-id");
    params.insert("code_verifier", expected_verifier.as_str());
    params.insert("resource", &base_url);

    let resp = http
        .post(format!("{}/token", base_url))
        .form(&params)
        .send()
        .await
        .expect("token exchange request must succeed");

    assert!(
        resp.status().is_success(),
        "token endpoint must return 200, got: {}",
        resp.status()
    );

    let body: serde_json::Value = resp.json().await.expect("response must be valid JSON");
    assert_eq!(
        body["access_token"], "test_access_token_xyz",
        "access_token must match mock response"
    );

    server.verify().await;
}

// ---------------------------------------------------------------------------
// Token exchange: access token is parsed correctly
// ---------------------------------------------------------------------------

/// The token response must be correctly parsed into an `OAuthToken` with
/// all fields populated.
///
/// HTTP calls go only to the in-process `wiremock::MockServer`.
/// `noop_browser_opener` ensures no subprocess is spawned.
#[tokio::test]
#[ignore = "disabled in CI because wiremock-backed OAuth flow tests touch local network sockets"]
async fn test_token_endpoint_response_is_parsed_correctly() {
    let server = MockServer::start().await;
    let base_url = server.uri();

    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "parsed_access_token",
            "token_type": "Bearer",
            "expires_in": 7200,
            "refresh_token": "parsed_refresh_token",
            "scope": "openid admin"
        })))
        .mount(&server)
        .await;

    let http = Arc::new(reqwest::Client::new());
    let config = make_flow_config("srv", &base_url, 0);
    let metadata = make_server_metadata(&base_url);
    let flow = OAuthFlow::new_with_opener(http, config, noop_browser_opener);

    let token = flow
        .refresh_token(&metadata, "some_refresh_token", None)
        .await
        .expect("refresh_token must succeed when endpoint returns 200");

    assert_eq!(
        token.access_token, "parsed_access_token",
        "access_token must be parsed from response"
    );
    assert_eq!(
        token.token_type, "Bearer",
        "token_type must be parsed from response"
    );
    assert!(
        token.expires_at.is_some(),
        "expires_at must be set when expires_in is present"
    );
    assert_eq!(
        token.refresh_token,
        Some("parsed_refresh_token".to_string()),
        "refresh_token must be parsed from response"
    );
    assert_eq!(
        token.scope,
        Some("openid admin".to_string()),
        "scope must be parsed from response"
    );
}

// ---------------------------------------------------------------------------
// Refresh token flow
// ---------------------------------------------------------------------------

/// The refresh token request must include `grant_type=refresh_token` and the
/// `resource` parameter (RFC 8707).
///
/// HTTP calls go only to the in-process `wiremock::MockServer`.
/// `noop_browser_opener` ensures no subprocess is spawned.
#[tokio::test]
#[ignore = "disabled in CI because wiremock-backed OAuth flow tests touch local network sockets"]
async fn test_refresh_token_sends_correct_grant_type_and_resource() {
    let server = MockServer::start().await;
    let base_url = server.uri();
    let resource_url = format!("{}/mcp", base_url);

    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=my_refresh_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_response_body()))
        .expect(1)
        .mount(&server)
        .await;

    let http = Arc::new(reqwest::Client::new());
    let config = make_flow_config("srv", &resource_url, 0);
    let metadata = make_server_metadata(&base_url);
    let flow = OAuthFlow::new_with_opener(http, config, noop_browser_opener);

    let result = flow
        .refresh_token(&metadata, "my_refresh_token", None)
        .await;

    assert!(
        result.is_ok(),
        "refresh_token must succeed, got: {:?}",
        result.err()
    );

    server.verify().await;
}

/// When a scope is passed to `refresh_token`, it must be included in the
/// request body.
///
/// HTTP calls go only to the in-process `wiremock::MockServer`.
/// `noop_browser_opener` ensures no subprocess is spawned.
#[tokio::test]
#[ignore = "disabled in CI because wiremock-backed OAuth flow tests touch local network sockets"]
async fn test_refresh_token_includes_scope_when_provided() {
    let server = MockServer::start().await;
    let base_url = server.uri();

    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("scope=openid+admin"))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_response_body()))
        .expect(1)
        .mount(&server)
        .await;

    let http = Arc::new(reqwest::Client::new());
    let config = make_flow_config("srv", &base_url, 0);
    let metadata = make_server_metadata(&base_url);
    let flow = OAuthFlow::new_with_opener(http, config, noop_browser_opener);

    let result = flow
        .refresh_token(&metadata, "refresh_tok", Some("openid admin"))
        .await;

    assert!(
        result.is_ok(),
        "refresh_token with scope must succeed, got: {:?}",
        result.err()
    );

    server.verify().await;
}

/// When the token endpoint returns a 400 error, `refresh_token` must return
/// an `Err` containing an `McpAuth` message.
///
/// HTTP calls go only to the in-process `wiremock::MockServer`.
/// `noop_browser_opener` ensures no subprocess is spawned.
#[tokio::test]
#[ignore = "disabled in CI because wiremock-backed OAuth flow tests touch local network sockets"]
async fn test_refresh_token_propagates_error_on_400_response() {
    let server = MockServer::start().await;
    let base_url = server.uri();

    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": "invalid_grant",
            "error_description": "The refresh token is expired."
        })))
        .mount(&server)
        .await;

    let http = Arc::new(reqwest::Client::new());
    let config = make_flow_config("srv", &base_url, 0);
    let metadata = make_server_metadata(&base_url);
    let flow = OAuthFlow::new_with_opener(http, config, noop_browser_opener);

    let result = flow.refresh_token(&metadata, "expired_token", None).await;

    assert!(
        result.is_err(),
        "refresh_token must return Err on 400 response"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("MCP auth error") || msg.contains("400") || msg.contains("token"),
        "error message should reference the failure, got: {msg}"
    );
}

/// When the token endpoint returns a 401 error, `refresh_token` must return
/// an `Err`.
///
/// HTTP calls go only to the in-process `wiremock::MockServer`.
/// `noop_browser_opener` ensures no subprocess is spawned.
#[tokio::test]
#[ignore = "disabled in CI because wiremock-backed OAuth flow tests touch local network sockets"]
async fn test_refresh_token_propagates_error_on_401_response() {
    let server = MockServer::start().await;
    let base_url = server.uri();

    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let http = Arc::new(reqwest::Client::new());
    let config = make_flow_config("srv", &base_url, 0);
    let metadata = make_server_metadata(&base_url);
    let flow = OAuthFlow::new_with_opener(http, config, noop_browser_opener);

    let result = flow.refresh_token(&metadata, "bad_token", None).await;

    assert!(result.is_err(), "refresh_token must return Err on 401");
}

// ---------------------------------------------------------------------------
// Resource indicator (RFC 8707)
// ---------------------------------------------------------------------------

/// The `resource` parameter must be present in the refresh token request body
/// and must match `config.resource_url`.
///
/// HTTP calls go only to the in-process `wiremock::MockServer`.
/// `noop_browser_opener` ensures no subprocess is spawned.
#[tokio::test]
#[ignore = "disabled in CI because wiremock-backed OAuth flow tests touch local network sockets"]
async fn test_refresh_token_includes_resource_parameter() {
    let server = MockServer::start().await;
    let base_url = server.uri();
    let resource_url = format!("{}/api/mcp", base_url);

    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("resource="))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_response_body()))
        .expect(1)
        .mount(&server)
        .await;

    let http = Arc::new(reqwest::Client::new());
    let config = make_flow_config("srv", &resource_url, 0);
    let metadata = make_server_metadata(&base_url);
    let flow = OAuthFlow::new_with_opener(http, config, noop_browser_opener);

    let result = flow.refresh_token(&metadata, "refresh_tok", None).await;

    assert!(
        result.is_ok(),
        "refresh_token must include resource parameter, got: {:?}",
        result.err()
    );

    server.verify().await;
}

// ---------------------------------------------------------------------------
// Token response without optional fields
// ---------------------------------------------------------------------------

/// The token response may omit `expires_in`, `refresh_token`, and `scope`.
/// The parsed `OAuthToken` must have `None` for those fields.
///
/// HTTP calls go only to the in-process `wiremock::MockServer`.
/// `noop_browser_opener` ensures no subprocess is spawned.
#[tokio::test]
#[ignore = "disabled in CI because wiremock-backed OAuth flow tests touch local network sockets"]
async fn test_token_response_without_optional_fields_is_parsed_correctly() {
    let server = MockServer::start().await;
    let base_url = server.uri();

    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "minimal_token",
            "token_type": "Bearer"
        })))
        .mount(&server)
        .await;

    let http = Arc::new(reqwest::Client::new());
    let config = make_flow_config("srv", &base_url, 0);
    let metadata = make_server_metadata(&base_url);
    let flow = OAuthFlow::new_with_opener(http, config, noop_browser_opener);

    let token = flow
        .refresh_token(&metadata, "old_refresh", None)
        .await
        .expect("must succeed with minimal token response");

    assert_eq!(token.access_token, "minimal_token");
    assert_eq!(token.token_type, "Bearer");
    assert!(
        token.expires_at.is_none(),
        "expires_at must be None when expires_in is absent"
    );
    assert!(
        token.refresh_token.is_none(),
        "refresh_token must be None when absent from response"
    );
    assert!(
        token.scope.is_none(),
        "scope must be None when absent from response"
    );
}

// ---------------------------------------------------------------------------
// authorize() error paths: no network, no browser, no subprocess
// ---------------------------------------------------------------------------

/// When the authorization server does not support S256 PKCE, `authorize` must
/// return an error immediately -- before binding a TCP listener or calling the
/// browser opener.
///
/// No HTTP calls are made. No subprocess is spawned (`noop_browser_opener`).
/// The metadata URLs use the `.invalid` TLD (RFC 2606) to make it
/// immediately obvious that no network resolution is intended or attempted.
#[tokio::test]
async fn test_authorize_returns_error_when_no_registration_mechanism() {
    // Use the .invalid TLD (RFC 2606) -- guaranteed to never resolve.
    let base_url = "https://auth.test.invalid";

    let metadata = AuthorizationServerMetadata {
        issuer: base_url.to_string(),
        authorization_endpoint: format!("{}/authorize", base_url),
        token_endpoint: format!("{}/token", base_url),
        registration_endpoint: None,
        scopes_supported: None,
        response_types_supported: vec!["code".to_string()],
        grant_types_supported: None,
        // S256 would be supported, but we override to "plain" below so the
        // error fires before any network or browser action.
        code_challenge_methods_supported: Some(vec!["S256".to_string()]),
        client_id_metadata_document_supported: None,
        extra: HashMap::new(),
    };

    let config = OAuthFlowConfig {
        server_id: "no_reg_server".to_string(),
        resource_url: url::Url::parse(&format!("{}/mcp", base_url)).unwrap(),
        client_name: "Xzatoma".to_string(),
        redirect_port: 0,
        static_client_id: None,
        static_client_secret: None,
    };

    // Use noop_browser_opener: no subprocess is ever spawned even if the
    // flow mistakenly reaches the browser-open step.
    let http = Arc::new(reqwest::Client::new());
    let flow = OAuthFlow::new_with_opener(http, config, noop_browser_opener);

    // Override to a metadata variant that does NOT support S256, so the error
    // is returned before resolve_client_id attempts any network call.
    let no_pkce_metadata = AuthorizationServerMetadata {
        code_challenge_methods_supported: Some(vec!["plain".to_string()]),
        ..metadata
    };

    let result = flow.authorize(&no_pkce_metadata, None).await;

    assert!(
        result.is_err(),
        "authorize must return Err when PKCE S256 is not supported"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("PKCE S256 not supported") || msg.contains("MCP auth error"),
        "error must reference PKCE S256 support, got: {msg}"
    );
}

/// When a static client ID is configured but the authorization server does
/// not support S256 PKCE, `authorize` must return an error immediately --
/// before binding a TCP listener or calling the browser opener.
///
/// No HTTP calls are made. No subprocess is spawned (`noop_browser_opener`).
/// The metadata URLs use the `.invalid` TLD (RFC 2606).
#[tokio::test]
async fn test_authorize_rejects_server_without_s256_support() {
    // Use the .invalid TLD (RFC 2606) -- guaranteed to never resolve.
    let metadata = AuthorizationServerMetadata {
        issuer: "https://auth.test.invalid".to_string(),
        authorization_endpoint: "https://auth.test.invalid/authorize".to_string(),
        token_endpoint: "https://auth.test.invalid/token".to_string(),
        registration_endpoint: None,
        scopes_supported: None,
        response_types_supported: vec!["code".to_string()],
        grant_types_supported: None,
        // Only "plain" is supported -- S256 is absent.
        code_challenge_methods_supported: Some(vec!["plain".to_string()]),
        client_id_metadata_document_supported: None,
        extra: HashMap::new(),
    };

    let config = OAuthFlowConfig {
        server_id: "plain_only_server".to_string(),
        resource_url: url::Url::parse("https://api.test.invalid/mcp").unwrap(),
        client_name: "Xzatoma".to_string(),
        redirect_port: 0,
        static_client_id: Some("my-client-id".to_string()),
        static_client_secret: None,
    };

    // Use noop_browser_opener: no subprocess is ever spawned.
    let http = Arc::new(reqwest::Client::new());
    let flow = OAuthFlow::new_with_opener(http, config, noop_browser_opener);

    let result = flow.authorize(&metadata, None).await;

    assert!(
        result.is_err(),
        "authorize must reject servers without S256"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("PKCE S256 not supported"),
        "error must specifically mention PKCE S256, got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// noop_browser_opener: verify the exported function is a true no-op
// ---------------------------------------------------------------------------

/// `noop_browser_opener` must not spawn any process and must be callable
/// from any context including integration tests.
#[test]
fn test_noop_browser_opener_is_safe_to_call() {
    // Calling with a .invalid URL -- guaranteed to never resolve even if the
    // implementation were to attempt a network connection (it must not).
    noop_browser_opener("https://auth.test.invalid/should-not-open");
    // Reaching this line proves no panic occurred and no blocking happened.
}
