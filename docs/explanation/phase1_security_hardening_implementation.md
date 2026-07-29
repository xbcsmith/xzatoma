# Phase 1 Security Hardening Implementation

## Overview

Phase 1 of the codebase cleanup plan addresses the highest-risk security
findings in XZatoma. It closes a DNS-rebinding SSRF gap in outbound HTTP paths,
replaces a timing-unsafe token comparison, adds a URL-scheme allowlist to the
MCP elicitation opener, replaces the IDE permission auto-approve stub with a
real Zed `session/request_permission` prompt, broadens secret redaction to JSON
payloads, hardens the terminal interpreter posture, surfaces a Kafka plaintext
transport warning, and wires dependency auditing into the build tooling.

All work in this document is complete. Every quality gate in `AGENTS.md` passes:
`cargo fmt`, `cargo check --all-targets --all-features`,
`cargo clippy --all-targets --all-features -- -D warnings`,
`cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth`, and
`cargo audit --deny warnings`.

## Current State Analysis

### Existing Infrastructure

- `src/security.rs` centralizes URL validation and secret redaction so
  providers, tools, ACP, and MCP paths share one policy.
- `src/tools/fetch.rs` already performed static SSRF validation of the target
  host through `SsrfValidator` before issuing a request.
- `src/acp/ide_bridge.rs` had the `write_text_file`/`create_terminal` request
  pattern (clone connection + session id, spawn task, `send_request_to`,
  `block_task().await`) but no permission request.
- `src/tools/ide_tools.rs` `IdeRequestPermissionTool::execute` auto-approved
  every request as a placeholder.
- `src/mcp/elicitation.rs` `handle_url` opened arbitrary URLs with the OS
  opener.
- `src/tools/terminal.rs` validated commands against an allowlist/denylist.
- Kafka watcher configuration defaulted to `PLAINTEXT` without warning.

### Identified Issues

- DNS-rebinding time-of-check/time-of-use gap: a host validated once could
  resolve to a private address at connection time.
- The ACP bearer token was compared with `==`, which is not constant-time.
- No minimum token length was enforced for the ACP bind token.
- The MCP elicitation opener accepted any scheme (`file://`, `vscode://`, etc.).
- The IDE permission tool never asked the user; it always returned approved.
- Secret redaction missed JSON key/value pairs without a trailing-space marker.
- Interpreter invocations were not confirmed even in `FullAutonomous` mode.
- Plaintext Kafka transport was silent.
- No dependency vulnerability audit existed.

## Implementation Phases

### Phase 1: Security Hardening

#### Task 1.1 Foundation Work

- Added the `subtle` crate (2.6) to `Cargo.toml` for constant-time comparison.
- Added `HardenedDnsResolver` in `src/security.rs`, a `reqwest::dns::Resolve`
  implementation that resolves a host and discards every loopback, private,
  link-local, unspecified, multicast, carrier-grade-NAT, or unique-local address
  before the client connects. Resolution fails when no public address remains,
  so a rebinding response cannot reach an internal target. The module is now
  `pub` so the resolver is a first-class reusable API.
- Added `IdeBridge::request_permission` in `src/acp/ide_bridge.rs` following the
  established request pattern. It issues a real ACP `session/request_permission`
  client request and returns a `PermissionDecision`.

#### Task 1.2 Add Foundation Functionality

- H1 (fetch): `src/tools/fetch.rs` now pins the resolved public IP for the
  request and re-checks `response.remote_addr()` against
  `SsrfValidator::validate_connected_ip` so a late rebind is rejected.
- H1 (OAuth/MCP): `HardenedDnsResolver` is installed on the single production
  `reqwest::Client` built in `build_mcp_manager_from_config`
  (`src/mcp/manager.rs`) via `ClientBuilder::dns_resolver`.
- H2: `src/acp/server.rs` `constant_time_str_eq` hashes both tokens with SHA-256
  and compares digests with `subtle::ConstantTimeEq`. `validate_acp_config`
  (`src/config.rs`) enforces a minimum token length of 16 characters.
- M1: `src/mcp/elicitation.rs` `is_allowed_elicitation_url` allows only `https`
  URLs, and `handle_url` requires confirmation with the full URL.
- IDE permission: `src/tools/ide_tools.rs` `IdeRequestPermissionTool::execute`
  now calls `IdeBridge::request_permission`, offers four options (allow once,
  allow always, reject once, reject always), maps the outcome via
  `decision_from_outcome`, and fails closed (`approved: false`) on cancellation,
  missing capability, or bridge error. Tool-call ids use `Ulid`.

#### Task 1.3 Integrate Foundation Work

- M3: `src/security.rs` `redact_sensitive_text` gained a JSON-aware regex
  (`JSON_SECRET_RE`) that redacts credential-named keys (`*token*`, `*secret*`,
  `password`, `code`, `client_secret`) in addition to the marker-based
  redaction. It remains applied at the `tracing::error!` sites in
  `providers/copilot.rs`.
- M2: `src/tools/terminal.rs` documents the denylist as best-effort defense in
  depth and requires confirmation for interpreter invocations
  (`is_interpreter_invocation`) even in `FullAutonomous` mode.
- L2: `src/watcher/xzepr/watcher.rs` `apply_security_config` warns when the
  Kafka security protocol is `PLAINTEXT`.

#### Task 1.4 Testing Requirements

- `HardenedDnsResolver` tests: a loopback-only host is rejected; a public host
  yields at least one address (network-tolerant).
- ACP token comparison and minimum-length tests in `config.rs`.
- `handle_url` scheme tests reject `file://`/`vscode://`/`smb://` and accept
  `https://`.
- IDE permission flow tests via `decision_from_outcome`: allow options yield
  `Approved`, reject/unknown yield `Denied`, cancellation yields `Cancelled`.
- Redaction tests for JSON `access_token` and `client_secret` payloads.

#### Task 1.5 Deliverables

- [x] Fetch tool and OAuth/MCP path pin validated IPs (H1).
- [x] Constant-time ACP bearer-token comparison plus minimum-length check (H2).
- [x] MCP `handle_url` scheme allowlist with confirmation (M1).
- [x] `IdeBridge::request_permission` implemented; `IdeRequestPermissionTool`
      issues a real Zed `session/request_permission` prompt and honors the
      user's choice, failing closed on error.
- [x] JSON-aware secret redaction (M3).
- [x] Interpreter-invocation confirmation and best-effort denylist docs (M2).
- [x] Kafka plaintext transport warning (L2).
- [x] `cargo audit --deny warnings` wired into the `Makefile` (`make audit`),
      documented in `AGENTS.md`, with justified ignores for unmaintained
      transitive advisories in `.cargo/audit.toml` (L1).

#### Task 1.6 Success Criteria

- All new security tests pass; clippy with `-D warnings` is clean; `cargo audit`
  reports no unresolved advisories. No production fetch or OAuth/MCP path can
  reach an internal address. The IDE permission tool proceeds only on explicit
  user approval and returns `approved: false` on denial or cancellation.

## Notes

- No CI workflow was created at the user's request; auditing runs through
  `make audit` and is documented in `AGENTS.md`.
- The `.cargo/audit.toml` ignore list covers `RUSTSEC-2025-0057` (fxhash via
  sled), `RUSTSEC-2024-0384` (instant via parking_lot via sled), and
  `RUSTSEC-2024-0436` (paste via the image codec stack). These are
  unmaintained-status warnings on transitive dependencies, not exploitable
  vulnerabilities, and are documented for removal when the parent crates drop
  them.
