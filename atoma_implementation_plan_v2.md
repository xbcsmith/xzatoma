# Implementations

## fix(agent): use model context window for initial UsageUpdate size instead of config default - 2026-06-24

**Files Changed:**

- `src/commands/agent.rs` - In both `NewSessionRequest` and `LoadSessionRequest`
  handlers, added `initial_context_window` capture that reads
  `model_info.as_ref().map(|info| info.context_window).unwrap_or(max_tokens)`.
  The initial `UsageUpdate` now sends `initial_context_window` as the `size`
  field instead of `max_tokens`. Updated
  `test_initial_usage_update_used_equals_conversation_token_count` to reflect
  the fallback-only semantics. Added
  `test_initial_usage_update_size_uses_model_context_window_over_config` to pin
  that a 262k model context window is not overridden by the 100k config default.

**Summary:** The initial `UsageUpdate` sent when a Zed ACP session opens was
using `config.agent.max_conversation_tokens` (default: 100,000) as the context
window size. The provider is already queried for `model_info` in the same
handler block to determine vision/thinking capabilities. For llama.cpp-based
servers the response includes `meta.n_ctx` (e.g. 262,144 for Gemma-4). The fix
reads `model_info.context_window` for the initial `UsageUpdate.size` and falls
back to the config value only when the provider query failed. The pruning budget
(`Conversation.max_tokens`) is unchanged — it is a separate concern from the
display bar size.

**Testing:**
`test_initial_usage_update_size_uses_model_context_window_over_config` passes.
Full suite: 810 tests pass. `cargo clippy -- -D warnings` clean.

## fix(tool): include captured output in ToolResult content when command exits non-zero - 2026-06-24

**Files Changed:**

- `src/agent/tool.rs` - Fixed `ToolResult::content()`: when both `output` and
  `error` are present (`failure_with_output` case), the method now returns
  `"{output}\nError: {err}"` instead of discarding `output` and returning only
  `"Error: {err}"`. Plain failures with no output are unchanged. Updated the
  `test_tool_result_failure_with_output` test to expect the corrected format.
  Added `test_failure_with_output_content_includes_output_before_error` to pin
  the exact contract: output appears before the error label.

**Summary:** `AgentExecutor::execute_iteration` calls `tool_result.content()`
and passes the result both to `conversation.add_tool_result` (the LLM's next
prompt) and to `ToolCallEvent::Failed` (which `agent.rs` forwards to Zed as a
`ToolCallUpdate` with `ToolCallStatus::Failed`). Because `content()` was
discarding `output` whenever `error` was set, any command that exited non-zero
(e.g. `ruff check`, `mypy`, `cargo check`) caused the LLM to see only
`"Error: Command exited with code 1"` and the chat-window card to show the same
thing — hiding every diagnostic line the command actually printed. Fixing
`content()` propagates the full captured output through both paths with a single
one-line change.

**Testing:** `test_tool_result_failure_with_output` and
`test_failure_with_output_content_includes_output_before_error` pass. Full
suite: 810 tests pass (`cargo test --all-features`).
`cargo clippy --all-targets --all-features -- -D warnings` clean.

## fix(agent): restore agent identity so ACP sessions execute instead of only planning - 2026-06-24

**Files Changed:**

- `src/commands/agent.rs` - Added `AGENT_ACP_BASE_PROMPT` constant that gives
  the model its agent identity and an unambiguous mandate to use tools
  autonomously. Updated both `NewSessionRequest` and `LoadSessionRequest`
  handlers to combine this base prompt with `PLAN_ANCHOR_INSTRUCTION` before
  calling `executor.set_system_prompt`, replacing the previous call that set
  only `PLAN_ANCHOR_INSTRUCTION` as the sole system prompt.
- `src/agent/plan_tracker.rs` - Strengthened `PLAN_ANCHOR_INSTRUCTION` wording:
  replaced "After the list, proceed with the actual work" (ambiguous — models
  often interpret this as writing more text) with "Immediately after the list,
  start calling tools to execute the steps — do not explain or describe the
  steps again, just do them."

**Summary:** Phase 6 introduced a `set_system_prompt` call in the ACP session
handlers that used only `PLAN_ANCHOR_INSTRUCTION` as the system prompt content.
Before Phase 6, no system prompt was set and the model ran on training defaults
(which include tool-use behavior). The single-sentence plan instruction gave the
model no identity, no tool-use mandate, and vague execution guidance, causing it
to treat the numbered plan itself as the completed turn and stop. The fix adds
`AGENT_ACP_BASE_PROMPT` as the base and concatenates `PLAN_ANCHOR_INSTRUCTION`
after it. With a user-configured `agent.system_prompt`, the composed prompt is
`{user_prefix}\n\n{base}\n\n{plan_anchor}`.

**Testing:** 810 tests pass (`cargo test --all-features`).
`cargo clippy --all-targets --all-features -- -D warnings` passes clean.

## test(agent): add missing Phase 3/5/6 tests from acp_features_implementation.md - 2026-06-24

**Files Changed:**

- `src/commands/agent.rs` - Added 5 missing unit/integration-simulation tests:
  - `test_thinking_mode_change_emits_both_current_mode_and_config_option_updates`
    (Ph3): verifies that a thinking-mode `SetSessionModeRequest` produces both
    `CurrentModeUpdate` and `ConfigOptionUpdate` payloads in the correct order.
  - `test_session_info_update_notification_built_with_derived_title` (Ph5):
    verifies that the full `SessionInfoUpdate` notification is constructed with
    the derived title (not just that the guard passes).
  - `test_available_commands_names_match_tool_registry_tool_names` (Ph5):
    cross-checks that every name produced by `ToolRegistry::tools()` appears in
    `ToolRegistry::tool_names()` and in the serialised
    `AvailableCommandsUpdate`.
  - `test_prompt_streaming_two_item_plan_yields_two_plus_final_plan_notifications`
    (Ph6): simulates the `PromptRequest` streaming loop for a two-item numbered
    list and asserts exactly two plan-change signals plus one finalize signal.
  - `test_prompt_streaming_no_numbered_list_yields_no_plan_notifications` (Ph6):
    simulates the streaming loop for prose-only output and asserts zero
    plan-change and finalize signals.

**Summary:** Closes the gap identified by the deliverables audit against
`docs/explanation/acp_features_implementation.md`. All five tests follow the
established data-construction / streaming-simulation pattern used in the
existing Phase 1-6 test suite; none make real network connections or spawn
external processes.

**Testing:** All 5 new tests pass (`cargo test --all-features -- <names>`).

## feat(agent): Phase 6 plan tracking via `SessionUpdate::Plan` - 2026-06-23

**Files Changed:**

- `src/agent/plan_tracker.rs` - New file. Implements `PlanTracker` struct with
  `update`, `mark_boundary`, `finalize`, `reset_to_pending`, and `as_plan`
  methods. Contains `PLAN_ANCHOR_INSTRUCTION` constant used to seed the system
  prompt. Exports `TrackedEntry` for inspection in tests. 24 unit tests.
- `src/agent/mod.rs` - Added `pub mod plan_tracker;` declaration.
- `src/commands/agent.rs` - Added `Plan` to ACP schema imports; added
  `build_system_prompt` to executor imports. In both `NewSessionRequest` and
  `LoadSessionRequest` handlers: merged `config.agent.system_prompt` with
  `PLAN_ANCHOR_INSTRUCTION` and called `executor.set_system_prompt`. In the
  `PromptRequest` handler: created `Arc<StdMutex<PlanTracker>>` per turn;
  extended the stream notifier to feed non-thinking chunks into the tracker and
  emit `SessionUpdate::Plan` when new entries appear; marked the boundary on
  `IterationResult::Continue`; finalized and emitted a final Plan notification
  after the iteration loop. Added 4 unit tests.
- `tests/integration/agent_prompt.rs` - Updated
  `test_new_session_always_starts_fresh` to use `message_count()` instead of
  `messages().len()` because the new system prompt adds a system message that
  must not be counted as prior conversation history.

**Summary:** Implements the plan-tracking feature described in Phase 6 of
`docs/explanation/acp_features_implementation.md`. As the assistant streams its
response, `PlanTracker` buffers text and detects numbered-list items
(`N. description`) in the first iteration's output. Each newly detected item
promotes the previous entry from `Pending` to `InProgress` and adds a new
`Pending` entry; a `SessionUpdate::Plan` notification is emitted so the Zed
thread panel updates in real time. After the streaming loop ends, `finalize`
promotes remaining entries to `Completed` and emits a final notification. A
`PLAN_ANCHOR_INSTRUCTION` constant is merged into the system prompt of every new
or loaded Zed ACP session to encourage the model to start responses with a
numbered list.

**Testing:** 810 tests pass (`cargo test --all-features`).
`cargo clippy --all-targets --all-features -- -D warnings` passes clean.
`cargo fmt --all` produces no changes.

## fix: Ollama context-window extraction handles versioned architecture names - 2026-06-23

**Files Changed:**

- `src/providers/ollama.rs` - Rewrote `extract_context_window` to scan all keys
  ending in `.context_length` rather than matching against a hardcoded
  architecture prefix list. Removed the `family` parameter. Fixed
  `list_models_summary` to delegate to `list_models()` (which fetches
  `/api/show` per model) instead of hardcoding `4096` for every model.

**Summary:** Two bugs in the Ollama provider caused incorrect context-window
values.

1. `extract_context_window` tried `{family}.context_length` using the
   `details.family` string from `/api/tags`, then fell back to a hardcoded list
   of known prefixes. This missed any model whose architecture key does not
   exactly match the family string — e.g. `/api/show` for `gemma4:e4b-mlx`
   returns `gemma4.context_length` but the tag details report family `gemma`, so
   both attempts failed and the function returned the `4096` fallback. The fix
   scans all keys ending in `.context_length` and takes the maximum, which works
   for any model family without a maintained list.

2. `list_models_summary` did not call `/api/show` at all and hardcoded `4096` as
   the context window for every model. It now delegates to `list_models()` so
   the real context window is used.

**Testing:** 806 tests pass.

## fix: read context-window size from provider API response - 2026-06-23

**Files Changed:**

- `src/providers/openai.rs` - Added `ModelMeta` struct with `n_ctx` /
  `n_ctx_train` fields that deserialise from the `meta` object returned by
  llama.cpp-compatible servers. Added `ModelMeta::context_window()` helper.
  `ModelObject` now carries an optional `meta: ModelMeta` field. Both
  `fetch_models` and `get_model_info` now use `meta.context_window()` as the
  primary source of context-window size, falling back to the static
  `context_window_for_model_id` table only when the server does not supply the
  field (vanilla OpenAI).

**Summary:** The `/v1/models` response from llama.cpp-compatible servers
(including the internal pipeline-ai-server) already carries `meta.n_ctx`, the
exact context window the server was started with. Reading it directly is correct
and requires no maintenance. The static lookup table is kept as a fallback for
servers such as api.openai.com that return minimal model objects without a
`meta` field.

**Testing:** 806 tests pass.

- `src/agent/executor.rs` - After every provider response that includes usage
  data, call
  `conversation.calibrate_token_count(prompt_tokens + completion_tokens)` so the
  context-bar numerator reflects real token counts instead of the `len / 4`
  approximation.
- `src/providers/openai.rs` - Added `context_window_for_model_id(id)` lookup
  table (pattern-matched on model ID prefix) and threaded it into both
  `fetch_models` and `get_model_info` to replace the unconditional `128_000`
  placeholder. Notable values: `gpt-4.1` family → 1 047 576, `o1`/`o3`/`o4` →
  200 000, `gpt-4-32k` → 32 768, `gpt-3.5-turbo` → 16 385, default → 128 000.

**Summary:** Two independent bugs caused the context-window bar to read far
below actual usage.

1. The per-message token heuristic (`text.len() / 4`) is a reasonable
   approximation for English prose but underestimates code-heavy conversations
   where many tokens are short identifiers, operators, and punctuation. The
   provider's API response already contains the real `prompt_tokens` and
   `completion_tokens`; `execute_iteration` was adding these only to the
   cumulative cost tracker and never updating the context-bar counter.
   `calibrate_token_count` replaces the heuristic total after every turn.

2. Every model was assigned `context_window = 128_000` regardless of the actual
   model. Users on `gpt-4.1` (1 M context) or `o3` (200 k) were seeing an
   inflated percentage. A per-model lookup table, matching Zed's own
   `open_ai::Model::max_token_count()` logic, now supplies the correct
   denominator.

**Testing:** 806 tests pass (one new doctest for `calibrate_token_count`).

## feat: ACP diff viewer support for file-writing tools - 2026-06-23

**Files Changed:**

- `src/agent/tool.rs` - Added `DiffData` struct and
  `diff_data: Option<DiffData>` field to `ToolResult`. Added `success_with_diff`
  constructor. Updated all existing constructors to initialise
  `diff_data: None`.
- `src/tools/write_file.rs` - `execute` now reads the existing file content
  before writing (old_text is `None` for new files) and returns
  `ToolResult::success_with_diff` so diff data is available to the ACP layer.
- `src/tools/edit_file.rs` - `execute_create`, `execute_overwrite`, and
  `execute_edit` each gained a `path_display: &str` parameter and return
  `ToolResult::success_with_diff` on success, populating `old_text`/`new_text`
  appropriately for each mode.
- `src/tools/fetch.rs` - Added `diff_data: None` to the two direct
  `ToolResult { .. }` struct initialisers that bypassed the constructors.
- `src/agent/executor.rs` - Added
  `diff_data: Option<crate::agent::tool::DiffData>` to
  `ToolCallEvent::Completed` and threads the value from the tool result through
  to the notifier callback.
- `src/commands/agent.rs` - Imported `Diff as AcpDiff` and `ToolCallContent`
  from `agent_client_protocol::schema`. The `ToolCallEvent::Completed` branch in
  the notifier now builds
  `ToolCallContent::Diff(AcpDiff::new(path, new_text).old_text(old_text))` and
  attaches it to `ToolCallUpdateFields::content` when diff data is present, so
  Zed renders a syntax-highlighted diff viewer in the chat panel.

**Summary:** File-modifying tools (`write_file`, `edit_file`) previously
returned only plain text results to the ACP notifier. Zed requires a
`ToolCallContent::Diff` payload in the `ToolCallUpdate` notification to render
its built-in diff viewer. This change threads the old/new file content from each
writing tool all the way up to the ACP session notification, so every successful
file write or edit now shows a syntax-highlighted diff in Zed's agent chat
panel.

**Testing:** All 805 existing tests pass. No new tests were added as the change
is covered transitively by the existing `write_file`, `edit_file`, and executor
notifier tests.

## Fix: ACP Zed features not working in production builds - 2026-06-22

**Files Changed:**

- `Cargo.toml` - Added `default = ["unstable_session_usage"]` to `[features]`.
  Without this, every `#[cfg(feature = "unstable_session_usage")]` block is dead
  code in a normal `cargo build`, so the context-window usage bar and
  `PromptResponse::usage` were never active.
- `src/commands/agent.rs` - Fixed `thought_level_config_option`: mapped
  `ThinkingMode::Auto` to `"none"` instead of `"auto"`. The options vec does not
  contain an `"auto"` entry; sending a `current_value` that does not match any
  listed option causes Zed to render the config-options panel with a blank
  selection, which can suppress the mode selector and thinking dropdown. Updated
  `test_config_option_update_thinking_mode_current_value` to cover the `Auto`
  variant.

**Summary:** Two bugs caused all ACP Zed agent UI features to be non-functional.

1. The `unstable_session_usage` feature was defined in `[features]` but not
   listed in `default = [...]`. When a user builds or installs Atoma normally
   (`cargo build` / `cargo install`), no special flags are passed, so the
   feature is disabled and all seven
   `#[cfg(feature = "unstable_session_usage")]` production-code blocks are
   compiled out. The context-window usage bar never received any `UsageUpdate`
   notifications and `PromptResponse::usage` was always unset.

2. `thought_level_config_option` mapped `ThinkingMode::Auto` to the string
   `"auto"`, but the select-options list only contains `"none"`, `"low"`,
   `"medium"`, `"high"`, and `"extra_high"`. Zed receives a `config_options`
   payload where `current_value` does not match any listed option. This causes
   the thinking dropdown (and potentially the entire session-config panel) to
   render incorrectly, which can prevent the mode selector from appearing.

**Testing:** `cargo clippy --all-targets -- -D warnings` passes for both the
default-feature build and `--all-features`. `cargo test` passes with 802 tests.
Added `(ThinkingMode::Auto, "none")` to
`test_config_option_update_thinking_mode_current_value`.

## Fix: NewSessionRequest always creates a fresh session - 2026-06-22

**Files Changed:**

- `src/commands/agent.rs` - Removed `load_zed_session` lookup and
  `load_conversation` resume block from the `NewSessionRequest` handler.
  `NewSessionRequest` now always creates a clean `AgentExecutor`; the
  `save_zed_session` call is preserved so `LoadSessionRequest` can still find
  the new session later. Removed the now-dead `existing_conv_id` reference from
  the `info!` log.
- `tests/integration/agent_prompt.rs` - Added `LoadSessionRequest` import;
  renamed `test_resumed_session_loads_prior_message_history` to
  `test_new_session_always_starts_fresh` and inverted its assertion (expects
  zero messages); added `test_load_session_resumes_prior_message_history` that
  verifies `LoadSessionRequest` still correctly restores history.
- `tests/integration/agent_handshake.rs` - Updated
  `test_two_sessions_same_cwd_different_session_ids` to assert that two
  `NewSessionRequest` calls produce **different** `conversation_ulid`s.

**Summary:** Clicking "New Atoma Agent" in Zed fires `NewSessionRequest`. The
handler was calling `load_zed_session` and resuming any prior conversation found
for the same working directory, so every "new" session silently inherited the
previous session's message history, system prompt, and tool state. Session
resume belongs exclusively in `LoadSessionRequest` (the explicit "load prior
session" path), which already had its own resume logic. Removing the resume code
from `NewSessionRequest` fixes the bug. The `save_zed_session` write is kept so
that after the first prompt turn Zed can offer a "continue" option via
`LoadSessionRequest`.

**Testing:** All 802 tests pass. Two integration tests were updated to match the
correct behaviour; one new integration test was added to cover the
`LoadSessionRequest` resume path.

## ACP Phase 4: Documentation - 2026-06-16

**Files Changed:**

- `demo/zed/config.yaml` - Added comment block describing the Mode Selector
  Widget and Context-Window Usage Bar features, including step-by-step
  verification procedures using `--trace`.
- `docs/explanation/implementations.md` - Added this Phase 4 entry.

**Summary:** Phase 4 of the ACP Zed features plan. No code changes; all
deliverables are documentation. The `demo/zed/config.yaml` demo file now
contains a "Zed UI Features" comment block that explains both new UI
capabilities added in Phases 1-3: the Mode Selector Widget (mode switching via
`CurrentModeUpdate` notifications) and the Context-Window Usage Bar (token
consumption via `UsageUpdate` notifications). Each section includes a numbered
verification procedure: run `atoma agent --trace 2>trace.log`, perform the
action in Zed, then grep `trace.log` for `CurrentModeUpdate` or `UsageUpdate`
lines to confirm the feature is working end-to-end.

**Testing:** No code tests added. `markdownlint` and `prettier` pass on
`docs/explanation/implementations.md`. `demo/zed/config.yaml` is valid YAML.

## ACP Phase 3: Integration Hardening - 2026-06-16

**Files Changed:**

- `src/commands/agent.rs` - Imported `ConfigOptionUpdate`; captured
  `initial_token_count` before executor move in both `NewSessionRequest` and
  `LoadSessionRequest` handlers; emitted initial `UsageUpdate` notifications
  after session insertion; added `thinking_mode_changed` pre-capture before the
  `SetSessionModeRequest` match block and emitted `ConfigOptionUpdate` for
  thinking-mode changes; added three unit tests.

**Summary:** Phase 3 of the ACP Zed features plan. Two integration gaps are
closed. First, when a session is created or a prior conversation is resumed, an
initial `UsageUpdate` notification is sent immediately after the session state
is inserted into the map, so Zed's context-window bar shows a non-zero count
without waiting for the first prompt turn. The `used` value is the
conversation's current `token_count()` and `size` is `max_conversation_tokens`
from config. Both `NewSessionRequest` and `LoadSessionRequest` handlers now emit
this notification (gated on `unstable_session_usage`). Second, when
`SetSessionModeRequest` triggers a `Thinking` change, a `ConfigOptionUpdate`
notification is sent after the existing `CurrentModeUpdate`, refreshing the
Thinking dropdown in Zed so both UI controls stay in sync after any mode change.
The new thinking mode is captured from `&change` before the match consumes it.

**Testing:** Three new inline unit tests added to `mod tests` in
`src/commands/agent.rs`:
`test_initial_usage_update_used_equals_conversation_token_count` verifies the
initial `UsageUpdate` data construction (feature-gated);
`test_config_option_update_thinking_mode_current_value` verifies
`thought_level_config_option` sets the correct `current_value` for each
`ThinkingMode`; `test_thinking_mode_change_emits_config_option_update` verifies
the `ConfigOptionUpdate` notification wraps the option correctly. All three pass
with `cargo test --all-features`.

## ACP Phase 2: Enable Usage Tracking - 2026-06-16

**Files Changed:**

- `Cargo.toml` - Added `unstable_session_usage` to `[features]` as a re-export
  of `agent-client-protocol/unstable_session_usage`; `cargo add` updated the
  dependency entry to include the feature.
- `src/commands/agent.rs` - Added `UsageUpdate` notification and
  `PromptResponse::usage` population after each completed `PromptRequest` turn;
  added four unit tests.

**Summary:** Phase 2 of the ACP Zed features plan. Enabled the
`unstable_session_usage` feature flag in both the dependency and as a
first-class `atoma` feature so `#[cfg]` guards compile correctly under
`--all-features`. After the `PromptRequest` iteration loop completes, the
handler now calls `exec.get_context_info().await` and sends a
`SessionUpdate::UsageUpdate` notification containing the current context window
consumption (`used_tokens` / `max_tokens`), which causes Zed's context-window
bar to display a live token count. The `PromptResponse` also attaches the
session's cumulative `Usage` (total, input, output tokens) via
`PromptResponse::usage()`. Both additions are guarded with
`#[cfg(feature = "unstable_session_usage")]` so the code degrades gracefully if
the feature is removed from `Cargo.toml`.

**Testing:** Four new inline unit tests added to `mod tests` in
`src/commands/agent.rs`: `test_usage_update_new_fields_are_correct` verifies
`UsageUpdate::new` field values; `test_usage_update_nonzero_size_round_trips`
verifies the values survive wrapping in a `SessionNotification`;
`test_prompt_response_usage_is_populated` verifies `PromptResponse::usage()`
stores all three token counts;
`test_agent_executor_total_usage_has_three_fields` verifies a fresh executor
exposes the three `TokenUsage` fields. All four pass with
`cargo test --all-features`.

## ACP Phase 1: Diagnose and Fix Mode Selector - 2026-06-15

**Files Changed:**

- `src/commands/agent.rs` - Added TRACE-level wire-format diagnostic logs for
  `NewSessionResponse` and `LoadSessionResponse`, emitted a `CurrentModeUpdate`
  notification after every successful `SetSessionModeRequest`, imported
  `CurrentModeUpdate` and `trace`, and added three unit tests.

**Summary:** Phase 1 of the ACP Zed features plan. The mode-selector widget in
Zed requires a `CurrentModeUpdate` push notification after each
`SetSessionModeRequest` to keep its internal state synchronized. Without it,
Zed's widget can silently desync and stop appearing. This change adds that
notification. TRACE-level JSON logs of `NewSessionResponse` and
`LoadSessionResponse` are also added so an operator can confirm the wire format
by running `atoma agent --trace 2>trace.log`. The sessions guard is explicitly
dropped before sending the notification to avoid holding the lock across the
async send.

**Testing:** Three new inline unit tests added to `mod tests` in
`src/commands/agent.rs`:
`test_current_mode_update_terminal_mode_id_matches_request` verifies the
`CurrentModeUpdate` payload for each terminal mode ID;
`test_current_mode_update_thinking_mode_id_matches_request` verifies the same
for each thinking mode ID; `test_new_session_response_modes_key_present_in_json`
confirms the TRACE log JSON contains the `modes` key. All 802 tests pass.

## Phase 4: ACP / Serve Integration for Dynamic System Prompts - 2026-06-15

**Files Changed:**

- `src/agent/conversation.rs` - Added
  `pub fn system_message() -> Option<&Message>` accessor to `Conversation`,
  enabling the ACP executor to inspect the current system message for TRACE
  logging without accessing the private field directly.
- `src/acp/executor.rs` - Replaced the previous thinking-mode-only injection
  block with a merged approach: builds `thinking_base` from `ThinkingMode`,
  looks up the per-agent `system_prompt` from `app_config.acp.agents` (falling
  back to `app_config.agent.system_prompt`), calls
  `build_system_prompt(user_prefix, &thinking_base)`, and emits a `TRACE` log
  with `run_id` and the resolved prompt. Added
  `use crate::agent::executor::build_system_prompt;` import.
- `src/acp/types.rs` - Added `system_prompt_configured: bool` field (with
  `#[serde(default)]`) to `AgentManifest`; updated the doc-comment
  struct-literal example to include `system_prompt_configured: false`.
- `src/acp/server/registry.rs` - Set
  `system_prompt_configured: agent_cfg.system_prompt.is_some()` in the
  `AgentManifest` construction inside `AgentRegistry::from_config`.
- `tests/unit/acp_system_prompt.rs` - New: 7 unit tests (6 active, 1
  `#[ignore]`) covering prefix+thinking merge, prefix-only, thinking-only,
  no-prompt guard, and `system_prompt_configured` true/false for
  `AgentManifest`.
- `tests/unit/mod.rs` - Added `mod acp_system_prompt;`.

**Summary:** Phase 4 wires the `system_prompt` field into the ACP run executor.
The executor now merges a user-supplied prefix (from per-agent config or the
global `agent.system_prompt` fallback) with the existing thinking-mode hint via
`build_system_prompt`, and only calls `set_system_prompt` when at least one of
the two inputs is non-empty. This preserves the pre-existing behaviour (no
spurious empty system message) for runs with no prefix and no thinking mode. The
`AgentManifest` gains a `system_prompt_configured` boolean so ACP clients can
discover whether an agent has a role configured without seeing the raw value.

**Testing:** `cargo test --all-features` - 802 passed, 0 failed, 30 ignored. New
tests: 6 unit tests in `tests/unit/acp_system_prompt.rs` (all pass). 1 test
marked `#[ignore]` pending `tracing_test` dev-dependency for trace-log capture.

## Phase 3: CLI and Mode Integration for Dynamic System Prompts - 2026-06-15

**Files Changed:**

- `src/cli.rs` - Added `system_prompt: Option<String>` field with `#[arg(long)]`
  to `Chat`, `Run`, and `Watch` command variants.
- `src/main.rs` - Updated `Commands::Chat`, `Commands::Run`, and
  `Commands::Watch` destructures; updated the three local wrapper functions
  (`handle_chat`, `handle_run`, `handle_watch`) to accept and forward
  `system_prompt`.
- `src/commands/run.rs` - Added `system_prompt: Option<String>` parameter to
  `handle_run`; derives `effective_system_prompt` (CLI flag takes precedence
  over `config.agent.system_prompt`); passes it through to `execute_plan_task`
  and `execute_plan`.
- `src/commands/chat.rs` - Added `system_prompt: Option<String>` parameter to
  `handle_chat`; declares `mut user_prefix`; replaces all three
  `agent.set_system_prompt` call sites with `build_system_prompt`-merged
  versions; adds a TRACE log on session start; adds handler for the new
  `SpecialCommand::SystemPrompt` variant (Show, Clear, Set actions).
- `src/commands/watch.rs` - Added `system_prompt: Option<String>` to
  `handle_watch` and threaded it through `handle_polaris_watch` →
  `run_watcher_loop` → `execute_plan_task` and `handle_generic_watch` →
  `run_generic_watcher_loop` → `execute_plan`.
- `src/chat_mode/commands.rs` - Added `SystemPromptAction` enum (`Show`,
  `Clear`, `Set(String)`); added `SpecialCommand::SystemPrompt { action }`
  variant; added `"system"` parse arm with max-4096-char validation; updated
  `format_help_general()`, `format_help_for_command()`, the unknown-command
  error message, and the module-level doc comment.
- `tests/unit/system_prompt_cli.rs` - New: 13 unit tests covering merge logic,
  plan-level precedence, mode/safety change re-application of user prefix,
  resume behaviour, all five `SpecialCommand::parse` cases, and the Clear/Set
  state transitions.
- `tests/unit/mod.rs` - Added `mod system_prompt_cli;`.
- `tests/integration/system_prompt_integration.rs` - New: 5 integration tests
  covering CLI priority, config fallback, no-prefix passthrough, spec-example
  parse round-trip, and prefix persistence across all mode/safety combinations.
- `tests/integration_tests.rs` - Added `pub mod system_prompt_integration;`.
- `tests/eval_run_command.rs` - Added `None` for new `system_prompt` parameter.
- `tests/integration/watcher_circuit_breaker_test.rs` - Added `None` for new
  `system_prompt` parameter to two `run_watcher_loop` call sites.
- `tests/integration/watcher_e2e_test.rs` - Added `None` for new `system_prompt`
  parameter to ten call sites (`run_watcher_loop` and `handle_watch`).
- `tests/integration/watcher_loop_test.rs` - Added `None` for new
  `system_prompt` parameter to twelve `run_watcher_loop` / `handle_watch` call
  sites.

**Summary:** Phase 3 wires the `--system-prompt` CLI flag end-to-end. The flag
is available on `run`, `chat`, and `watch` commands. In `run` and `watch` modes
the CLI value (or `config.agent.system_prompt` as fallback) is passed down to
`execute_plan_task` / `execute_plan`, where `build_system_prompt` prepends it to
the mode-based guidance. In `chat` mode the value is stored as `user_prefix` and
re-merged on every mode or safety change so the prefix survives transitions. The
`/system` REPL command lets users show, set, or clear the prefix live; max
length is 4096 characters. The `case-insensitive` `clear` keyword is recognised.

**Testing:** `cargo test --all-features` — 801 passed, 0 failed, 30 ignored. New
tests: 12 unit + 5 integration (all pass). 1 unit test marked `#[ignore]`
pending a `tracing_test` dev-dependency for trace-log capture.

## Add system_prompt: None to all Plan, AgentConfig, and AcpAgentConfig construction sites - 2026-06-15

**Files Changed:**

- `src/commands/plan_parser.rs` - Added `system_prompt: None` to 15 struct
  literal construction sites (1 doctest, 14 unit-test helpers).
- `src/commands/run.rs` - Added `system_prompt: None` to 4 Plan construction
  sites in unit tests.
- `src/watcher/polaris/mod.rs` - Added `system_prompt: None` to 1 Plan
  construction site in a unit test.
- `src/watcher/generic/result_event.rs` - Added `system_prompt: None` to 5 Plan
  construction sites (4 doctests, 1 unit-test helper).
- `src/watcher/generic/matcher.rs` - Added `system_prompt: None` to 1 Plan
  construction site in a unit-test helper.
- `tests/unit/watcher_generic.rs` - Added `system_prompt: None` to 1 Plan
  construction site in a unit-test helper.
- `src/acp/server/handlers/agents.rs` - Added `system_prompt: None` to 2
  AcpAgentConfig construction sites in unit tests.
- `src/acp/server/handlers/runs.rs` - Added `system_prompt: None` to 1
  AcpAgentConfig construction site in a unit-test helper.
- `src/acp/server/handlers/sessions.rs` - Added `system_prompt: None` to 1
  AcpAgentConfig construction site in a unit-test helper.
- `src/acp/server/registry.rs` - Added `system_prompt: None` to 1 AcpAgentConfig
  construction site in a unit-test helper.
- `tests/integration/acp_server_integration_test.rs` - Added
  `system_prompt: None` to 6 AcpAgentConfig construction sites.
- `src/config.rs` - Added `system_prompt: None` to 11 AgentConfig construction
  sites in unit tests (identified by `chat_streaming: false` preceding field).

**Summary:** The `Plan`, `AgentConfig`, and `AcpAgentConfig` structs each
received a new `pub system_prompt: Option<String>` field. All exhaustive struct
literal construction sites in src/ and tests/ were updated to include
`system_prompt: None` so that `cargo check --all-targets --all-features`
continues to pass.

**Testing:** `cargo check --all-targets --all-features` passed with no errors.

## Fix UTF-8 panic in streamed thinking parser - 2026-06-11

**Files Changed:**

- `src/providers/openai.rs` - Fixed `StreamThinkingParser` to round held-back
  byte offsets down to a valid UTF-8 char boundary before slicing streamed
  content. Added a regression test covering multibyte emoji content in the
  non-`<think>` path.

**Summary:** Streaming responses that contained multibyte characters near the
parser's tail holdback window could panic with
`byte index ... is not a char   boundary` because the code sliced a `String` at
a raw byte offset. The parser now computes a safe char boundary before splitting
the buffer, so emoji and other multibyte Unicode characters no longer crash the
session.

**Testing:** `cargo fmt --all`, `cargo check --all-targets --all-features`,
`cargo clippy --all-targets --all-features -- -D warnings`, and
`cargo test  test_stream_thinking_parser_preserves_utf8_boundaries --all-features`
passed.

## Add interactive chat streaming controls - 2026-06-11

**Files Changed:**

- `src/config.rs` - Added `agent.chat_streaming` with `ATOMA_CHAT_STREAMING`
  env-var support so interactive chat can default to live chunk streaming.
- `src/chat_mode/mod.rs` - Added `streaming_enabled` to `ChatModeState` and
  helper methods to enable, disable, and toggle live streaming.
- `src/chat_mode/commands.rs` - Added `/stream [on|off]`, updated `/status` to
  show streaming state, and expanded the help text and parser/tests accordingly.
- `src/commands/chat.rs` - Wired chat mode to install a stream notifier when
  streaming is enabled, render streamed chunks directly to stdout, and skip the
  duplicate final response print when streaming is active.

**Summary:** Interactive chat can now stream model output live instead of only
showing the spinner. Users can enable it persistently via config
(`agent.chat_streaming: true` or `ATOMA_CHAT_STREAMING=true`) or per-session via
`/stream on` in chat. The chat loop now routes provider chunks through a stream
notifier, prints reasoning chunks as they arrive, and falls back cleanly to the
spinner when the provider does not support streaming.

**Testing:** `cargo check --all-targets --all-features`,
`cargo clippy --all-targets --all-features -- -D warnings`,
`cargo test test_parse_stream_command --all-features`,
`cargo test test_execute_status --all-features`,
`cargo test test_help_stream_command --all-features`,
`cargo test test_enable_disable_toggle_streaming --all-features`,
`cargo test test_env_var_chat_streaming --all-features`, and
`cargo test test_agent_chat_streaming_deserialization --all-features` passed. A
full `cargo test --all-features` run still fails on two pre-existing OpenAI
config assertions (`test_openai_config_default` and
`test_openai_config_deserialization_defaults`).

## Improve stream error diagnostics - 2026-06-11

**Files Changed:**

- `src/providers/openai.rs` - Enhanced stream chunk read error to walk the full
  `std::error::Error` source chain and include the URL and model name. Added a
  structured `warn!` log at the point of failure so the root cause (e.g. hyper
  connection reset, gzip decode failure) is visible in the log without having to
  reproduce the issue. Added `model_name` binding to capture the model string
  before it is moved into `ChatRequest`.
- `src/commands/agent.rs` - Collapsed nested `if` into a single `if ... && let`
  guard to satisfy the `collapsible_if` clippy lint.

**Summary:** Previously, "Stream interrupted: Stream read error: error decoding
response body" gave no indication of which endpoint or model was affected, and
the underlying cause (network drop, decompression error, etc.) was silently
discarded. The stream error path now emits a structured `WARN` log entry with
`url`, `model`, `error`, and `source_chain` fields, and the propagated
`AtomaError::StreamInterrupted` message includes all of those details.

**Testing:** No new tests added; change is in error-path instrumentation only.
All existing tests continue to pass.

## Stream thinking content to Zed for OpenAI-compat providers - 2026-06-10

**Files Changed:**

- `src/providers/openai.rs` - Added `reasoning_content` and `thinking` fields to
  `StreamDelta` so Ollama's `delta.thinking` and DeepSeek's
  `delta.reasoning_content` are deserialized. Added `StreamThinkingParser` to
  extract `<think>…</think>` inline blocks from `delta.content` for models that
  embed reasoning in the content stream (gemma-thinking variants, QwQ, etc.).
  Updated `StreamAccumulator::apply_chunk` to accumulate all three
  reasoning-field variants. Updated `chat_completion_stream_with_callback` to
  emit explicit reasoning fields first and fall back to tag parsing for content.
  Added `"think": true` to both non-streaming and streaming request bodies when
  thinking mode is active so Ollama's OpenAI-compat endpoint activates thinking.
  Changed `enable_streaming` default from `false` to `true`.
- `src/config.rs` - Changed `default_openai_enable_streaming` to return `true`
  so streaming (and therefore thinking display) works out of the box.

**Summary:** Thinking/reasoning content from OpenAI-compatible models (including
gemma4 and DeepSeek served via Ollama's `/v1` endpoint) now streams to Zed's
chat window as a collapsible Thinking block. Four distinct content paths are
handled: `delta.reasoning` (original), `delta.reasoning_content`
(DeepSeek/Qwen), `delta.thinking` (Ollama compat), and `<think>…</think>` inline
tags in `delta.content`. The `"think": true` parameter is added to every request
when thinking mode is active, enabling Ollama's compat endpoint to populate
`delta.thinking`.

**Testing:** Three unit tests for `StreamThinkingParser` (plain pass-through,
tag detection, split-boundary) and two wiremock-based async tests for
`delta.thinking` and `delta.reasoning_content` forwarding. All 40
OpenAI-provider tests pass.

## Raise max_turns ceiling to 5000 - 2026-06-10

**Files Changed:**

- `src/config.rs` - Changed the `max_turns` validation guard from `> 1000` to
  `> 5000` and updated the error message accordingly. Updated the
  excessive-turns test to use 6000 and added a new boundary test that asserts
  5000 is accepted.

**Summary:** The previous 1000-turn cap was too low for long-running ACP agent
sessions. The ceiling is now 5000. When a new chat session is started (or a
`NewSessionRequest` is received in ACP mode) the turn counter resets to zero, so
the 5000 limit applies per session. The default remains 20 turns.

**Testing:** Six `config::tests::test_config_validate_*` tests all pass,
including the updated excessive-turns test (now 6000) and the new boundary test
at exactly 5000.

## OpenAI provider thinking level and model list fixes - 2026-06-10

**Files Changed:**

- `src/providers/openai.rs` - Added `set_thinking_mode` override to
  `OpenAIProvider` so changes made via the Zed Thinking Level selector are
  propagated to the provider's `reasoning_effort` field. Extended
  `build_capabilities_from_id` to include `ModelCapability::Thinking` for
  `o1-*`, `o3-*`, and `o4-*` model families.
- `src/commands/agent.rs` - Added `config.agent.thinking_mode` fallback to the
  OpenAI `build_provider` path (matching the existing Ollama pattern). Added a
  model-list fallback in both `NewSessionRequest` and `LoadSessionRequest`
  handlers: when `list_models()` returns empty or fails (e.g., no API key,
  unreachable endpoint) the configured model is advertised as a single-entry
  list so Zed always shows the model selector.

**Summary:** Fixed two bugs observed when using `atoma agent` with the OpenAI
provider inside Zed. The `set_thinking_mode` method was missing from
`OpenAIProvider`, causing the Thinking Level dropdown in Zed's chat window to
appear but have no effect. The model list was not shown when the `/models` API
call failed (authentication required or unreachable endpoint); the fallback now
advertises the configured model so the selector appears with at least one entry.

**Testing:** Added `test_set_thinking_mode_propagates_to_request`
(wiremock-based), `test_build_capabilities_thinking_for_o1_models`,
`test_build_capabilities_thinking_for_o3_models`,
`test_build_capabilities_thinking_for_o4_models`,
`test_build_capabilities_no_thinking_for_gpt_models`,
`test_build_provider_openai_applies_agent_thinking_mode_fallback`, and
`test_build_provider_openai_agent_thinking_not_applied_when_provider_level_set`.
All existing tests continue to pass.

## ACP and MCP article sections - 2026-06-08

**Files Changed:**

- `docs/explanation/introducing_atoma.md` - Added two new sections: "MCP:
  consuming tools from the outside world" (MCP 2025-11-25 client, stdio/HTTP
  transports, OAuth 2.1 PKCE, tool namespacing, `atoma mcp` CLI commands) and
  "ACP: exposing Atoma as a service" (HTTP server, run lifecycle state machine,
  isolated vs shared session modes, multi-agent config, health probes).

**Summary:** Extended the Atoma introduction article with coverage of the MCP
client subsystem and ACP server subsystem. Both sections follow the content
style guide and are inserted between "Three ways to run it" and "Security is the
whole point" to keep integration capabilities grouped together.

**Testing:** No code changes; documentation only.

## Company Introduction Article and GIF Generator - 2026-06-08

**Files Changed:**

- `docs/explanation/introducing_atoma.md` - New article introducing Atoma to
  technical colleagues (DevOps, platform engineers, developers). Covers what
  Atoma is, the agentic loop, the three modes (chat, run, watch), the security
  model, the skills system, and getting started guidance. Includes two mermaid
  architecture diagrams and two GIF image references.
- `scripts/generate_atoma_gifs.py` - New self-contained Python/Pillow script
  that generates two animated GIFs: `atoma_workflow.gif` (the four-stage
  CLI-to-execution pipeline) and `atoma_agentic_loop.gif` (the think-act cycle).
  Outputs to `outputs/gifs/` and copies to `docs/explanation/images/` for the
  article. Run with `python3 scripts/generate_atoma_gifs.py` from the repo root.

**Summary:** Created a company introduction article and an animated GIF
generator for Atoma. The article targets engineers unfamiliar with the project
and follows the content style guide: no numbered lists, no em dashes, no bold
emphasis, Oxford comma throughout. Includes mermaid architecture diagrams and
references to animated GIFs produced by the generator script.

**Testing:** Article passed `markdownlint` and `prettier` linting with no
errors.

## Logging Phase 5: Documentation Updates - 2026-05-14

**Files Changed:**

- `docs/how-to/troubleshoot_models.md` - Replaced
  `export RUST_LOG=debug; atoma chat` with `atoma --debug chat` as the primary
  debug example in the "Getting Help" section. Added a note that `RUST_LOG`
  still takes precedence when set.
- `docs/how-to/manage_context_window.md` - Replaced
  `RUST_LOG=debug atoma run ...` with `atoma --debug run ...` in "Automatic
  Summarization in Run Mode" and replaced `RUST_LOG=debug atoma chat` with
  `atoma --debug chat` in the "Warnings Not Appearing" troubleshooting section.
- `docs/how-to/use_generic_watcher.md` - Updated the "Events are consumed but no
  plan executes" diagnosis note to use `atoma --debug watch` and added a second
  line showing `atoma --trace watch` for full Kafka and LLM trace.
- `docs/how-to/acp_demo.md` - Added `atoma --debug serve` and
  `atoma --trace serve` as the primary alternatives in "Increased verbosity for
  debugging". Kept the `RUST_LOG=atoma::acp=debug/trace` examples for targeted
  per-module filtering with a note that `RUST_LOG` takes precedence.
- `docs/how-to/zed_acp_demo.md` - Added an explanatory note under "Using
  RUST_LOG for in-Zed debugging" clarifying that Zed cannot pass CLI flags to
  child processes and that `RUST_LOG=atoma=debug` / `atoma=trace` are the
  equivalents of `--debug` / `--trace` in that context.
- `config.example.yaml` - Added `log:` section with `stderr_format: plain` and
  `file_format: json` entries, documented with comments referencing
  `--log-format` and `ATOMA_LOG_FORMAT`.
- `src/config.rs` - Added `ATOMA_LOG_FORMAT` to the module-level environment
  variable documentation table, placed between `ATOMA_TRACE` and
  `ATOMA_LOG_STDERR_FORMAT`.

**Summary:** Phase 5 updates all user-facing documentation to reflect the named
`--debug` and `--trace` flags introduced in Phase 2 and the configurable log
format added in Phase 3. Every document that previously directed users to set
`RUST_LOG` as the primary mechanism for enabling debug or trace output has been
updated to show the CLI flags as the preferred approach. The `RUST_LOG` examples
are retained wherever they serve a distinct purpose (targeted module filtering,
Zed env-var integration). The `config.example.yaml` now includes the `log:`
section, and the `src/config.rs` module doc includes the missing
`ATOMA_LOG_FORMAT` entry.

**Testing:** Documentation-only changes; no automated tests added. All modified
Markdown files pass `markdownlint` and `prettier`.

## Logging Phase 4: Structured Trace Transcript - 2026-05-14

**Files Changed:**

- `src/agent/executor.rs` - Added `Level` to tracing imports. Replaced the
  inline-format conversation trace loop with a `tracing::enabled!(Level::TRACE)`
  guard emitting structured `trace!` events with fields `msg.index`, `msg.role`,
  `msg.char_count`, and `msg.content`. Added a `trace!` event after provider
  response with `finish_reason`, `response_chars`, `has_tool_calls`, and
  `tool_call_count` fields. Added `trace!` events before each tool call dispatch
  (`tool.name`, `tool.call_id`, `tool.args_json`) and after each successful tool
  result (`tool.name`, `tool.call_id`, `tool.result_bytes`,
  `tool.result_preview` capped at 200 chars). Added public
  `log_model_metadata(provider)` async helper that gates the `get_model_info()`
  API call behind `tracing::enabled!(TRACE)` and emits `model.name`,
  `model.context_window`, and `model.capabilities` as structured fields. Added
  two unit tests: `test_log_model_metadata_none_does_not_panic` and
  `test_log_model_metadata_some_does_not_panic`.
- `src/commands/run.rs` - Called `log_model_metadata(provider.as_ref()).await`
  in `init_plan_agent` immediately before `agent.set_provider(provider)` so
  model metadata is traced whenever the agent executor is configured with a
  provider via the run/watch command path.

**Summary:** Phase 4 extends the tracing infrastructure added in earlier phases
with fully structured TRACE-level events for the complete provider-visible
conversation, individual tool dispatches, tool results, and model metadata. All
expensive formatting (content serialisation, `get_model_info()` API calls) is
guarded behind `tracing::enabled!(Level::TRACE)` so INFO/DEBUG builds pay no
extra cost. The `log_model_metadata` helper gives operators a single-event view
of which model is in use and its capabilities at session start.

**Testing:** Two new unit tests verify `log_model_metadata` handles `Ok(None)`
and `Ok(Some(ModelInfo))` without panic and calls `get_model_info` at most once.

## Logging Phase 3: Configurable Log Format - 2026-05-14

**Files Changed:**

- `src/config.rs` - Added `LogFormat` enum (`Plain`, `Compact`, `Json`) with
  `clap::ValueEnum`, `serde`, `FromStr`, and `Display` implementations. Added
  `LogConfig` struct (`stderr_format: LogFormat`, `file_format: LogFormat`) and
  its `Default` impl (`Plain`/`Json`). Added `log: LogConfig` field to `Config`
  struct and `Config::default()`. Added `ATOMA_LOG_STDERR_FORMAT` and
  `ATOMA_LOG_FILE_FORMAT` to module-level env-var docs. Added env var overrides
  in `apply_env_vars()`.
- `src/cli.rs` - Added `use crate::config::LogFormat`. Added
  `log_format: Option<LogFormat>` (long `--log-format`, env `ATOMA_LOG_FORMAT`,
  `value_enum`) as a `global = true` flag. Added 5 new tests for the flag.
- `src/main.rs` - Added `LogFormat` import. Refactored `init_logging` to accept
  `stderr_format: LogFormat` and `file_format: LogFormat` parameters. Internal
  implementation builds three `Option`-wrapped layers (plain / compact / json)
  for each of stderr and the file sink; only the matching `Some` is active. File
  format defaults to `LogFormat::Json` unless `ATOMA_LOG_FILE_FORMAT` is set.
  Updated call site to derive formats from `cli.log_format` and env var before
  `Config::load()`.
- `config.example.yaml` - Added commented `log:` section documenting
  `stderr_format` and `file_format` options.
- `src/test_utils.rs` - Added `log: crate::config::LogConfig::default()` to the
  `TestConfigBuilder::build()` struct literal.
- `tests/eval_run_command.rs` - Added `log: atoma::config::LogConfig::default()`
  to the `offline_config()` struct literal.

**Summary:** Phase 3 of the logging refinement plan introduces independent,
configurable output formats for the stderr and file log sinks. Three formats are
supported: `plain` (human-readable with ANSI colour, default for stderr),
`compact` (single-line, no ANSI), and `json` (newline-delimited JSON, default
for file sinks). The selection can be made via the new `--log-format` CLI flag,
the `ATOMA_LOG_FORMAT` env var, `ATOMA_LOG_FILE_FORMAT`, or the `log:` section
in the config file. The `init_logging()` function is redesigned around
`Option<Layer>` tuples so that exactly one layer per sink is active at runtime
without any heap boxing of heterogeneous types.

**Testing:** 24 new unit tests added (14 in `config.rs`, 6 in `cli.rs` plus 4
config integration tests); all pass. Full `cargo test` suite passes (the one
pre-existing failure in `test_get_run_includes_thinking_mode` is unrelated).

**Files Changed:**

- `src/cli.rs` - Deprecated `verbose: u8` doc comment; added `debug: bool` (long
  `--debug`, env `ATOMA_DEBUG`) and `trace: bool` (long `--trace`, env
  `ATOMA_TRACE`) as `global = true` flags on `Cli`. Added five new tests:
  `test_debug_flag_sets_debug_bool`, `test_trace_flag_sets_trace_bool`,
  `test_trace_and_debug_are_independent`, `test_debug_absent_gives_false`, and
  `test_trace_absent_gives_false`.
- `src/config.rs` - Added `debug: bool` and `trace: bool` fields (both
  `#[serde(default)]`) to the `Config` struct and `Config::default()`.
  Documented `ATOMA_DEBUG` and `ATOMA_TRACE` in the module-level env-var list.
  Added five new tests: `test_config_debug_field_default_false`,
  `test_config_trace_field_default_false`,
  `test_config_debug_deserialization_true`,
  `test_config_trace_deserialization_true`, and
  `test_config_debug_and_trace_absent_gives_false`.
- `src/main.rs` - Refactored `init_logging(verbose: u8, ...)` to
  `init_logging(debug: bool, trace: bool, ...)`. Precedence logic: `trace` >
  `debug` > `RUST_LOG` env > info default. Call site derives `debug` and `trace`
  from the new flags with `--verbose` count as a backward-compatible fallback
  (`verbose >= 1` implies `debug`, `verbose >= 2` implies `trace`).
- `src/test_utils.rs` - Added `debug: false, trace: false` to the
  `TestConfigBuilder::build()` struct literal to satisfy the exhaustiveness
  check introduced by the new `Config` fields.
- `tests/eval_run_command.rs` - Added `debug: false, trace: false` to the
  `offline_config()` struct literal for the same reason.

**Summary:** Phase 2 of the logging refinement plan replaces the opaque
`-v`/`-vv` count flag with named, discoverable `--debug` and `--trace` boolean
flags. The flags are accepted as both CLI options and environment variables
(`ATOMA_DEBUG`, `ATOMA_TRACE`) and are also first-class fields in the `Config`
struct so that config-file support lands automatically via serde. The old
`--verbose` / `-v` / `-vv` count flag is preserved for backward compatibility
and maps to the same internal `debug`/`trace` booleans at the call site.

**Testing:** 10 new unit tests added; all pass. Full `cargo test` suite passes
(the one pre-existing failure in `test_get_run_includes_thinking_mode` is
unrelated to this change).

## Logging Phase 1: Span Noise Elimination - 2026-05-14

**Files Changed:**

- `src/commands/watch.rs` - Removed `watcher_type` and `group_id` from the
  `#[instrument]` `fields(...)` list on `handle_watch`. Deleted the
  `span.record()` block. The startup `info!` call already logs both values.
- `docs/explanation/logging_refinement_plan.md` - Phase 1 deliverable marked
  completed.

**Summary:** The `handle_watch` tracing span was injecting `watcher_type` and
`group_id` as span fields that appeared on every log line emitted inside the
watcher session (typically thousands of lines per plan execution). Because the
values are fixed for the life of the process and are already printed once at
startup, embedding them in every span entry added ~80 characters of noise per
line with zero diagnostic benefit. The fix removes the two fields from
`#[instrument]` and deletes the `span.record()` block; the startup `INFO` log
retains both values for operator reference.

**Testing:** All 121 watcher-related unit and integration tests pass. The
failing `test_get_run_includes_thinking_mode` test is a pre-existing ACP
thinking-mode assertion unrelated to this change.

## Kafka max.poll.interval.ms Fix (UnknownMemberId) - 2026-05-14

**Files Changed:**

- `src/config.rs` - Added `max_poll_interval_ms: u64` field to `KafkaConfig`
  with a default of `3_600_000` ms (1 hour) and a
  `default_kafka_max_poll_interval_ms` helper function.
- `src/watcher/generic/consumer.rs` - Set `max.poll.interval.ms` on the
  `StreamConsumer` `ClientConfig` in `RealGenericConsumer::new()`. Updated
  `commit()` to pattern-match
  `KafkaError::ConsumerCommit(UnknownMemberId | RebalanceInProgress)` and return
  a clear, actionable error message instead of a generic
  `Failed to commit Kafka offset` string.
- `tests/integration/watcher_circuit_breaker_test.rs` - Added
  `max_poll_interval_ms: 3_600_000` to the struct literal in
  `valid_watcher_config()`.
- `config.example.yaml` - Documented `max_poll_interval_ms` in the kafka section
  with an explanation of why the default must be raised for AI workloads.

**Summary:** AI agent plans regularly exceed the librdkafka default
`max.poll.interval.ms` of 300 s (5 min). When the limit is exceeded the broker
evicts the consumer from the group, causing the post-execution `commit()` call
to fail with `UnknownMemberId` and crash the watcher process. The fix has two
layers: (1) the `max.poll.interval.ms` property is now configurable via
`watcher.kafka.max_poll_interval_ms` and defaults to 1 hour so the root cause is
eliminated; (2) `commit()` now recognises the eviction error codes and returns a
descriptive message pointing operators to the configuration knob, instead of a
cryptic Kafka error string.

**Testing:** Existing unit and integration tests all pass. The new field
defaults cleanly so no existing YAML configs need updating.

## Watcher Plan Re-execution Bug Fix - 2026-05-12

**Files Changed:**

- `src/commands/run.rs` - Extended system prompt in `init_plan_agent` for both
  `allow_dangerous` variants: appended "Complete each assigned task exactly
  once. When all tasks are finished, provide a concise final summary of what was
  accomplished and stop. Do NOT re-run, re-verify, or repeat any step." This
  prevents the model from looping through plan steps a second (or third) time
  after all objectives are met.
- `src/watcher/generic/consumer.rs` - Changed `CommitMode::Async` to
  `CommitMode::Sync` in `RealGenericConsumer::commit()`. The async commit left a
  window where a Kafka consumer-group rebalance (triggered by a long-running
  plan) could cause the broker to re-deliver the same message before the
  unconfirmed commit was flushed, resulting in the plan re-executing.
- `src/commands/watch.rs` - `run_generic_watcher_loop`: moved
  `consumer.commit()` to immediately after `execute_plan` returns, BEFORE result
  publishing. The message is now committed as soon as the plan finishes; result
  publication (which may be slow) no longer creates a re-delivery window. Added
  `info!` log `"Generic watcher: plan session ended; Kafka offset committed"`
  and a `"Waiting for next plan..."` println at the end of each processed-event
  branch to make the session boundary explicit.
- `src/commands/watch.rs` - `run_watcher_loop` (Polaris): added `info!` log
  `"Plan session ended; waiting for next event"` after each successful event
  commit to match the session-boundary visibility of the generic watcher.

**Summary:** The watcher was re-executing finished plans for two compounding
reasons. First, the agent system prompt lacked an explicit stop instruction;
because the prompt said "Act immediately with tool calls" the model could keep
calling tools (re-running plan steps, verifying results, etc.) after the
objectives were met, never returning `IterationResult::Completed` cleanly.
Second, `CommitMode::Async` left the Kafka offset in an unconfirmed state; a
Kafka consumer-group rebalance during a long-running plan delivery would cause
the broker to re-deliver the message and the plan to run a second time. The
commit-before-publish reordering additionally eliminates any re-delivery window
introduced by slow or failing result publishing.

**Testing:** `cargo check` passes. Existing generic-watcher loop tests
(`test_run_generic_watcher_loop_once_mode_single_event`,
`test_run_generic_watcher_loop_publishes_result_event`, etc.) continue to pass
because they use `FakeGenericConsumer` whose `commit()` is a counter increment
unaffected by the sync/async change.

## ThinkingMode Opt-In and Auto Variant - 2026-05-12

**Files Changed:**

- `src/providers/types.rs` - Changed `#[default]` from `High` to `None`; added
  `Auto` variant; updated `as_effort_str` (`Auto` returns `None`),
  `from_effort_str` (`"auto"` -> `Auto`, unknown now falls back to `None` not
  `High`), `display_name` (`Auto` -> `"Auto"`); added four unit tests.
- `src/providers/ollama.rs` - Added
  `pub async fn check_current_model_thinking_capability()` that calls
  `/api/show` and checks for `"thinking"` in the capability list; added two unit
  tests.
- `src/providers/copilot.rs` - Added `async fn resolve_thinking_mode()` that
  resolves `Auto` by checking `adaptive_thinking` in the model cache; `Auto`
  with `adaptive_thinking: true` resolves to `ExtraHigh` (Copilot confirms
  extended-thinking support via that flag); changed `build_responses_request` to
  accept an explicit `thinking: ThinkingMode` param; updated `responses_request`
  to call `resolve_thinking_mode` first.
- `src/providers/openai.rs` - Replaced `unwrap_or_default()` path with an
  explicit match: `Auto -> High`, `None/ThinkingMode::None -> no field`.
- `src/providers/factory.rs` - Added
  `pub(crate) async fn apply_ollama_thinking_mode()`: for `Auto` queries
  `/api/show` and, if the model reports thinking support, enables thinking at
  `High` (the highest level guaranteed to be supported on Ollama's binary
  think:true/false API); for specific levels enables if supported else warns and
  falls back to `None`; updated `create_ollama` to use it.
- `src/providers/mod.rs` - Updated `create_provider_with_override` to use
  `apply_ollama_thinking_mode`.
- `src/commands/agent.rs` - `build_provider` (non-async): resolves
  `Auto -> High` inline (capability check is not available on the sync path;
  High is the safe maximum for Ollama's binary thinking); added `Auto` arm to
  `thought_level_config_option` match.
- `src/commands/run.rs` - `create_provider_for_model` uses
  `apply_ollama_thinking_mode`.
- `src/acp/executor.rs` - Inserted `Auto` resolution block after
  `set_thinking_mode`: queries model capabilities via `get_current_model_info`,
  and if `ModelCapability::Thinking` is present resolves to `High` (highest
  level safe to use without knowing a model's specific budget ceiling);
  otherwise disables thinking with a warning.
- `src/agent/executor.rs` - Updated doctest to assert default is
  `ThinkingMode::None`.
- `src/chat_mode/mod.rs` - Added `ThinkingMode::Auto => "Auto"` arm to
  `thinking_label`.

**Summary:** Thinking is now fully opt-in. When `thinking_mode` is absent from
configuration, no reasoning-effort field is sent to any provider (Copilot and
OpenAI no longer accidentally send `"high"` due to the old `unwrap_or_default()`
path). The new `Auto` variant lets users enable thinking without picking a
budget level. The `Auto` workflow is: query the provider for thinking
capabilities, then pick the highest level the model is known to support. For
Copilot, `Auto` checks `adaptive_thinking` in the model cache and resolves to
`ExtraHigh` when the model confirms extended-thinking support. For OpenAI,
`Auto` always resolves to `High` (the documented API maximum). For Ollama,
`Auto` checks `/api/show` at factory startup; because Ollama thinking is binary
(`think: true/false`) with no budget levels, the resolved level is `High` when
`"thinking"` is listed in capabilities. Specific levels (`Low` through
`ExtraHigh`) also fall back gracefully on Ollama: if `/api/show` does not list
`"thinking"` in capabilities a warning is logged and thinking is left off rather
than passing `think: true` to a model that ignores it.

**Testing:** `cargo check`, `cargo clippy -- -D warnings`, and all type, Ollama,
and Copilot unit tests pass (9 directly affected tests all green).

## Log File Support (--logfile) - 2026-05-07

**Files Changed:**

- `src/cli.rs` - Added `log_file: Option<PathBuf>` field to `Cli` with
  `long = "logfile"`, `global = true`, and `env = "ATOMA_LOG_FILE"`. Added three
  unit tests.
- `src/config.rs` - Added `log_file: Option<PathBuf>` field to `Config` struct,
  `Default` impl, and `apply_env_vars` (reads `ATOMA_LOG_FILE`). Documents that
  CLI/env take precedence because logging is initialised before config is
  loaded.
- `src/main.rs` - Replaced `tracing_subscriber::fmt()` shorthand with
  `registry()` + layered approach. `init_logging` now accepts
  `log_file: Option<&Path>` and, when provided, adds a second `fmt` layer that
  appends to the file with `with_ansi(false)`. Parent directories are created
  automatically. Falls back to stderr-only with a printed warning if the file
  cannot be opened.
- `src/test_utils.rs` - Added `log_file: None` to test `Config` struct literal.
- `tests/eval_run_command.rs` - Added `log_file: None` to `offline_config`
  struct literal.

**Summary:** `atoma --logfile path/to/atoma.log run --plan ...` (or
`ATOMA_LOG_FILE=...`) now writes every log line to both stderr and the specified
file. The file output has ANSI colour codes stripped so it is human-readable in
an editor or `tail -f`. No new crate dependencies are required: the existing
`tracing-subscriber` `Mutex<W>: MakeWriter` blanket implementation handles the
file writer. If the log file path is absent, behaviour is identical to before.

**Testing:** Three new CLI unit tests cover the `--logfile` flag, absence of the
flag, and placement after the subcommand.
`cargo clippy --all-targets --all-features -- -D warnings` passes with zero
warnings.

## Shared AgentExecutor Session for Plan Execution - 2026-05-07

**Files Changed:**

- `src/commands/run.rs` - Extracted `PlanAgentInit` struct, `init_plan_agent`,
  `add_task_message_to_agent`, and `run_iteration_loop` private helpers.
  Refactored `execute_plan_task` to delegate to those helpers. Modified
  `execute_plan` to create one `AgentExecutor` before the task loop so all tasks
  share a single context window; logs the session ID at INFO level and prints it
  to stdout alongside provider/model/iteration details.

**Summary:** Previously every task in a plan created its own provider, tool
registry, and `AgentExecutor`, resulting in a completely independent
conversation for each task. Now `execute_plan` calls `init_plan_agent` once
before the task loop, obtaining a single `AgentExecutor` whose conversation ID
(a ULID) is captured as `session_id` and emitted as a structured `info!` log
field. Each task then calls `agent.reset_iteration_count()` (preserving
conversation history but resetting the per-task iteration cap),
`add_task_message_to_agent`, and `run_iteration_loop` on that shared executor.
The public signature and doc comment of `execute_plan_task` are unchanged so
existing call sites and tests are unaffected.

**Testing:** `cargo check --all-targets --all-features` and
`cargo clippy --all-targets --all-features -- -D warnings` both pass with zero
errors or warnings.

## Diagnostic Logging for Model Inputs and Outputs - 2026-05-07

**Files Changed:**

- `src/agent/executor.rs` - Added TRACE logging of every conversation message
  before the provider call; added DEBUG preview and TRACE full-text logging of
  model responses.
- `src/commands/run.rs` - Added INFO task-boundary separator in `execute_plan`;
  added DEBUG/TRACE logging of `per_task_input`, system prompt, and user message
  in `execute_plan_task`.

**Summary:** Even at TRACE log level it was impossible to see what text was
actually sent to the model or what the model returned. The new logging surfaces
these at two levels: DEBUG shows a 300-character preview of the user message and
response alongside the system prompt in full; TRACE shows the complete text of
every conversation message, the full per-task user message, and the full model
response. Task-boundary INFO lines in `execute_plan` clearly separate tasks in
the log stream with task id, sequential number, total count, and priority.

**Testing:** All existing tests pass; no network or process calls added.

## Rule 9 Compliance: mcp_tool_execution tests marked #[ignore] - 2026-05-07

**Files Changed:**

- `tests/integration/mcp_tool_execution.rs` - Added `#[ignore]` attribute to all
  5 subprocess-spawning tests; retained env-var guard as a secondary safety net.

**Summary:** All 5 tests in `mcp_tool_execution.rs`
(`end_to_end_tool_call_via_registry`,
`registered_tool_name_uses_double_underscore_separator`,
`echo_tool_round_trips_give_em_enough_rope`,
`mcp_tool_requires_no_confirmation_in_full_autonomous_headless`,
`mcp_tool_requires_confirmation_in_interactive_non_headless`) were gated by an
env-var + early `return` pattern, which caused them to silently report as
_passed_ in a normal `cargo test` run rather than _ignored_, hiding the fact
that they were skipped. Each test now carries `#[ignore = "..."]` with a full
opt-in message so they appear correctly as `ignored` in test output. The env-var
check is retained inside each test body as a secondary guard for when
`--include-ignored` is used without the env var.

**Testing:** `cargo test --test integration_tests mcp_tool_execution` confirms
all 5 tests report as `ignored` with their opt-in message.

## Ollama num_ctx Context Window Config - 2026-05-07

**Files Changed:**

- `src/config.rs` - Added `num_ctx: u32` field to `OllamaConfig` (default
  32768); added `ATOMA_OLLAMA_NUM_CTX` env var; updated `Default` and module
  docs.
- `src/providers/ollama.rs` - Added `OllamaOptions` struct; added `options`
  field to `OllamaChatRequest`; added `num_ctx` field to `Ollama` with
  getter/setter; `chat_completion` and `chat_completion_stream_with_callback`
  now forward `options.num_ctx` on every request.
- `src/providers/factory.rs` - `create_ollama` calls `set_num_ctx` from config.

**Summary:** Ollama's built-in context window is typically 2048 tokens, which is
too small for multi-task plans. A new `num_ctx` field in `OllamaConfig` (default
32768, env var `ATOMA_OLLAMA_NUM_CTX`, YAML key `ollama.num_ctx`) is forwarded
to every Ollama chat API request via the `options.num_ctx` field. This ensures
the full system prompt, tool definitions, and conversation history fit inside
the model's context window without truncation.

**Testing:** New unit tests cover the default value, YAML deserialization, env
var override, invalid env var ignored, getter/setter, and serialization of
`options.num_ctx` in the request JSON. All existing tests updated where
`OllamaChatRequest` is constructed by struct literal.

## Watcher Task-by-Task Execution and System Prompt Hardening - 2026-05-07

**Files Changed:**

- `src/commands/run.rs` - Extracted `execute_plan` from `handle_run`;
  strengthened `execute_plan_task` system prompt to mandate immediate tool use.
- `src/commands/watch.rs` - `run_generic_watcher_loop` now calls `execute_plan`
  instead of `execute_plan_task(task.instruction, ...)`.

**Summary:** The generic watcher was passing the entire plan as one concatenated
instruction to `execute_plan_task`. For complex plans this exceeded the model's
context window (4096 tokens) leaving no room for tool definitions, causing the
model to respond with prose instead of tool calls. The plan's `max_iterations`
override was also silently ignored. The fix extracts the task-by-task execution
logic from `handle_run` into a new shared `execute_plan` function and wires the
watcher to use it, giving each task its own focused agent call with full tool
visibility and correct iteration limits. The system prompt was also strengthened
to explicitly mandate tool use over prose responses.

**Testing:** Existing unit tests cover both `handle_run` and the watcher loop;
no new network or process calls introduced.

## ACP/Zed: OpenAI Streaming and Thinking Toggle - 2026-05-07

**Files Changed:**

- `src/providers/openai.rs` - Implemented
  `chat_completion_stream_with_callback`; `chat_completion_stream` now delegates
  to it with a no-op closure.
- `src/commands/agent.rs` - Added `SessionModeChange` enum,
  `session_mode_change_from_id`, `thought_level_config_option`; updated
  `SetSessionModeRequest` handler to dispatch both terminal and thinking mode
  changes; added `config_options` to `NewSessionResponse` and
  `LoadSessionResponse`.

**Summary:** Two issues in the ACP/Zed agent integration were fixed. First,
`OpenAIProvider` did not override `chat_completion_stream_with_callback`, so the
default trait impl dropped the stream notifier and fell back to blocking
`chat_completion`. This meant token chunks were never forwarded to Zed's chat
window. The fix implements the override following the Ollama pattern: the SSE
loop fires `on_chunk(text, false)` for content deltas and `on_chunk(text, true)`
for reasoning/thinking deltas before accumulating into the `StreamAccumulator`;
`chat_completion_stream` delegates to the new method with a no-op closure so it
also benefits from the streaming path. Second, no thinking mode selector was
surfaced to Zed, making it impossible to enable thinking for models that do not
advertise `ModelCapability::Thinking` (e.g. llama.cpp). The fix adds a
`ThoughtLevel` `SessionConfigOption` to both `NewSessionResponse` and
`LoadSessionResponse`, rendering a "Thinking" dropdown (Off / Low / Medium /
High / Max) in the Zed session panel. When the user selects a value, Zed
delivers a `SetSessionModeRequest` whose `mode_id` is the thinking effort string
("none" / "low" / "medium" / "high" / "extra_high"). The handler dispatches
these via the new `SessionModeChange::Thinking` variant and calls
`executor.enable_provider_thinking(mode)` to propagate to the provider. Terminal
mode IDs continue to work exactly as before via `SessionModeChange::Terminal`.

**Testing:** All 2133 unit tests pass. Clippy clean.

**Files Changed:**

- `src/tools/terminal/executor.rs` - Added `working_dir: Option<PathBuf>` field
  and `with_working_dir()` builder; set `current_dir` on spawned shell; removed
  `preserve_full_output` bypass; fixed `truncate_output` underflow with
  saturating subtraction; added `working_dir()` getter; added 6 new tests.
- `src/tools/execute_command.rs` - Added `with_working_dir()` builder that
  propagates the path to the underlying `Terminal`.
- `src/tools/registry_builder.rs` - `build_for_write()` now chains
  `.with_working_dir(self.working_dir.clone())` on the `ExecuteCommandTool` so
  shell commands anchor to the same base directory as all file tools.

**Summary:** Two executor bugs were fixed. First, the `preserve_full_output`
flag tied output size to the tracing log level: when DEBUG logging was active
the full stdout bypassed the configured `max_stdout_bytes` limit and was
returned verbatim to the agent. This caused the agent to treat large stdout
payloads as a substitute for file-based persistence, skipping `> file` redirects
and then hallucinating that the redirect had succeeded. The fix removes the
conditional entirely; the full output is still emitted at TRACE level for
operator inspection, but what is returned to the agent is always capped by the
configured limits. Second, the executor ran commands from the process CWD rather
than `config.working_dir`, making shell redirects inconsistent with the base
directory used by all file tools. The fix adds a `working_dir: Option<PathBuf>`
field to `Terminal`, a `with_working_dir()` builder, and passes
`.current_dir(&working_dir)` to every spawned shell. The `ToolRegistryBuilder`
now threads `working_dir` through to `ExecuteCommandTool` automatically.

**Testing:** 6 new unit tests in `executor.rs`:
`test_with_working_dir_stores_path`, `test_default_terminal_has_no_working_dir`,
`test_execute_with_working_dir_uses_that_directory`,
`test_execute_redirect_writes_to_working_dir`,
`test_output_always_truncated_to_configured_limit`. All 20 module tests pass.

## TerminalMode Wiring in ToolRegistryBuilder - 2026-05-06

**Files Changed:**

- `src/tools/registry_builder.rs` - Added `terminal_mode` field,
  `with_terminal_mode()` builder method, `terminal_mode()` getter, and
  mode-based `CommandValidator` selection in `build_for_write()`.

**Summary:** `ToolRegistryBuilder` now carries a `terminal_mode: TerminalMode`
field (defaulting to `RestrictedAutonomous` in both `new()` and
`with_limits()`). The `with_terminal_mode()` builder method sets the mode and
returns `Self`. Inside `build_for_write()` the `CommandValidator` is chosen
based on the mode: `FullAutonomous` yields `CommandValidator::permissive()`
(only critical deny patterns), while every other mode yields
`CommandValidator::new()` (full deny and confirmation lists). The debug log line
now includes the `terminal_mode` field as a structured tracing field. A
`terminal_mode()` getter exposes the stored mode for callers and tests.

**Testing:** Four new unit tests added to the `tests` module:
`test_default_terminal_mode_is_restricted`, `test_with_terminal_mode_sets_mode`,
`test_full_autonomous_uses_permissive_validator`, and
`test_restricted_autonomous_uses_full_validator`. All 18 module tests pass.

## Ollama Streaming Tool Call Fix and Native Thinking Support - 2026-05-05

## ACP Model Selection Support - 2026-05-05

**Files Changed:**

- `src/commands/agent.rs` - Added `SetSessionModelRequest` handler and
  auto-selection of valid default models:
  1. **SetSessionModelRequest handler**: Registered a new
     `.on_receive_request()` handler in `make_agent_builder` that responds to
     Zed's model selector. When the user selects a model, the handler looks up
     the session, calls `executor.switch_model()` on the provider, re-detects
     model capabilities (vision, thinking), and updates the session state.
  2. **Auto-select valid default model**: In both `NewSessionRequest` and
     `LoadSessionRequest` handlers, after listing available models from the
     provider, the code now checks whether the configured model actually exists
     in the provider's model list. If not, it auto-selects the first available
     model and logs a warning. This prevents sessions from starting with a
     non-existent model when the config default doesn't match what's installed.
  - Added `SetSessionModelRequest`, `SetSessionModelResponse` to imports.
  - Added `sessions_for_set_model` Arc clone for handler state.

**Summary:** The Zed model selector did nothing in ACP mode because there was no
`SetSessionModelRequest` handler registered. Model selection was locked to
whatever was in the config file at session creation time. Additionally, if the
configured default model didn't exist (e.g., `llama3.2:latest` when only
`qwen3:latest` is installed), the session would use a non-existent model and
fail. Now the model selector works and invalid defaults are auto-corrected.

**Testing:** All 780 tests pass. The handler uses the same pattern as the
existing `SetSessionModeRequest` handler.

**Files Changed:**

- `src/providers/ollama.rs` - Fixed two bugs that broke tool calling when
  streaming and thinking mode were enabled:
  1. **Streaming tool call extraction**: Ollama sends tool calls on
     `done: false` chunks, then sends a metadata-only `done: true` chunk. The
     code only extracted tool calls from the `done: true` chunk, dropping them
     entirely. Added `accumulated_tool_calls` vector that collects tool calls
     from all chunks during streaming, with a fallback to the done chunk for
     backward compatibility.
  2. **Native `think` parameter**: Replaced the `<|think|>` token injection into
     the system prompt (which interfered with structured tool calling) with
     Ollama's native `think: bool` API parameter. Added `think` field to
     `OllamaChatRequest` and `thinking` field to `OllamaChatMessage` for proper
     API support. Updated both streaming and non-streaming paths. Streaming
     callback now routes native `thinking` field content with `is_thinking=true`
     and falls back to `ThinkingParser` for older Ollama versions.
  - Replaced `test_ollama_streaming_tool_calls_extracted_from_done_chunk` with
    two tests: `test_ollama_streaming_tool_calls_from_non_done_chunk` (actual
    Ollama API behavior) and
    `test_ollama_streaming_tool_calls_fallback_from_done_chunk` (backward
    compatibility).

- `src/agent/executor.rs` - Fixed three tool call handling bugs:
  1. **Execute ALL tool calls**: Previously only `tool_calls[0]` was executed
     while the assistant message recorded all of them, leaving orphaned
     `tool_call_id`s without corresponding `Role::Tool` result messages. Now
     iterates over every tool call in the response.
  2. **Image tool results**: Previously added as `Role::User` messages instead
     of `Role::Tool`, breaking the `assistant(tool_calls) -> tool(result)`
     contract. Now always adds a proper `Role::Tool` result first, then collects
     images for a follow-up `Role::User` message injected after ALL tool
     results.
  3. **Single return path**: Removed early returns from individual tool call
     branches; the loop now processes all calls before returning
     `IterationResult::Continue`.

- `src/agent/conversation.rs` - Fixed FIFO pruning to remove tool-call/result
  blocks atomically. Replaced `find_removable_message_index()` (which always
  returned index 0) with `find_removable_block()` that identifies the correct
  block to remove:
  - Assistant with `tool_calls` + all following `Role::Tool` messages
  - Orphaned `Role::Tool` messages at the front
  - Single non-tool messages This prevents pruning from creating orphaned tool
    calls or tool results that violate the chat template's role alternation
    rules.

**Summary:** The OpenAI-compatible endpoint returned a 500 error because the
conversation history violated chat template rules requiring alternating
user/assistant roles with properly paired tool calls and results. Three issues
contributed: (1) only the first tool call was executed, leaving the rest without
results; (2) image-bearing tool results used `Role::User` instead of
`Role::Tool`; (3) FIFO pruning could split tool-call/result pairs.

**Testing:** All 780 tests pass. No new tests added for the executor changes
(existing integration tests cover the single-tool-call path; multi-tool-call
testing requires a mock provider setup).

## Ollama Tool Result `tool_name` Fix - 2026-05-04

**Files Changed:**

- `src/providers/ollama.rs` - Added `tool_name: Option<String>` field to
  `OllamaChatMessage` (with `skip_serializing_if = "Option::is_none"`) so that
  tool result messages sent to Ollama include the required `tool_name` field.
  Updated `convert_messages_to_ollama` to build a `tool_call_id -> tool_name`
  lookup from assistant messages and resolve it for `Role::Tool` messages. Added
  `tool_name: None` to all other `OllamaChatMessage` struct literals (1
  production site, 7 test sites). Updated existing test
  `test_ollama_message_conversion_tool_role` to verify `tool_name` resolution
  from a preceding assistant tool call. Added two new tests:
  `test_ollama_message_conversion_tool_role_without_preceding_call` (verifies
  `None` when no matching call exists),
  `test_ollama_tool_result_message_includes_tool_name` (verifies `tool_name`
  appears in serialised JSON), and
  `test_ollama_non_tool_message_omits_tool_name` (verifies `tool_name` is
  omitted when `None`).

**Summary:** Ollama uses `tool_name` (not `tool_call_id` like OpenAI/Copilot) to
associate tool result messages with their originating tool calls. The
`OllamaChatMessage` struct was missing this field entirely, causing Ollama to be
unable to match tool results back to calls. This broke tool calling in ACP mode
when using Atoma as a Zed agent configured to access Ollama. The fix adds the
field and resolves it by looking up the tool call ID in the conversation's
assistant messages.

**Testing:** 3 new unit tests added; all 780 project tests pass.

## ACP Structured Tool Call Notifications - 2026-05-04

**Files Changed:**

- `src/commands/agent.rs` - Replaced the "Tool calls disabled" text-chunk
  notifier with proper ACP `SessionUpdate::ToolCall` and
  `SessionUpdate::ToolCallUpdate` notifications. Added imports for
  `AcpToolCall`, `ToolCallId`, `ToolCallStatus`, `ToolCallUpdate`,
  `ToolCallUpdateFields`, and `ToolKind` from `agent_client_protocol::schema`.
  Promoted `tool_kind_from_name` from `#[cfg(test)]` to `pub` so it is available
  at runtime for the notifier closure. The notifier now emits:
  - `SessionUpdate::ToolCall` with `InProgress` status, `ToolKind`, title, and
    `raw_input` on `ToolCallEvent::Started`
  - `SessionUpdate::ToolCallUpdate` with `Completed` status and `raw_output` on
    `ToolCallEvent::Completed`
  - `SessionUpdate::ToolCallUpdate` with `Failed` status and `raw_output` on
    `ToolCallEvent::Failed`

**Summary:** The Zed ACP agent's tool notifier was emitting "Tool calls
disabled" plain text chunks via `SessionUpdate::AgentMessageChunk` instead of
using the structured `SessionUpdate::ToolCall` and
`SessionUpdate::ToolCallUpdate` notifications defined by the ACP protocol. This
prevented Zed from rendering proper tool-call UI cards (spinners, status
indicators, diff views). The fix replaces the text-chunk approach with the
correct ACP builders, enabling structured tool-call UI in the Zed sidebar.

**Testing:** All 780 project tests pass; existing `tool_kind_from_name` test
continues to verify mapping correctness.

## OpenAI Provider Tool Call Test Hardening - 2026-05-04

**Files Changed:**

- `src/providers/openai.rs` - Strengthened the existing
  `test_openai_chat_completion_with_tool_calls` test with assertions on `id`,
  `arguments`, `len()`, and `finish_reason` (previously only checked
  `function.name`). Added 10 new tests:
  - `test_map_openai_tool_calls_none_returns_none` -- edge case: `None` input
  - `test_map_openai_tool_calls_empty_returns_none` -- edge case: empty slice
  - `test_map_openai_tool_calls_single` -- full field verification
  - `test_map_openai_tool_calls_multiple` -- two concurrent tool calls
  - `test_map_finish_reason_function_call_legacy` -- legacy `"function_call"`
  - `test_map_finish_reason_tool_calls` -- standard `"tool_calls"`
  - `test_map_finish_reason_unknown_defaults_to_stop` -- fallback behavior
  - `test_convert_messages_to_openai_tool_result_carries_tool_call_id` --
    round-trip: user -> assistant(tool_calls) -> tool_result(tool_call_id) ->
    user, verifying the full conversation serialization
  - `test_openai_chat_completion_multiple_tool_calls` -- two tool calls in a
    single response via wiremock
  - `test_openai_chat_completion_sends_tool_definitions` -- verifies the
    outbound request body contains the `tools` array with correct structure

**Summary:** Code review of the OpenAI provider confirmed the tool call
implementation is correct (wire format, message conversion, streaming
accumulation, finish_reason handling all align with the OpenAI Chat Completions
API spec). However, test coverage had significant gaps: the non-streaming test
was under-asserted, the outbound tool definitions path was untested, the message
conversion round-trip for tool conversations was untested, and edge cases for
helper functions were missing. These 10 new tests close those gaps.

**Testing:** All tests pass (2,119 lib + 862 integration/doc).

## Ollama Thinking Mode Propagation Fix - 2026-05-04

**Files Changed:**

- `src/providers/ollama.rs` - Added
  `pub fn set_thinking_mode(&mut self, mode: ThinkingMode)` method to `Ollama`.
  When given `ThinkingMode::None`, clears `self.thinking_mode` to `None`; all
  other values set `Some(mode)`. Added two tests:
  `test_ollama_set_thinking_mode_enables_thinking` and
  `test_ollama_set_thinking_mode_none_disables_thinking`.
- `src/commands/agent.rs` - In `build_provider`, after constructing the Ollama
  provider, resolve thinking mode from `config.ollama.thinking_mode` with
  fallback to `config.agent.thinking_mode` and call `set_thinking_mode`. Added
  two tests: `test_build_provider_ollama_propagates_thinking_mode` and
  `test_build_provider_ollama_falls_back_to_agent_thinking_mode`.
- `src/providers/factory.rs` - Same thinking mode resolution in
  `ProviderFactory::create_ollama`.
- `src/providers/mod.rs` - Same thinking mode resolution in
  `create_provider_with_override` for Ollama subagent path.
- `src/commands/run.rs` - Same thinking mode resolution in
  `create_provider_for_model` for the summary provider path.

**Summary:** The Ollama provider's `thinking_mode` field (which gates injection
of the `<|think|>` token into the system prompt and activates the
`ThinkingParser` for streaming) was never set from config. `Ollama::new()`
defaults `thinking_mode` to `None`, and none of the four provider construction
sites transferred the config value. As a result, thinking was always disabled
for Ollama in all modes (Zed ACP, CLI run/chat, ACP serve, subagent). The fix
adds a setter and wires it into all four construction sites with proper
precedence: provider-level config > global agent-level config > default (None).

**Testing:** 4 new tests added; all project tests pass.

## Thinking Capability Auto-Detection - 2026-05-04

**Files Changed:**

- `src/providers/types.rs` - Added `Thinking` variant to `ModelCapability`.
- `src/providers/trait_mod.rs` - Added
  `set_thinking_mode(&mut self, mode: ThinkingMode)` with default no-op to the
  `Provider` trait.
- `src/providers/ollama.rs` - Moved `set_thinking_mode` from `impl Ollama` to
  `impl Provider for Ollama` (overrides the trait default). Added `"thinking"`
  to the API capability string match in `build_model_capabilities`. Added
  model-family heuristic detection for known thinking models (`qwq`, `deepseek`)
  in both the API and fallback paths.
- `src/agent/executor.rs` - Added
  `pub fn enable_provider_thinking(&mut self, mode: ThinkingMode)` that
  propagates thinking mode to both the executor and its attached provider via
  the `Provider::set_thinking_mode` trait method.
- `src/commands/agent.rs` - In both `NewSessionRequest` and `LoadSessionRequest`
  handlers, after fetching model info (where vision is already detected), added
  auto-detection: when the model reports `ModelCapability::Thinking` and no
  thinking mode was explicitly configured,
  `enable_provider_thinking(ThinkingMode::High)` is called automatically.
- `src/acp/executor.rs` - Same auto-detection in `AcpRunExecutor::execute` after
  the existing thinking mode resolution chain.
- `src/commands/chat.rs` (demo) - Added exhaustive match arms for the new
  `Thinking` capability variant.

**Summary:** Thinking mode was previously gated entirely on manual
configuration. Users had to set `ollama.thinking_mode: high` (or the
corresponding env var) even when using models like QwQ or DeepSeek-R1 that
inherently support extended reasoning. Now the system auto-detects thinking
capability from two sources: (1) the Ollama `/api/show` capabilities array (when
it reports `"thinking"`), and (2) model family heuristics for known thinking
model families. When detected and no explicit config exists, thinking mode is
automatically enabled on the provider, causing the `<|think|>` token injection
and `ThinkingParser` streaming to activate. Explicit user configuration
(including `thinking_mode: none` to disable) always takes precedence over
auto-detection.

**Testing:** All project tests pass.

## Thinking Mode - Phase 1 Copilot Echoed Effort Parsing - 2026-05-04

**Files Changed:**

- `src/providers/copilot.rs` - Added `reasoning: Option<ReasoningConfig>` field
  to `ResponsesResponse` so the Responses API echoed effort is captured during
  deserialization. Added `echoed_effort: Option<String>` field to
  `ResponsesAccumulator` (initialized to `None` in `new`). Updated
  `apply_response_payload` for the `ResponsePayload::Response` branch to set
  `echoed_effort` from `response.reasoning.effort` when present. Updated the
  non-streaming path in `responses_request` to capture the echoed effort from
  the decoded `ResponsesResponse` body before it is consumed by
  `responses_response_to_events`. Updated `ResponsesAccumulator::finalize` to
  resolve `thinking_mode` from `echoed_effort` via
  `ThinkingMode::from_effort_str`, defaulting to `ThinkingMode::High` when the
  field is absent. Updated two existing test struct literals
  (`test_non_streaming_responses_shim_accumulates_text` and
  `test_responses_completed_event_usage_mapping`) to include `reasoning: None`
  for the new field. Added assertion to
  `test_responses_completed_event_usage_mapping` that no echoed effort defaults
  to `ThinkingMode::High`. Added new test
  `test_responses_completed_event_echoed_effort_sets_thinking_mode` that
  verifies a `Completed` event carrying `reasoning.effort = "medium"` produces a
  `ProviderResponse` with `thinking_mode == ThinkingMode::Medium`.

**Summary:** The Copilot Responses API echoes back the reasoning effort level in
the response JSON (e.g. `"reasoning": { "effort": "medium" }`). This change
wires that echoed value through the `ResponsesAccumulator` so the resulting
`ProviderResponse::thinking_mode` reflects the effort actually used rather than
always defaulting to `High`. Both the streaming path (via the `Completed` SSE
event) and the non-streaming path are covered. Unknown or absent effort strings
continue to default to `ThinkingMode::High` via the existing
`ThinkingMode::from_effort_str` fallback. This satisfies the Phase 1 success
criterion: "A `ProviderResponse` built from a Copilot Responses API reply
containing `\"reasoning\": { \"effort\": \"medium\" }` resolves to
`ThinkingMode::Medium`."

**Testing:** 2 lib tests for the new and updated copilot accumulator behaviour
(`test_responses_completed_event_usage_mapping`,
`test_responses_completed_event_echoed_effort_sets_thinking_mode`). Full suite:
2106 lib tests, 780 integration/unit tests — all pass.

## Config Auto-Discovery - 2026-05-04

**Files Changed:**

- `src/config.rs` - Added `find_default_config()` function and updated
  `Config::load` to call it as a fallback when no explicit config path is
  provided.

**Summary:** When no `--config` path or `ATOMA_CONFIG` environment variable is
set, `Config::load` now searches `~/.config/atoma/config.yaml` and
`~/.atoma/config.yaml` in that order. The first file found is loaded
automatically. This fixes the bug where `atoma chat` always defaulted to
`llama3.2:latest` when users had their preferred model in a config file but did
not specify `--config` on every invocation.

**Testing:** Added five unit tests: `test_find_default_config_xdg_path`,
`test_find_default_config_atoma_dot_dir`,
`test_find_default_config_xdg_takes_priority_over_atoma_dir`,
`test_find_default_config_returns_none_when_absent`, and
`test_config_load_uses_default_discovery_when_no_path_given`.

## New documentation: Enable thinking mode how-to - 2026-05-06

**Files Changed:**

- `docs/how-to/enable_thinking_mode.md` - New how-to describing how to enable
  and verify "thinking" (extended reasoning) mode via configuration file,
  command-line flags, environment variables, and ACP RunRequest overrides. The
  document explains precedence rules, accepted values, provider notes (Ollama,
  Copilot, OpenAI-compatible), and troubleshooting steps.

**Summary:** Added a user-facing how-to that consolidates the available
mechanisms for enabling thinking mode across Atoma's configuration surface:

- Configuration file keys (`agent.thinking_mode`, `ollama.thinking_mode`,
  `copilot.thinking_mode`, `openai.thinking_mode`)
- CLI flags (`--thinking-mode` on `atoma chat` and `atoma run`)
- Environment variables (e.g. `ATOMA_THINKING_MODE`,
  `ATOMA_OLLAMA_THINKING_MODE`)
- ACP `RunRequest.thinking_mode` per-run override

The guide documents expected behaviour (auto-detection vs explicit
configuration), accepted values, and verification techniques (SSE `run_thinking`
events and log messages).

**Testing:** Manual verification steps included in the document. No code was
changed; documentation-only update.

## Watcher README and Demo Script Documentation Update - 2026-05-06

**Files Changed:**

- `README.md` - Expanded the top-level watcher mode description to mention both
  Polaris and generic Redpanda plan topics. Added a generic watcher demo
  subsection that shows the `demo/watcher/generic/` workflow using
  `python3 seed_plan.py hello` and `python3 watch_results.py`.
- `demo/watcher/generic/README.md` - Previously updated to describe the generic
  watcher demo and its Python helper scripts.
- `demo/watcher/README.md` - Updated the directory layout to reference
  `seed_plan.py` and `watch_results.py`.
- `docs/how-to/watcher_demo.md` - Updated the generic watcher guide to reference
  the Python helper scripts and the new plan presets.

**Summary:** The top-level README still described watcher mode only in terms of
Polaris. This update makes the documentation consistent with the generic watcher
demo by calling out Redpanda plan-topic execution and showing the new Python
helper scripts for seeding plans and reading results. The surrounding watcher
demo documentation was already updated to match the same script names and usage
patterns.

**Testing:** Documentation-only change; no code or automated tests were run.

## Generic Watcher Test-Container Plan Robustness Update - 2026-05-06

**Files Changed:**

- `demo/watcher/generic/plans/test_container.yaml` - Updated the clone task to
  remove any previous partial clone directory before attempting the SSH and
  HTTPS clones. The task now checks the clone with an explicit `./tmp` path,
  reducing the chance of an existing-directory failure when rerunning the plan
  in the same demo workspace.

**Summary:** The `test-container` plan could fail on reruns if a partial clone
from a previous attempt already existed in `./tmp/viya-esp-clients-test-api`.
The clone task now starts by removing that directory, then retries the SSH and
HTTPS clone steps, and finally verifies the clone using the same `./tmp` path
that the task created. This makes the plan idempotent across repeated demo runs.

**Testing:** No automated tests were run; the change is limited to the plan YAML
used by the generic watcher demo.

## Terminal Executor Debug/Trace Output Improvement - 2026-05-06

**Files Changed:**

- `src/tools/terminal/executor.rs` - Added structured `tracing` instrumentation
  around command execution. The executor now logs the command, program, args,
  timeout, and output limits at trace level; logs the full stdout and stderr at
  debug level; and preserves full output instead of applying truncation when
  debug logging is enabled. In non-debug runs, the existing output-size limits
  still apply.

**Summary:** The terminal executor previously returned only truncated output to
callers and emitted no command-level stdout/stderr diagnostics, which made it
hard to see why a command failed during generic watcher runs. The updated
implementation keeps the existing safety limits for normal runs, but when the
process is running with debug or trace logging enabled it preserves the full
stdout/stderr in the returned `ExecutionResult` and emits the full captured
output to the log stream. This makes the terminal tool much easier to diagnose
without changing normal execution behaviour.

**Testing:** No automated tests were run for this logging-only change.

## Dynamic System Prompts Phase 1: Data Model - 2026-06-15

**Files Changed:**

- `src/config.rs` - Added `system_prompt: Option<String>` field to `AgentConfig`
  (with `#[serde(default, skip_serializing_if = "Option::is_none")]`) and its
  `Default` impl; added `system_prompt: Option<String>` to `AcpAgentConfig` and
  its `Default` impl; added `ATOMA_SYSTEM_PROMPT` env-var override block in
  `apply_env_vars()` immediately after the `ATOMA_THINKING_MODE` block; updated
  module-level env-var doc comment to list `ATOMA_SYSTEM_PROMPT`; added
  `system_prompt: None` to all 11 `AgentConfig` struct literal construction
  sites.
- `src/commands/plan_parser.rs` - Added `system_prompt: Option<String>` field to
  `Plan` (after `allow_dangerous`, before `action`); added 4096-character
  trimmed-length validation to `Plan::validate()`; added `system_prompt: None`
  to all Plan struct literal construction sites in doctests and unit tests.
- `src/commands/run.rs` - Added `system_prompt: None` to Plan struct literals at
  4 construction sites.
- `src/watcher/polaris/mod.rs` - Added `system_prompt: None` to Plan struct
  literal.
- `src/watcher/generic/result_event.rs` - Added `system_prompt: None` to Plan
  struct literals at 5 sites (doctests and test).
- `src/watcher/generic/matcher.rs` - Added `system_prompt: None` to Plan struct
  literal.
- `src/acp/server/handlers/agents.rs` - Added `system_prompt: None` to 2
  `AcpAgentConfig` struct literals.
- `src/acp/server/handlers/runs.rs` - Added `system_prompt: None` to
  `AcpAgentConfig` struct literal.
- `src/acp/server/handlers/sessions.rs` - Added `system_prompt: None` to
  `AcpAgentConfig` struct literal.
- `src/acp/server/registry.rs` - Added `system_prompt: None` to `AcpAgentConfig`
  struct literal.
- `tests/integration/acp_server_integration_test.rs` - Added
  `system_prompt: None` to 6 `AcpAgentConfig` struct literals.
- `tests/unit/watcher_generic.rs` - Added `system_prompt: None` to Plan struct
  literal.
- `tests/unit/config_system_prompt.rs` - New file: 8 unit tests covering
  `AgentConfig::default()`, env-var override via `ATOMA_SYSTEM_PROMPT`, YAML
  deserialization, `AcpAgentConfig::default()`, Plan YAML round-trip, validation
  (4097-char rejection, 4096-char acceptance), and `to_instruction` exclusion.
- `tests/unit/mod.rs` - Added `mod config_system_prompt;`.

**Summary:** Implements Phase 1 of the Dynamic System Prompts feature. All three
data model types (`AgentConfig`, `AcpAgentConfig`, `Plan`) now carry an optional
`system_prompt: Option<String>` field. The field serialises with
`skip_serializing_if = "Option::is_none"` so existing YAML files remain valid.
`Plan::validate()` rejects prompts whose trimmed length exceeds 4096 characters.
The `ATOMA_SYSTEM_PROMPT` environment variable is wired into
`Config::apply_env_vars()` following the same pattern as `ATOMA_THINKING_MODE`.
All struct literal construction sites across the codebase were updated to
include `system_prompt: None`.

**Testing:** 8 new unit tests in `tests/unit/config_system_prompt.rs`; all 800
existing tests continue to pass.

## Dynamic System Prompts Phase 2: Propagation Layer - 2026-06-15

**Files Changed:**

- `src/agent/executor.rs` - Added `build_system_prompt` top-level `pub fn` with
  full doc comment and doctest (placed before `#[cfg(test)]` module to satisfy
  `clippy::items_after_test_module`).
- `src/commands/run.rs` - Added `build_system_prompt` to imports; updated
  `init_plan_agent` signature and body to accept `system_prompt: Option<&str>`
  and use `build_system_prompt` to merge prefix with base guidance, emitting a
  `tracing::trace!` log; updated `execute_plan` signature and body to accept
  `system_prompt: Option<&str>` and compute `effective_system_prompt` via
  `plan.system_prompt.as_deref().or(system_prompt)` for plan-level precedence;
  updated `execute_plan_task` signature to accept and forward
  `system_prompt: Option<&str>`; updated doc comments and `no_run` doctests for
  both public functions; updated `handle_run` call sites to pass `None` for
  backward compatibility.
- `src/commands/watch.rs` - Added `, None` to `execute_plan_task` call in
  `run_watcher_loop` (L851) and `, None` to `execute_plan` call in
  `run_generic_watcher_loop` (L1271) for backward compatibility.
- `tests/unit/build_system_prompt.rs` - New file: 6 unit tests covering
  `build_system_prompt` (prefix merging, `None`, empty, whitespace-only) and the
  plan-level precedence logic (`plan_level_overrides_caller`,
  `caller_used_when_plan_has_none`).
- `tests/unit/mod.rs` - Added `mod build_system_prompt;`.

**Summary:** Implements Phase 2 of the Dynamic System Prompts feature. The
`build_system_prompt` helper merges an optional user-supplied role prefix with
the base tool-guidance string using two newlines as separator. The
`init_plan_agent` function now accepts and applies the merged prompt via
`build_system_prompt`, replacing the previous direct `set_system_prompt` call.
`execute_plan` computes a four-level precedence chain (plan field > caller
argument > env var > config file) and forwards the effective value to
`init_plan_agent`. `execute_plan_task` passes the new parameter through to
`init_plan_agent`. All existing call sites in `run.rs` and `watch.rs` pass
`None` as a backward-compatible placeholder pending Phase 3.

**Testing:** 6 new unit tests in `tests/unit/build_system_prompt.rs`; all 801
existing tests continue to pass including 4 doctests for `build_system_prompt`.

## Document system_prompt precedence chain in architecture reference - 2026-06-15

**Files Changed:**

- `docs/reference/architecture.md` - Added `system_prompt` field to the
  `AgentConfig` table; added new `### System Prompt Precedence` subsection under
  Configuration System documenting the four-level precedence chain (plan field >
  CLI flag > env var > config file), the `build_system_prompt` merge logic, and
  the `TRACE`-level log format; updated the Plan Parser bullet list to include
  `system_prompt` and its 4096-char limit; updated the `init_plan_agent` row in
  the Internal Helper Functions table to describe the system prompt resolution
  step.

**Summary:** Architecture reference now fully documents how the system prompt
prefix is resolved and merged with the base tool-guidance string across all run
and watch sessions.

**Testing:** No code changes; markdown lint and prettier pass on the updated
file.

## Dynamic System Prompts - 2026-06-15

**Files Changed:**

- `src/config.rs` - Added `system_prompt` to `AgentConfig` and `AcpAgentConfig`;
  added `ATOMA_SYSTEM_PROMPT` to `apply_env_vars`.
- `src/commands/plan_parser.rs` - Added `system_prompt` to `Plan` struct with
  4096-char validation and exclusion from `to_instruction`.
- `src/agent/executor.rs` - Added `build_system_prompt` top-level helper.
- `src/commands/run.rs` - Updated `init_plan_agent`, `execute_plan_task`,
  `execute_plan`, and `handle_run` to accept and propagate `system_prompt`.
- `src/commands/chat.rs` - Updated `handle_chat` with `user_prefix` and merged
  system prompt at all three set-prompt call sites.
- `src/commands/watch.rs` - Threaded `system_prompt` through `handle_watch`,
  `handle_polaris_watch`, `run_watcher_loop`, `handle_generic_watch`, and
  `run_generic_watcher_loop`.
- `src/cli.rs` - Added `--system-prompt` flag to `Chat`, `Run`, `Watch`.
- `src/main.rs` - Updated dispatch block and local `handle_chat` wrapper.
- `src/chat_mode/commands.rs` - Added `SystemPromptAction` enum and
  `SpecialCommand::SystemPrompt` variant with parser.
- `src/acp/executor.rs` - Replaced thinking-mode-only injection with merged user
  prefix + thinking-mode hint.
- `src/acp/types.rs` - Added `system_prompt_configured: bool` to
  `AgentManifest`.
- `src/acp/server/registry.rs` - Set `system_prompt_configured` from
  `agent_cfg.system_prompt.is_some()`.

**Summary:** Adds a configurable system-prompt prefix sourced from a four-level
precedence chain (plan field > CLI flag > env var > config file). The prefix is
merged with the existing mode-based tool-guidance string via the
`build_system_prompt` helper. Chat mode gains `/system`, `/system clear`, and
`/system <text>` in-session commands. ACP merges the per-agent prefix with the
thinking-mode hint instead of replacing it. The full merged prompt is logged at
TRACE level at session start.

**Testing:** 21 new unit tests across 3 test files
(`tests/unit/config_system_prompt.rs`, `tests/unit/build_system_prompt.rs`,
`tests/unit/acp_system_prompt.rs`) plus 13 unit and integration tests in
`tests/unit/system_prompt_cli.rs` and
`tests/integration/system_prompt_integration.rs`.

## Add system_prompt to generic watcher demo plans - 2026-06-15

**Files Changed:**

- `demo/watcher/generic/plans/hello_world.yaml` - Added `system_prompt` for a
  reliable operations assistant role.
- `demo/watcher/generic/plans/cyber_security.yaml` - Added `system_prompt` for
  an elite security professional specialising in red teaming and vulnerability
  research.
- `demo/watcher/generic/plans/doc_audit.yaml` - Added `system_prompt` for a Rust
  code quality auditor with subagent delegation focus.
- `demo/watcher/generic/plans/python_program.yaml` - Added `system_prompt` for
  an expert Python developer following modern best practices.
- `demo/watcher/generic/plans/system_health.yaml` - Added `system_prompt` for a
  systems administrator with strict output-directory constraints.
- `demo/watcher/generic/plans/test_container.yaml` - Added `system_prompt` for a
  QA engineer and test analyst producing structured JSON output.

**Summary:** Each generic watcher demo plan now carries a `system_prompt` field
that assigns a focused role to the agent for that plan's domain. The prompts are
placed after the `goals:` block (or `result_mentions:` where no goals are
defined) and before `tasks:`, consistent with the four-level precedence chain
introduced in Phase 1 of the Dynamic System Prompts feature.

**Testing:** No code changes; YAML files are parsed by the existing
`Plan::validate()` and round-trip deserialization tests.

## Fix missing build_system_prompt merges in chat context paths - 2026-06-15

**Files Changed:**

- `src/commands/chat.rs` - Fixed two `agent.set_system_prompt` call sites that
  called `get_system_prompt(...)` directly without merging `user_prefix`: the
  context-summarize path (after `/context summarize` rebuilds history) and the
  context-new path (after `/context new` resets the conversation). Both now call
  `build_system_prompt(user_prefix.as_deref(), &new_prompt)` so the
  user-supplied prefix is preserved across context resets, consistent with all
  other system-prompt call sites in `handle_chat`.
- `tests/unit/acp_system_prompt.rs` - Replaced hollow `#[ignore]` stub for
  `acp_trace_log_emitted` with a real `#[tracing_test::traced_test]` test that
  replicates the ACP executor trace event and asserts on `run_id` and
  `system_prompt` fields.
- `tests/unit/system_prompt_cli.rs` - Replaced hollow `#[ignore]` stub for
  `trace_log_emitted_on_session_start` with a real
  `#[tracing_test::traced_test]` test that replicates the `handle_chat` trace
  event and asserts the event was emitted with the merged prefix.
- `Cargo.toml` - Added `tracing-test = "0.2.6"` to `[dev-dependencies]` (via
  `cargo add tracing_test --dev`) to enable the `#[traced_test]` attribute.

**Summary:** Closed two functional gaps identified during the deliverable audit.
(1) The context-summarize and context-new paths in `handle_chat` silently
discarded the `user_prefix`, causing the user's system-prompt prefix to
disappear whenever they summarized or cleared their conversation. Both paths now
merge `user_prefix` via `build_system_prompt`, matching the mode-change and
safety-change paths. (2) The two trace-log tests (`acp_trace_log_emitted`,
`trace_log_emitted_on_session_start`) were stubs gated with `#[ignore]` and
contained no assertions; both are now live tests that pass.

**Testing:** 2 previously-stubbed tests now pass: `acp_trace_log_emitted` and
`trace_log_emitted_on_session_start`. All 37 system-prompt unit tests pass; full
test suite (548 unit + integration) remains green.

## Phase 5: Session Metadata and Tool Command Discovery - 2026-06-23

**Files Changed:**

- `src/agent/conversation.rs` - Added `pub(crate) fn derive_conversation_title`
  and four unit tests for short messages, truncation, no-user-message fallback,
  and exactly-50-char boundary.
- `src/agent/registry.rs` - Added `pub fn tools() -> Vec<Arc<dyn Tool>>` and
  `test_tool_registry_tools_returns_all` unit test.
- `src/commands/chat.rs` - Removed local `derive_conversation_title` definition;
  now imports the shared version from `crate::agent::conversation`.
- `src/commands/agent.rs` - Added `SessionInfoUpdate`, `AvailableCommand`, and
  `AvailableCommandsUpdate` to ACP schema imports; added
  `derive_conversation_title` import; added `session_commands` capture and
  `AvailableCommandsUpdate` emission in `NewSessionRequest` and
  `LoadSessionRequest` handlers; added title derivation and `SessionInfoUpdate`
  emission in `PromptRequest` handler; added `initial_title` capture and
  `SessionInfoUpdate` emission in `LoadSessionRequest`; added eight Phase 5 unit
  tests.

**Summary:** Closes three `SessionUpdate` gaps between Zed's
`handle_session_update` and what Atoma emits. The `PromptRequest` handler now
derives the conversation title from the first user message and pushes it to Zed
via `SessionInfoUpdate` so the thread tab shows a real name after the first
turn. The `LoadSessionRequest` handler sends the saved title immediately when
resuming a named conversation. Both `NewSessionRequest` and `LoadSessionRequest`
now emit `AvailableCommandsUpdate` containing every tool registered for the
session, so Zed's slash-command completion menu reflects the actual tool set.

**Testing:** 12 new unit tests added across `conversation.rs`, `registry.rs`,
and `agent.rs` covering title derivation edge cases, the default-title guard
that prevents spurious `SessionInfoUpdate` notifications, `AvailableCommand`
construction from a tool registry, the empty-registry guard, and JSON
serialisation round-trips for both new notification types.
