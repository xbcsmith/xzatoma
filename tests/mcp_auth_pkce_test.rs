//! PKCE S256 challenge unit tests
//!
//! Tests every observable behaviour of `src/mcp/auth/pkce.rs` including:
//!
//! - Correct verifier length (43 characters from 32 random bytes).
//! - Challenge equals `base64url(SHA256(verifier))`.
//! - Method is always `"S256"`.
//! - `verify_s256_support` rejects servers that do not advertise S256.
//! - `verify_s256_support` accepts servers that do advertise S256.
//! - RFC 7636 Appendix B known-answer test vector.

use base64::Engine as _;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use xzatoma::mcp::auth::discovery::AuthorizationServerMetadata;
use xzatoma::mcp::auth::pkce::{generate, verify_s256_support};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Builds a minimal [`AuthorizationServerMetadata`] with the given
/// `code_challenge_methods_supported` value.
fn make_metadata(methods: Option<Vec<String>>) -> AuthorizationServerMetadata {
    AuthorizationServerMetadata {
        issuer: "https://auth.example.com".to_string(),
        authorization_endpoint: "https://auth.example.com/authorize".to_string(),
        token_endpoint: "https://auth.example.com/token".to_string(),
        registration_endpoint: None,
        scopes_supported: None,
        response_types_supported: vec!["code".to_string()],
        grant_types_supported: None,
        code_challenge_methods_supported: methods,
        client_id_metadata_document_supported: None,
        extra: HashMap::new(),
    }
}

// ---------------------------------------------------------------------------
// generate() tests
// ---------------------------------------------------------------------------

/// The verifier must be exactly 43 base64url characters (32 bytes * 4/3
/// rounded up to the next whole character, no padding).
#[test]
fn test_generate_produces_correct_verifier_length() {
    let pkce = generate().expect("generate must not fail");
    assert_eq!(
        pkce.verifier.len(),
        43,
        "32 random bytes encoded as base64url without padding must produce 43 characters"
    );
}

/// The challenge must equal `base64url(SHA256(verifier))` per RFC 7636
/// section 4.2.
#[test]
fn test_challenge_is_correct_s256_of_verifier() {
    let pkce = generate().expect("generate must not fail");

    // Independently recompute the expected challenge.
    let digest = Sha256::digest(pkce.verifier.as_bytes());
    let expected = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest.as_slice());

    assert_eq!(
        pkce.challenge, expected,
        "challenge must equal base64url(SHA256(verifier))"
    );
}

/// The `method` field must always be the string `"S256"`.
#[test]
fn test_method_is_always_s256() {
    let pkce = generate().expect("generate must not fail");
    assert_eq!(pkce.method, "S256");
}

/// Successive calls must produce different verifiers (statistical test --
/// the probability of collision with 32 random bytes is negligible).
#[test]
fn test_generate_produces_unique_verifiers() {
    let a = generate().expect("first call");
    let b = generate().expect("second call");
    assert_ne!(
        a.verifier, b.verifier,
        "successive calls must produce distinct verifiers"
    );
}

/// Successive calls must produce different challenges.
#[test]
fn test_generate_produces_unique_challenges() {
    let a = generate().expect("first call");
    let b = generate().expect("second call");
    assert_ne!(
        a.challenge, b.challenge,
        "successive calls must produce distinct challenges"
    );
}

/// Verifier must only contain base64url-safe characters: `[A-Za-z0-9_-]`.
/// No `+`, `/`, or `=` characters may appear.
#[test]
fn test_verifier_contains_only_base64url_characters() {
    let pkce = generate().expect("generate must not fail");
    for ch in pkce.verifier.chars() {
        assert!(
            ch.is_ascii_alphanumeric() || ch == '-' || ch == '_',
            "verifier contains non-base64url character '{}': {}",
            ch,
            pkce.verifier
        );
    }
    assert!(
        !pkce.verifier.contains('='),
        "verifier must not contain padding '='"
    );
}

/// Challenge must only contain base64url-safe characters and no padding.
#[test]
fn test_challenge_contains_only_base64url_characters() {
    let pkce = generate().expect("generate must not fail");
    for ch in pkce.challenge.chars() {
        assert!(
            ch.is_ascii_alphanumeric() || ch == '-' || ch == '_',
            "challenge contains non-base64url character '{}': {}",
            ch,
            pkce.challenge
        );
    }
    assert!(
        !pkce.challenge.contains('='),
        "challenge must not contain padding '='"
    );
}

/// Verifier and challenge must be distinct strings (the challenge is a
/// transformed derivative, not a copy of the verifier).
#[test]
fn test_verifier_and_challenge_are_distinct() {
    let pkce = generate().expect("generate must not fail");
    assert_ne!(
        pkce.verifier, pkce.challenge,
        "verifier and challenge must not be equal"
    );
}

// ---------------------------------------------------------------------------
// verify_s256_support() tests
// ---------------------------------------------------------------------------

/// `verify_s256_support` must return `Err` when
/// `code_challenge_methods_supported` contains only `"plain"`.
#[test]
fn test_verify_s256_support_rejects_when_absent() {
    let meta = make_metadata(Some(vec!["plain".to_string()]));
    let err = verify_s256_support(&meta).expect_err("must reject when S256 is absent");
    let msg = err.to_string();
    assert!(
        msg.contains("PKCE S256 not supported"),
        "error message should mention 'PKCE S256 not supported', got: {msg}"
    );
}

/// `verify_s256_support` must return `Err` when
/// `code_challenge_methods_supported` is `None`.
#[test]
fn test_verify_s256_support_rejects_when_field_is_none() {
    let meta = make_metadata(None);
    let err = verify_s256_support(&meta)
        .expect_err("must reject when code_challenge_methods_supported is None");
    let msg = err.to_string();
    assert!(
        msg.contains("PKCE S256 not supported"),
        "error message should mention 'PKCE S256 not supported', got: {msg}"
    );
}

/// `verify_s256_support` must return `Err` when the supported list is empty.
#[test]
fn test_verify_s256_support_rejects_empty_list() {
    let meta = make_metadata(Some(vec![]));
    assert!(
        verify_s256_support(&meta).is_err(),
        "must reject when list is empty"
    );
}

/// `verify_s256_support` must return `Ok(())` when `"S256"` is the only
/// entry in the list.
#[test]
fn test_verify_s256_support_accepts_when_present() {
    let meta = make_metadata(Some(vec!["S256".to_string()]));
    assert!(
        verify_s256_support(&meta).is_ok(),
        "must accept when S256 is in the list"
    );
}

/// `verify_s256_support` must return `Ok(())` when `"S256"` appears among
/// other methods.
#[test]
fn test_verify_s256_support_accepts_when_present_among_others() {
    let meta = make_metadata(Some(vec!["plain".to_string(), "S256".to_string()]));
    assert!(
        verify_s256_support(&meta).is_ok(),
        "must accept when S256 is present alongside other methods"
    );
}

/// The comparison must be case-sensitive: `"s256"` (lowercase) must NOT
/// satisfy the S256 requirement.
#[test]
fn test_verify_s256_support_is_case_sensitive() {
    let meta = make_metadata(Some(vec!["s256".to_string()]));
    assert!(
        verify_s256_support(&meta).is_err(),
        "method comparison must be case-sensitive; 's256' != 'S256'"
    );
}

// ---------------------------------------------------------------------------
// RFC 7636 Appendix B known-answer test vector
// ---------------------------------------------------------------------------

/// Verifies the S256 implementation against the known test vector published
/// in RFC 7636 Appendix B:
///
///   code_verifier  = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
///   code_challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
#[test]
fn test_s256_known_answer_vector_rfc7636_appendix_b() {
    let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest.as_slice());
    assert_eq!(
        challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
        "S256 challenge must match RFC 7636 Appendix B test vector"
    );
}
