//! High-level OAuth 2.1 authorization manager for MCP HTTP transport
//!
//! This module coordinates token storage, discovery, and the authorization
//! code flow into a single façade used by the MCP HTTP transport layer.
//!
//! The [`AuthManager`] is the sole entry point for all authorization
//! operations.  Callers interact with it through four methods:
//!
//! - [`AuthManager::get_token`] -- returns a valid access token, refreshing
//!   or re-authorizing as necessary.
//! - [`AuthManager::handle_401`] -- responds to an unexpected `401
//!   Unauthorized` by clearing the cached token and triggering full
//!   re-authorization.
//! - [`AuthManager::handle_403_scope`] -- responds to a `403 Forbidden` with
//!   `insufficient_scope` by running a step-up authorization flow.
//! - [`AuthManager::inject_token`] -- inserts the `Authorization: Bearer
//!   <token>` header into a header map.
//!
//! # Examples
//!
//! ```no_run
//! use std::sync::Arc;
//! use url::Url;
//! use xzatoma::mcp::auth::manager::AuthManager;
//! use xzatoma::mcp::auth::flow::OAuthFlowConfig;
//! use xzatoma::mcp::auth::token_store::TokenStore;
//!
//! # async fn example() -> xzatoma::error::Result<()> {
//! let http = Arc::new(reqwest::Client::new());
//! let token_store = Arc::new(TokenStore);
//! let mut manager = AuthManager::new(http, token_store);
//!
//! manager.add_server(
//!     "my_server".to_string(),
//!     OAuthFlowConfig {
//!         server_id: "my_server".to_string(),
//!         resource_url: Url::parse("https://api.example.com/mcp")?,
//!         client_name: "Xzatoma".to_string(),
//!         redirect_port: 0,
//!         static_client_id: Some("my-client-id".to_string()),
//!         static_client_secret: None,
//!     },
//! );
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{Result, XzatomaError};
use crate::mcp::auth::discovery::AuthorizationServerMetadata;
use crate::mcp::auth::flow::{OAuthFlow, OAuthFlowConfig};
use crate::mcp::auth::token_store::{OAuthToken, TokenStore};

// ---------------------------------------------------------------------------
// AuthManager
// ---------------------------------------------------------------------------

/// High-level coordinator for OAuth 2.1 token lifecycle.
///
/// `AuthManager` owns a shared HTTP client, a reference to the OS keyring
/// [`TokenStore`], and a map of per-server [`OAuthFlowConfig`] values.  It
/// handles the full token lifecycle: load from cache, refresh when expired,
/// and run the full authorization code flow when no usable token is available.
///
/// # Thread safety
///
/// `AuthManager` is `Send` but not internally synchronized.  Wrap it in an
/// `Arc<Mutex<AuthManager>>` or `Arc<tokio::sync::Mutex<AuthManager>>` when
/// sharing across tasks.
///
/// # Examples
///
/// ```no_run
/// use std::sync::Arc;
/// use url::Url;
/// use xzatoma::mcp::auth::manager::AuthManager;
/// use xzatoma::mcp::auth::flow::OAuthFlowConfig;
/// use xzatoma::mcp::auth::token_store::TokenStore;
///
/// let http = Arc::new(reqwest::Client::new());
/// let token_store = Arc::new(TokenStore);
/// let manager = AuthManager::new(http, token_store);
/// ```
pub struct AuthManager {
    /// Shared HTTP client used by all [`OAuthFlow`] instances.
    http: Arc<reqwest::Client>,

    /// Reference to the OS keyring token store.
    token_store: Arc<TokenStore>,

    /// Per-server OAuth flow configurations keyed by server identifier.
    flow_configs: HashMap<String, OAuthFlowConfig>,
}

impl AuthManager {
    /// Creates a new `AuthManager` with no servers registered.
    ///
    /// Use [`add_server`](Self::add_server) to register server configurations
    /// before calling [`get_token`](Self::get_token).
    ///
    /// # Arguments
    ///
    /// * `http` - Shared HTTP client for all authorization requests.
    /// * `token_store` - Reference to the OS keyring token store.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use xzatoma::mcp::auth::manager::AuthManager;
    /// use xzatoma::mcp::auth::token_store::TokenStore;
    ///
    /// let manager = AuthManager::new(
    ///     Arc::new(reqwest::Client::new()),
    ///     Arc::new(TokenStore),
    /// );
    /// ```
    pub fn new(http: Arc<reqwest::Client>, token_store: Arc<TokenStore>) -> Self {
        Self {
            http,
            token_store,
            flow_configs: HashMap::new(),
        }
    }

    /// Registers an OAuth flow configuration for a named MCP server.
    ///
    /// Overwrites any existing configuration for the same `server_id`.
    ///
    /// # Arguments
    ///
    /// * `server_id` - Unique identifier for the MCP server.
    /// * `config` - OAuth flow configuration for this server.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use url::Url;
    /// use xzatoma::mcp::auth::manager::AuthManager;
    /// use xzatoma::mcp::auth::flow::OAuthFlowConfig;
    /// use xzatoma::mcp::auth::token_store::TokenStore;
    ///
    /// let mut manager = AuthManager::new(
    ///     Arc::new(reqwest::Client::new()),
    ///     Arc::new(TokenStore),
    /// );
    ///
    /// manager.add_server(
    ///     "server1".to_string(),
    ///     OAuthFlowConfig {
    ///         server_id: "server1".to_string(),
    ///         resource_url: Url::parse("https://api.example.com/mcp").unwrap(),
    ///         client_name: "Xzatoma".to_string(),
    ///         redirect_port: 0,
    ///         static_client_id: Some("client-id".to_string()),
    ///         static_client_secret: None,
    ///     },
    /// );
    /// ```
    pub fn add_server(&mut self, server_id: String, config: OAuthFlowConfig) {
        self.flow_configs.insert(server_id, config);
    }

    /// Returns a valid access token for the named MCP server.
    ///
    /// The resolution order is:
    ///
    /// 1. Load the cached token from the OS keyring.
    /// 2. If the token is present and not expired, return its `access_token`.
    /// 3. If the token is expired and has a `refresh_token`, attempt to refresh
    ///    it.  On success, persist the new token and return the access token.
    ///    On failure, fall through to step 4.
    /// 4. Run the full authorization code flow, persist the token, and return
    ///    the access token.
    ///
    /// # Arguments
    ///
    /// * `server_id` - Unique identifier for the MCP server.
    /// * `server_metadata` - Authorization server metadata from discovery.
    ///
    /// # Returns
    ///
    /// A valid access token string on success.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::McpServerNotFound`] if `server_id` has not been
    /// registered via [`add_server`](Self::add_server).
    ///
    /// Returns [`XzatomaError::McpAuth`] if authorization fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use url::Url;
    /// use xzatoma::mcp::auth::manager::AuthManager;
    /// use xzatoma::mcp::auth::flow::OAuthFlowConfig;
    /// use xzatoma::mcp::auth::token_store::TokenStore;
    /// use xzatoma::mcp::auth::discovery::AuthorizationServerMetadata;
    /// use std::collections::HashMap;
    ///
    /// # async fn example() -> xzatoma::error::Result<()> {
    /// let mut manager = AuthManager::new(
    ///     Arc::new(reqwest::Client::new()),
    ///     Arc::new(TokenStore),
    /// );
    /// manager.add_server(
    ///     "srv".to_string(),
    ///     OAuthFlowConfig {
    ///         server_id: "srv".to_string(),
    ///         resource_url: Url::parse("https://api.example.com/mcp")?,
    ///         client_name: "Xzatoma".to_string(),
    ///         redirect_port: 0,
    ///         static_client_id: Some("client".to_string()),
    ///         static_client_secret: None,
    ///     },
    /// );
    /// let metadata = AuthorizationServerMetadata {
    ///     issuer: "https://auth.example.com".to_string(),
    ///     authorization_endpoint: "https://auth.example.com/authorize".to_string(),
    ///     token_endpoint: "https://auth.example.com/token".to_string(),
    ///     registration_endpoint: None,
    ///     scopes_supported: None,
    ///     response_types_supported: vec!["code".to_string()],
    ///     grant_types_supported: None,
    ///     code_challenge_methods_supported: Some(vec!["S256".to_string()]),
    ///     client_id_metadata_document_supported: None,
    ///     extra: HashMap::new(),
    /// };
    /// let token = manager.get_token("srv", &metadata).await?;
    /// println!("access token: {token}");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_token(
        &self,
        server_id: &str,
        server_metadata: &AuthorizationServerMetadata,
    ) -> Result<String> {
        let config = self.require_config(server_id)?;

        // Step 1: load cached token.
        let cached = self.token_store.load_token(server_id)?;

        if let Some(ref token) = cached {
            // Step 2: not expired -- return immediately.
            if !token.is_expired() {
                return Ok(token.access_token.clone());
            }

            // Step 3: expired but has a refresh token -- try to refresh.
            if let Some(ref refresh) = token.refresh_token {
                let flow = OAuthFlow::new(Arc::clone(&self.http), config.clone());
                match flow.refresh_token(server_metadata, refresh, None).await {
                    Ok(new_token) => {
                        self.token_store.save_token(server_id, &new_token)?;
                        return Ok(new_token.access_token);
                    }
                    Err(e) => {
                        // Refresh failed; log and fall through to full auth.
                        eprintln!(
                            "Token refresh failed for server '{}': {}. Running full auth flow.",
                            server_id, e
                        );
                    }
                }
            }
        }

        // Step 4: full authorization code flow.
        let flow = OAuthFlow::new(Arc::clone(&self.http), config.clone());
        let new_token = flow.authorize(server_metadata, None).await?;
        self.token_store.save_token(server_id, &new_token)?;
        Ok(new_token.access_token)
    }

    /// Handles a `401 Unauthorized` response from an MCP HTTP server.
    ///
    /// Deletes the current cached token (if any) to force full re-authorization
    /// on the next request.  This covers the case where the authorization
    /// server revoked the access token out-of-band.
    ///
    /// # Arguments
    ///
    /// * `server_id` - Unique identifier for the MCP server.
    /// * `www_authenticate` - The `WWW-Authenticate` header value from the
    ///   `401` response (currently unused but reserved for future parsing).
    /// * `server_metadata` - Authorization server metadata from discovery.
    ///
    /// # Returns
    ///
    /// A fresh access token string after re-authorization.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::McpServerNotFound`] if the server is not
    /// registered.  Returns [`XzatomaError::McpAuth`] if re-authorization
    /// fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::Arc;
    /// # use url::Url;
    /// # use xzatoma::mcp::auth::manager::AuthManager;
    /// # use xzatoma::mcp::auth::flow::OAuthFlowConfig;
    /// # use xzatoma::mcp::auth::token_store::TokenStore;
    /// # use xzatoma::mcp::auth::discovery::AuthorizationServerMetadata;
    /// # use std::collections::HashMap;
    /// # async fn example() -> xzatoma::error::Result<()> {
    /// # let mut manager = AuthManager::new(Arc::new(reqwest::Client::new()), Arc::new(TokenStore));
    /// # manager.add_server("srv".to_string(), OAuthFlowConfig {
    /// #     server_id: "srv".to_string(),
    /// #     resource_url: Url::parse("https://api.example.com/mcp")?,
    /// #     client_name: "Xzatoma".to_string(),
    /// #     redirect_port: 0,
    /// #     static_client_id: Some("client".to_string()),
    /// #     static_client_secret: None,
    /// # });
    /// # let metadata = AuthorizationServerMetadata {
    /// #     issuer: String::new(), authorization_endpoint: String::new(),
    /// #     token_endpoint: String::new(), registration_endpoint: None,
    /// #     scopes_supported: None, response_types_supported: vec![],
    /// #     grant_types_supported: None, code_challenge_methods_supported: None,
    /// #     client_id_metadata_document_supported: None, extra: HashMap::new(),
    /// # };
    /// let token = manager
    ///     .handle_401("srv", "Bearer error=\"invalid_token\"", &metadata)
    ///     .await?;
    /// println!("new token: {token}");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn handle_401(
        &self,
        server_id: &str,
        _www_authenticate: &str,
        server_metadata: &AuthorizationServerMetadata,
    ) -> Result<String> {
        // Delete the stale token so get_token runs the full auth flow.
        self.token_store.delete_token(server_id)?;
        self.get_token(server_id, server_metadata).await
    }

    /// Handles a `403 Forbidden` response indicating insufficient scope.
    ///
    /// Parses the required scope from the `WWW-Authenticate` header and runs
    /// a step-up authorization flow to obtain an elevated token.
    ///
    /// # Arguments
    ///
    /// * `server_id` - Unique identifier for the MCP server.
    /// * `www_authenticate` - The `WWW-Authenticate: Bearer
    ///   error="insufficient_scope", scope="..."` header value.
    /// * `server_metadata` - Authorization server metadata from discovery.
    /// * `current_token` - The token that triggered the `403` response.
    ///
    /// # Returns
    ///
    /// An elevated access token string on success.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::McpServerNotFound`] if the server is not
    /// registered.  Returns [`XzatomaError::McpAuth`] if step-up
    /// authorization fails or the retry limit is exceeded.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::sync::Arc;
    /// # use url::Url;
    /// # use xzatoma::mcp::auth::manager::AuthManager;
    /// # use xzatoma::mcp::auth::flow::OAuthFlowConfig;
    /// # use xzatoma::mcp::auth::token_store::{OAuthToken, TokenStore};
    /// # use xzatoma::mcp::auth::discovery::AuthorizationServerMetadata;
    /// # use std::collections::HashMap;
    /// # async fn example() -> xzatoma::error::Result<()> {
    /// # let mut manager = AuthManager::new(Arc::new(reqwest::Client::new()), Arc::new(TokenStore));
    /// # manager.add_server("srv".to_string(), OAuthFlowConfig {
    /// #     server_id: "srv".to_string(),
    /// #     resource_url: Url::parse("https://api.example.com/mcp")?,
    /// #     client_name: "Xzatoma".to_string(),
    /// #     redirect_port: 0,
    /// #     static_client_id: Some("client".to_string()),
    /// #     static_client_secret: None,
    /// # });
    /// # let metadata = AuthorizationServerMetadata {
    /// #     issuer: String::new(), authorization_endpoint: String::new(),
    /// #     token_endpoint: String::new(), registration_endpoint: None,
    /// #     scopes_supported: None, response_types_supported: vec![],
    /// #     grant_types_supported: None, code_challenge_methods_supported: None,
    /// #     client_id_metadata_document_supported: None, extra: HashMap::new(),
    /// # };
    /// let current = OAuthToken {
    ///     access_token: "old".to_string(),
    ///     token_type: "Bearer".to_string(),
    ///     expires_at: None,
    ///     refresh_token: None,
    ///     scope: Some("openid".to_string()),
    /// };
    /// let token = manager
    ///     .handle_403_scope(
    ///         "srv",
    ///         "Bearer error=\"insufficient_scope\", scope=\"openid admin\"",
    ///         &metadata,
    ///         &current,
    ///     )
    ///     .await?;
    /// println!("elevated token: {token}");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn handle_403_scope(
        &self,
        server_id: &str,
        www_authenticate: &str,
        server_metadata: &AuthorizationServerMetadata,
        current_token: &OAuthToken,
    ) -> Result<String> {
        let config = self.require_config(server_id)?;
        let flow = OAuthFlow::new(Arc::clone(&self.http), config.clone());
        let new_token = flow
            .handle_step_up(server_metadata, www_authenticate, current_token)
            .await?;
        self.token_store.save_token(server_id, &new_token)?;
        Ok(new_token.access_token)
    }

    /// Inserts an `Authorization: Bearer <token>` header into the given map.
    ///
    /// This is a pure utility function with no I/O.  It is provided on
    /// `AuthManager` for discoverability, but does not require `&self`.
    ///
    /// # Arguments
    ///
    /// * `headers` - Mutable reference to the request header map.
    /// * `token` - The access token string to inject.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use xzatoma::mcp::auth::manager::AuthManager;
    ///
    /// let mut headers = HashMap::new();
    /// AuthManager::inject_token(&mut headers, "my_access_token");
    /// assert_eq!(
    ///     headers.get("Authorization"),
    ///     Some(&"Bearer my_access_token".to_string()),
    /// );
    /// ```
    pub fn inject_token(headers: &mut HashMap<String, String>, token: &str) {
        headers.insert("Authorization".to_string(), format!("Bearer {}", token));
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Looks up the flow configuration for `server_id`, returning an error if
    /// the server has not been registered.
    fn require_config(&self, server_id: &str) -> Result<&OAuthFlowConfig> {
        self.flow_configs
            .get(server_id)
            .ok_or_else(|| XzatomaError::McpServerNotFound(server_id.to_string()).into())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_manager() -> AuthManager {
        AuthManager::new(Arc::new(reqwest::Client::new()), Arc::new(TokenStore))
    }

    fn make_config(server_id: &str) -> OAuthFlowConfig {
        OAuthFlowConfig {
            server_id: server_id.to_string(),
            resource_url: Url::parse("https://api.example.com/mcp").unwrap(),
            client_name: "Xzatoma".to_string(),
            redirect_port: 0,
            static_client_id: Some("test-client".to_string()),
            static_client_secret: None,
        }
    }

    // -----------------------------------------------------------------------
    // new() and add_server()
    // -----------------------------------------------------------------------

    #[test]
    fn test_new_creates_empty_manager() {
        let manager = make_manager();
        assert!(manager.flow_configs.is_empty());
    }

    #[test]
    fn test_add_server_registers_config() {
        let mut manager = make_manager();
        manager.add_server("server1".to_string(), make_config("server1"));
        assert!(manager.flow_configs.contains_key("server1"));
    }

    #[test]
    fn test_add_server_overwrites_existing_config() {
        let mut manager = make_manager();
        manager.add_server("server1".to_string(), make_config("server1"));

        let new_config = OAuthFlowConfig {
            server_id: "server1".to_string(),
            resource_url: Url::parse("https://new.example.com/mcp").unwrap(),
            client_name: "NewClient".to_string(),
            redirect_port: 9999,
            static_client_id: Some("new-client-id".to_string()),
            static_client_secret: None,
        };
        manager.add_server("server1".to_string(), new_config);

        let stored = manager.flow_configs.get("server1").unwrap();
        assert_eq!(stored.client_name, "NewClient");
        assert_eq!(stored.redirect_port, 9999);
    }

    #[test]
    fn test_add_multiple_servers() {
        let mut manager = make_manager();
        manager.add_server("server1".to_string(), make_config("server1"));
        manager.add_server("server2".to_string(), make_config("server2"));
        assert_eq!(manager.flow_configs.len(), 2);
    }

    // -----------------------------------------------------------------------
    // require_config()
    // -----------------------------------------------------------------------

    #[test]
    fn test_require_config_returns_error_for_unknown_server() {
        let manager = make_manager();
        let result = manager.require_config("nonexistent");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("nonexistent"),
            "error should mention server id: {msg}"
        );
    }

    #[test]
    fn test_require_config_returns_ok_for_registered_server() {
        let mut manager = make_manager();
        manager.add_server("server1".to_string(), make_config("server1"));
        let result = manager.require_config("server1");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().server_id, "server1");
    }

    // -----------------------------------------------------------------------
    // inject_token()
    // -----------------------------------------------------------------------

    #[test]
    fn test_inject_token_sets_authorization_header() {
        let mut headers = HashMap::new();
        AuthManager::inject_token(&mut headers, "my_access_token");
        assert_eq!(
            headers.get("Authorization"),
            Some(&"Bearer my_access_token".to_string()),
        );
    }

    #[test]
    fn test_inject_token_overwrites_existing_header() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer old_token".to_string());
        AuthManager::inject_token(&mut headers, "new_token");
        assert_eq!(
            headers.get("Authorization"),
            Some(&"Bearer new_token".to_string()),
        );
    }

    #[test]
    fn test_inject_token_uses_bearer_scheme() {
        let mut headers = HashMap::new();
        AuthManager::inject_token(&mut headers, "tok123");
        let value = headers.get("Authorization").unwrap();
        assert!(
            value.starts_with("Bearer "),
            "Authorization header must use Bearer scheme: {value}"
        );
    }

    #[test]
    fn test_inject_token_preserves_other_headers() {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        AuthManager::inject_token(&mut headers, "tok");
        assert_eq!(
            headers.get("Content-Type"),
            Some(&"application/json".to_string()),
        );
        assert!(headers.contains_key("Authorization"));
    }

    #[test]
    fn test_inject_token_empty_token_still_inserts_header() {
        let mut headers = HashMap::new();
        AuthManager::inject_token(&mut headers, "");
        assert_eq!(headers.get("Authorization"), Some(&"Bearer ".to_string()),);
    }

    // -----------------------------------------------------------------------
    // get_token() -- unregistered server
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_token_returns_error_for_unregistered_server() {
        use std::collections::HashMap;

        let manager = make_manager();
        let metadata = AuthorizationServerMetadata {
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

        let result = manager.get_token("unregistered", &metadata).await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("unregistered"),
            "error should mention server id: {msg}"
        );
    }

    // -----------------------------------------------------------------------
    // handle_401() -- unregistered server
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_handle_401_returns_error_for_unregistered_server() {
        use std::collections::HashMap;

        let manager = make_manager();
        let metadata = AuthorizationServerMetadata {
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

        let result = manager
            .handle_401("unregistered", "Bearer error=\"invalid_token\"", &metadata)
            .await;
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // handle_403_scope() -- unregistered server
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_handle_403_scope_returns_error_for_unregistered_server() {
        use std::collections::HashMap;

        let manager = make_manager();
        let metadata = AuthorizationServerMetadata {
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
        let current_token = OAuthToken {
            access_token: "old_token".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: None,
            refresh_token: None,
            scope: Some("openid".to_string()),
        };

        let result = manager
            .handle_403_scope(
                "unregistered",
                "Bearer error=\"insufficient_scope\", scope=\"openid admin\"",
                &metadata,
                &current_token,
            )
            .await;
        assert!(result.is_err());
    }
}
