# Security Hardening Implementation

## Overview

Phase 1 of the codebase cleanup plan hardened the project’s file, terminal,
fetch, provider, ACP, and MCP security boundaries. The implementation focuses on
preventing workspace escapes, reducing implicit command and MCP trust, blocking
network-based credential exfiltration paths, and adding ACP HTTP controls.

## Implemented Changes

### Workspace Path Validation

- Hardened `PathValidator` in `src/tools/file_utils.rs` to canonicalize the
  workspace root and nearest existing ancestor.
- Rejected symbolic link components during validation so newly created paths
  cannot traverse through a symlinked ancestor.
- Revalidated write, edit, copy, move, and create-directory destinations after
  parent directory creation.
- Added tests for symlink ancestors and existing symlink targets.

### Terminal Execution Policy

- Restricted autonomous terminal mode now defaults to read-oriented commands.
- Removed interpreters, package managers, compilers, build tools, and `git` from
  the restricted allowlist.
- Added argument checks for `find -exec`, `find -execdir`, and `find -delete`.
- Timeout handling now attempts to terminate the full process group on Unix and
  process tree on Windows, and logs failures instead of silently dropping them.

### Fetch SSRF and Size Controls

- Disabled automatic redirects in the fetch HTTP client.
- Revalidated the final response URL before reading the body.
- Replaced full-body buffering with streaming reads capped at the configured
  maximum byte size.
- Added regression coverage for response truncation.

### Provider URL and Secret Protections

- Added shared security helpers in `src/security.rs` for HTTP base URL
  validation, loopback-only overrides, public HTTPS OAuth URL validation, same
  origin checks, and diagnostic redaction.
- Made `CopilotConfig::api_base` skip serialization and deserialization so
  config files cannot redirect GitHub or Copilot tokens.
- Limited programmatic Copilot `api_base` overrides to loopback mock servers.
- Validated and normalized OpenAI base URLs and Ollama hosts at provider
  construction and configuration validation time.
- Redacted sensitive provider response diagnostics before logging or returning
  errors.

### ACP HTTP Controls

- Added `acp.auth_token`, `acp.max_request_bytes`, and
  `acp.rate_limit_per_minute` configuration fields with environment variable
  overrides.
- Added bearer-token authentication middleware for ACP HTTP routes when a token
  is configured.
- Added a global sliding-window rate limiter for ACP HTTP routes.
- Added `DefaultBodyLimit` enforcement for ACP HTTP request bodies.
- Rejected non-loopback ACP bind addresses unless `acp.auth_token` is set.
- Added route tests for auth rejection, auth success, rate limiting, request
  body limiting, and non-loopback bind behavior.

### MCP OAuth and Approval Policy

- Added per-server MCP approval policy types with explicit `allow`, `prompt`,
  and `deny` actions.
- MCP tool, resource, and prompt bridge executors now use trust-aware approval
  decisions rather than automatically approving headless runs.
- Headless MCP operations without an explicit allow policy are denied before the
  server is contacted.
- Added MCP OAuth validation for HTTPS, public endpoints, issuer match,
  same-origin authorization, token, and registration endpoints.
- Disabled redirects in the MCP OAuth HTTP client.
- Implemented secure `oauth.metadata_url` override handling instead of leaving
  it ignored.
- Added tests for approval policy decisions and OAuth metadata validation.

## Security Outcomes

- Workspace file mutation tools no longer create or write through symlinked
  ancestors outside the project root.
- Restricted terminal mode no longer executes arbitrary interpreter snippets,
  package scripts, or build tools by default.
- Fetch requests cannot automatically follow redirects into internal network
  targets and no longer buffer oversized responses before truncating.
- Copilot token exchange cannot be redirected to untrusted hosts through a YAML
  configuration file.
- ACP run-control routes can be protected with bearer-token auth and cannot bind
  externally without auth.
- MCP tool execution in headless contexts requires explicit server trust and
  operation allow rules.
- MCP OAuth discovery and token exchange reject insecure, private, mismatched,
  or cross-origin endpoints.

## Validation

The following quality gates were run successfully after implementation:

- `cargo fmt --all`
- `cargo check --all-targets --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features -- --format terse`

Markdown formatting and linting were also applied to this implementation
summary.
