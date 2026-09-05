# Phase 6: Reference Documentation Accuracy Implementation

## Overview

Phase 6 reconciles the `docs/reference/` tree with the actual `src/` code. This
phase contains no code changes -- only Markdown. Every edited file passes the
AGENTS.md Rule 4 Markdown lint and format gate (`markdownlint --fix` and
`prettier --write --prose-wrap always`). Because no Rust source changed, the
Rust compile, clippy, and test gates are unaffected by this work.

The goal was that every referent named in `docs/reference/` (file paths,
symbols, providers, commands, config keys, env vars, and links) resolves to real
code.

## Ground Truth Established

The following facts were verified directly against the code and used to drive
every edit:

- Three providers exist: GitHub Copilot (`src/providers/copilot.rs`), Ollama
  (`src/providers/ollama.rs`), and OpenAI (`src/providers/openai.rs`). The
  structs are `CopilotProvider`, `OllamaProvider`, and `OpenAIProvider`.
- The `providers/` module has no `base.rs`. The trait lives in `trait_mod.rs`
  and shared types in `types.rs`, alongside `factory.rs`, `cache.rs`,
  `capabilities.rs`, `conversion.rs`, `http.rs`, `streaming.rs`, and `util.rs`.
- Copilot default model is `gpt-5-mini` (`config.rs::default_copilot_model`).
  OpenAI default model is `gpt-4o-mini` (`config.rs::default_openai_model`).
- Tools are per-file (`read_file.rs`, `write_file.rs`, `edit_file.rs`,
  `terminal.rs`, `grep.rs`, `find_path.rs`, `fetch.rs`, etc.). There is no
  `tools/file_ops.rs`, no `FileOpsTool`, and no `file_ops` registered tool name.
- The tool/plan executor lives in `src/acp/executor.rs`, not in `src/agent/`.
- The ACP module has no `routes.rs`, `handlers.rs`, `run.rs`, or `events.rs`. It
  exposes an HTTP server (`server.rs`, inline router) and a JSON-RPC stdio
  transport (`stdio.rs`), over a transport-independent domain model in
  `types.rs`.
- There is no top-level `src/xzepr/` shim. XZepr lives only under
  `src/watcher/xzepr/`.
- The generic watcher producer file is `result_producer.rs`, not `producer.rs`.
- MCP sampling is implemented by `XzatomaSamplingHandler`
  (`src/mcp/sampling.rs`); elicitation by `XzatomaElicitationHandler`
  (`src/mcp/elicitation.rs`).
- The `agent` subcommand exists (`Commands::Agent` in `src/cli.rs`) with flags
  `--provider`, `--model`, `--allow-dangerous`, `--working-dir`,
  `--system-prompt`, and `--streaming`.

## Corrections Applied

### `architecture.md`

- Top-level module tree: removed the nonexistent top-level `xzepr/` shim, added
  `security.rs` and `test_utils.rs`, expanded the `watcher/` subtree
  (`kafka_security.rs`, `lifecycle.rs`, `plan_executor.rs`, `topic_admin.rs`),
  and corrected the `acp/` comment.
- Removed the "Key Architectural Areas" bullet describing the nonexistent
  top-level `xzepr/` compatibility shim.
- Agent Layer: replaced `src/agent/executor.rs` with the real agent files and a
  note that execution lives in `src/acp/executor.rs`.
- Tools Layer: replaced `src/tools/file_ops.rs` with the real per-file tools.
- Watcher Module Structure tree: renamed `producer.rs` to `result_producer.rs`;
  added the generic tree's `consumer.rs`, `event.rs`, `event_handler.rs`, and
  `result_event.rs`; added the top-level watcher files.
- Updated the "Top-Level Watcher Module" `pub mod` list to match
  `watcher/mod.rs`.
- Renamed the `watcher/generic/producer.rs` component heading to
  `result_producer.rs`.
- Summary: removed the "one compatibility shim for legacy XZepr imports" bullet
  and the "in later phases" aspirational wording.
- Rewrote the ACP Architecture section (prose, module tree, request flow, and
  key components) to reflect the real files and the dual HTTP + stdio transport.

### `provider_abstraction.md`

- Corrected the Copilot default model to `gpt-5-mini`. The File Layout section
  was already correct (already listed `trait_mod.rs`, `types.rs`, `factory.rs`,
  `cache.rs`, `capabilities.rs` and no `base.rs`) and OpenAI was already
  present.

### `api.md`

- Added `OpenAIProvider` to the provider list.
- Removed the nonexistent `file_ops::FileOpsTool` reference; replaced with the
  real `read_file::ReadFileTool`, `write_file::WriteFileTool`, and
  `edit_file::EditFileTool`.
- Corrected the example Copilot default model to `gpt-5-mini`.

### `copilot_provider.md`

- Verified: the default model was already `gpt-5-mini` everywhere. No change
  required.

### `mcp_configuration.md`

- Removed the stale "sampling handler is not yet implemented" claim; documented
  `XzatomaSamplingHandler` (`src/mcp/sampling.rs`).
- Removed the equally stale "elicitation handler is not yet fully implemented"
  claim; documented `XzatomaElicitationHandler` (`src/mcp/elicitation.rs`).
- Updated the section heading and the two cross-reference anchors so intra-doc
  links resolve.

### `chat_commands.md`

- Removed the nonexistent `/summarize` command everywhere it appeared.
- Added the real commands and aliases missing from the doc (`/models`,
  `/mentions`, `/auth`, `/planning`, `/write`, `/safe`, `/yolo`, `/?`,
  `/streaming` toggles and status, and the `/context summary` model options),
  verified against `parse_special_command` in
  `src/commands/special_commands.rs`.

### `model_management.md`, `cli.md`, `quick_reference.md`

- Added the OpenAI provider everywhere provider options are listed.
- `cli.md`: added the `agent` subcommand with accurate flags read from
  `src/cli.rs`.
- `model_management.md`: corrected the Copilot config example default to
  `gpt-5-mini` and added an OpenAI provider subsection.
- `quick_reference.md`: rewrote the project structure tree to match the real
  `src/` layout (removing `agent/agent.rs`, `agent/executor.rs`,
  `providers/base.rs`, `tools/file_ops.rs`, the nonexistent command files, the
  ACP `handlers.rs`/`routes.rs`/`events.rs`, `mcp/transport.rs`, `mcp/auth.rs`,
  `mcp/task_manager.rs`, `skills/parsing.rs`, and the top-level `xzepr/` shim);
  added OpenAI to the Core Modules table.

### `watcher_environment_variables.md`

- Added the five missing env vars: `XZATOMA_WATCHER_EXECUTION_MODE`,
  `XZATOMA_WATCHER_GROUP_ID`, `XZEPR_KAFKA_SSL_CA_LOCATION`,
  `XZEPR_KAFKA_SSL_CERT_LOCATION`, and `XZEPR_KAFKA_SSL_KEY_LOCATION`, verified
  in `config.rs` and `src/watcher/xzepr/consumer/config.rs`.

### `subagent_api.md` (out-of-list, corrected for deliverable compliance)

- Replaced the nonexistent `file_ops` tool name in the `allowed_tools` examples
  with the real `read_file` tool, satisfying the deliverable "No reference doc
  names a nonexistent file, symbol, command, or env var."

## Discrepancies Between the Plan and the Code

The plan's task list contained a few items that did not match reality. These
were resolved in favor of accuracy (Phase 6's entire purpose):

1. `/mod` is not a command. `parse_special_command("/mod")` returns an error
   (asserted by `test_parse_partial_command_returns_none`). It was therefore not
   added to `chat_commands.md`, despite appearing in the Task 6.2 command list.
2. The `auth` device flow only supports `copilot` and `ollama`; `openai` is
   authenticated via `XZATOMA_OPENAI_API_KEY`, so OpenAI was not added to the
   `auth --provider` listings.
3. There is no `environment` top-level CLI subcommand; the `Commands` enum is
   `Chat, Run, Agent, Watch, Auth, Models, History, Replay, Mcp, Acp, Skills`.
4. `provider_abstraction.md` and `copilot_provider.md` were already largely
   accurate; only the Copilot default-model value needed correction in the
   former.

## Verification

- Grepped every edited doc for known-nonexistent referents (`file_ops`,
  `FileOpsTool`, `providers/base.rs`, `generic/producer.rs`, `acp/routes`,
  `acp/handlers`, `acp/run.rs`, `acp/events.rs`, `task_manager.rs`,
  `skills/parsing.rs`, `agent/executor.rs`, `agent/agent.rs`, `/summarize`,
  `/mod`): no matches remain.
- Confirmed `OpenAIProvider`, `XzatomaSamplingHandler`, and
  `XzatomaElicitationHandler` exist in the code.
- Confirmed the `Commands::Agent` subcommand and its flags in `src/cli.rs`.
- Confirmed the five watcher env vars in `config.rs` and
  `src/watcher/xzepr/consumer/config.rs`.
- Confirmed the `mcp_configuration.md` intra-doc anchors resolve
  (`#sampling-and-elicitation` matches `## Sampling and elicitation`).
- Ran `markdownlint --config .markdownlint.json` (no `--fix`) across all edited
  docs: all clean.
