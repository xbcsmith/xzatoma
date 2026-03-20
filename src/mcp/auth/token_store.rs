//! OAuth token persistence via OS keyring
//!
//! This module provides secure storage and retrieval of OAuth 2.1 tokens
//! using the operating system's native credential store (Keychain on macOS,
//! Secret Service on Linux, Windows Credential Manager on Windows).
//!
//! Tokens are serialized to JSON before storage and deserialized on load.
//! The keyring is stateless; [`TokenStore`] is a zero-field struct that acts
//! as a namespaced accessor.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Result, XzatomaError};

// ---------------------------------------------------------------------------
// OAuthToken
// ---------------------------------------------------------------------------

/// A complete OAuth 2.1 token response.
///
/// Fields map directly to the token endpoint response defined in RFC 6749 and
/// refined by OAuth 2.1.  The `expires_at` field is a computed UTC timestamp
/// derived from the `expires_in` seconds returned by the server; it is stored
/// in the keyring alongside the access token so that expiry can be determined
/// without a server round-trip.
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::auth::token_store::OAuthToken;
/// use chrono::Utc;
///
/// let token = OAuthToken {
///     access_token: "my_access_token".to_string(),
///     token_type: "Bearer".to_string(),
///     expires_at: None,
///     refresh_token: None,
///     scope: None,
/// };
///
/// // A token with no expiry is never considered expired.
/// assert!(!token.is_expired());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthToken {
    /// The access token string issued by the authorization server.
    pub access_token: String,

    /// The token type, typically `"Bearer"`.
    pub token_type: String,

    /// UTC timestamp at which the access token expires.
    ///
    /// When `None`, the token is treated as non-expiring.  The value is
    /// stored as an RFC-3339 string via the `chrono` serde feature so that it
    /// survives a round-trip through the keyring JSON representation.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "chrono::serde::ts_seconds_option"
    )]
    pub expires_at: Option<DateTime<Utc>>,

    /// Refresh token that can be used to obtain a new access token without
    /// re-running the full authorization flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,

    /// Space-separated OAuth scopes granted by the authorization server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

impl OAuthToken {
    /// Returns `true` when the access token is expired or about to expire.
    ///
    /// A 60-second buffer is applied so that callers have time to exchange a
    /// refresh token before the access token is rejected by the resource
    /// server.  Tokens with no `expires_at` value are considered perpetually
    /// valid.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::mcp::auth::token_store::OAuthToken;
    /// use chrono::{Duration, Utc};
    ///
    /// // Expired one second ago -- considered expired.
    /// let past = OAuthToken {
    ///     access_token: "tok".to_string(),
    ///     token_type: "Bearer".to_string(),
    ///     expires_at: Some(Utc::now() - Duration::seconds(1)),
    ///     refresh_token: None,
    ///     scope: None,
    /// };
    /// assert!(past.is_expired());
    ///
    /// // Expires in one hour -- not expired.
    /// let future = OAuthToken {
    ///     access_token: "tok".to_string(),
    ///     token_type: "Bearer".to_string(),
    ///     expires_at: Some(Utc::now() + Duration::hours(1)),
    ///     refresh_token: None,
    ///     scope: None,
    /// };
    /// assert!(!future.is_expired());
    /// ```
    pub fn is_expired(&self) -> bool {
        match self.expires_at {
            None => false,
            Some(expires_at) => {
                let buffer = chrono::Duration::seconds(60);
                Utc::now() >= expires_at - buffer
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TokenStore
// ---------------------------------------------------------------------------

/// Stateless accessor for the OS native keyring.
///
/// Each MCP server's token is stored under a unique service name derived from
/// the server identifier, preventing collisions between servers.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::mcp::auth::token_store::{OAuthToken, TokenStore};
///
/// # async fn example() -> xzatoma::error::Result<()> {
/// let store = TokenStore;
/// let token = OAuthToken {
///     access_token: "my_token".to_string(),
///     token_type: "Bearer".to_string(),
///     expires_at: None,
///     refresh_token: None,
///     scope: None,
/// };
/// store.save_token("my_server", &token)?;
/// let loaded = store.load_token("my_server")?;
/// assert!(loaded.is_some());
/// # Ok(())
/// # }
/// ```
pub struct TokenStore;

impl TokenStore {
    /// Builds the keyring service name for the given MCP server identifier.
    ///
    /// The name is prefixed with `xzatoma-mcp-` to avoid collisions with
    /// other applications that use the same keyring.
    fn service_name(server_id: &str) -> String {
        format!("xzatoma-mcp-{}", server_id)
    }

    /// Persists an [`OAuthToken`] for the named MCP server.
    ///
    /// The token is serialized to JSON and stored in the OS keyring under the
    /// service name derived from `server_id`.
    ///
    /// # Arguments
    ///
    /// * `server_id` - Unique identifier for the MCP server (matches the key
    ///   used in the agent configuration).
    /// * `token` - The token to persist.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::Serialization`] if JSON serialization fails or
    /// [`XzatomaError::Keyring`] if the OS credential store rejects the write.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzatoma::mcp::auth::token_store::{OAuthToken, TokenStore};
    ///
    /// let store = TokenStore;
    /// let token = OAuthToken {
    ///     access_token: "access".to_string(),
    ///     token_type: "Bearer".to_string(),
    ///     expires_at: None,
    ///     refresh_token: None,
    ///     scope: None,
    /// };
    /// store.save_token("server1", &token).unwrap();
    /// ```
    pub fn save_token(&self, server_id: &str, token: &OAuthToken) -> Result<()> {
        let json_str = serde_json::to_string(token)?;
        let service = Self::service_name(server_id);
        let entry = keyring::Entry::new(&service, server_id).map_err(XzatomaError::Keyring)?;
        entry
            .set_password(&json_str)
            .map_err(XzatomaError::Keyring)?;
        Ok(())
    }

    /// Loads the stored [`OAuthToken`] for the named MCP server.
    ///
    /// Returns `Ok(None)` when no token has been saved for the server,
    /// allowing callers to distinguish between "not authenticated yet" and a
    /// genuine keyring error.
    ///
    /// # Arguments
    ///
    /// * `server_id` - Unique identifier for the MCP server.
    ///
    /// # Returns
    ///
    /// `Ok(Some(token))` if a valid token was found and deserialized,
    /// `Ok(None)` if no token exists for this server.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::Keyring`] if the OS credential store returns
    /// an unexpected error, or [`XzatomaError::Serialization`] if the stored
    /// JSON is malformed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzatoma::mcp::auth::token_store::TokenStore;
    ///
    /// let store = TokenStore;
    /// match store.load_token("server1").unwrap() {
    ///     Some(token) => println!("Found token: {}", token.access_token),
    ///     None => println!("No token stored"),
    /// }
    /// ```
    pub fn load_token(&self, server_id: &str) -> Result<Option<OAuthToken>> {
        let service = Self::service_name(server_id);
        let entry = keyring::Entry::new(&service, server_id).map_err(XzatomaError::Keyring)?;

        match entry.get_password() {
            Ok(json_str) => {
                let token: OAuthToken = serde_json::from_str(&json_str)?;
                Ok(Some(token))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(XzatomaError::Keyring(e).into()),
        }
    }

    /// Deletes the stored token for the named MCP server.
    ///
    /// This is a no-op when no token exists for the server, so it is safe to
    /// call even when the caller is not sure whether a token was previously
    /// saved.
    ///
    /// # Arguments
    ///
    /// * `server_id` - Unique identifier for the MCP server.
    ///
    /// # Errors
    ///
    /// Returns [`XzatomaError::Keyring`] if the OS credential store returns
    /// an unexpected error.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzatoma::mcp::auth::token_store::TokenStore;
    ///
    /// let store = TokenStore;
    /// store.delete_token("server1").unwrap();
    /// ```
    pub fn delete_token(&self, server_id: &str) -> Result<()> {
        let service = Self::service_name(server_id);
        let entry = keyring::Entry::new(&service, server_id).map_err(XzatomaError::Keyring)?;

        match entry.delete_password() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(XzatomaError::Keyring(e).into()),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    // -----------------------------------------------------------------------
    // OAuthToken::is_expired
    // -----------------------------------------------------------------------

    #[test]
    fn test_oauth_token_is_expired_when_past_expiry() {
        let token = OAuthToken {
            access_token: "tok".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: Some(Utc::now() - Duration::seconds(1)),
            refresh_token: None,
            scope: None,
        };
        assert!(token.is_expired());
    }

    #[test]
    fn test_oauth_token_is_expired_within_buffer_window() {
        // 30 seconds in the future is still within the 60-second buffer.
        let token = OAuthToken {
            access_token: "tok".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: Some(Utc::now() + Duration::seconds(30)),
            refresh_token: None,
            scope: None,
        };
        assert!(token.is_expired());
    }

    #[test]
    fn test_oauth_token_not_expired_when_future_expiry() {
        let token = OAuthToken {
            access_token: "tok".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: Some(Utc::now() + Duration::hours(1)),
            refresh_token: None,
            scope: None,
        };
        assert!(!token.is_expired());
    }

    #[test]
    fn test_oauth_token_not_expired_when_no_expiry() {
        let token = OAuthToken {
            access_token: "tok".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: None,
            refresh_token: None,
            scope: None,
        };
        assert!(!token.is_expired());
    }

    // -----------------------------------------------------------------------
    // JSON round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn test_token_roundtrip_through_json() {
        let original = OAuthToken {
            access_token: "access_abc".to_string(),
            token_type: "Bearer".to_string(),
            // Use a fixed timestamp to avoid sub-second precision issues.
            expires_at: Some(DateTime::from_timestamp(1_800_000_000, 0).expect("valid timestamp")),
            refresh_token: Some("refresh_xyz".to_string()),
            scope: Some("openid profile".to_string()),
        };

        let json = serde_json::to_string(&original).expect("serialize");
        let restored: OAuthToken = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.access_token, original.access_token);
        assert_eq!(restored.token_type, original.token_type);
        assert_eq!(restored.expires_at, original.expires_at);
        assert_eq!(restored.refresh_token, original.refresh_token);
        assert_eq!(restored.scope, original.scope);
    }

    #[test]
    fn test_token_roundtrip_no_optional_fields() {
        let original = OAuthToken {
            access_token: "tok".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: None,
            refresh_token: None,
            scope: None,
        };

        let json = serde_json::to_string(&original).expect("serialize");
        let restored: OAuthToken = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.access_token, original.access_token);
        assert_eq!(restored.token_type, original.token_type);
        assert!(restored.expires_at.is_none());
        assert!(restored.refresh_token.is_none());
        assert!(restored.scope.is_none());
    }

    // -----------------------------------------------------------------------
    // service_name helper
    // -----------------------------------------------------------------------

    #[test]
    fn test_service_name_has_correct_prefix() {
        let name = TokenStore::service_name("my_server");
        assert_eq!(name, "xzatoma-mcp-my_server");
    }

    #[test]
    fn test_service_name_is_unique_per_server() {
        let a = TokenStore::service_name("server_a");
        let b = TokenStore::service_name("server_b");
        assert_ne!(a, b);
    }

    // -----------------------------------------------------------------------
    // Keyring integration tests  (require system keyring; skipped in CI)
    // -----------------------------------------------------------------------

    #[test]
    #[ignore = "requires system keyring"]
    fn test_save_and_load_token_roundtrip_via_keyring() {
        let store = TokenStore;
        let server_id = "test_integration_server";

        let token = OAuthToken {
            access_token: "integration_access".to_string(),
            token_type: "Bearer".to_string(),
            expires_at: Some(Utc::now() + Duration::hours(1)),
            refresh_token: Some("integration_refresh".to_string()),
            scope: Some("read write".to_string()),
        };

        store.save_token(server_id, &token).expect("save");
        let loaded = store.load_token(server_id).expect("load");
        let loaded = loaded.expect("token should be present");

        assert_eq!(loaded.access_token, token.access_token);
        assert_eq!(loaded.refresh_token, token.refresh_token);
        assert_eq!(loaded.scope, token.scope);

        store.delete_token(server_id).expect("delete");
        let after_delete = store.load_token(server_id).expect("load after delete");
        assert!(after_delete.is_none());
    }

    #[test]
    #[ignore = "requires system keyring"]
    fn test_load_token_returns_none_when_absent() {
        let store = TokenStore;
        let result = store
            .load_token("definitely_nonexistent_server_xzatoma_test")
            .expect("should not error");
        assert!(result.is_none());
    }

    #[test]
    #[ignore = "requires system keyring"]
    fn test_delete_token_is_idempotent() {
        let store = TokenStore;
        let server_id = "idempotent_delete_test_xzatoma";
        // Deleting a non-existent entry must not return an error.
        store.delete_token(server_id).expect("first delete");
        store
            .delete_token(server_id)
            .expect("second delete is no-op");
    }
}
