//! OAuth 2.1 authorization code flow with PKCE for MCP HTTP transport
//!
//! This module implements the full browser-based OAuth 2.1 authorization code
//! flow with PKCE (RFC 7636) and resource indicators (RFC 8707) as required
//! by the MCP `2025-11-25` specification for HTTP transport connections.
//!
//! # Flow overview
//!
//! 1. Verify the authorization server supports PKCE S256 (`verify_s256_support`).
//! 2. Determine the `client_id` via one of three mechanisms:
//!    - Static client ID from configuration
//!    - Client ID metadata document (MCP extension)
//!    - Dynamic Client Registration (RFC 7591)
//! 3. Generate a PKCE challenge and a random `state` value.
//! 4. Bind a local TCP listener for the redirect callback.
//! 5. Build the authorization URL and open it in the user's browser.
//! 6. Accept the callback connection, extract `code` and `state`.
//! 7. Validate `state`; exchange `code` for tokens.
//!
//! # References
//!
//! - OAuth 2.1 draft <https://datatracker.ietf.org/doc/draft-ietf-oauth-v2-1/>
//! - RFC 7636 PKCE <https://www.rfc-editor.org/rfc/rfc7636>
//! - RFC 7591 Dynamic Registration <https://www.rfc-editor.org/rfc/rfc7591>
//! - RFC 8707 Resource Indicators <https://www.rfc-editor.org/rfc/rfc8707>

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;

use base64::Engine as _;
use url::Url;

use crate::error::{Result, XzatomaError};
use crate::mcp::auth::discovery::AuthorizationServerMetadata;
use crate::mcp::auth::pkce;
use crate::mcp::auth::token_store::OAuthToken;

// ---------------------------------------------------------------------------
// OAuthFlowConfig
// ---------------------------------------------------------------------------

/// Configuration for a single MCP server's OAuth 2.1 authorization flow.
///
/// This struct holds all the parameters needed to drive the authorization code
/// flow for one MCP HTTP server.  It is created from the agent configuration
/// and stored in the [`AuthManager`](super::manager::AuthManager).
///
/// # Examples
///
/// ```
/// use url::Url;
/// use xzatoma::mcp::auth::flow::OAuthFlowConfig;
///
/// let config = OAuthFlowConfig {
///     server_id: "my_server".to_string(),
///     resource_url: Url::parse("https://api.example.com/mcp").unwrap(),
///     client_name: "Xzatoma".to_string(),
///     redirect_port: 0,
///     static_client_id: None,
///     static_client_secret: None,
/// };
///
/// assert_eq!(config.server_id, "my_server");
/// assert_eq!(config.redirect_port, 0);
/// ```
#[derive(Debug, Clone)]
pub struct OAuthFlowConfig {
    /// Unique identifier for the MCP server (matches the config key).
    pub server_id: String,

    /// The base URL of the MCP resource server used as the `resource`
    /// parameter in OAuth requests (RFC 8707).
    pub resource_url: Url,

    /// Human-readable name sent to the authorization server during Dynamic
    /// Client Registration.
    pub client_name: String,

    /// Local TCP port to bind for the redirect callback.  Use `0` to let the
    /// OS assign a free port.
    pub redirect_port: u16,

    /// Optional static `client_id` to use instead of performing client
    /// registration.  Takes highest priority when set.
    pub static_client_id: Option<String>,

    /// Optional static `client_secret`.  Only used when `static_client_id` is
    /// also set and the server requires a confidential client.
    pub static_client_secret: Option<String>,
}

// ---------------------------------------------------------------------------
// Token endpoint response (raw deserialization)
// ---------------------------------------------------------------------------

/// Raw JSON response from an OAuth token endpoint.
///
/// This private type is used only inside [`OAuthFlow`] to deserialize the
/// token response before converting it into the canonical [`OAuthToken`].
#[derive(Debug, serde::Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    scope: Option<String>,
}

impl TokenResponse {
    /// Converts the raw token response into an [`OAuthToken`].
    ///
    /// `expires_in` seconds are converted to an absolute UTC `expires_at`
    /// timestamp.
    fn into_oauth_token(self) -> OAuthToken {
        let expires_at = self.expires_in.map(|secs| {
            chrono::Utc::now() + chrono::Duration::seconds(i64::try_from(secs).unwrap_or(i64::MAX))
        });

        OAuthToken {
            access_token: self.access_token,
            token_type: self.token_type,
            expires_at,
            refresh_token: self.refresh_token,
            scope: self.scope,
        }
    }
}

// ---------------------------------------------------------------------------
// Dynamic Client Registration response
// ---------------------------------------------------------------------------

/// Minimal Dynamic Client Registration response (RFC 7591).
#[derive(Debug, serde::Deserialize)]
struct DcrResponse {
    client_id: String,
    #[serde(default)]
    client_secret: Option<String>,
}

// ---------------------------------------------------------------------------
// OAuthFlow
// ---------------------------------------------------------------------------

/// Drives the OAuth 2.1 authorization code flow with PKCE for one MCP server.
///
/// An `OAuthFlow` is constructed for a specific server and reused across
/// multiple authorization attempts.  It does not persist tokens; that is the
/// responsibility of [`TokenStore`](super::token_store::TokenStore) and
/// [`AuthManager`](super::manager::AuthManager).
///
/// # Examples
///
/// ```no_run
/// use std::sync::Arc;
/// use url::Url;
/// use xzatoma::mcp::auth::flow::{OAuthFlow, OAuthFlowConfig};
/// use xzatoma::mcp::auth::discovery::AuthorizationServerMetadata;
///
/// # async fn example() -> xzatoma::error::Result<()> {
/// let config = OAuthFlowConfig {
///     server_id: "my_server".to_string(),
///     resource_url: Url::parse("https://api.example.com/mcp")?,
///     client_name: "Xzatoma".to_string(),
///     redirect_port: 0,
///     static_client_id: Some("my-client-id".to_string()),
///     static_client_secret: None,
/// };
///
/// let http = Arc::new(reqwest::Client::new());
/// let flow = OAuthFlow::new(http, config);
///
/// // flow.authorize(&server_metadata, None).await?;
/// # Ok(())
/// # }
/// ```
pub struct OAuthFlow {
    http: Arc<reqwest::Client>,
    config: OAuthFlowConfig,
}

impl OAuthFlow {
    /// Creates a new `OAuthFlow` for the given server configuration.
    ///
    /// # Arguments
    ///
    /// * `http` - Shared HTTP client for all authorization requests.
    /// * `config` - Server-specific OAuth flow configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use url::Url;
    /// use xzatoma::mcp::auth::flow::{OAuthFlow, OAuthFlowConfig};
    ///
    /// let config = OAuthFlowConfig {
    ///     server_id: "srv".to_string(),
    ///     resource_url: Url::parse("https://api.example.com").unwrap(),
    ///     client_name: "Xzatoma".to_string(),
    ///     redirect_port: 0,
    ///     static_client_id: None,
    ///     static_client_secret: None,
    /// };
    /// let flow = OAuthFlow::new(Arc::new(reqwest::Client::new()), config);
    /// ```
    pub fn new(http: Arc<reqwest::Client>, config: OAuthFlowConfig) -> Self {
        Self { http, config }
    }

    /// Runs the full OAuth 2.1 authorization code flow with PKCE.
    ///
    /// This is the primary entry point for obtaining a new token when no
    /// valid cached token is available.  The method:
    ///
    /// 1. Verifies PKCE S256 support on the authorization server.
    /// 2. Resolves the `client_id` using the configured strategy.
    /// 3. Generates a PKCE challenge and a random `state` nonce.
    /// 4. Binds a local TCP listener for the redirect callback.
    /// 5. Prints the authorization URL to stderr and attempts to open it in
    ///    the system browser.
    /// 6. Waits for the browser redirect, validates `state`, and exchanges
    ///    the authorization code for tokens.
    ///
    /// # Arguments
    ///
    /// * `server_metadata` - Authorization server metadata from discovery.
    /// * `scope` - Optional space-separated scope string to request.
    ///
    /// # Returns
    ///
    /// An [`OAuthToken`] on success.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::McpAuth`] if:
    /// - PKCE S256 is not supported by the server.
    /// - No viable client registration mechanism is available.
    /// - The `state` parameter in the callback does not match.
    /// - The token endpoint returns an error.
    pub async fn authorize(
        &self,
        server_metadata: &AuthorizationServerMetadata,
        scope: Option<&str>,
    ) -> Result<OAuthToken> {
        // Step 1: verify PKCE S256 support.
        pkce::verify_s256_support(server_metadata)?;

        // Step 2: determine client_id.
        let client_id = self.resolve_client_id(server_metadata).await?;

        // Step 3: PKCE challenge + state nonce.
        let pkce_challenge = pkce::generate()?;
        let state = self.generate_state()?;

        // Step 4: bind local TCP listener for redirect callback.
        let listener =
            tokio::net::TcpListener::bind(format!("127.0.0.1:{}", self.config.redirect_port))
                .await
                .map_err(|e| {
                    XzatomaError::McpAuth(format!("failed to bind redirect listener: {e}"))
                })?;
        let local_addr = listener
            .local_addr()
            .map_err(|e| XzatomaError::McpAuth(format!("failed to get local address: {e}")))?;
        let redirect_uri = format!("http://127.0.0.1:{}/callback", local_addr.port());

        // Step 5: build the authorization URL.
        let auth_url = self.build_authorization_url(
            server_metadata,
            &client_id,
            &redirect_uri,
            scope,
            &state,
            &pkce_challenge.challenge,
        )?;

        // Step 6: print and attempt to open in browser.
        eprintln!(
            "Open the following URL in your browser to authorize Xzatoma:\n{}",
            auth_url
        );
        self.try_open_browser(&auth_url);

        // Step 7: accept callback, validate state, extract code.
        let code = self
            .accept_callback(listener, &state, &redirect_uri)
            .await?;

        // Step 8: exchange code for tokens.
        let token = self
            .exchange_code(
                server_metadata,
                &code,
                &redirect_uri,
                &client_id,
                &pkce_challenge.verifier,
            )
            .await?;

        Ok(token)
    }

    /// Exchanges a refresh token for a new access token.
    ///
    /// POSTs to the token endpoint with `grant_type=refresh_token`.  The
    /// `resource` parameter is included per RFC 8707.
    ///
    /// # Arguments
    ///
    /// * `server_metadata` - Authorization server metadata.
    /// * `refresh_token` - The refresh token string to exchange.
    /// * `scope` - Optional scope string.  When `None`, the previously granted
    ///   scope is preserved.
    ///
    /// # Returns
    ///
    /// A new [`OAuthToken`] on success.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::McpAuth`] if the token endpoint request fails
    /// or the response cannot be parsed.
    pub async fn refresh_token(
        &self,
        server_metadata: &AuthorizationServerMetadata,
        refresh_token: &str,
        scope: Option<&str>,
    ) -> Result<OAuthToken> {
        let mut params: HashMap<&str, &str> = HashMap::new();
        params.insert("grant_type", "refresh_token");
        params.insert("refresh_token", refresh_token);

        let resource_str = self.config.resource_url.as_str().to_string();
        params.insert("resource", &resource_str);

        if let Some(s) = scope {
            params.insert("scope", s);
        }

        let resp = self
            .http
            .post(&server_metadata.token_endpoint)
            .form(&params)
            .send()
            .await
            .map_err(|e| XzatomaError::McpAuth(format!("refresh token request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(XzatomaError::McpAuth(format!(
                "refresh token endpoint returned {status}: {body}"
            ))
            .into());
        }

        let raw: TokenResponse = resp.json().await.map_err(|e| {
            XzatomaError::McpAuth(format!("failed to parse refresh token response: {e}"))
        })?;

        Ok(raw.into_oauth_token())
    }

    /// Handles a `Bearer error="insufficient_scope"` step-up authorization
    /// challenge.
    ///
    /// Parses the `scope=` value from the `WWW-Authenticate` header, then
    /// triggers a fresh `authorize` call with the new scope.  Limited to
    /// 3 total attempts to prevent infinite loops.
    ///
    /// # Arguments
    ///
    /// * `server_metadata` - Authorization server metadata.
    /// * `www_authenticate` - The `WWW-Authenticate` header value from the
    ///   `403` response.
    /// * `current_token` - The token that triggered the scope challenge (used
    ///   for logging).
    ///
    /// # Returns
    ///
    /// A new [`OAuthToken`] with the elevated scope on success.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::McpAuth`] if more than 3 attempts are made or
    /// if the authorization flow fails.
    pub async fn handle_step_up(
        &self,
        server_metadata: &AuthorizationServerMetadata,
        www_authenticate: &str,
        _current_token: &OAuthToken,
    ) -> Result<OAuthToken> {
        let scope = parse_scope_from_www_authenticate(www_authenticate);

        // Limit to 3 total attempts.
        for attempt in 1..=3u32 {
            match self.authorize(server_metadata, scope.as_deref()).await {
                Ok(token) => return Ok(token),
                Err(e) => {
                    if attempt == 3 {
                        return Err(XzatomaError::McpAuth(
                            "step-up authorization loop limit reached".to_string(),
                        )
                        .into());
                    }
                    // Log the error and retry.
                    eprintln!("Step-up authorization attempt {attempt} failed: {e}. Retrying...");
                }
            }
        }

        // Unreachable: the loop always returns on attempt 3, but the compiler
        // needs a value here.
        Err(XzatomaError::McpAuth("step-up authorization loop limit reached".to_string()).into())
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Resolves the `client_id` using the priority order defined by the MCP
    /// specification.
    ///
    /// Priority:
    /// 1. Static client ID from configuration (highest priority).
    /// 2. Client ID metadata document URL (when the server supports it).
    /// 3. Dynamic Client Registration (RFC 7591).
    /// 4. Error if none of the above apply.
    async fn resolve_client_id(
        &self,
        server_metadata: &AuthorizationServerMetadata,
    ) -> Result<String> {
        // Priority 1: static client ID.
        if let Some(ref static_id) = self.config.static_client_id {
            return Ok(static_id.clone());
        }

        // Priority 2: client_id_metadata_document.
        if server_metadata.client_id_metadata_document_supported == Some(true) {
            // The client_id is the URL of the local metadata endpoint.
            // We use a conventional path; in production this would be served
            // by the client application's web server.
            let client_metadata_url = format!(
                "http://127.0.0.1/.well-known/xzatoma-mcp-client-{}",
                self.config.server_id
            );
            return Ok(client_metadata_url);
        }

        // Priority 3: Dynamic Client Registration (RFC 7591).
        if let Some(ref registration_endpoint) = server_metadata.registration_endpoint {
            let client_id = self
                .dynamic_client_registration(registration_endpoint)
                .await?;
            return Ok(client_id);
        }

        // None of the mechanisms apply.
        Err(XzatomaError::McpAuth("no viable client registration mechanism".to_string()).into())
    }

    /// Performs Dynamic Client Registration (RFC 7591).
    ///
    /// POSTs the client metadata to the registration endpoint and returns the
    /// `client_id` from the response.
    async fn dynamic_client_registration(&self, registration_endpoint: &str) -> Result<String> {
        let redirect_uri = format!("http://127.0.0.1:{}/callback", self.config.redirect_port);

        let body = serde_json::json!({
            "client_name": self.config.client_name,
            "redirect_uris": [redirect_uri],
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none",
            "code_challenge_methods": ["S256"],
        });

        let resp = self
            .http
            .post(registration_endpoint)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                XzatomaError::McpAuth(format!("dynamic client registration failed: {e}"))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(XzatomaError::McpAuth(format!(
                "client registration endpoint returned {status}: {text}"
            ))
            .into());
        }

        let dcr: DcrResponse = resp.json().await.map_err(|e| {
            XzatomaError::McpAuth(format!("failed to parse registration response: {e}"))
        })?;

        Ok(dcr.client_id)
    }

    /// Generates a cryptographically random state nonce.
    ///
    /// 16 random bytes encoded as base64url without padding.
    fn generate_state(&self) -> Result<String> {
        use rand::RngCore as _;
        let mut bytes = [0u8; 16];
        rand::rng().fill_bytes(&mut bytes);
        Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
    }

    /// Builds the authorization URL with all required query parameters.
    ///
    /// Includes `resource` (RFC 8707), `code_challenge`, and `code_challenge_method`.
    fn build_authorization_url(
        &self,
        server_metadata: &AuthorizationServerMetadata,
        client_id: &str,
        redirect_uri: &str,
        scope: Option<&str>,
        state: &str,
        code_challenge: &str,
    ) -> Result<String> {
        let mut url = Url::parse(&server_metadata.authorization_endpoint).map_err(|e| {
            XzatomaError::McpAuth(format!("invalid authorization endpoint URL: {e}"))
        })?;

        {
            let mut query = url.query_pairs_mut();
            query.append_pair("response_type", "code");
            query.append_pair("client_id", client_id);
            query.append_pair("redirect_uri", redirect_uri);
            if let Some(s) = scope {
                query.append_pair("scope", s);
            }
            query.append_pair("state", state);
            query.append_pair("code_challenge", code_challenge);
            query.append_pair("code_challenge_method", "S256");
            query.append_pair("resource", self.config.resource_url.as_str());
        }

        Ok(url.to_string())
    }

    /// Attempts to open the authorization URL in the user's default browser.
    ///
    /// Errors are intentionally ignored; if the browser does not open the
    /// user can copy the URL from stderr.
    fn try_open_browser(&self, url: &str) {
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("open").arg(url).spawn();
        }
        #[cfg(target_os = "linux")]
        {
            let _ = std::process::Command::new("xdg-open").arg(url).spawn();
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            // On other platforms (e.g. Windows) we do not attempt to open the
            // browser; the user must copy the URL manually.
            let _ = url;
        }
    }

    /// Accepts a single TCP connection on the callback listener, parses the
    /// HTTP GET request line to extract `code` and `state` query parameters,
    /// validates the `state` nonce, sends a success response, and returns
    /// the authorization `code`.
    async fn accept_callback(
        &self,
        listener: tokio::net::TcpListener,
        expected_state: &str,
        _redirect_uri: &str,
    ) -> Result<String> {
        // Accept one connection.
        let (stream, _peer) = listener.accept().await.map_err(|e| {
            XzatomaError::McpAuth(format!("failed to accept OAuth callback connection: {e}"))
        })?;

        // Move to a blocking task so we can use std I/O for simple HTTP
        // request parsing without pulling in a full HTTP server.
        let expected_state = expected_state.to_string();
        let (code, _) =
            tokio::task::spawn_blocking(move || -> Result<(String, ())> {
                let std_stream = stream.into_std().map_err(|e| {
                    XzatomaError::McpAuth(format!("stream conversion failed: {e}"))
                })?;

                let mut write_stream = std_stream.try_clone().map_err(|e| {
                    XzatomaError::McpAuth(format!("stream clone failed: {e}"))
                })?;

                let reader = BufReader::new(std_stream);
                let mut request_line = String::new();

                for line in reader.lines() {
                    let line = line.map_err(|e| {
                        XzatomaError::McpAuth(format!("failed to read callback request: {e}"))
                    })?;
                    // HTTP headers end at the first empty line.
                    if line.is_empty() {
                        break;
                    }
                    if request_line.is_empty() {
                        request_line = line;
                    }
                }

                // Send HTTP 200 response immediately so the browser does not
                // spin indefinitely.
                let response = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nAuthorization successful. You may close this tab.";
                let _ = write_stream.write_all(response.as_bytes());

                // Parse request line: "GET /callback?code=...&state=... HTTP/1.1"
                let path = request_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/");

                let query_string = path.split_once('?').map(|x| x.1).unwrap_or("");
                let params = parse_query_string(query_string);

                let state = params
                    .get("state")
                    .cloned()
                    .unwrap_or_default();

                if state != expected_state {
                    return Err(XzatomaError::McpAuth(
                        "state mismatch in OAuth callback".to_string(),
                    )
                    .into());
                }

                let code = params
                    .get("code")
                    .cloned()
                    .ok_or_else(|| {
                        XzatomaError::McpAuth(
                            "authorization code missing from callback".to_string(),
                        )
                    })?;

                Ok((code, ()))
            })
            .await
            .map_err(|e| XzatomaError::McpAuth(format!("callback task panicked: {e}")))?
            .map_err(|e: anyhow::Error| e)?;

        Ok(code)
    }

    /// Exchanges an authorization code for tokens at the token endpoint.
    async fn exchange_code(
        &self,
        server_metadata: &AuthorizationServerMetadata,
        code: &str,
        redirect_uri: &str,
        client_id: &str,
        code_verifier: &str,
    ) -> Result<OAuthToken> {
        let resource_str = self.config.resource_url.as_str().to_string();

        let mut params: HashMap<&str, &str> = HashMap::new();
        params.insert("grant_type", "authorization_code");
        params.insert("code", code);
        params.insert("redirect_uri", redirect_uri);
        params.insert("client_id", client_id);
        params.insert("code_verifier", code_verifier);
        params.insert("resource", &resource_str);

        // Include client_secret when available (confidential clients).
        let secret_owned;
        if let Some(ref secret) = self.config.static_client_secret {
            secret_owned = secret.clone();
            params.insert("client_secret", &secret_owned);
        }

        let resp = self
            .http
            .post(&server_metadata.token_endpoint)
            .form(&params)
            .send()
            .await
            .map_err(|e| XzatomaError::McpAuth(format!("token exchange request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(
                XzatomaError::McpAuth(format!("token endpoint returned {status}: {body}")).into(),
            );
        }

        let raw: TokenResponse = resp
            .json()
            .await
            .map_err(|e| XzatomaError::McpAuth(format!("failed to parse token response: {e}")))?;

        Ok(raw.into_oauth_token())
    }
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Parses a URL query string into a key-value map.
///
/// Values are percent-decoded.  Duplicate keys are overwritten by the last
/// occurrence.
fn parse_query_string(query: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in query.split('&') {
        let mut iter = pair.splitn(2, '=');
        let key = iter.next().unwrap_or("").to_string();
        let value = iter.next().unwrap_or("").to_string();
        if !key.is_empty() {
            // Simple percent-decode for '+' (space) and %XX sequences.
            let decoded_value = percent_decode(&value);
            map.insert(key, decoded_value);
        }
    }
    map
}

/// Performs minimal percent-decoding of a URL query parameter value.
///
/// Converts `+` to space and `%XX` sequences to the corresponding byte.
fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'+' {
            out.push(' ');
            i += 1;
        } else if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte as char);
                    i += 3;
                    continue;
                }
            }
            out.push(bytes[i] as char);
            i += 1;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// Parses the `scope=` value from a `WWW-Authenticate: Bearer` challenge
/// header.
///
/// Returns `None` when the attribute is absent.
fn parse_scope_from_www_authenticate(www_authenticate: &str) -> Option<String> {
    let key = "scope=";
    let pos = www_authenticate.find(key)?;
    let rest = &www_authenticate[pos + key.len()..];

    if let Some(inner) = rest.strip_prefix('"') {
        let end = inner.find('"')?;
        Some(inner[..end].to_string())
    } else {
        let end = rest
            .find(|c: char| c.is_whitespace() || c == ',')
            .unwrap_or(rest.len());
        Some(rest[..end].to_string())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // parse_query_string
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_query_string_with_code_and_state() {
        let qs = "code=abc123&state=xyz789";
        let map = parse_query_string(qs);
        assert_eq!(map.get("code"), Some(&"abc123".to_string()));
        assert_eq!(map.get("state"), Some(&"xyz789".to_string()));
    }

    #[test]
    fn test_parse_query_string_empty_returns_empty_map() {
        let map = parse_query_string("");
        assert!(map.is_empty());
    }

    #[test]
    fn test_parse_query_string_single_param() {
        let map = parse_query_string("key=value");
        assert_eq!(map.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_parse_query_string_decodes_plus_as_space() {
        let map = parse_query_string("greeting=hello+world");
        assert_eq!(map.get("greeting"), Some(&"hello world".to_string()));
    }

    #[test]
    fn test_parse_query_string_decodes_percent_encoding() {
        let map = parse_query_string("scope=openid%20profile");
        assert_eq!(map.get("scope"), Some(&"openid profile".to_string()));
    }

    // -----------------------------------------------------------------------
    // percent_decode
    // -----------------------------------------------------------------------

    #[test]
    fn test_percent_decode_plain_string_unchanged() {
        assert_eq!(percent_decode("hello"), "hello");
    }

    #[test]
    fn test_percent_decode_converts_plus_to_space() {
        assert_eq!(percent_decode("hello+world"), "hello world");
    }

    #[test]
    fn test_percent_decode_hex_sequence() {
        assert_eq!(percent_decode("a%20b"), "a b");
    }

    #[test]
    fn test_percent_decode_incomplete_percent_passes_through() {
        // A lone '%' without two hex digits should pass through safely.
        let result = percent_decode("%zz");
        assert!(!result.is_empty());
    }

    // -----------------------------------------------------------------------
    // parse_scope_from_www_authenticate
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_scope_quoted_value() {
        let header = r#"Bearer error="insufficient_scope", scope="openid profile""#;
        let scope = parse_scope_from_www_authenticate(header);
        assert_eq!(scope, Some("openid profile".to_string()));
    }

    #[test]
    fn test_parse_scope_unquoted_value() {
        let header = "Bearer error=insufficient_scope, scope=openid";
        let scope = parse_scope_from_www_authenticate(header);
        assert_eq!(scope, Some("openid".to_string()));
    }

    #[test]
    fn test_parse_scope_absent_returns_none() {
        let header = "Bearer error=invalid_token";
        let scope = parse_scope_from_www_authenticate(header);
        assert!(scope.is_none());
    }

    #[test]
    fn test_parse_scope_empty_header_returns_none() {
        assert!(parse_scope_from_www_authenticate("").is_none());
    }

    // -----------------------------------------------------------------------
    // OAuthFlowConfig construction
    // -----------------------------------------------------------------------

    #[test]
    fn test_flow_config_fields_are_accessible() {
        let config = OAuthFlowConfig {
            server_id: "test_server".to_string(),
            resource_url: Url::parse("https://api.example.com/mcp").unwrap(),
            client_name: "Xzatoma".to_string(),
            redirect_port: 0,
            static_client_id: Some("my-client".to_string()),
            static_client_secret: None,
        };

        assert_eq!(config.server_id, "test_server");
        assert_eq!(config.client_name, "Xzatoma");
        assert_eq!(config.redirect_port, 0);
        assert!(config.static_client_secret.is_none());
    }

    // -----------------------------------------------------------------------
    // generate_state
    // -----------------------------------------------------------------------

    #[test]
    fn test_generate_state_produces_non_empty_string() {
        let config = OAuthFlowConfig {
            server_id: "srv".to_string(),
            resource_url: Url::parse("https://example.com").unwrap(),
            client_name: "Xzatoma".to_string(),
            redirect_port: 0,
            static_client_id: None,
            static_client_secret: None,
        };
        let flow = OAuthFlow::new(Arc::new(reqwest::Client::new()), config);
        let state = flow.generate_state().unwrap();
        assert!(!state.is_empty());
    }

    #[test]
    fn test_generate_state_produces_unique_values() {
        let config = OAuthFlowConfig {
            server_id: "srv".to_string(),
            resource_url: Url::parse("https://example.com").unwrap(),
            client_name: "Xzatoma".to_string(),
            redirect_port: 0,
            static_client_id: None,
            static_client_secret: None,
        };
        let flow = OAuthFlow::new(Arc::new(reqwest::Client::new()), config);
        let a = flow.generate_state().unwrap();
        let b = flow.generate_state().unwrap();
        assert_ne!(a, b);
    }

    // -----------------------------------------------------------------------
    // build_authorization_url
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_authorization_url_contains_required_params() {
        use std::collections::HashMap;

        let config = OAuthFlowConfig {
            server_id: "srv".to_string(),
            resource_url: Url::parse("https://api.example.com/mcp").unwrap(),
            client_name: "Xzatoma".to_string(),
            redirect_port: 12345,
            static_client_id: Some("test_client".to_string()),
            static_client_secret: None,
        };
        let flow = OAuthFlow::new(Arc::new(reqwest::Client::new()), config);

        let server_metadata = AuthorizationServerMetadata {
            issuer: "https://auth.example.com".to_string(),
            authorization_endpoint: "https://auth.example.com/authorize".to_string(),
            token_endpoint: "https://auth.example.com/token".to_string(),
            registration_endpoint: None,
            scopes_supported: None,
            response_types_supported: vec!["code".to_string()],
            grant_types_supported: None,
            code_challenge_methods_supported: Some(vec!["S256".to_string()]),
            client_id_metadata_document_supported: None,
            extra: HashMap::new(),
        };

        let url = flow
            .build_authorization_url(
                &server_metadata,
                "test_client",
                "http://127.0.0.1:12345/callback",
                Some("openid"),
                "test_state",
                "test_challenge",
            )
            .unwrap();

        assert!(
            url.contains("response_type=code"),
            "missing response_type: {url}"
        );
        assert!(
            url.contains("client_id=test_client"),
            "missing client_id: {url}"
        );
        assert!(url.contains("redirect_uri="), "missing redirect_uri: {url}");
        assert!(url.contains("state=test_state"), "missing state: {url}");
        assert!(
            url.contains("code_challenge=test_challenge"),
            "missing code_challenge: {url}"
        );
        assert!(
            url.contains("code_challenge_method=S256"),
            "missing method: {url}"
        );
        assert!(url.contains("resource="), "missing resource: {url}");
        assert!(url.contains("scope=openid"), "missing scope: {url}");
    }

    #[test]
    fn test_build_authorization_url_omits_scope_when_none() {
        use std::collections::HashMap;

        let config = OAuthFlowConfig {
            server_id: "srv".to_string(),
            resource_url: Url::parse("https://api.example.com/mcp").unwrap(),
            client_name: "Xzatoma".to_string(),
            redirect_port: 0,
            static_client_id: Some("test_client".to_string()),
            static_client_secret: None,
        };
        let flow = OAuthFlow::new(Arc::new(reqwest::Client::new()), config);

        let server_metadata = AuthorizationServerMetadata {
            issuer: "https://auth.example.com".to_string(),
            authorization_endpoint: "https://auth.example.com/authorize".to_string(),
            token_endpoint: "https://auth.example.com/token".to_string(),
            registration_endpoint: None,
            scopes_supported: None,
            response_types_supported: vec!["code".to_string()],
            grant_types_supported: None,
            code_challenge_methods_supported: Some(vec!["S256".to_string()]),
            client_id_metadata_document_supported: None,
            extra: HashMap::new(),
        };

        let url = flow
            .build_authorization_url(
                &server_metadata,
                "test_client",
                "http://127.0.0.1:0/callback",
                None,
                "state123",
                "challenge_abc",
            )
            .unwrap();

        assert!(
            !url.contains("scope="),
            "URL should not contain scope when None: {url}"
        );
    }

    // -----------------------------------------------------------------------
    // TokenResponse conversion
    // -----------------------------------------------------------------------

    #[test]
    fn test_token_response_into_oauth_token_sets_expires_at() {
        let raw = TokenResponse {
            access_token: "tok".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: Some(3600),
            refresh_token: None,
            scope: None,
        };

        let token = raw.into_oauth_token();
        assert!(
            token.expires_at.is_some(),
            "expires_at should be set when expires_in is present"
        );
    }

    #[test]
    fn test_token_response_into_oauth_token_no_expiry() {
        let raw = TokenResponse {
            access_token: "tok".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: None,
            refresh_token: Some("refresh".to_string()),
            scope: Some("openid".to_string()),
        };

        let token = raw.into_oauth_token();
        assert!(
            token.expires_at.is_none(),
            "expires_at should be None when expires_in is absent"
        );
        assert_eq!(token.refresh_token, Some("refresh".to_string()));
        assert_eq!(token.scope, Some("openid".to_string()));
    }
}
