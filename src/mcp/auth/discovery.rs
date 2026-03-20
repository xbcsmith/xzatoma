//! OAuth 2.1 / OIDC discovery for MCP HTTP transport
//!
//! This module implements RFC 9728 Protected Resource Metadata discovery and
//! RFC 8414 / OpenID Connect Discovery to locate authorization server
//! endpoints before running the authorization code flow.
//!
//! # Discovery sequence
//!
//! 1. The MCP client issues an unauthenticated request to the resource server.
//! 2. The server responds with `401 Unauthorized` and a `WWW-Authenticate`
//!    header that may contain a `resource_metadata` attribute pointing to the
//!    protected resource metadata document.
//! 3. [`fetch_protected_resource_metadata`] retrieves that document (or falls
//!    back to the RFC 9728 well-known URI).
//! 4. The document lists one or more authorization servers; the client picks
//!    the first one and calls [`fetch_authorization_server_metadata`].
//! 5. [`fetch_authorization_server_metadata`] tries five well-known endpoint
//!    orderings defined by RFC 8414 and OpenID Connect Discovery 1.0.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{Result, XzatomaError};

// ---------------------------------------------------------------------------
// Protected Resource Metadata (RFC 9728)
// ---------------------------------------------------------------------------

/// Metadata document describing a protected OAuth 2.1 resource.
///
/// Retrieved from the well-known URI
/// `/.well-known/oauth-protected-resource<path>` or from the URL embedded in
/// a `WWW-Authenticate: Bearer resource_metadata=<url>` challenge header.
///
/// # References
///
/// - RFC 9728 <https://www.rfc-editor.org/rfc/rfc9728>
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::auth::discovery::ProtectedResourceMetadata;
///
/// let json = r#"{
///     "resource": "https://api.example.com",
///     "authorization_servers": ["https://auth.example.com"]
/// }"#;
///
/// let meta: ProtectedResourceMetadata = serde_json::from_str(json).unwrap();
/// assert_eq!(meta.resource, "https://api.example.com");
/// assert_eq!(meta.authorization_servers.len(), 1);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProtectedResourceMetadata {
    /// The URI of the protected resource itself.
    pub resource: String,

    /// List of authorization server issuer URIs that protect this resource.
    pub authorization_servers: Vec<String>,

    /// OAuth scopes supported by this resource, if advertised.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scopes_supported: Option<Vec<String>>,

    /// Supported methods for presenting bearer tokens (e.g. `"header"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bearer_methods_supported: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// Authorization Server Metadata (RFC 8414 / OIDC Discovery)
// ---------------------------------------------------------------------------

/// Metadata document describing an OAuth 2.1 / OIDC authorization server.
///
/// Retrieved from one of several well-known URIs tried in order by
/// [`fetch_authorization_server_metadata`].
///
/// # References
///
/// - RFC 8414 <https://www.rfc-editor.org/rfc/rfc8414>
/// - OpenID Connect Discovery 1.0 <https://openid.net/specs/openid-connect-discovery-1_0.html>
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::auth::discovery::AuthorizationServerMetadata;
///
/// let json = r#"{
///     "issuer": "https://auth.example.com",
///     "authorization_endpoint": "https://auth.example.com/authorize",
///     "token_endpoint": "https://auth.example.com/token",
///     "response_types_supported": ["code"]
/// }"#;
///
/// let meta: AuthorizationServerMetadata = serde_json::from_str(json).unwrap();
/// assert_eq!(meta.issuer, "https://auth.example.com");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AuthorizationServerMetadata {
    /// The issuer identifier URI for this authorization server.
    pub issuer: String,

    /// The URL of the authorization endpoint (RFC 6749 section 3.1).
    pub authorization_endpoint: String,

    /// The URL of the token endpoint (RFC 6749 section 3.2).
    pub token_endpoint: String,

    /// Optional URL of the Dynamic Client Registration endpoint (RFC 7591).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registration_endpoint: Option<String>,

    /// List of OAuth scopes the server supports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scopes_supported: Option<Vec<String>>,

    /// List of `response_type` values the server supports (e.g. `["code"]`).
    pub response_types_supported: Vec<String>,

    /// List of `grant_type` values the server supports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant_types_supported: Option<Vec<String>>,

    /// PKCE challenge methods the server supports (e.g. `["S256"]`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_challenge_methods_supported: Option<Vec<String>>,

    /// Whether the server supports `client_id_metadata_document` (MCP
    /// extension allowing the client to use its own metadata URL as the
    /// `client_id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id_metadata_document_supported: Option<bool>,

    /// Additional server metadata fields not explicitly modelled above.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Client ID Metadata Document (MCP extension)
// ---------------------------------------------------------------------------

/// A client identity document served at a stable URL that can be used
/// directly as the `client_id` value (MCP 2025-11-25 extension).
///
/// When `server_metadata.client_id_metadata_document_supported == Some(true)`
/// the client POSTs neither a static client ID nor performs dynamic
/// registration; instead it presents the URL at which this document is
/// hosted as the `client_id` parameter in all OAuth requests.
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::auth::discovery::ClientIdMetadataDocument;
///
/// let json = r#"{
///     "client_id": "https://xzatoma.example.com/.well-known/client-metadata",
///     "client_name": "Xzatoma",
///     "redirect_uris": ["http://127.0.0.1:0/callback"]
/// }"#;
///
/// let doc: ClientIdMetadataDocument = serde_json::from_str(json).unwrap();
/// assert_eq!(doc.client_name, "Xzatoma");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientIdMetadataDocument {
    /// The client identifier (the URL of this document).
    pub client_id: String,

    /// Human-readable name for this client application.
    pub client_name: String,

    /// URI of the client's homepage, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_uri: Option<String>,

    /// List of redirect URIs registered for this client.
    pub redirect_uris: Vec<String>,

    /// Grant types this client supports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant_types: Option<Vec<String>>,

    /// Response types this client supports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_types: Option<Vec<String>>,

    /// Token endpoint authentication method (e.g. `"none"` for public
    /// clients).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_endpoint_auth_method: Option<String>,
}

// ---------------------------------------------------------------------------
// Discovery helpers
// ---------------------------------------------------------------------------

/// Parses the `resource_metadata` attribute value from a `WWW-Authenticate`
/// header string.
///
/// Returns `Some(url_string)` when the attribute is present, `None`
/// otherwise.
fn parse_resource_metadata_url(www_authenticate: &str) -> Option<String> {
    // Look for resource_metadata="<url>" or resource_metadata=<url>
    let key = "resource_metadata=";
    let pos = www_authenticate.find(key)?;
    let rest = &www_authenticate[pos + key.len()..];

    if let Some(inner) = rest.strip_prefix('"') {
        // Quoted string -- extract up to the closing quote.
        let end = inner.find('"')?;
        Some(inner[..end].to_string())
    } else {
        // Unquoted -- extract up to the next whitespace or comma.
        let end = rest
            .find(|c: char| c.is_whitespace() || c == ',')
            .unwrap_or(rest.len());
        Some(rest[..end].to_string())
    }
}

/// Fetches the RFC 9728 Protected Resource Metadata document for a resource.
///
/// The function first checks the optional `www_authenticate` header for a
/// `resource_metadata=<url>` attribute and GETs that URL directly.  If no
/// such attribute is present it constructs the RFC 9728 well-known URI:
///
/// ```text
/// https://<host>/.well-known/oauth-protected-resource<path>
/// ```
///
/// where `<path>` is the path component of `resource_url`.
///
/// # Arguments
///
/// * `http` - Shared [`reqwest::Client`] used to issue the discovery request.
/// * `resource_url` - The base URL of the MCP resource server.
/// * `www_authenticate` - Optional value of the `WWW-Authenticate` response
///   header returned by the resource server on a `401` response.
///
/// # Returns
///
/// A [`ProtectedResourceMetadata`] document on success.
///
/// # Errors
///
/// Returns [`XzatomaError::McpAuth`] if the HTTP request fails or if
/// neither discovery strategy yields a valid document.
///
/// # Examples
///
/// ```no_run
/// use std::sync::Arc;
/// use url::Url;
/// use xzatoma::mcp::auth::discovery::fetch_protected_resource_metadata;
///
/// # async fn example() -> xzatoma::error::Result<()> {
/// let http = reqwest::Client::new();
/// let resource = Url::parse("https://api.example.com/mcp")?;
/// let meta = fetch_protected_resource_metadata(&http, &resource, None).await?;
/// println!("auth server: {}", meta.authorization_servers[0]);
/// # Ok(())
/// # }
/// ```
pub async fn fetch_protected_resource_metadata(
    http: &reqwest::Client,
    resource_url: &Url,
    www_authenticate: Option<&str>,
) -> Result<ProtectedResourceMetadata> {
    // Strategy 1: use the URL embedded in the WWW-Authenticate header.
    if let Some(header) = www_authenticate {
        if let Some(meta_url_str) = parse_resource_metadata_url(header) {
            if let Ok(meta_url) = Url::parse(&meta_url_str) {
                let resp =
                    http.get(meta_url).send().await.map_err(|e| {
                        XzatomaError::McpAuth(format!("metadata fetch failed: {e}"))
                    })?;

                if resp.status().is_success() {
                    let meta: ProtectedResourceMetadata = resp.json().await.map_err(|e| {
                        XzatomaError::McpAuth(format!(
                            "failed to parse protected resource metadata: {e}"
                        ))
                    })?;
                    return Ok(meta);
                }
            }
        }
    }

    // Strategy 2: RFC 9728 well-known URI construction.
    //   https://<host>/.well-known/oauth-protected-resource<path>
    let path = resource_url.path();
    let well_known_path = if path == "/" || path.is_empty() {
        "/.well-known/oauth-protected-resource".to_string()
    } else {
        format!("/.well-known/oauth-protected-resource{}", path)
    };

    let mut well_known_url = resource_url.clone();
    well_known_url.set_path(&well_known_path);
    // Remove query / fragment from the well-known URL.
    well_known_url.set_query(None);
    well_known_url.set_fragment(None);

    let resp = http
        .get(well_known_url.clone())
        .send()
        .await
        .map_err(|e| XzatomaError::McpAuth(format!("well-known metadata fetch failed: {e}")))?;

    if resp.status().is_success() {
        let meta: ProtectedResourceMetadata = resp.json().await.map_err(|e| {
            XzatomaError::McpAuth(format!(
                "failed to parse well-known protected resource metadata: {e}"
            ))
        })?;
        return Ok(meta);
    }

    Err(XzatomaError::McpAuth(format!(
        "protected resource metadata not found for {}",
        resource_url
    ))
    .into())
}

/// Constructs a candidate well-known URL for authorization server metadata
/// discovery.
///
/// The MCP spec requires trying five orderings:
///
/// 1. `/.well-known/oauth-authorization-server/<path>` (path insertion)
/// 2. `/.well-known/openid-configuration/<path>` (path insertion)
/// 3. `<issuer>/.well-known/openid-configuration` (path appending)
/// 4. `/.well-known/oauth-authorization-server`
/// 5. `/.well-known/openid-configuration`
fn build_as_candidate_urls(issuer: &Url) -> Vec<Url> {
    let path = issuer.path().trim_end_matches('/').to_string();
    let mut candidates = Vec::with_capacity(5);

    // Helper closure -- returns None on parse failure so we can skip bad URLs.
    let make = |s: String| Url::parse(&s).ok();

    let origin = format!(
        "{}://{}",
        issuer.scheme(),
        issuer.host_str().unwrap_or_default()
    );
    let origin_with_port = if let Some(port) = issuer.port() {
        format!(
            "{}://{}:{}",
            issuer.scheme(),
            issuer.host_str().unwrap_or_default(),
            port
        )
    } else {
        origin.clone()
    };

    // 1. Path-inserted oauth-authorization-server
    if let Some(u) = make(format!(
        "{}/.well-known/oauth-authorization-server{}",
        origin_with_port, path
    )) {
        candidates.push(u);
    }

    // 2. Path-inserted openid-configuration
    if let Some(u) = make(format!(
        "{}/.well-known/openid-configuration{}",
        origin_with_port, path
    )) {
        candidates.push(u);
    }

    // 3. Path-appended openid-configuration  (<issuer>/.well-known/openid-configuration)
    {
        let mut appended = issuer.clone();
        let new_path = format!("{}/.well-known/openid-configuration", path);
        appended.set_path(&new_path);
        appended.set_query(None);
        appended.set_fragment(None);
        candidates.push(appended);
    }

    // 4. Root oauth-authorization-server (no path)
    if let Some(u) = make(format!(
        "{}/.well-known/oauth-authorization-server",
        origin_with_port
    )) {
        candidates.push(u);
    }

    // 5. Root openid-configuration (no path)
    if let Some(u) = make(format!(
        "{}/.well-known/openid-configuration",
        origin_with_port
    )) {
        candidates.push(u);
    }

    candidates
}

/// Fetches the authorization server metadata document.
///
/// Tries up to five well-known endpoint orderings defined by RFC 8414 and
/// OpenID Connect Discovery 1.0, returning the first successful response.
///
/// The five orderings tried (in order) are:
///
/// 1. `/.well-known/oauth-authorization-server/<path>` (path insertion)
/// 2. `/.well-known/openid-configuration/<path>` (path insertion)
/// 3. `<issuer>/.well-known/openid-configuration` (path appending)
/// 4. `/.well-known/oauth-authorization-server` (root, no path)
/// 5. `/.well-known/openid-configuration` (root, no path)
///
/// # Arguments
///
/// * `http` - Shared [`reqwest::Client`].
/// * `issuer` - The issuer URI from the protected resource metadata.
///
/// # Returns
///
/// An [`AuthorizationServerMetadata`] document on success.
///
/// # Errors
///
/// Returns [`XzatomaError::McpAuth`] if all five orderings fail.
///
/// # Examples
///
/// ```no_run
/// use url::Url;
/// use xzatoma::mcp::auth::discovery::fetch_authorization_server_metadata;
///
/// # async fn example() -> xzatoma::error::Result<()> {
/// let http = reqwest::Client::new();
/// let issuer = Url::parse("https://auth.example.com")?;
/// let meta = fetch_authorization_server_metadata(&http, &issuer).await?;
/// println!("token endpoint: {}", meta.token_endpoint);
/// # Ok(())
/// # }
/// ```
pub async fn fetch_authorization_server_metadata(
    http: &reqwest::Client,
    issuer: &Url,
) -> Result<AuthorizationServerMetadata> {
    let candidates = build_as_candidate_urls(issuer);

    for candidate in &candidates {
        let resp = match http.get(candidate.clone()).send().await {
            Ok(r) => r,
            Err(_) => continue,
        };

        if resp.status().is_success() {
            match resp.json::<AuthorizationServerMetadata>().await {
                Ok(meta) => return Ok(meta),
                Err(_) => continue,
            }
        }
    }

    Err(XzatomaError::McpAuth(format!(
        "authorization server metadata not found for issuer {}",
        issuer
    ))
    .into())
}

/// Fetches a Client ID Metadata Document from the given URL.
///
/// This document is used by MCP clients when the authorization server
/// advertises `client_id_metadata_document_supported: true`.  The client
/// presents the document URL as its `client_id` in OAuth requests.
///
/// # Arguments
///
/// * `http` - Shared [`reqwest::Client`].
/// * `client_id_url` - The URL at which the client metadata document is
///   hosted.
///
/// # Returns
///
/// A deserialized [`ClientIdMetadataDocument`] on success.
///
/// # Errors
///
/// Returns [`XzatomaError::McpAuth`] if the HTTP request fails or the
/// response body cannot be parsed.
///
/// # Examples
///
/// ```no_run
/// use url::Url;
/// use xzatoma::mcp::auth::discovery::fetch_client_id_metadata_document;
///
/// # async fn example() -> xzatoma::error::Result<()> {
/// let http = reqwest::Client::new();
/// let url = Url::parse("https://xzatoma.example.com/.well-known/client-metadata")?;
/// let doc = fetch_client_id_metadata_document(&http, &url).await?;
/// println!("client_id: {}", doc.client_id);
/// # Ok(())
/// # }
/// ```
pub async fn fetch_client_id_metadata_document(
    http: &reqwest::Client,
    client_id_url: &Url,
) -> Result<ClientIdMetadataDocument> {
    let resp = http
        .get(client_id_url.clone())
        .send()
        .await
        .map_err(|e| XzatomaError::McpAuth(format!("client id metadata fetch failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(XzatomaError::McpAuth(format!(
            "client id metadata endpoint returned {}: {}",
            resp.status(),
            client_id_url
        ))
        .into());
    }

    let doc: ClientIdMetadataDocument = resp.json().await.map_err(|e| {
        XzatomaError::McpAuth(format!("failed to parse client id metadata document: {e}"))
    })?;

    Ok(doc)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // parse_resource_metadata_url
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_resource_metadata_url_quoted() {
        let header = r#"Bearer realm="example", resource_metadata="https://api.example.com/.well-known/oauth-protected-resource""#;
        let result = parse_resource_metadata_url(header);
        assert_eq!(
            result,
            Some("https://api.example.com/.well-known/oauth-protected-resource".to_string())
        );
    }

    #[test]
    fn test_parse_resource_metadata_url_unquoted() {
        let header =
            "Bearer resource_metadata=https://api.example.com/.well-known/oauth-protected-resource";
        let result = parse_resource_metadata_url(header);
        assert_eq!(
            result,
            Some("https://api.example.com/.well-known/oauth-protected-resource".to_string())
        );
    }

    #[test]
    fn test_parse_resource_metadata_url_absent() {
        let header = r#"Bearer realm="example", error="invalid_token""#;
        let result = parse_resource_metadata_url(header);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_resource_metadata_url_empty_header() {
        let result = parse_resource_metadata_url("");
        assert!(result.is_none());
    }

    // -----------------------------------------------------------------------
    // build_as_candidate_urls
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_as_candidate_urls_root_issuer_produces_five_candidates() {
        let issuer = Url::parse("https://auth.example.com").unwrap();
        let candidates = build_as_candidate_urls(&issuer);
        assert_eq!(candidates.len(), 5);
    }

    #[test]
    fn test_build_as_candidate_urls_first_is_oauth_with_path() {
        let issuer = Url::parse("https://auth.example.com/tenant/v2").unwrap();
        let candidates = build_as_candidate_urls(&issuer);
        assert!(
            candidates[0]
                .as_str()
                .contains("/.well-known/oauth-authorization-server"),
            "first candidate should be oauth-authorization-server: {}",
            candidates[0]
        );
        assert!(
            candidates[0].as_str().contains("/tenant/v2"),
            "first candidate should include path: {}",
            candidates[0]
        );
    }

    #[test]
    fn test_build_as_candidate_urls_second_is_openid_with_path() {
        let issuer = Url::parse("https://auth.example.com/tenant/v2").unwrap();
        let candidates = build_as_candidate_urls(&issuer);
        assert!(
            candidates[1]
                .as_str()
                .contains("/.well-known/openid-configuration"),
            "second candidate should be openid-configuration: {}",
            candidates[1]
        );
        assert!(
            candidates[1].as_str().contains("/tenant/v2"),
            "second candidate should include path: {}",
            candidates[1]
        );
    }

    #[test]
    fn test_build_as_candidate_urls_fourth_and_fifth_have_no_path() {
        let issuer = Url::parse("https://auth.example.com/some/path").unwrap();
        let candidates = build_as_candidate_urls(&issuer);

        // Candidate 4: root oauth-authorization-server
        let c4 = candidates[3].as_str();
        assert!(c4.contains("/.well-known/oauth-authorization-server"));
        assert!(
            !c4.contains("/some/path"),
            "candidate 4 must not include issuer path: {c4}"
        );

        // Candidate 5: root openid-configuration
        let c5 = candidates[4].as_str();
        assert!(c5.contains("/.well-known/openid-configuration"));
        assert!(
            !c5.contains("/some/path"),
            "candidate 5 must not include issuer path: {c5}"
        );
    }

    // -----------------------------------------------------------------------
    // Serde round-trips
    // -----------------------------------------------------------------------

    #[test]
    fn test_protected_resource_metadata_deserializes() {
        let json = r#"{
            "resource": "https://api.example.com",
            "authorization_servers": ["https://auth.example.com"],
            "scopes_supported": ["openid", "profile"],
            "bearer_methods_supported": ["header"]
        }"#;

        let meta: ProtectedResourceMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(meta.resource, "https://api.example.com");
        assert_eq!(meta.authorization_servers.len(), 1);
        assert_eq!(
            meta.scopes_supported,
            Some(vec!["openid".to_string(), "profile".to_string()])
        );
        assert_eq!(
            meta.bearer_methods_supported,
            Some(vec!["header".to_string()])
        );
    }

    #[test]
    fn test_protected_resource_metadata_deserializes_minimal() {
        let json = r#"{
            "resource": "https://api.example.com",
            "authorization_servers": []
        }"#;

        let meta: ProtectedResourceMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(meta.resource, "https://api.example.com");
        assert!(meta.authorization_servers.is_empty());
        assert!(meta.scopes_supported.is_none());
        assert!(meta.bearer_methods_supported.is_none());
    }

    #[test]
    fn test_authorization_server_metadata_deserializes() {
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

        let meta: AuthorizationServerMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(meta.issuer, "https://auth.example.com");
        assert_eq!(
            meta.authorization_endpoint,
            "https://auth.example.com/authorize"
        );
        assert_eq!(meta.token_endpoint, "https://auth.example.com/token");
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

    #[test]
    fn test_authorization_server_metadata_captures_extra_fields() {
        let json = r#"{
            "issuer": "https://auth.example.com",
            "authorization_endpoint": "https://auth.example.com/authorize",
            "token_endpoint": "https://auth.example.com/token",
            "response_types_supported": ["code"],
            "custom_field": "custom_value"
        }"#;

        let meta: AuthorizationServerMetadata = serde_json::from_str(json).unwrap();
        assert!(meta.extra.contains_key("custom_field"));
        assert_eq!(
            meta.extra["custom_field"],
            serde_json::Value::String("custom_value".to_string())
        );
    }

    #[test]
    fn test_client_id_metadata_document_deserializes() {
        let json = r#"{
            "client_id": "https://xzatoma.example.com/.well-known/client-metadata",
            "client_name": "Xzatoma",
            "client_uri": "https://xzatoma.example.com",
            "redirect_uris": ["http://127.0.0.1:0/callback"],
            "grant_types": ["authorization_code"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none"
        }"#;

        let doc: ClientIdMetadataDocument = serde_json::from_str(json).unwrap();
        assert_eq!(
            doc.client_id,
            "https://xzatoma.example.com/.well-known/client-metadata"
        );
        assert_eq!(doc.client_name, "Xzatoma");
        assert_eq!(doc.redirect_uris.len(), 1);
        assert_eq!(doc.token_endpoint_auth_method, Some("none".to_string()));
    }

    // -----------------------------------------------------------------------
    // fetch_protected_resource_metadata -- wiremock integration tests
    // -----------------------------------------------------------------------

    // Wiremock integration tests are in tests/mcp_auth_discovery_test.rs
}
