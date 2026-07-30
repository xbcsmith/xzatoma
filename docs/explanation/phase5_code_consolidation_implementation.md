# Phase 5: Code Consolidation (DRY) Implementation

## Overview

Phase 5 of the codebase cleanup plan removes large-scale duplication across the
provider layer, the watcher layer, and the tool layer without changing any
observable behavior. The work was split into disjoint write scopes
(`src/providers/`, `src/watcher/`, `src/tools/`) so the changes could be
implemented and verified independently and then integrated.

All work preserves behavior: existing tests continue to pass, provider-specific
divergent behavior is retained, and no environment variable, configuration key,
wire-format field name, or public command surface changed.

## What Changed

### Provider layer (`src/providers/`)

- **P7 - Shared config read-lock helper.** Added `providers/util.rs` with
  `read_config_lock`, replacing the nine verbatim
  `map_err(|_| XzatomaError::Provider("Failed to acquire read lock on config"))`
  sites across `copilot.rs`, `ollama.rs`, and `openai.rs`. The error variant and
  message are unchanged.
- **P1 - Canonical provider wire types and conversion helpers.** The
  OpenAI-style chat wire tool-call structs are now canonical in `types.rs`
  (`ChatToolCall`, `ChatFunctionCall`). `copilot.rs` and `openai.rs` alias them
  (`type CopilotToolCall = ChatToolCall;` /
  `type OpenAIToolCall = ChatToolCall;`), byte-compatible, the way `ollama.rs`
  already aliases shared types. Shared tool-call conversion
  (`chat_tool_calls_from_message`, `chat_tool_calls_to_domain`) lives in
  `providers/conversion.rs`.
- **P6 - Test boilerplate.** The inline `MockProvider` structs in
  `providers/trait_mod.rs` tests were replaced by the shared
  `TestProviderBuilder` in `src/test_utils.rs`.
- **P8 - Shared HTTP error helpers.** Added `providers/http.rs` with
  `api_error`, `provider_http_status`, `redacted_body`, and `check_response`,
  centralizing `UNAUTHORIZED` handling and body redaction. `copilot.rs`'s
  `format_copilot_api_error` and `openai.rs`'s `http_error` now delegate to
  these helpers; the multi-step Copilot token-refresh/retry control flow is
  preserved inline and only its terminal error construction is centralized.
- **P2 - Shared streaming reader/accumulator.** Added `providers/streaming.rs`
  with:
  - `ChatDeltaAccumulator<K>` - a single generic accumulator that merges the
    former `StreamAccumulator` (OpenAI, `K = u32` delta index) and
    `ChatCompletionsAccumulator` (Copilot, `K = String` call id). Being generic
    over the tool-call key type, it preserves OpenAI's numeric index ordering
    and Copilot's lexical call-id ordering exactly. The Copilot `/responses`
    endpoint keeps its own separate `ResponsesAccumulator`.
  - `LineBuffer` - byte-to-line buffering that tolerates chunk boundaries,
    reused by the OpenAI and Copilot SSE paths and the Ollama JSON-Lines path.
  - `parse_sse_line` / `SseLine` and `next_sse_data` / `SseDataEvent` - shared
    SSE line classification (`data:` prefix, `[DONE]` sentinel, comments) and an
    idle-timeout-aware reader.

### Watcher layer (`src/watcher/`)

- **P3 - Single Kafka-security module.** Added `watcher/kafka_security.rs` with
  the canonical `SecurityProtocol`, `SaslMechanism`, `SaslConfig`, and
  `SslConfig`, plus `parse_security_protocol`, `parse_sasl_mechanism`, and
  `apply_security_config`. The duplicate definitions in
  `generic/result_producer.rs`, `topic_admin.rs`, and `xzepr/consumer/config.rs`
  were deleted; `xzepr/consumer/config.rs` now re-exports the canonical types so
  existing import paths keep working.
- **P4/P5 - Watcher lifecycle helpers.** Added `watcher/lifecycle.rs` with
  `resolve_output_topic`, `build_execution_semaphore`, and `build_producer`,
  each replacing duplicated logic across both watcher backends.
- **P6 - Kafka config test builder.** Added a `Default` implementation for
  `KafkaWatcherConfig` (mirroring its serde field defaults) and replaced the
  verbatim struct literals across watcher test modules with
  `..Default::default()` plus per-test overrides.

Note: the generic result producer's security-validation errors previously used
`XzatomaError::Watcher`; routed through the shared `apply_security_config`, that
path now uses `XzatomaError::Config` (matching the pre-existing `topic_admin`
behavior). The error message content is unchanged, an error is still produced
for invalid input, and no test asserts the variant.

### Tool layer (`src/tools/`)

- **P4/P5 - Path-validation boilerplate.** Added `tools/path_tool.rs` with
  `validate_or_err`, collapsing the repeated "validate a path, and on error
  return an early `ToolResult::error`" pattern in `copy_path.rs`,
  `move_path.rs`, and `create_directory.rs`. All original error message strings
  are preserved byte-for-byte. A full `PathTool` base struct was intentionally
  not introduced: composition would only relocate the per-tool
  `path_validator`/`working_dir()` boilerplate rather than remove it, so
  introducing it would be premature abstraction (AGENTS.md "wait for 3
  examples"). The `?`-based tools (`delete_path`, `read_file`, `write_file`,
  `edit_file`, `list_directory`) were left unchanged because they intentionally
  propagate `Err(XzatomaError)` rather than returning
  `Ok(ToolResult::error(..))`.

## Documentation Updates

- `docs/reference/provider_abstraction.md`: rewrote the "File Layout" section to
  list the real module set, including the new `streaming.rs`, `http.rs`,
  `conversion.rs`, and `util.rs` modules, and replaced the nonexistent `base.rs`
  with `trait_mod.rs` + `types.rs`.
- `docs/reference/provider_api_comparison.md`: added a note mapping the
  "Implementation Recommendations" to the shared modules that now realize them.

No `XZEPR_KAFKA_*` or other security/configuration keys were renamed or
relocated, so `watcher_environment_variables.md` and `configuration.md` required
no changes. The `architecture.md` module trees are reconciled in Phase 6.

## Verification

The full AGENTS.md quality gate was run against the integrated tree:

- `cargo fmt --all` - clean.
- `cargo check --all-targets --all-features` - success.
- `cargo clippy --all-targets --all-features -- -D warnings` - zero warnings.
- `cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth` -
  `2392 passed`. The only two failures are the pre-existing `commands::history`
  tests, which spawn the compiled `xzatoma` CLI binary that a `--lib`-only run
  never builds; they are unrelated to this phase.

New unit tests were added for the shared SSE reader (buffering across chunk
boundaries, `[DONE]` handling, idle timeout), the merged `ChatDeltaAccumulator`,
the `http.rs` error helpers, the `kafka_security` parsers, and the
`validate_or_err` helper.
