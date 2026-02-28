//! PKCE S256 challenge generation and verification
//!
//! This module implements the Proof Key for Code Exchange (PKCE) extension
//! to OAuth 2.0 as defined in RFC 7636, specifically the `S256` challenge
//! method required by OAuth 2.1 and the MCP `2025-11-25` specification.
//!
//! # How PKCE works
//!
//! 1. The client generates a high-entropy random string called the `code_verifier`.
//! 2. The client computes a SHA-256 hash of the verifier and base64url-encodes
//!    it to produce the `code_challenge`.
//! 3. The authorization request includes `code_challenge` and
//!    `code_challenge_method=S256`.
//! 4. The token exchange request includes the original `code_verifier`.
//! 5. The authorization server recomputes the challenge and compares it to
//!    the value sent in step 3, proving possession of the verifier.
//!
//! # References
//!
//! - RFC 7636 <https://www.rfc-editor.org/rfc/rfc7636>
//! - OAuth 2.1 draft <https://datatracker.ietf.org/doc/draft-ietf-oauth-v2-1/>

use base64::Engine as _;
use sha2::{Digest, Sha256};

use crate::error::{Result, XzatomaError};
use crate::mcp::auth::discovery::AuthorizationServerMetadata;

// ---------------------------------------------------------------------------
// PkceChallenge
// ---------------------------------------------------------------------------

/// A PKCE S256 challenge pair consisting of a verifier and its derived
/// challenge value.
///
/// Created by [`generate`] and consumed by the authorization flow in
/// `src/mcp/auth/flow.rs`.
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::auth::pkce::{generate, PkceChallenge};
///
/// let challenge = generate().expect("PKCE generation must not fail");
/// assert_eq!(challenge.method, "S256");
/// assert_eq!(challenge.verifier.len(), 43);
/// ```
#[derive(Debug, Clone)]
pub struct PkceChallenge {
    /// The code verifier: a base64url-encoded (no padding) random string of
    /// exactly 43 characters derived from 32 random bytes.
    ///
    /// This value is sent to the token endpoint in the `code_verifier`
    /// parameter during the authorization code exchange.
    pub verifier: String,

    /// The code challenge: the base64url-encoded (no padding) SHA-256 digest
    /// of the UTF-8 representation of [`Self::verifier`].
    ///
    /// This value is sent to the authorization endpoint in the
    /// `code_challenge` parameter.
    pub challenge: String,

    /// The challenge method.  Always `"S256"` for challenges produced by this
    /// module.
    pub method: String,
}

// ---------------------------------------------------------------------------
// Public functions
// ---------------------------------------------------------------------------

/// Generates a fresh PKCE S256 challenge.
///
/// The verifier is 32 cryptographically random bytes encoded as a
/// base64url string without padding (43 characters).  The challenge is the
/// base64url-encoded SHA-256 digest of the verifier string's UTF-8 bytes,
/// as specified in RFC 7636 section 4.2.
///
/// # Returns
///
/// A [`PkceChallenge`] containing `verifier`, `challenge`, and `method`.
///
/// # Errors
///
/// This function is infallible in practice; it returns a `Result` so that
/// callers can use `?` uniformly.  The error variant
/// [`XzatomaError::McpAuth`] would only be returned if the random number
/// generator itself failed, which does not happen on supported platforms.
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::auth::pkce::generate;
///
/// let pkce = generate().unwrap();
///
/// // Verifier is exactly 43 base64url characters (32 bytes * 4/3 rounded).
/// assert_eq!(pkce.verifier.len(), 43);
///
/// // Method is always S256.
/// assert_eq!(pkce.method, "S256");
///
/// // Verifier and challenge are distinct strings.
/// assert_ne!(pkce.verifier, pkce.challenge);
/// ```
pub fn generate() -> Result<PkceChallenge> {
    use rand::RngCore as _;

    // Step 1: 32 cryptographically random bytes.
    let mut random_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut random_bytes);

    // Step 2: base64url-encode (no padding) to produce the verifier.
    let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random_bytes);

    // Step 3: SHA-256 of the UTF-8 bytes of the verifier string
    //         (RFC 7636 section 4.2: ASCII(BASE64URL(SHA256(ASCII(code_verifier)))))
    let digest = Sha256::digest(verifier.as_bytes());

    // Step 4: base64url-encode (no padding) the digest bytes.
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest.as_slice());

    Ok(PkceChallenge {
        verifier,
        challenge,
        method: "S256".to_string(),
    })
}

/// Verifies that the authorization server supports the PKCE `S256` method.
///
/// If the server's metadata does not advertise `code_challenge_methods_supported`
/// at all, or the list does not contain `"S256"`, this function returns an
/// error.  OAuth 2.1 mandates PKCE for all public clients; refusing to proceed
/// without `S256` support is the correct security posture.
///
/// # Arguments
///
/// * `metadata` - The authorization server metadata retrieved during
///   discovery.
///
/// # Returns
///
/// `Ok(())` when `S256` is supported.
///
/// # Errors
///
/// Returns [`XzatomaError::McpAuth`] when `S256` is absent from
/// `code_challenge_methods_supported` or the field is missing entirely.
///
/// # Examples
///
/// ```
/// use xzatoma::mcp::auth::discovery::AuthorizationServerMetadata;
/// use xzatoma::mcp::auth::pkce::verify_s256_support;
///
/// let meta = AuthorizationServerMetadata {
///     issuer: "https://auth.example.com".to_string(),
///     authorization_endpoint: "https://auth.example.com/authorize".to_string(),
///     token_endpoint: "https://auth.example.com/token".to_string(),
///     registration_endpoint: None,
///     scopes_supported: None,
///     response_types_supported: vec!["code".to_string()],
///     grant_types_supported: None,
///     code_challenge_methods_supported: Some(vec!["S256".to_string()]),
///     client_id_metadata_document_supported: None,
///     extra: std::collections::HashMap::new(),
/// };
///
/// assert!(verify_s256_support(&meta).is_ok());
/// ```
pub fn verify_s256_support(metadata: &AuthorizationServerMetadata) -> Result<()> {
    let supported = metadata
        .code_challenge_methods_supported
        .as_deref()
        .unwrap_or(&[]);

    if supported.iter().any(|m| m == "S256") {
        Ok(())
    } else {
        Err(
            XzatomaError::McpAuth("PKCE S256 not supported by authorization server".to_string())
                .into(),
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use sha2::{Digest, Sha256};
    use std::collections::HashMap;

    // -----------------------------------------------------------------------
    // generate()
    // -----------------------------------------------------------------------

    #[test]
    fn test_generate_produces_correct_verifier_length() {
        let pkce = generate().expect("generate must not fail");
        assert_eq!(
            pkce.verifier.len(),
            43,
            "32 random bytes in base64url without padding produces 43 chars"
        );
    }

    #[test]
    fn test_challenge_is_correct_s256_of_verifier() {
        let pkce = generate().expect("generate must not fail");

        // Recompute the challenge from the verifier.
        let digest = Sha256::digest(pkce.verifier.as_bytes());
        let expected_challenge =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest.as_slice());

        assert_eq!(
            pkce.challenge, expected_challenge,
            "challenge must equal base64url(SHA256(verifier))"
        );
    }

    #[test]
    fn test_method_is_always_s256() {
        let pkce = generate().expect("generate must not fail");
        assert_eq!(pkce.method, "S256");
    }

    #[test]
    fn test_generate_produces_unique_verifiers() {
        let a = generate().expect("first call");
        let b = generate().expect("second call");
        assert_ne!(
            a.verifier, b.verifier,
            "successive calls must produce distinct verifiers"
        );
    }

    #[test]
    fn test_generate_produces_unique_challenges() {
        let a = generate().expect("first call");
        let b = generate().expect("second call");
        assert_ne!(
            a.challenge, b.challenge,
            "successive calls must produce distinct challenges"
        );
    }

    #[test]
    fn test_verifier_uses_url_safe_base64_no_padding() {
        let pkce = generate().expect("generate must not fail");
        // base64url characters are [A-Za-z0-9_-]; no '+', '/', or '=' allowed.
        assert!(
            pkce.verifier
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "verifier must only contain base64url characters, got: {}",
            pkce.verifier
        );
        assert!(
            !pkce.verifier.contains('='),
            "verifier must not contain padding '='"
        );
    }

    #[test]
    fn test_challenge_uses_url_safe_base64_no_padding() {
        let pkce = generate().expect("generate must not fail");
        assert!(
            pkce.challenge
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "challenge must only contain base64url characters, got: {}",
            pkce.challenge
        );
        assert!(
            !pkce.challenge.contains('='),
            "challenge must not contain padding '='"
        );
    }

    #[test]
    fn test_verifier_and_challenge_are_distinct() {
        let pkce = generate().expect("generate must not fail");
        assert_ne!(
            pkce.verifier, pkce.challenge,
            "verifier and challenge must not be equal"
        );
    }

    // -----------------------------------------------------------------------
    // verify_s256_support()
    // -----------------------------------------------------------------------

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

    #[test]
    fn test_verify_s256_support_accepts_when_present() {
        let meta = make_metadata(Some(vec!["S256".to_string()]));
        assert!(
            verify_s256_support(&meta).is_ok(),
            "must accept when S256 is in the list"
        );
    }

    #[test]
    fn test_verify_s256_support_accepts_when_present_among_others() {
        let meta = make_metadata(Some(vec!["plain".to_string(), "S256".to_string()]));
        assert!(verify_s256_support(&meta).is_ok());
    }

    #[test]
    fn test_verify_s256_support_rejects_when_absent() {
        let meta = make_metadata(Some(vec!["plain".to_string()]));
        let err = verify_s256_support(&meta).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("PKCE S256 not supported"),
            "error message should mention PKCE S256: {msg}"
        );
    }

    #[test]
    fn test_verify_s256_support_rejects_when_list_is_none() {
        let meta = make_metadata(None);
        let err = verify_s256_support(&meta).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("PKCE S256 not supported"),
            "error message should mention PKCE S256: {msg}"
        );
    }

    #[test]
    fn test_verify_s256_support_rejects_empty_list() {
        let meta = make_metadata(Some(vec![]));
        assert!(verify_s256_support(&meta).is_err());
    }

    #[test]
    fn test_verify_s256_support_is_case_sensitive() {
        // "s256" (lowercase) must not match "S256".
        let meta = make_metadata(Some(vec!["s256".to_string()]));
        assert!(
            verify_s256_support(&meta).is_err(),
            "method comparison must be case-sensitive"
        );
    }

    // -----------------------------------------------------------------------
    // Known-answer test vector
    // -----------------------------------------------------------------------

    /// Verifies the PKCE S256 implementation against a known test vector from
    /// RFC 7636 Appendix B.
    ///
    /// RFC 7636 Appendix B specifies:
    ///   code_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
    ///   code_challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    #[test]
    fn test_s256_known_answer_rfc7636_appendix_b() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let digest = Sha256::digest(verifier.as_bytes());
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest.as_slice());
        assert_eq!(
            challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
            "S256 challenge must match RFC 7636 Appendix B test vector"
        );
    }
}
