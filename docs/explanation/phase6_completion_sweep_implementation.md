# Phase 6 Completion Sweep Implementation

## Summary

Phase 6 closes the highest-value residual gaps from the codebase cleanup plan.
The work focused on security policy consistency, inactive MCP task API pruning,
shared provider capability logic, shared cache usage, ignored-test ownership,
and canonical documentation paths.

## Completed work

### Security and trust policy

- Provider base URL validation now uses a shared policy that permits plaintext
  HTTP only for loopback hosts and rejects private literal IPs for remote HTTPS
  provider endpoints.
- OpenAI and Ollama provider construction use the shared provider URL policy.
- Copilot production OAuth token exchange no longer routes through
  `CopilotConfig::api_base`, so GitHub OAuth tokens are not sent to mock API
  overrides.
- MCP legacy auto-approval no longer grants trust based on headless or
  autonomous mode. Sampling rejects headless requests unless explicit trust
  metadata is added through policy-aware paths.
- MCP initialized notification failures are logged instead of silently dropped.

### Unsupported MCP task cleanup

- Removed the inactive public `mcp::task_manager` module and its ownership field
  from `McpClientManager`.
- Removed public task request helpers from the initialized MCP protocol wrapper.
- Kept explicit `_meta.taskId` handling in manager/tool bridge paths so callers
  receive a stable typed unsupported-task error rather than a partial result.

### Duplicate consolidation

- Added `providers::capabilities` as the single fallback source for provider
  vision heuristics.
- ACP prompt validation, OpenAI, and Ollama now use the shared vision capability
  helpers.
- Moved Copilot model caching to the shared provider timed cache helper used by
  other providers.
- Moved `find_path` glob matching to the shared `glob_match_pattern` helper.
- Split generic result producer configuration resolution from `rdkafka`
  `FutureProducer` construction so pure configuration tests run without broker
  or producer setup.

### Placeholder and stale terminology cleanup

- Removed source comments that described current behavior as phases,
  placeholders, or unfinished image serialization.
- Renamed ACP resource-link and mention-parser fallback wording to reference
  markers or fallback text.
- Clarified tool registry builder comments around dynamic subagent registration.

### Ignored tests and documentation

- Reclassified remaining ignored tests with explicit service or policy reasons.
- Unignored pure generic result producer configuration tests by avoiding
  `FutureProducer` construction.
- Updated documentation references from removed paths such as
  `src/providers/base.rs` and `src/xzepr/mod.rs` to canonical paths.

## Intentional remaining exceptions

The following items remain intentionally outside this sweep and are documented
for future ownership:

- Wiremock MCP OAuth and HTTP transport tests remain ignored because they use
  local HTTP endpoints that intentionally violate the production HTTPS OAuth
  validation policy. Running them requires
  `cargo test --all-features -- --ignored` and policy-specific test setup.
- System keyring tests remain ignored because they require an OS keyring.
- Real Kafka broker tests remain ignored because they require a running Kafka
  broker at `localhost:9092` and test topics.
- Full provider conversion unification remains a follow-up refactor because
  Copilot, OpenAI, and Ollama still have materially different streaming,
  response, and tool-call wire formats. Shared low-risk pieces are already
  centralized in provider types and capability/cache helpers.
- Complete replacement of all local provider mocks with `TestProviderBuilder`
  remains a broad test-suite refactor. The shared builder exists and should be
  preferred for new tests.

## Verification

Phase 6 changes should be validated with the standard project quality gates:

- `cargo fmt --all`
- `cargo check --all-targets --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`

Markdown touched by this phase should also be checked with markdownlint and
prettier.
