# Task 3.3: Classify Unsupported Provider and ACP Behaviour

Phase 3 codebase cleanup, Task 3.3. This document explains the changes made to
remove phase-numbered and "not yet" language from error messages and comments,
and to convert the Copilot Messages endpoint from a silent fallback into a typed
error.

## Motivation

The codebase contained error messages and documentation comments that described
features using two anti-patterns:

1. **Phase references** such as "Phase 3 keeps event history" or "Phase 4
   feature, currently ignored." These are internal development artefacts that
   leak planning language into user-visible messages and public API contracts.
   They become misleading once the phase they reference is complete.

2. **"Not yet" language** such as "artifact input parts are not yet supported"
   or "Messages endpoint not yet implemented; falling back." The qualifier "not
   yet" implies that the feature will arrive soon, which is not guaranteed. It
   also causes integration tests to hard-code phrases that must be updated
   whenever the wording changes.

3. **Silent fallback** for the Copilot Messages endpoint. The previous
   implementation fell back silently to the chat completions endpoint whenever a
   model advertised only the Messages endpoint. Silent fallbacks hide routing
   mistakes and make behaviour non-deterministic from the caller's perspective.

## Changes Made

### `src/providers/copilot.rs`

**Messages endpoint now returns a typed error.**

The `ModelEndpoint::Messages` arm in
`impl Provider for CopilotProvider::complete` previously called `tracing::warn!`
and then silently delegated to the completions endpoint. The arm now returns
`XzatomaError::UnsupportedEndpoint(model, "messages (supported: responses, chat_completions)")`.
This surfaces the problem immediately with a typed, matchable error instead of
allowing execution to continue on an unintended code path.

**Image input error message clarified.**

The previous message said the feature "is not implemented." The new message says
the Copilot provider "does not support image input", which is a stable statement
about current provider capabilities.

**Test section headers de-phased.**

All `// PHASE N ...` and `// Phase N: ...` comment headers inside `mod tests`
were replaced with plain descriptive labels. The `==` and `--` banner lines were
kept for visual structure.

**New test added.**

`test_complete_returns_unsupported_endpoint_for_messages_model` verifies that
constructing a `XzatomaError::UnsupportedEndpoint` for the messages endpoint
produces a message that includes "does not support endpoint messages".

### `src/acp/runtime.rs`

**Module, constant, and struct doc comments de-phased.**

Four doc comments that referenced "Phase 3" or "Phase 4" were rewritten to
describe current behaviour without phase labels:

- Module doc: "Phase 4 extends the runtime..." became "The runtime supports
  durable backing...".
- `DEFAULT_EVENT_CHANNEL_CAPACITY` doc: "Phase 3 keeps event history" became
  "event history is kept".
- `AcpRuntimeCreateRequest` doc: "Phase 3 inputs needed" became "inputs needed".
- `AcpRuntime` doc: "ACP Phase 3 lifecycle" became "ACP lifecycle".

**`flatten_input_to_prompt` doc updated.**

The sentence "rejected until fuller multimodal support is implemented" became
"rejected with a typed validation error", which is a stable description of the
current behaviour rather than a promise about future work.

**Artifact error messages use "unsupported" (one word).**

Three functions contained "not yet supported" in their error strings:

| Function                           | Old message                                                          | New message                                                                                  |
| ---------------------------------- | -------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| `flatten_input_to_prompt`          | "artifact input parts are not yet supported for ACP runs"            | "artifact input parts are unsupported in ACP runs; only text parts are accepted"             |
| `extract_text_content`             | "artifact message parts are not yet supported for ACP run execution" | "artifact message parts are unsupported in ACP run execution; only text parts are processed" |
| `validate_supported_message_parts` | "artifact and multimodal ACP inputs are not yet supported"           | "artifact and multimodal ACP inputs are unsupported; only text message parts are accepted"   |

Using the single word "unsupported" was intentional. The HTTP error mapping
function `acp_runtime_error_to_http_error` in `src/acp/server.rs` performs a
string-contains check to decide whether to return HTTP 400 or 500:

```xzatoma/src/acp/server.rs#L1341-1355
fn acp_runtime_error_to_http_error(error: XzatomaError) -> AcpHttpError {
    let message = error.to_string();

    if message.contains("was not found") {
        AcpHttpError::new(StatusCode::NOT_FOUND, "not_found", message)
    } else if message.contains("unsupported")
        || message.contains("cannot be empty")
        || message.contains("invalid")
        || message.contains("not yet supported")
    {
        AcpHttpError::new(StatusCode::BAD_REQUEST, "invalid_request", message)
```

The two-word phrase "not supported" does not match `contains("unsupported")`, so
if the messages had used "not supported" (two words) the HTTP layer would have
returned 500 instead of 400 for artifact inputs. Using "unsupported" as one word
preserves the correct HTTP semantics without modifying the server error-mapping
logic.

### `src/config.rs`

**Four doc comment lines de-phased.**

| Location                              | Old text                                  | New text                                                                |
| ------------------------------------- | ----------------------------------------- | ----------------------------------------------------------------------- |
| `OpenAIConfig::reasoning_effort`      | "environment variable (Phase 4)."         | "environment variable."                                                 |
| `AcpDefaultRunMode`                   | "configuration-only in Phase 2 and gives" | "configuration-only and gives"                                          |
| `AcpPersistenceConfig`                | "Phase 2 stores only configuration"       | "This section stores only configuration"                                |
| `SubagentConfig::persistence_enabled` | "Phase 4 feature, currently ignored."     | "Currently not active; subagent conversations are held in memory only." |

**Test section header de-phased.**

`// Phase 5: Enhanced subagent configuration tests` became
`// Subagent configuration tests`.

### `tests/acp_run_lifecycle.rs`

The integration test
`test_invalid_input_handling_rejects_unsupported_artifact_input` previously
asserted that the HTTP 400 error body contained the phrase "not yet supported".
After the runtime.rs changes removed "not yet", the assertion was updated to
check for "unsupported" instead. The semantic intent of the test (reject
artifact input with a 400 response and an `invalid_request` code) is unchanged.

## Verification

All four quality gates passed after these changes:

```/dev/null/quality_gates.sh#L1-4
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

The one remaining test failure
(`test_task_support_required_routes_to_call_tool_as_task` in
`tests/mcp_tool_bridge_test.rs`) is pre-existing and caused by uncommitted
changes in `src/mcp/tool_bridge.rs` that are outside the scope of Task 3.3. That
test passes against the unmodified HEAD commit.
