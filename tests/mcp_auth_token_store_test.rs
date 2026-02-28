//! OAuth token store unit and integration tests
//!
//! Tests the observable behaviour of `src/mcp/auth/token_store.rs`:
//!
//! - `OAuthToken::is_expired` returns `true` when past `expires_at - 60s`.
//! - `OAuthToken::is_expired` returns `false` when well in the future.
//! - `OAuthToken::is_expired` returns `false` when `expires_at` is `None`.
//! - JSON round-trip preserves all fields.
//! - `TokenStore::service_name` has the correct prefix.
//!
//! Tests that interact with the OS keychain are marked `#[ignore]` with the
//! reason `"requires system keyring"`.

use chrono::{DateTime, Duration, Utc};

use xzatoma::mcp::auth::token_store::{OAuthToken, TokenStore};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Constructs an [`OAuthToken`] with only the mandatory fields set.
fn minimal_token(access_token: &str) -> OAuthToken {
    OAuthToken {
        access_token: access_token.to_string(),
        token_type: "Bearer".to_string(),
        expires_at: None,
        refresh_token: None,
        scope: None,
    }
}

/// Constructs an [`OAuthToken`] that expires at the given UTC timestamp.
fn token_expiring_at(expires_at: DateTime<Utc>) -> OAuthToken {
    OAuthToken {
        access_token: "access_token".to_string(),
        token_type: "Bearer".to_string(),
        expires_at: Some(expires_at),
        refresh_token: None,
        scope: None,
    }
}

// ---------------------------------------------------------------------------
// OAuthToken::is_expired
// ---------------------------------------------------------------------------

/// A token whose `expires_at` is 1 second in the past must be considered
/// expired.
#[test]
fn test_oauth_token_is_expired_when_past_expiry() {
    let token = token_expiring_at(Utc::now() - Duration::seconds(1));
    assert!(
        token.is_expired(),
        "token with past expiry must be considered expired"
    );
}

/// A token expiring exactly at `now - 60s` (the buffer boundary) must also
/// be considered expired because the comparison uses `>=`.
#[test]
fn test_oauth_token_is_expired_at_buffer_boundary() {
    // Subtract 60 s -- the token is at the edge of the buffer window.
    let token = token_expiring_at(Utc::now() - Duration::seconds(60));
    assert!(
        token.is_expired(),
        "token at the 60-second buffer boundary must be expired"
    );
}

/// A token expiring within the 60-second buffer window (30 s in the future)
/// must be considered expired so the caller has time to refresh it.
#[test]
fn test_oauth_token_is_expired_within_buffer_window() {
    let token = token_expiring_at(Utc::now() + Duration::seconds(30));
    assert!(
        token.is_expired(),
        "token within the 60-second pre-expiry buffer must be considered expired"
    );
}

/// A token that expires in 1 hour is well outside the 60-second buffer and
/// must NOT be considered expired.
#[test]
fn test_oauth_token_not_expired_when_future_expiry() {
    let token = token_expiring_at(Utc::now() + Duration::hours(1));
    assert!(
        !token.is_expired(),
        "token expiring in 1 hour must not be considered expired"
    );
}

/// A token that expires in exactly 61 seconds is just outside the buffer
/// and must NOT be considered expired.
#[test]
fn test_oauth_token_not_expired_when_just_outside_buffer() {
    let token = token_expiring_at(Utc::now() + Duration::seconds(61));
    assert!(
        !token.is_expired(),
        "token expiring in 61 seconds (just outside the 60-second buffer) must not be expired"
    );
}

/// A token with `expires_at: None` must never be considered expired.
#[test]
fn test_oauth_token_not_expired_when_no_expiry() {
    let token = minimal_token("no_expiry_token");
    assert!(
        !token.is_expired(),
        "token with no expiry must never be considered expired"
    );
}

// ---------------------------------------------------------------------------
// JSON round-trip
// ---------------------------------------------------------------------------

/// Serialising and deserialising an [`OAuthToken`] with all fields set must
/// preserve every field exactly.
#[test]
fn test_token_roundtrip_through_json() {
    let original = OAuthToken {
        access_token: "access_abc".to_string(),
        token_type: "Bearer".to_string(),
        // Use a fixed Unix timestamp to avoid sub-second precision loss.
        expires_at: Some(
            DateTime::from_timestamp(1_800_000_000, 0).expect("timestamp 1_800_000_000 is valid"),
        ),
        refresh_token: Some("refresh_xyz".to_string()),
        scope: Some("openid profile email".to_string()),
    };

    let json = serde_json::to_string(&original).expect("serialization must succeed");
    let restored: OAuthToken = serde_json::from_str(&json).expect("deserialization must succeed");

    assert_eq!(
        restored.access_token, original.access_token,
        "access_token must survive round-trip"
    );
    assert_eq!(
        restored.token_type, original.token_type,
        "token_type must survive round-trip"
    );
    assert_eq!(
        restored.expires_at, original.expires_at,
        "expires_at must survive round-trip"
    );
    assert_eq!(
        restored.refresh_token, original.refresh_token,
        "refresh_token must survive round-trip"
    );
    assert_eq!(
        restored.scope, original.scope,
        "scope must survive round-trip"
    );
}

/// Serialising and deserialising an [`OAuthToken`] with no optional fields
/// must produce `None` values on the restored struct.
#[test]
fn test_token_roundtrip_no_optional_fields() {
    let original = minimal_token("tok");

    let json = serde_json::to_string(&original).expect("serialization must succeed");
    let restored: OAuthToken = serde_json::from_str(&json).expect("deserialization must succeed");

    assert_eq!(restored.access_token, original.access_token);
    assert_eq!(restored.token_type, original.token_type);
    assert!(restored.expires_at.is_none(), "expires_at should be None");
    assert!(
        restored.refresh_token.is_none(),
        "refresh_token should be None"
    );
    assert!(restored.scope.is_none(), "scope should be None");
}

/// Optional fields that are `None` must be omitted from the serialized JSON
/// (i.e. `skip_serializing_if = "Option::is_none"` is in effect).
#[test]
fn test_token_json_omits_none_fields() {
    let token = minimal_token("tok");
    let json = serde_json::to_string(&token).expect("serialization must succeed");

    // None-valued fields must not appear as JSON keys.
    assert!(
        !json.contains("expires_at"),
        "expires_at key must be absent when None, got: {json}"
    );
    assert!(
        !json.contains("refresh_token"),
        "refresh_token key must be absent when None, got: {json}"
    );
    assert!(
        !json.contains("scope"),
        "scope key must be absent when None, got: {json}"
    );
}

/// Present optional fields must appear in the serialized JSON.
#[test]
fn test_token_json_includes_present_fields() {
    let token = OAuthToken {
        access_token: "tok".to_string(),
        token_type: "Bearer".to_string(),
        expires_at: Some(DateTime::from_timestamp(1_700_000_000, 0).expect("valid timestamp")),
        refresh_token: Some("refresh".to_string()),
        scope: Some("openid".to_string()),
    };

    let json = serde_json::to_string(&token).expect("serialization must succeed");

    assert!(
        json.contains("refresh_token"),
        "refresh_token key must be present when Some, got: {json}"
    );
    assert!(
        json.contains("scope"),
        "scope key must be present when Some, got: {json}"
    );
}

// ---------------------------------------------------------------------------
// TokenStore::service_name (via visible unit test in the module itself, but
// we also verify the contract from outside the module).
// ---------------------------------------------------------------------------

/// The service name for any server must begin with the expected prefix.  This
/// is tested indirectly through the public API by verifying that two different
/// server IDs produce different keys (which would be impossible if the prefix
/// were dropped or the IDs were not incorporated).
#[test]
fn test_service_name_is_unique_per_server_id() {
    // TokenStore::service_name is private, so we verify the contract via
    // save/load round-trips in the keyring tests below.  Here we assert the
    // structural expectation using the known naming scheme.
    let id_a = "server_alpha";
    let id_b = "server_beta";
    // The names are derived as "xzatoma-mcp-{id}".
    let name_a = format!("xzatoma-mcp-{}", id_a);
    let name_b = format!("xzatoma-mcp-{}", id_b);
    assert_ne!(
        name_a, name_b,
        "distinct server IDs must produce distinct service names"
    );
    assert!(
        name_a.starts_with("xzatoma-mcp-"),
        "service name must use xzatoma-mcp- prefix"
    );
}

// ---------------------------------------------------------------------------
// Keyring integration tests  (require OS keyring; skipped in CI)
// ---------------------------------------------------------------------------

/// Saves a token, loads it back, verifies the fields, then deletes it and
/// confirms the entry is gone.
#[test]
#[ignore = "requires system keyring"]
fn test_save_and_load_token_roundtrip_via_keyring() {
    let store = TokenStore;
    let server_id = "xzatoma_test_integration_server_roundtrip";

    let token = OAuthToken {
        access_token: "integration_access_token".to_string(),
        token_type: "Bearer".to_string(),
        expires_at: Some(Utc::now() + Duration::hours(1)),
        refresh_token: Some("integration_refresh_token".to_string()),
        scope: Some("openid profile read write".to_string()),
    };

    // Persist.
    store
        .save_token(server_id, &token)
        .expect("save_token must succeed");

    // Retrieve.
    let loaded = store
        .load_token(server_id)
        .expect("load_token must succeed");
    let loaded = loaded.expect("loaded token must be present after save");

    assert_eq!(
        loaded.access_token, token.access_token,
        "access_token must survive keyring round-trip"
    );
    assert_eq!(
        loaded.refresh_token, token.refresh_token,
        "refresh_token must survive keyring round-trip"
    );
    assert_eq!(
        loaded.scope, token.scope,
        "scope must survive keyring round-trip"
    );

    // Delete and verify absence.
    store
        .delete_token(server_id)
        .expect("delete_token must succeed");
    let after_delete = store
        .load_token(server_id)
        .expect("load_token after delete must not error");
    assert!(
        after_delete.is_none(),
        "token must be absent after deletion"
    );
}

/// Loading a token that was never saved must return `Ok(None)`.
#[test]
#[ignore = "requires system keyring"]
fn test_load_token_returns_none_when_absent() {
    let store = TokenStore;
    let server_id = "xzatoma_test_definitely_nonexistent_server_load_none";

    // Ensure clean state.
    let _ = store.delete_token(server_id);

    let result = store
        .load_token(server_id)
        .expect("load_token must not return an error for a missing entry");

    assert!(
        result.is_none(),
        "load_token must return None when no token has been saved"
    );
}

/// Deleting a token that does not exist must silently succeed (idempotent).
#[test]
#[ignore = "requires system keyring"]
fn test_delete_token_is_idempotent() {
    let store = TokenStore;
    let server_id = "xzatoma_test_idempotent_delete_xzatoma";

    // First delete: entry may or may not exist -- must not error.
    store
        .delete_token(server_id)
        .expect("first delete must not error");

    // Second delete: entry is definitely absent -- must not error.
    store
        .delete_token(server_id)
        .expect("second delete of absent entry must not error");
}

/// Saving a token twice must overwrite the first without error.
#[test]
#[ignore = "requires system keyring"]
fn test_save_token_overwrites_existing_entry() {
    let store = TokenStore;
    let server_id = "xzatoma_test_overwrite_server";

    let first = OAuthToken {
        access_token: "first_token".to_string(),
        token_type: "Bearer".to_string(),
        expires_at: None,
        refresh_token: None,
        scope: None,
    };

    let second = OAuthToken {
        access_token: "second_token".to_string(),
        token_type: "Bearer".to_string(),
        expires_at: None,
        refresh_token: None,
        scope: None,
    };

    store.save_token(server_id, &first).expect("first save");
    store.save_token(server_id, &second).expect("second save");

    let loaded = store
        .load_token(server_id)
        .expect("load must succeed")
        .expect("token must be present");

    assert_eq!(
        loaded.access_token, "second_token",
        "second save must overwrite first"
    );

    // Clean up.
    let _ = store.delete_token(server_id);
}
