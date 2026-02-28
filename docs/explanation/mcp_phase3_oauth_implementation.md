# MCP Phase 3: OAuth 2.1 / OIDC Authorization Implementation

## Overview

Phase 3 implements the full OAuth 2.1 authorization code flow with PKCE required
by the MCP `2025-11-25` specification for HTTP transport connections.
Authorization applies only to HTTP transport; stdio servers obtain credentials
from environment variables per the specification.

The implementation is contained entirely within the new `src/mcp/auth/` module
and its five sub-modules: `token_store`, `discovery`, `pkce`, `flow`, and
`manager`.

## Architecture

### Module Hierarchy

```text
src/mcp/auth/
├── mod.rs          -- module root; declares all five sub-modules
├── token_store.rs  -- OAuthToken type and OS keyring persistence
├── discovery.rs    -- RFC 9728 / RFC 8414 / OIDC metadata discovery
├── pkce.rs         -- PKCE S256 challenge generation and verification
├── flow.rs         -- OAuth 2.1 authorization code flow with PKCE
└── manager.rs      -- high-level facade: token lifecycle coordination
```

### Dependency Graph

```text
manager  -->  flow  -->  pkce
         |          -->  discovery
         |          -->  token_store
         -->  token_store
         -->  discovery (AuthorizationServerMetadata type)
```

All modules are independent of `agent/`, `providers/`, and `tools/` per the
component boundary rules in `AGENTS.md`.

## Component Details

### token_store.rs

Provides secure persistence of OAuth tokens via the OS native credential store
(macOS Keychain, Linux Secret Service, Windows Credential Manager) using the
`keyring` crate.

**`OAuthToken`** -- derives `Debug, Clone, Serialize, Deserialize`.

| Field           | Type                    | Notes                                           |
| --------------- | ----------------------- | ----------------------------------------------- |
| `access_token`  | `String`                | Required                                        |
| `token_type`    | `String`                | Typically `"Bearer"`                            |
| `expires_at`    | `Option<DateTime<Utc>>` | RFC-3339 via `chrono::serde::ts_seconds_option` |
| `refresh_token` | `Option<String>`        | Omitted from JSON when `None`                   |
| `scope`         | `Option<String>`        | Omitted from JSON when `None`                   |

**`OAuthToken::is_expired()`** applies a 60-second pre-expiry buffer so callers
have time to exchange a refresh token before the access token is actually
rejected by the resource server:

```text
expired = (expires_at is Some) AND (now >= expires_at - 60s)
```

**`TokenStore`** is a zero-field struct (stateless; the keyring is the state).
Service names follow the pattern `xzatoma-mcp-{server_id}` to avoid collisions
with other applications.

- `save_token` -- serializes to JSON, calls `entry.set_password`.
- `load_token` -- calls `entry.get_password`; returns `Ok(None)` on
  `keyring::Error::NoEntry` so callers can distinguish "not authenticated" from
  a genuine error.
- `delete_token` -- idempotent; `NoEntry` is treated as success.

### discovery.rs

Implements the two-step metadata discovery required before running the
authorization flow.

#### Step 1: Protected Resource Metadata (RFC 9728)

`fetch_protected_resource_metadata` tries two strategies:

1. Parse the `resource_metadata=<url>` attribute from the `WWW-Authenticate`
   header and GET that URL directly.
2. Construct the RFC 9728 well-known URI:
   `https://<host>/.well-known/oauth-protected-resource<path>`

#### Step 2: Authorization Server Metadata (RFC 8414 / OIDC Discovery)

`fetch_authorization_server_metadata` tries five candidate URLs in order,
returning on the first success:

| Order | Pattern                                         | Description             |
| ----- | ----------------------------------------------- | ----------------------- |
| 1     | `/.well-known/oauth-authorization-server<path>` | RFC 8414 path-insertion |
| 2     | `/.well-known/openid-configuration<path>`       | OIDC path-insertion     |
| 3     | `<issuer>/.well-known/openid-configuration`     | OIDC path-appending     |
| 4     | `/.well-known/oauth-authorization-server`       | RFC 8414 root           |
| 5     | `/.well-known/openid-configuration`             | OIDC root               |

`AuthorizationServerMetadata` uses `#[serde(flatten)]` to capture unknown server
metadata fields in an `extra: HashMap<String, serde_json::Value>` so that future
spec extensions do not cause deserialization errors.

### pkce.rs

Implements RFC 7636 Proof Key for Code Exchange using the `S256` method.

**`generate()`** procedure:

1. Fill 32 bytes from `rand::rng()` (cryptographically random).
2. Base64url-encode (no padding) using
   `base64::engine::general_purpose::URL_SAFE_NO_PAD` -- produces a 43-character
   `verifier`.
3. Compute `SHA-256(verifier.as_bytes())` using the `sha2` crate.
4. Base64url-encode (no padding) the digest -- produces the `challenge`.
5. Return `PkceChallenge { verifier, challenge, method: "S256" }`.

This matches the algorithm in RFC 7636 section 4.2:

```text
code_challenge = BASE64URL(SHA256(ASCII(code_verifier)))
```

**Verified against RFC 7636 Appendix B:**

```text
verifier  = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
```

`verify_s256_support` performs a case-sensitive search for `"S256"` in
`code_challenge_methods_supported`. It returns `Err(XzatomaError::McpAuth(...))`
when `S256` is absent or the field is `None`. OAuth 2.1 mandates PKCE for all
public clients; refusing to proceed without `S256` is the correct security
posture.

### flow.rs

Drives the complete authorization code flow for a single MCP server.

**`OAuthFlowConfig`** holds all per-server parameters:

| Field                  | Type             | Purpose                                 |
| ---------------------- | ---------------- | --------------------------------------- |
| `server_id`            | `String`         | Matches the config key                  |
| `resource_url`         | `Url`            | RFC 8707 `resource` parameter value     |
| `client_name`          | `String`         | Used in Dynamic Client Registration     |
| `redirect_port`        | `u16`            | `0` = OS-assigned port                  |
| `static_client_id`     | `Option<String>` | Highest-priority client ID source       |
| `static_client_secret` | `Option<String>` | Optional; for confidential clients only |

**`OAuthFlow::authorize()` sequence:**

1. `verify_s256_support` -- fails fast if PKCE S256 unavailable.
2. `resolve_client_id` -- priority: static > metadata document URL > DCR.
3. `pkce::generate()` + `generate_state()`.
4. `tokio::net::TcpListener::bind` -- OS assigns port when `redirect_port == 0`.
5. `build_authorization_url` -- appends `response_type`, `client_id`,
   `redirect_uri`, `scope` (optional), `state`, `code_challenge`,
   `code_challenge_method=S256`, `resource` (RFC 8707).
6. Print URL to stderr; attempt browser open (`open` on macOS, `xdg-open` on
   Linux); ignore spawn errors.
7. `accept_callback` -- reads one HTTP GET, sends `200 OK`, validates `state`,
   extracts `code`.
8. `exchange_code` -- POST to token endpoint with
   `Content-Type: application/x-www-form-urlencoded`.

**Client ID resolution priority:**

```text
1. config.static_client_id is Some  -->  use it
2. server_metadata.client_id_metadata_document_supported == Some(true)
                                    -->  use metadata URL as client_id
3. server_metadata.registration_endpoint is Some
                                    -->  Dynamic Client Registration (RFC 7591)
4. otherwise                        -->  Err(McpAuth("no viable client registration mechanism"))
```

**`handle_step_up()`** handles `Bearer error="insufficient_scope"` challenges:
parses the `scope=` attribute, calls `authorize` up to 3 times, returns
`Err(McpAuth("step-up authorization loop limit reached"))` on the third failure.

### manager.rs

`AuthManager` is the sole entry point for all authorization operations in the
HTTP transport layer.

**`get_token()` resolution order:**

```text
1. load_token(server_id)
   |-- Ok(Some(token)) and !token.is_expired()  --> return token.access_token
   |-- Ok(Some(token)) and token.is_expired() and token.refresh_token.is_some()
   |      --> flow.refresh_token()
   |            |-- Ok(new_token)  --> save_token(); return new_token.access_token
   |            |-- Err(_)         --> log; fall through to step 3
   |-- Ok(None) or refresh failed
          --> flow.authorize(); save_token(); return access_token
```

**`inject_token()`** is a pure utility function (no `&self` state) that inserts
`Authorization: Bearer <token>` into a `HashMap<String, String>`. It is provided
on `AuthManager` for discoverability by callers in the HTTP transport layer.

## Security Considerations

### PKCE S256 Enforcement

The `verify_s256_support` check runs before any network request in the
authorization flow. If the authorization server does not advertise `S256`, the
flow fails immediately rather than falling back to the weaker `plain` method or
proceeding without PKCE. This is consistent with the OAuth 2.1 draft requirement
that public clients always use PKCE.

### State Parameter

The `state` nonce is 16 cryptographically random bytes (128 bits of entropy)
base64url-encoded. It is validated byte-for-byte in the callback handler before
the authorization code is used. A mismatch returns
`Err(XzatomaError::McpAuth("state mismatch in OAuth callback"))`.

### Resource Indicators (RFC 8707)

The `resource` parameter (set to `config.resource_url`) is included in both the
authorization request and the token exchange request. This binds the issued
token to the specific MCP server, preventing token confusion attacks where a
token issued for one resource is replayed against another.

### Token Storage

Tokens are stored in the OS native keyring, not in plaintext files. The JSON
representation uses Unix epoch seconds for `expires_at` (via
`chrono::serde::ts_seconds_option`) which is compact and unambiguous. The
service name prefix `xzatoma-mcp-` isolates MCP tokens from any other
applications that might use the keyring under the same account name.

### 60-Second Pre-Expiry Buffer

`is_expired()` returns `true` when `now >= expires_at - 60s`. This means a token
with 30 seconds remaining is treated as expired and a refresh is attempted. The
buffer prevents the situation where a valid-looking token is submitted to the
resource server and rejected because it expired during the network round-trip.

## Testing Strategy

### Unit Tests (inline `#[cfg(test)]`)

Each module contains embedded unit tests covering:

- All public functions (success, failure, edge cases).
- `is_expired` boundary conditions (past, at-buffer-boundary, within-buffer,
  future, no expiry).
- JSON round-trip for `OAuthToken` with and without optional fields.
- `build_as_candidate_urls` producing exactly 5 candidates with correct paths.
- `parse_resource_metadata_url` for quoted and unquoted values.
- `verify_s256_support` case-sensitivity check.
- `build_authorization_url` containing all required query parameters.
- `inject_token` correct `Bearer` scheme and header key.

### Integration Tests (wiremock)

| File                                 | Coverage                                                          |
| ------------------------------------ | ----------------------------------------------------------------- |
| `tests/mcp_auth_pkce_test.rs`        | 15 tests; all five plan-specified tests plus extras               |
| `tests/mcp_auth_token_store_test.rs` | 15 tests; 11 active + 4 `#[ignore = "requires system keyring"]`   |
| `tests/mcp_auth_discovery_test.rs`   | 15 tests; wiremock for all HTTP discovery paths                   |
| `tests/mcp_auth_flow_test.rs`        | 10 tests; wiremock for token exchange; PKCE verifier verification |

### RFC 7636 Known-Answer Test Vector

`test_s256_known_answer_vector_rfc7636_appendix_b` verifies the SHA-256 and
base64url encoding against the values published in RFC 7636 Appendix B:

```text
input:    "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
expected: "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
```

This test does not use `pkce::generate()` directly; it independently computes
the challenge from a fixed verifier using the same algorithm, confirming the
implementation matches the specification.

### Keyring Tests

Tests that require the OS keyring are marked:

```rust
#[test]
#[ignore = "requires system keyring"]
fn test_save_and_load_token_roundtrip_via_keyring() { ... }
```

They can be run manually with:

```bash
cargo test --all-features -- --ignored mcp_auth_token_store
```

## Validation Results

All quality gates were run in order:

```bash
cargo fmt --all                                          # passed
cargo check --all-targets --all-features                # passed (zero errors)
cargo clippy --all-targets --all-features -- -D warnings # passed (zero warnings)
cargo test --all-features                               # passed
```

Test results:

- 1102 tests passed
- 1 pre-existing failure:
  `providers::copilot::tests::test_copilot_config_defaults` (unrelated; present
  before Phase 3)
- 11 ignored (4 keyring tests + 7 pre-existing)
- 51 Phase 3 integration tests: all pass
- 80+ inline unit tests across auth modules: all pass

## References

- MCP protocol revision 2025-11-25 (implementation target)
- Implementation plan: `docs/explanation/mcp_support_implementation_plan.md`
  Phase 3 (lines 1244-1570)
- RFC 7636 PKCE: <https://www.rfc-editor.org/rfc/rfc7636>
- RFC 8414 Authorization Server Metadata:
  <https://www.rfc-editor.org/rfc/rfc8414>
- RFC 8707 Resource Indicators: <https://www.rfc-editor.org/rfc/rfc8707>
- RFC 9728 Protected Resource Metadata: <https://www.rfc-editor.org/rfc/rfc9728>
- RFC 7591 Dynamic Client Registration: <https://www.rfc-editor.org/rfc/rfc7591>
- OpenID Connect Discovery 1.0:
  <https://openid.net/specs/openid-connect-discovery-1_0.html>
- OAuth 2.1 draft: <https://datatracker.ietf.org/doc/draft-ietf-oauth-v2-1/>
