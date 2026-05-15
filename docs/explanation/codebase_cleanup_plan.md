# Codebase Cleanup Implementation Plan

## Overview

This plan prioritizes cleanup work for the Rust source tree under `src/` after a
read-only review of duplicate patterns, unused exports and suppression
attributes, error handling consistency, unfinished placeholders, stale phase
language, and security risks. Backwards compatibility is not a constraint, so
the plan intentionally removes compatibility shims, narrows public APIs, and
replaces placeholder surfaces instead of preserving them indefinitely.

The recommended implementation order is:

1. Security hardening for file, terminal, fetch, provider, ACP, and MCP
   surfaces.
2. Error handling consistency and source preservation.
3. Placeholder, stale phase reference, and unfinished feature cleanup.
4. Duplicate code consolidation across tools, providers, caches, and tests.
5. Public API, ignored test, and dead-code suppression pruning.

## Current State Analysis

### Existing Infrastructure

- The crate has a shared [`XzatomaError`](../../src/error.rs) enum and a crate
  `Result<T>` alias used by many modules.
- File tools share some utilities through
  [`file_utils.rs`](../../src/tools/file_utils.rs), including `PathValidator`,
  `ensure_parent_dirs`, and file-size checks.
- Tool execution already distinguishes infrastructure failures through
  `Err(XzatomaError)` and model-visible tool failures through `ToolResult`, but
  this policy is implicit rather than consistently documented or applied.
- Provider modules share common domain types in
  [`providers/types.rs`](../../src/providers/types.rs), including `Message`,
  `ToolCall`, `ModelInfo`, and `convert_tools_from_json`.
- ACP and MCP have modularized runtime, server, transport, and type layers, but
  several compatibility modules re-export broad public surfaces.
- The project already has documentation structure under `docs/explanation/`,
  `docs/how-to/`, `docs/reference/`, and `docs/tutorials/`.

### Identified Issues

- Security-sensitive paths have high-priority gaps:
  [`PathValidator`](../../src/tools/file_utils.rs) can accept non-existent paths
  beneath symlinked ancestors, restricted terminal mode allowlists interpreters
  and build tools, [`FetchTool`](../../src/tools/fetch.rs) validates only the
  original URL before redirects, Copilot `api_base` can redirect GitHub and
  Copilot tokens, MCP OAuth discovery trusts metadata endpoints, ACP HTTP routes
  have no authentication, and headless MCP approvals bypass confirmation.
- Error handling is inconsistent. Some public APIs return `Result<_, String>`,
  XZepr watcher code mixes `anyhow`, boxed errors, and typed errors, storage and
  provider errors often lose source chains through `to_string()`, and several
  ACP/MCP notification or cleanup results are ignored silently.
- Unfinished implementation markers remain in production code, especially MCP
  task management and sampling or elicitation handler registration. Many code
  comments and test names include stale phase labels that are historical rather
  than useful.
- Duplicate code exists in recursive directory copy logic, parent directory
  creation, custom glob matching, provider message conversion, model cache TTL
  handling, vision model heuristics, tool registration, and test provider mocks.
- Suppression attributes hide cleanup targets, including `#[ignore]` tests,
  `#[allow(dead_code)]` placeholders, one
  `#[allow(clippy::too_many_arguments)]`, and several `#[allow(deprecated)]`
  uses around deprecated model capabilities and test command helpers.
- Broad compatibility exports such as
  [`providers/base.rs`](../../src/providers/base.rs), ACP compatibility modules,
  `mcp::types::*`, and `xzepr` re-exports increase API clutter now that
  backwards compatibility is not required.

## Implementation Phases

### Phase 1: Security Hardening

#### Task 1.1 Harden workspace path validation

- Update [`PathValidator`](../../src/tools/file_utils.rs) to canonicalize the
  nearest existing ancestor for every validated path.
- Reject symlink components for create, write, edit, copy, move, and directory
  creation targets unless an explicit trusted-mode design is added later.
- Re-check validated destinations after creating parent directories in
  [`write_file.rs`](../../src/tools/write_file.rs),
  [`edit_file.rs`](../../src/tools/edit_file.rs),
  [`create_directory.rs`](../../src/tools/create_directory.rs),
  [`copy_path.rs`](../../src/tools/copy_path.rs), and
  [`move_path.rs`](../../src/tools/move_path.rs).
- Add tests for symlink escape attempts with existing targets, non-existent
  targets, nested non-existent parents, and copy or move destinations.

#### Task 1.2 Restrict command execution policy

- Remove interpreters, package managers, and build systems from the default
  restricted-mode allowlist in [`terminal.rs`](../../src/tools/terminal.rs) or
  require explicit confirmation for code-execution arguments such as `-c`, `-e`,
  `run`, and script subcommands.
- Convert restricted autonomous mode toward read-only commands by default.
- Kill timed-out commands by process group rather than only the parent process.
- Log timeout kill failures instead of ignoring them.

#### Task 1.3 Fix fetch SSRF and response-size handling

- Disable automatic redirects in [`FetchTool`](../../src/tools/fetch.rs) or add
  a redirect policy that revalidates every `Location` target.
- Revalidate final resolved hosts and block private, loopback, link-local,
  multicast, and metadata-service ranges by default.
- Stream response bodies with a hard byte cap instead of reading the full body
  before truncation.
- Add tests covering redirect-to-localhost, DNS or final-host revalidation,
  oversize responses, and allowed public redirects.

#### Task 1.4 Protect credentials and provider base URLs

- Split Copilot mock endpoints from production auth endpoints in
  [`copilot.rs`](../../src/providers/copilot.rs) so GitHub OAuth tokens are
  never sent to arbitrary `api_base` hosts.
- Enforce HTTPS and trusted host allowlists for provider base URL overrides, or
  require a clearly named unsafe configuration flag.
- Apply similar validation to OpenAI base URLs and Ollama hosts where API keys,
  prompts, or code can leave the machine.
- Centralize response-body and header redaction for provider errors and logs.

#### Task 1.5 Secure ACP and MCP control planes

- Add authentication to ACP HTTP run/session routes in
  [`acp/server.rs`](../../src/acp/server.rs), especially `/runs`, run events,
  cancel, resume, and session endpoints.
- Prevent non-loopback ACP binds unless authentication is configured.
- Add request-size and rate-limit controls for ACP HTTP routes.
- Add SSRF validation, HTTPS requirements, issuer checks, and endpoint
  allowlists to MCP OAuth discovery and token exchange in
  [`mcp/auth/discovery.rs`](../../src/mcp/auth/discovery.rs) and
  [`mcp/auth/flow.rs`](../../src/mcp/auth/flow.rs).
- Replace headless auto-approval in
  [`mcp/approval.rs`](../../src/mcp/approval.rs) with server trust and per-tool
  allowlist policy.

#### Task 1.6 Testing Requirements

- Add unit tests for path validator symlink handling and fetch redirect
  validation.
- Add policy tests for terminal restricted mode and MCP approval behavior.
- Add ACP route tests covering unauthenticated rejection and authenticated
  success.
- Add MCP auth tests for untrusted metadata endpoints, non-HTTPS endpoints, and
  issuer mismatch.

#### Task 1.7 Deliverables

- [ ] Hardened path validation and revalidation in all file mutation tools.
- [ ] Safer restricted terminal command policy and process-group timeout
      cleanup.
- [ ] Redirect-safe and streaming-size-capped fetch implementation.
- [ ] Provider base URL validation and credential redaction.
- [ ] ACP HTTP authentication and safer bind behavior.
- [ ] MCP OAuth endpoint validation and trust-aware approval policy.
- [ ] Security-focused unit tests for all changed policies.

#### Task 1.8 Success Criteria

- Symlink-based workspace escape tests fail before the change and pass after it.
- Restricted autonomous mode no longer runs arbitrary interpreter snippets or
  package scripts without confirmation.
- Fetch cannot follow redirects into blocked network ranges and never reads more
  than the configured body cap.
- Copilot GitHub tokens and provider API keys cannot be sent to untrusted hosts
  by normal configuration.
- ACP run-control endpoints reject unauthenticated requests when served over
  HTTP.
- MCP OAuth and tool approval behavior is governed by explicit trust policy.

### Phase 2: Error Handling Consistency

#### Task 2.1 Add typed parse and validation errors

- Replace `Result<_, String>` in [`chat_mode.rs`](../../src/chat_mode.rs) with
  typed parse errors for `ChatMode` and `SafetyMode`.
- Replace string errors in prompt and image validation in
  [`providers/types.rs`](../../src/providers/types.rs) with typed prompt input
  errors.
- Add `From` conversions into [`XzatomaError`](../../src/error.rs) only at crate
  boundaries where needed.
- Ensure ACP prompt conversion maps user input validation failures to protocol
  validation errors rather than generic internal errors.

#### Task 2.2 Preserve source chains and context

- Replace storage and provider `to_string()` conversions with structured
  `XzatomaError` variants that retain operation context and source errors.
- Add storage-specific variants for database open, migration, query, row decode,
  serialization, and persistence path failures.
- Reclassify runtime timeouts currently reported as configuration errors into a
  dedicated timeout or runtime error variant.
- Audit provider HTTP errors to retain status, endpoint category, and redacted
  response context.

#### Task 2.3 Normalize XZepr watcher errors

- Replace `anyhow::Result`, `anyhow!`, and boxed string errors in
  [`watcher/xzepr/watcher.rs`](../../src/watcher/xzepr/watcher.rs) and
  [`watcher/xzepr/plan_extractor.rs`](../../src/watcher/xzepr/plan_extractor.rs)
  with typed watcher and plan extraction errors.
- Convert to `XzatomaError` once at command or public API boundaries.
- Preserve sources with `#[source]` fields instead of flattening to strings.

#### Task 2.4 Handle ignored fallible operations consistently

- Add small helpers for best-effort ACP/MCP sends, cleanup actions, and flushes,
  such as domain-specific log-on-error wrappers.
- Use required-response errors as hard failures, best-effort notifications as
  debug or warn logs, and cleanup failures as diagnostic logs.
- Update ACP runtime event broadcasting so all broadcast failure paths follow
  the same policy.
- Update terminal, IDE terminal, MCP transport, MCP approval prompts, and ACP
  stdio notification paths to stop silently dropping unexpected failures.

#### Task 2.5 Clarify tool error boundaries

- Document the policy that `Err(XzatomaError)` means tool infrastructure failed,
  while `Ok(ToolResult::error(...))` means the tool ran and produced a
  model-visible operational failure.
- Audit MCP tool bridge, resource bridge, and prompt bridge paths so protocol or
  manager failures are not inconsistently downgraded to plain tool text.
- Consider adding structured `ToolResult` error metadata for retryability and
  recoverability after the policy is documented.

#### Task 2.6 Testing Requirements

- Add unit tests for typed parse and validation errors, including display text
  and `XzatomaError` conversion behavior.
- Add watcher error tests for invalid security protocol, missing SASL fields,
  invalid mechanisms, and plan extraction failures.
- Add ACP/MCP notification tests where feasible to verify failed sends are
  logged or propagated according to policy.
- Add regression tests for runtime timeout classification.

#### Task 2.7 Deliverables

- [ ] Typed chat mode, safety mode, prompt input, image, watcher, and plan
      extraction errors.
- [ ] Structured `XzatomaError` variants preserving source chains for storage,
      provider, watcher, and timeout failures.
- [ ] Common helpers for best-effort sends, flushes, cleanup actions, and logs.
- [ ] Documented `ToolExecutor` and `ToolResult` error policy.
- [ ] Updated tests for new error types and classifications.

#### Task 2.8 Success Criteria

- Public non-test APIs no longer return `Result<_, String>` for chat or prompt
  validation paths.
- Watcher code no longer uses `anyhow!` for expected configuration or extraction
  errors.
- Storage and provider errors preserve source context rather than only formatted
  strings.
- ACP/MCP send and cleanup failures are either propagated or logged through a
  documented policy.
- Tool execution errors are consistently classified as infrastructure failures
  or model-visible operational failures.

### Phase 3: Placeholder and Phase Reference Cleanup

#### Task 3.1 Resolve MCP task placeholder behavior

- Decide whether long-running MCP task polling is in scope for this cleanup.
- If in scope, implement task registration, polling, completion delivery,
  cancellation, and status notifications in
  [`mcp/task_manager.rs`](../../src/mcp/task_manager.rs) and
  [`mcp/manager.rs`](../../src/mcp/manager.rs).
- If out of scope, remove the placeholder field and API surface, make task
  responses explicitly unsupported, and return a clear typed error when a server
  returns `_meta.taskId`.
- Remove comments describing the module as a future phase placeholder.

#### Task 3.2 Wire or remove MCP sampling and elicitation stubs

- Wire existing sampling and elicitation handlers from
  [`mcp/sampling.rs`](../../src/mcp/sampling.rs) and
  [`mcp/elicitation.rs`](../../src/mcp/elicitation.rs) into manager connection
  setup when enabled.
- If the handlers cannot be supported now, remove or hide the configuration
  flags until behavior can be honored.
- Replace “not yet implemented” comments with explicit unsupported-capability
  errors where behavior remains unavailable.

#### Task 3.3 Classify unsupported provider and ACP behavior

- Replace “not yet implemented” wording for Copilot image serialization and the
  messages endpoint in [`copilot.rs`](../../src/providers/copilot.rs) with
  stable unsupported-feature errors or implement the missing behavior.
- Replace ACP artifact and multimodal unsupported paths in
  [`acp/runtime.rs`](../../src/acp/runtime.rs) with explicit capability errors
  and tests.
- Review config fields marked as ignored or future-only in
  [`config.rs`](../../src/config.rs) and either implement, hide, or document
  them as currently unsupported without historical labels.

#### Task 3.4 Remove stale phase terminology from source

- Replace phase labels in module docs, comments, and test names throughout ACP,
  MCP, skills, watcher, providers, CLI, and tool modules with feature-based
  descriptions.
- Prioritize production source comments before test section headers.
- Preserve meaningful architectural explanations while removing historical
  implementation schedule references.

#### Task 3.5 Rename harmless placeholder language

- Keep intentional user-facing fallback behavior in prompt input, mention
  parsing, MCP tool bridge, and available commands.
- Rename comments from “placeholder” to “fallback text”, “reference marker”, or
  “display hint” where no unfinished work exists.
- Keep tests focused on behavior rather than placeholder terminology.

#### Task 3.6 Testing Requirements

- Add tests for MCP task unsupported behavior or full task lifecycle behavior,
  depending on the selected scope.
- Add sampling and elicitation registration tests when handlers are wired.
- Add provider and ACP unsupported-feature tests using stable error variants.
- Run documentation linting for changed markdown files.

#### Task 3.7 Deliverables

- [ ] MCP task placeholder either implemented or removed from active API paths.
- [ ] MCP sampling and elicitation stubs either wired or hidden.
- [ ] Stable unsupported-feature errors for provider and ACP limitations.
- [ ] Stale phase references removed from Rust source comments and test names.
- [ ] Harmless fallback terminology clarified where needed.

#### Task 3.8 Success Criteria

- Source comments no longer describe code as Phase 1 through Phase 6 work.
- Production code no longer exposes placeholder APIs that silently return
  partial behavior.
- Unsupported provider, ACP, and MCP capabilities fail explicitly and are
  covered by tests.
- Remaining fallback text comments describe current behavior, not unfinished
  implementation work.

### Phase 4: Duplicate Code Consolidation

#### Task 4.1 Consolidate file operation helpers

- Move recursive directory copy logic from
  [`copy_path.rs`](../../src/tools/copy_path.rs) and
  [`move_path.rs`](../../src/tools/move_path.rs) into a shared helper in
  [`file_utils.rs`](../../src/tools/file_utils.rs).
- Replace repeated parent-directory creation with `ensure_parent_dirs` or a new
  destination-specific wrapper that includes validation rechecks from Phase 1.
- Add shared helpers for content size checks and text-file reads used by
  `write_file` and `edit_file`.
- Keep tool-specific user messages in the individual tools while sharing common
  filesystem mechanics.

#### Task 4.2 Use one glob matching implementation

- Remove custom recursive glob matching from
  [`list_directory.rs`](../../src/tools/list_directory.rs) and
  [`grep.rs`](../../src/tools/grep.rs).
- Use the existing `glob_match` crate already used by
  [`find_path.rs`](../../src/tools/find_path.rs), or add a single wrapper in
  `file_utils.rs`.
- Add shared tests for glob patterns used by list, grep, and find-path tools.

#### Task 4.3 Consolidate provider conversion and cache helpers

- Extract shared tool-call conversion and assistant response conversion helpers
  from Copilot, OpenAI, and Ollama providers where behavior is identical.
- Keep provider-specific content handling for image support, remote URLs, and
  Ollama JSON argument parsing behind small hooks.
- Add a small timed cache helper for provider model caches to replace repeated
  TTL logic in Copilot, OpenAI, and Ollama.
- Move duplicated OpenAI and Ollama vision heuristics out of ACP prompt input
  and provider modules into one provider capability source of truth.

#### Task 4.4 Simplify tool registry and test mocks

- Add registry builder helpers such as read-only tools, file mutation tools,
  terminal tools, MCP tools, and skill tools in
  [`registry_builder.rs`](../../src/tools/registry_builder.rs).
- Add a configurable test provider builder in
  [`test_utils.rs`](../../src/test_utils.rs) and replace repeated test-only
  provider mocks across agent, commands, subagent, ACP, and MCP tests.
- Extract small helpers for duplicated ACP MCP HTTP and SSE transport conversion
  branches if still present after other cleanup.

#### Task 4.5 Testing Requirements

- Preserve existing behavior with focused unit tests before replacing duplicate
  helpers.
- Add shared helper tests for directory copy, parent directory creation, glob
  matching, timed cache expiry, provider tool-call conversion, and vision
  capability detection.
- Update affected tool and provider tests to assert behavior rather than private
  implementation details.

#### Task 4.6 Deliverables

- [ ] Shared recursive directory copy and destination preparation helpers.
- [ ] Single glob matching implementation for list, grep, and find-path tools.
- [ ] Shared provider conversion helpers with provider-specific hooks.
- [ ] Reusable timed model cache helper.
- [ ] Single vision capability source of truth.
- [ ] Tool registry grouping helpers.
- [ ] Reusable test provider builder.

#### Task 4.7 Success Criteria

- File mutation tools share common filesystem operations without changing their
  user-facing messages.
- Glob behavior is consistent across list, grep, and find-path tools.
- Provider conversion logic has one implementation for common tool-call mapping
  and response conversion.
- Model cache TTL behavior is configured in one helper rather than repeated.
- Test-only provider mocks are centralized and easy to configure.

### Phase 5: Public API and Test Clutter Pruning

#### Task 5.1 Remove or narrow compatibility exports

- Remove the broad compatibility shim in
  [`providers/base.rs`](../../src/providers/base.rs) after updating doctests and
  internal references to canonical `providers` imports.
- Replace wildcard `pub use types::*` in [`mcp/mod.rs`](../../src/mcp/mod.rs)
  with explicit exports or require callers to use `mcp::types` directly.
- Remove or narrow ACP compatibility modules such as
  [`acp/events.rs`](../../src/acp/events.rs),
  [`acp/handlers.rs`](../../src/acp/handlers.rs),
  [`acp/routes.rs`](../../src/acp/routes.rs), and
  [`acp/run.rs`](../../src/acp/run.rs) if they are not needed internally.
- Remove or narrow the `xzepr` compatibility re-exports in
  [`xzepr/mod.rs`](../../src/xzepr/mod.rs).
- Remove unused command-level re-exports such as
  [`commands/chat_mode.rs`](../../src/commands/chat_mode.rs) if no longer
  needed.

#### Task 5.2 Prune dead-code suppressions

- Delete unused helpers or move test-only helpers under `#[cfg(test)]` in
  Copilot, Ollama, ACP stdio, MCP manager, MCP task manager, and plan format
  modules.
- Remove future-only fields where no current behavior reads them, including MCP
  task manager state if Task 3.1 chooses removal.
- For serde response fields kept only for deserialization, decide whether to
  remove the field or document why it is intentionally retained without
  suppressing broader dead code.

#### Task 5.3 Remove deprecated capability paths

- Remove deprecated `ModelCapability::Completion` and
  `ModelCapability::JsonMode` variants if no active behavior needs them.
- If JSON mode remains useful, un-deprecate it and remove the deprecated
  suppressions around display and Ollama capability construction.
- Replace deprecated `assert_cmd::Command::cargo_bin` test usages in
  [`commands/history.rs`](../../src/commands/history.rs) with the recommended
  helper or a shared test wrapper.

#### Task 5.4 Unignore or isolate ignored tests

- Convert ignored OpenAI wiremock tests into CI-safe tests or gate them behind a
  deliberate integration-test feature.
- Isolate ACP server and runtime tests that currently hang on shared storage by
  using per-test storage paths and runtime state.
- Serialize environment-mutating config tests with an environment lock instead
  of ignoring them.
- Split Kafka tests into pure unit tests and broker-required integration tests;
  unignore dry-run or config-only watcher tests.
- Keep only tests that truly need external services ignored, and document the
  required service and command to run them.

#### Task 5.5 Refactor too-many-arguments helper

- Replace the `#[allow(clippy::too_many_arguments)]` on ACP stdio
  `execute_queued_prompt` with a private request or context struct grouping
  session, agent, cancellation, connection, storage, conversation, and model
  data.
- Add tests around cancellation, observer execution, storage update, and
  fallback execution to preserve behavior.

#### Task 5.6 Testing Requirements

- Update doctests and import examples after removing compatibility exports.
- Run ignored-test cleanup incrementally by cluster so each cluster has clear
  ownership and failure diagnostics.
- Add tests for API surface changes where public exports are intentionally
  removed or narrowed.
- Keep code coverage above the project target while reducing ignored tests.

#### Task 5.7 Deliverables

- [ ] Compatibility re-export modules removed or narrowed.
- [ ] Dead-code suppressions eliminated or moved to legitimate test-only code.
- [ ] Deprecated capability and test helper suppressions removed.
- [ ] Ignored tests converted, gated, or documented as true external-service
      integration tests.
- [ ] ACP queued prompt execution refactored without a clippy suppression.
- [ ] Doctests and documentation imports updated to canonical paths.

#### Task 5.8 Success Criteria

- `#[allow(dead_code)]`, `#[allow(deprecated)]`, and
  `#[allow(clippy::too_many_arguments)]` usages are substantially reduced and
  every remaining use has a current, specific justification.
- Normal test runs execute all unit tests that do not require external services.
- Public exports reflect current supported APIs rather than historical
  compatibility layers.
- Deprecated model capability handling is removed or clarified as active API.

## Cross-Phase Execution Notes

- Keep phases independent where possible, but complete Phase 1 before broad file
  operation refactors so shared helpers do not preserve insecure behavior.
- Prefer typed errors and explicit unsupported behavior before deleting
  placeholder APIs so users receive actionable failures during the transition.
- Keep each pull request scoped to one phase or one major task cluster.
- Because backwards compatibility is not required, remove shims directly once
  internal tests and doctests are updated.
- Maintain documentation in `docs/explanation/` and update public doc comments
  for changed public APIs.
