//! MCP OAuth discovery integration tests using wiremock
//!
//! Verifies the behaviour of `src/mcp/auth/discovery.rs`:
//!
//! - `fetch_protected_resource_metadata` parses the metadata URL from a
//!   `WWW-Authenticate` header and fetches it directly.
//! - `fetch_protected_resource_metadata` falls back to the RFC 9728 well-known
//!   URI when no header is present.
//! - `fetch_authorization_server_metadata` tries all five well-known endpoint
//!   orderings and succeeds on the fifth after four 404 responses.

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use xzatoma::mcp::auth::discovery::{
    fetch_authorization_server_metadata, fetch_protected_resource_metadata,
    AuthorizationServerMetadata, ProtectedResourceMetadata,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns a minimal valid [`ProtectedResourceMetadata`] JSON body whose
/// `resource` and `authorization_servers` fields reference `base_url`.
fn protected_resource_body(base_url: &str) -> serde_json::Value {
    serde_json::json!({
        "resource": base_url,
        "authorization_servers": [base_url]
    })
}

/// Returns a minimal valid [`AuthorizationServerMetadata`] JSON body.
fn authorization_server_body(base_url: &str) -> serde_json::Value {
    serde_json::json!({
        "issuer": base_url,
        "authorization_endpoint": format!("{}/authorize", base_url),
        "token_endpoint": format!("{}/token", base_url),
        "response_types_supported": ["code"],
        "code_challenge_methods_supported": ["S256"]
    })
}

// ---------------------------------------------------------------------------
// fetch_protected_resource_metadata
// ---------------------------------------------------------------------------

/// When the `WWW-Authenticate` header contains a `resource_metadata=<url>`
/// attribute, the function must fetch that URL directly and parse it.
#[tokio::test]
async fn test_fetch_protected_resource_metadata_from_www_authenticate_header() {
    let server = MockServer::start().await;
    let base_url = server.uri();

    // Serve the metadata document at a specific path.
    Mock::given(method("GET"))
        .and(path("/.well-known/oauth-protected-resource-header"))
        .respond_with(ResponseTemplate::new(200).set_body_json(protected_resource_body(&base_url)))
        .mount(&server)
        .await;

    let metadata_url = format!("{}/.well-known/oauth-protected-resource-header", base_url);
    let www_authenticate = format!(
        r#"Bearer realm="example", resource_metadata="{}" "#,
        metadata_url
    );

    let http = reqwest::Client::new();
    let resource_url = url::Url::parse(&base_url).unwrap();

    let result =
        fetch_protected_resource_metadata(&http, &resource_url, Some(&www_authenticate)).await;

    assert!(
        result.is_ok(),
        "must succeed when metadata URL is in WWW-Authenticate header, got: {:?}",
        result.err()
    );
    let meta = result.unwrap();
    assert_eq!(
        meta.resource, base_url,
        "resource field must match the mock response"
    );
    assert_eq!(
        meta.authorization_servers.len(),
        1,
        "authorization_servers must have one entry"
    );
}

/// When no `WWW-Authenticate` header is provided, the function must fall back
/// to constructing the RFC 9728 well-known URI and fetching it.
#[tokio::test]
async fn test_fetch_protected_resource_metadata_falls_back_to_well_known() {
    let server = MockServer::start().await;
    let base_url = server.uri();

    // Serve at the well-known path (root resource URL, so path is "/").
    Mock::given(method("GET"))
        .and(path("/.well-known/oauth-protected-resource"))
        .respond_with(ResponseTemplate::new(200).set_body_json(protected_resource_body(&base_url)))
        .mount(&server)
        .await;

    let http = reqwest::Client::new();
    let resource_url = url::Url::parse(&base_url).unwrap();

    let result = fetch_protected_resource_metadata(&http, &resource_url, None).await;

    assert!(
        result.is_ok(),
        "must succeed when well-known URI responds with 200, got: {:?}",
        result.err()
    );
    let meta = result.unwrap();
    assert_eq!(meta.resource, base_url);
}

/// When the `WWW-Authenticate` header contains a `resource_metadata` URL but
/// that URL returns a non-success status, the function must fall back to the
/// RFC 9728 well-known URI.
#[tokio::test]
async fn test_fetch_protected_resource_metadata_falls_back_when_header_url_404() {
    let server = MockServer::start().await;
    let base_url = server.uri();

    // The header URL returns 404.
    Mock::given(method("GET"))
        .and(path("/.well-known/oauth-protected-resource-missing"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    // The well-known fallback returns 200.
    Mock::given(method("GET"))
        .and(path("/.well-known/oauth-protected-resource"))
        .respond_with(ResponseTemplate::new(200).set_body_json(protected_resource_body(&base_url)))
        .mount(&server)
        .await;

    let header_url = format!("{}/.well-known/oauth-protected-resource-missing", base_url);
    let www_authenticate = format!(r#"Bearer resource_metadata="{}""#, header_url);

    let http = reqwest::Client::new();
    let resource_url = url::Url::parse(&base_url).unwrap();

    let result =
        fetch_protected_resource_metadata(&http, &resource_url, Some(&www_authenticate)).await;

    assert!(
        result.is_ok(),
        "must fall back to well-known URI when header URL returns 404, got: {:?}",
        result.err()
    );
}

/// When both the `WWW-Authenticate` URL and the well-known fallback return
/// non-success responses, the function must return an error.
#[tokio::test]
async fn test_fetch_protected_resource_metadata_returns_error_when_all_strategies_fail() {
    let server = MockServer::start().await;
    let base_url = server.uri();

    // All paths return 404 -- nothing is mounted.
    let http = reqwest::Client::new();
    let resource_url = url::Url::parse(&base_url).unwrap();

    let result = fetch_protected_resource_metadata(&http, &resource_url, None).await;

    assert!(
        result.is_err(),
        "must return Err when no discovery strategy succeeds"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("MCP auth error") || msg.contains("not found") || msg.contains("metadata"),
        "error message should reference the failure, got: {msg}"
    );
}

/// When the resource URL contains a non-root path, the well-known URI must
/// include that path (RFC 9728 path-insertion rule).
#[tokio::test]
async fn test_fetch_protected_resource_metadata_uses_path_insertion_for_sub_resource() {
    let server = MockServer::start().await;
    let base_url = server.uri();

    // The resource is at /api/v2; well-known path must be
    // /.well-known/oauth-protected-resource/api/v2.
    Mock::given(method("GET"))
        .and(path("/.well-known/oauth-protected-resource/api/v2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resource": format!("{}/api/v2", base_url),
            "authorization_servers": [base_url]
        })))
        .mount(&server)
        .await;

    let http = reqwest::Client::new();
    let resource_url = url::Url::parse(&format!("{}/api/v2", base_url)).unwrap();

    let result = fetch_protected_resource_metadata(&http, &resource_url, None).await;

    assert!(
        result.is_ok(),
        "must fetch well-known URI with path insertion for sub-resource, got: {:?}",
        result.err()
    );
}

// ---------------------------------------------------------------------------
// fetch_authorization_server_metadata
// ---------------------------------------------------------------------------

/// The function must try all five well-known endpoint orderings and succeed
/// when only the fifth returns a 200 response.
///
/// Ordering (for a root issuer):
///   1. /.well-known/oauth-authorization-server        (path-inserted, empty path)
///   2. /.well-known/openid-configuration              (path-inserted, empty path)
///   3. <issuer>/.well-known/openid-configuration      (path-appended)
///   4. /.well-known/oauth-authorization-server        (root, no path)
///   5. /.well-known/openid-configuration              (root, no path)
///
/// For a root issuer (path = "/") candidates 1, 2, 4, and 5 collapse to the
/// same two paths, so we use an issuer with a non-trivial path to force all
/// five to be distinct.
#[tokio::test]
async fn test_fetch_authorization_server_metadata_tries_five_orderings() {
    let server = MockServer::start().await;
    let base_url = server.uri();

    // Use an issuer with a path so that path-inserted and root URLs differ.
    let issuer_url = url::Url::parse(&format!("{}/tenant/v2", base_url)).unwrap();

    // The first four candidates return 404; only the fifth succeeds.
    // Candidate 1: /.well-known/oauth-authorization-server/tenant/v2
    Mock::given(method("GET"))
        .and(path("/.well-known/oauth-authorization-server/tenant/v2"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    // Candidate 2: /.well-known/openid-configuration/tenant/v2
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration/tenant/v2"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    // Candidate 3: /tenant/v2/.well-known/openid-configuration
    Mock::given(method("GET"))
        .and(path("/tenant/v2/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    // Candidate 4: /.well-known/oauth-authorization-server
    Mock::given(method("GET"))
        .and(path("/.well-known/oauth-authorization-server"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    // Candidate 5: /.well-known/openid-configuration  -- SUCCESS
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(authorization_server_body(&base_url)),
        )
        .mount(&server)
        .await;

    let http = reqwest::Client::new();
    let result = fetch_authorization_server_metadata(&http, &issuer_url).await;

    assert!(
        result.is_ok(),
        "must succeed when fifth ordering returns 200, got: {:?}",
        result.err()
    );
    let meta = result.unwrap();
    assert!(
        meta.token_endpoint.contains("/token"),
        "token_endpoint must be present in response: {}",
        meta.token_endpoint
    );
}

/// When all five orderings return non-success responses, the function must
/// return an error.
#[tokio::test]
async fn test_fetch_authorization_server_metadata_returns_error_when_all_fail() {
    let server = MockServer::start().await;
    let base_url = server.uri();

    // No mocks mounted -- all requests return 404 (wiremock default).
    let http = reqwest::Client::new();
    let issuer_url = url::Url::parse(&base_url).unwrap();

    let result = fetch_authorization_server_metadata(&http, &issuer_url).await;

    assert!(
        result.is_err(),
        "must return Err when all five orderings fail"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("MCP auth error") || msg.contains("not found") || msg.contains("metadata"),
        "error should reference discovery failure, got: {msg}"
    );
}

/// When the first ordering succeeds, the function must return without trying
/// the remaining four.
#[tokio::test]
async fn test_fetch_authorization_server_metadata_succeeds_on_first_ordering() {
    let server = MockServer::start().await;
    let base_url = server.uri();

    // Candidate 1 for a root issuer: /.well-known/oauth-authorization-server
    Mock::given(method("GET"))
        .and(path("/.well-known/oauth-authorization-server"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(authorization_server_body(&base_url)),
        )
        .mount(&server)
        .await;

    let http = reqwest::Client::new();
    let issuer_url = url::Url::parse(&base_url).unwrap();

    let result = fetch_authorization_server_metadata(&http, &issuer_url).await;

    assert!(
        result.is_ok(),
        "must succeed immediately on first ordering, got: {:?}",
        result.err()
    );
    let meta = result.unwrap();
    assert!(
        !meta.authorization_endpoint.is_empty(),
        "authorization_endpoint must be populated"
    );
}

/// When the second ordering succeeds, the function returns the parsed
/// metadata correctly.
#[tokio::test]
async fn test_fetch_authorization_server_metadata_succeeds_on_second_ordering() {
    let server = MockServer::start().await;
    let base_url = server.uri();

    let issuer_url = url::Url::parse(&format!("{}/realm/myrealm", base_url)).unwrap();

    // Candidate 1: /.well-known/oauth-authorization-server/realm/myrealm  -> 404
    Mock::given(method("GET"))
        .and(path(
            "/.well-known/oauth-authorization-server/realm/myrealm",
        ))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    // Candidate 2: /.well-known/openid-configuration/realm/myrealm  -> 200
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration/realm/myrealm"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(authorization_server_body(&base_url)),
        )
        .mount(&server)
        .await;

    let http = reqwest::Client::new();
    let result = fetch_authorization_server_metadata(&http, &issuer_url).await;

    assert!(
        result.is_ok(),
        "must succeed on second ordering, got: {:?}",
        result.err()
    );
}

/// The parsed [`AuthorizationServerMetadata`] must include an extra field
/// captured by the `#[serde(flatten)]` map when the server returns unknown
/// fields.
#[tokio::test]
async fn test_fetch_authorization_server_metadata_captures_extra_fields() {
    let server = MockServer::start().await;
    let base_url = server.uri();

    let mut body = authorization_server_body(&base_url);
    body["custom_extension_field"] = serde_json::json!("custom_value");

    Mock::given(method("GET"))
        .and(path("/.well-known/oauth-authorization-server"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;

    let http = reqwest::Client::new();
    let issuer_url = url::Url::parse(&base_url).unwrap();

    let result = fetch_authorization_server_metadata(&http, &issuer_url).await;

    assert!(result.is_ok());
    let meta = result.unwrap();
    assert!(
        meta.extra.contains_key("custom_extension_field"),
        "extra fields must be captured in the `extra` map"
    );
    assert_eq!(
        meta.extra["custom_extension_field"],
        serde_json::Value::String("custom_value".to_string())
    );
}

// ---------------------------------------------------------------------------
// Serde round-trips (no network required)
// ---------------------------------------------------------------------------

/// [`ProtectedResourceMetadata`] must deserialise from minimal JSON.
#[test]
fn test_protected_resource_metadata_deserializes_minimal_json() {
    let json = r#"{
        "resource": "https://api.example.com",
        "authorization_servers": ["https://auth.example.com"]
    }"#;

    let meta: ProtectedResourceMetadata = serde_json::from_str(json).expect("must deserialize");
    assert_eq!(meta.resource, "https://api.example.com");
    assert_eq!(meta.authorization_servers.len(), 1);
    assert!(meta.scopes_supported.is_none());
    assert!(meta.bearer_methods_supported.is_none());
}

/// [`ProtectedResourceMetadata`] must deserialise all optional fields when
/// they are present.
#[test]
fn test_protected_resource_metadata_deserializes_all_fields() {
    let json = r#"{
        "resource": "https://api.example.com",
        "authorization_servers": ["https://auth.example.com"],
        "scopes_supported": ["openid", "profile"],
        "bearer_methods_supported": ["header"]
    }"#;

    let meta: ProtectedResourceMetadata = serde_json::from_str(json).expect("must deserialize");
    assert_eq!(
        meta.scopes_supported,
        Some(vec!["openid".to_string(), "profile".to_string()])
    );
    assert_eq!(
        meta.bearer_methods_supported,
        Some(vec!["header".to_string()])
    );
}

/// [`AuthorizationServerMetadata`] must deserialise from minimal JSON.
#[test]
fn test_authorization_server_metadata_deserializes_minimal_json() {
    let json = r#"{
        "issuer": "https://auth.example.com",
        "authorization_endpoint": "https://auth.example.com/authorize",
        "token_endpoint": "https://auth.example.com/token",
        "response_types_supported": ["code"]
    }"#;

    let meta: AuthorizationServerMetadata = serde_json::from_str(json).expect("must deserialize");
    assert_eq!(meta.issuer, "https://auth.example.com");
    assert!(meta.registration_endpoint.is_none());
    assert!(meta.code_challenge_methods_supported.is_none());
    assert!(meta.extra.is_empty());
}

/// [`AuthorizationServerMetadata`] with all fields present must deserialise
/// correctly.
#[test]
fn test_authorization_server_metadata_deserializes_full_json() {
    let json = r#"{
        "issuer": "https://auth.example.com",
        "authorization_endpoint": "https://auth.example.com/authorize",
        "token_endpoint": "https://auth.example.com/token",
        "registration_endpoint": "https://auth.example.com/register",
        "scopes_supported": ["openid", "profile"],
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "client_id_metadata_document_supported": true
    }"#;

    let meta: AuthorizationServerMetadata = serde_json::from_str(json).expect("must deserialize");
    assert_eq!(
        meta.registration_endpoint,
        Some("https://auth.example.com/register".to_string())
    );
    assert_eq!(
        meta.code_challenge_methods_supported,
        Some(vec!["S256".to_string()])
    );
    assert_eq!(meta.client_id_metadata_document_supported, Some(true));
}

/// Unknown fields in the authorization server metadata response must be
/// captured in the `extra` map rather than causing a deserialisation error.
#[test]
fn test_authorization_server_metadata_extra_fields_do_not_fail_deserialization() {
    let json = r#"{
        "issuer": "https://auth.example.com",
        "authorization_endpoint": "https://auth.example.com/authorize",
        "token_endpoint": "https://auth.example.com/token",
        "response_types_supported": ["code"],
        "unknown_future_field": true,
        "another_custom_field": 42
    }"#;

    let meta: AuthorizationServerMetadata =
        serde_json::from_str(json).expect("unknown fields must not cause an error");
    assert!(
        meta.extra.contains_key("unknown_future_field"),
        "unknown_future_field must be in extra map"
    );
    assert!(
        meta.extra.contains_key("another_custom_field"),
        "another_custom_field must be in extra map"
    );
}
