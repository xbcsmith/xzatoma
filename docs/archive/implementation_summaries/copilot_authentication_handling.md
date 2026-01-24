# Copilot authentication handling

## Overview

This document describes recent changes to how the Copilot provider handles authentication failures (notably HTTP 401 Unauthorized responses that indicate an expired/invalid Copilot token). The goals were:

- Provide clearer, actionable errors to users when Copilot tokens are invalid or expired.
- Ensure the provider invalidates cached credentials (best-effort) so subsequent authentication attempts start fresh.
- Maintain clear, testable behavior and good logging for diagnostics.

## Components delivered

- Code:

 - `src/providers/copilot.rs` — added helper to map API error responses, added best-effort cache invalidation on authentication failures, and updated request error handling to return authentication-specific errors for 401 responses.
 - `src/error.rs` — added `XzatomaError::Authentication(String)` error variant to represent authentication failures explicitly.

- Tests:

 - Unit tests validating the API-error → `XzatomaError::Authentication` mapping (`test_format_copilot_api_error_unauthorized`).
 - Display test for the new `Authentication` error (`test_authentication_error_display`).

- Docs:
 - This document: `copilot_authentication_handling.md`

## Implementation details

### Problem

Copilot can return function/tool-call style responses without a `content` field, and more importantly, Copilot tokens are subject to server-side invalidation and expiry. Previously, a 401 response from Copilot surfaced as a generic provider error (HTTP status + body) without clear guidance on how to recover.

### What changed

1. New error variant

- Introduced an explicit error variant for authentication issues:

 - `XzatomaError::Authentication(String)`

 This makes handling and logging of auth failures clearer across the codebase and enables CLI UX improvements (e.g., special-purpose messaging or retry strategies in the future).

2. API error formatting helper

- A small helper was added to centralize the mapping of HTTP response status + body into a meaningful `XzatomaError`. Behavior:

 - 401 → `XzatomaError::Authentication` with an actionable message that suggests re-authenticating:
  - "Copilot returned error 401 Unauthorized: ... Token may have expired. Please run `xzatoma auth --provider copilot` to re-authenticate."
 - Other statuses → `XzatomaError::Provider` (preserves the status and body in the message).

- Example implementation (refer to provider implementation for the exact source):

```xzatoma/src/providers/copilot.rs#L230-247
fn format_copilot_api_error(status: reqwest::StatusCode, body: &str) -> XzatomaError {
  if status == reqwest::StatusCode::UNAUTHORIZED {
    XzatomaError::Authentication(format!(
      "Copilot returned error {}: {}. Token may have expired; please re-authenticate with `xzatoma auth --provider copilot`",
      status, body
    ))
  } else {
    XzatomaError::Provider(format!("Copilot returned error {}: {}", status, body))
  }
}
```

3. Best-effort cache invalidation

- When a 401 is detected, the provider performs a best-effort invalidation of the local cached token so subsequent calls to `authenticate()` will re-run the OAuth/device flow instead of repeatedly returning 401. The invalidation uses a lightweight approach (`set_password("")`) to avoid depending on platform-specific delete behavior.

- Example:

```xzatoma/src/providers/copilot.rs#L489-511
fn clear_cached_token(&self) -> Result<()> {
  match keyring::Entry::new(&self.keyring_service, &self.keyring_user) {
    Ok(entry) => {
      if let Err(e) = entry.set_password("") {
        tracing::warn!("Failed to clear cached Copilot token: {}", e);
      } else {
        tracing::info!("Cleared cached Copilot token (set empty password) in keyring");
      }
    }
    Err(e) => {
      tracing::warn!("Keyring not available while clearing cached token: {}", e);
    }
  }
  Ok(())
}
```

4. Updated request error handling

- Both `fetch_copilot_models()` and the main completion path now:
 - Detect non-success HTTP responses.
 - If 401, attempt cache invalidation (best-effort) and return `XzatomaError::Authentication` with an actionable message.
 - Otherwise, return a `XzatomaError::Provider` with status + body.

### Rationale and trade-offs

- We opted _not_ to automatically force a device flow (interactive re-auth) inside the request path. Device flow is interactive (user needs to visit a URL + enter the code), so we prefer to show an explicit, actionable error to the user and invalidate cached tokens so the next `xzatoma auth --provider copilot` behaves cleanly.
- Best-effort invalidation uses `set_password("")` to avoid platform-specific delete semantics for keyrings. This makes the invalidation robust across environments; `authenticate()` will detect invalid/empty cached tokens and then start the device flow as appropriate.
- The behavior is conservative: we avoid automatic blocking UI interactions and provide clear guidance. Future improvements could optionally attempt a non-interactive re-exchange of the GitHub token if available, or provide a configurable "auto-retry" switch for interactive sessions.

## Testing

- Unit tests added:

 - `test_format_copilot_api_error_unauthorized`: ensures 401 maps to `XzatomaError::Authentication`.
 - `test_format_copilot_api_error_other`: ensures non-401 errors map to `XzatomaError::Provider`.
 - `test_authentication_error_display` (in `src/error.rs`): validates the `Authentication` display string.

- Manual verification:
 - Simulated 401 response yields an error message containing "token expired" and clear instruction to run:
  ```
  Copilot returned error 401 Unauthorized: unauthorized: token expired. Token may have expired. Please run `xzatoma auth --provider copilot` to re-authenticate.
  ```

## Usage examples

- If Copilot returns 401 due to an expired token, the CLI will show an authentication-focused error message and logs will contain a note about cache invalidation. Example (user-facing):

```/dev/null/example.txt#L1-1
Copilot returned error 401 Unauthorized: unauthorized: token expired. Token may have expired. Please run `xzatoma auth --provider copilot` to re-authenticate.
```

- To recover:

 - Run: `xzatoma auth --provider copilot`
 - Follow the device-flow instructions printed by the CLI (visit verification URL and enter code).
 - Re-run your model/list or completion command.

## Validation results

- Formatting: `cargo fmt --all` applied successfully.
- Compile: `cargo check --all-targets --all-features` finished with 0 errors.
- Lint: `cargo clippy --all-targets --all-features -- -D warnings` — no warnings.
- Tests: `cargo test --all-features` — unit tests (including new tests) passed.

## Notes / Next steps

- Add an integration test that mocks the Copilot `/models` and completions endpoints to verify end-to-end behavior (including cache invalidation and retry flows).
- Consider adding optional non-interactive copilot token refresh using the cached GitHub token when available (with careful error handling and telemetry).
- Add telemetry to surface repeated authentication failures so we can detect token invalidation events in the field.
- Document the auth-recovery steps in the user-facing `how-to` docs (how to `xzatoma auth` for each provider).

## References

- Source changes:
 - `src/providers/copilot.rs` — API error mapping & cache invalidation additions
 - `src/error.rs` — new `XzatomaError::Authentication` variant
- Related docs:
 - `copilot_dynamic_model_fetching.md` (model management)
 - This file: `copilot_authentication_handling.md`
