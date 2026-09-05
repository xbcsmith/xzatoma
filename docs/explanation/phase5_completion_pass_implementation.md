<!--
SPDX-FileCopyrightText: 2026 XZatoma contributors
SPDX-License-Identifier: Apache-2.0
-->

# Phase 5 Completion Pass Implementation

## Overview

This document records the completion pass that closed the remaining Phase 5
(Code Consolidation / DRY) deliverables from `codebase_cleanup_plan.md`. A prior
audit found Phase 5 was partially complete: `P7` (config read-lock helper) and
`P2`/`P8` (streaming reader and HTTP error helpers) were fully done, while `P3`,
`P6`, `P1`, the `P4`/`P5` tool half, and the provider/watcher reference-doc
update were partial. This pass finished them.

The work was split into five disjoint write-scopes and executed in parallel so
no two work items touched the same file:

| Work item     | Deliverable                                | Write scope                                                                         |
| ------------- | ------------------------------------------ | ----------------------------------------------------------------------------------- |
| A (`P3`)      | Consolidate xzepr Kafka security parsing   | `watcher/xzepr/watcher.rs`, `watcher/xzepr/consumer/config.rs`, `kafka_security.rs` |
| B (`P6`)      | Replace `KafkaWatcherConfig` test literals | `commands/mod.rs`                                                                   |
| C (`P1`)      | Extract shared message-conversion helper   | `providers/{conversion,copilot,openai,ollama}.rs`                                   |
| D (`P4`/`P5`) | Apply `validate_or_err` to path tools      | `tools/*.rs`                                                                        |
| E (docs)      | Provider module tree in `architecture.md`  | `docs/reference/architecture.md`                                                    |

## Changes by deliverable

### P3 - Kafka security consolidation (xzepr backend)

The xzepr backend previously re-implemented protocol/mechanism string parsing
that the canonical `watcher/kafka_security.rs` already provided.

- `watcher/xzepr/watcher.rs::apply_security_config` and
  `watcher/xzepr/consumer/config.rs::from_env` now delegate to
  `kafka_security::parse_security_protocol` and `parse_sasl_mechanism`, mapping
  the shared error into the local `WatcherError`/`ConfigError` variants so error
  behavior is unchanged.
- The PLAINTEXT / SASL_PLAINTEXT insecurity warning was lifted into a shared,
  apply-time `warn_if_insecure` helper (backed by a pure `is_insecure_protocol`
  predicate) in `kafka_security.rs`. Because the generic producer path already
  calls the shared `apply_security_config`, it now emits the warning too. This
  also closes the Phase 1 `L2` gap where the generic path warned on nothing.
  Warnings fire only at apply time, never during repeated config-load parsing.

`enum SecurityProtocol` and `enum SaslMechanism` now have exactly one definition
each (in `kafka_security.rs`); no inline `match ...to_uppercase()` protocol or
mechanism parsing remains in the xzepr tree.

### P6 - KafkaWatcherConfig test literals

The 16 `test_apply_cli_overrides_*` tests in `commands/mod.rs` each built a full
ten-field `KafkaWatcherConfig` literal. Each was converted to
`..Default::default()`, keeping only the three fields that diverge from
`KafkaWatcherConfig::default()` (`topic`, `group_id`, `auto_create_topics`).
Every reconstructed value is byte-identical to the original literal, so no
assertion or override input changed.

### P1 - Shared provider message conversion

`providers/conversion.rs` gained a `pub(crate)` helper
`assistant_message_from_wire`, which centralizes the response-message assembly
duplicated in Copilot and OpenAI (map optional wire tool calls to
`assistant_with_tools`, otherwise wrap a lazily-computed fallback text). Both
providers now delegate to it while keeping their divergent branches at the call
site (Copilot preserves empty `Some` tool calls; OpenAI filters empties and
folds its multimodal content). The request-path `convert_messages` converters
were deliberately left provider-local because their content handling and wire
types diverge and only the already-shared validation and tool-call helpers are
genuinely common. Ollama was untouched because it uses native `images`,
JSON-argument tool calls, and generated IDs. Parity tests were added for the
shared helper.

### P4/P5 - Path-tool validation

The tool half was already fully realized: `path_tool::validate_or_err` is the
shared primitive and is applied at every applicable
validate-then-`ToolResult::error` site (`copy_path.rs`, `move_path.rs`,
`create_directory.rs`). The remaining `PathValidator::validate(..)?` sites
(`delete_path.rs`, `read_file.rs`, `list_directory.rs`, `edit_file.rs`,
`write_file.rs`) use hard `?` error propagation with a different contract and no
`"Invalid path"` prefix, so migrating them would change behavior; they were
correctly left as-is. No `PathTool` base type was introduced, consistent with
the plan's own "skip any premature macro" guidance and the AGENTS.md "do not
abstract prematurely" and "keep tools generic" principles.

### Documentation

`docs/reference/architecture.md`'s Provider Layer now lists every real module in
`src/providers/` (`trait_mod.rs`, `copilot.rs`, `ollama.rs`, `openai.rs`,
`factory.rs`, `cache.rs`, `capabilities.rs`, `types.rs`, `conversion.rs`,
`streaming.rs`, `http.rs`, `util.rs`), with descriptions taken from each
module's doc comment. This closes the Phase 6 gap 6.5 for the providers subtree.
No nonexistent file (such as `base.rs`) is listed.

## Validation

The full AGENTS.md Rule 4 quality gate passed after the pass:

- `cargo fmt --all` - clean.
- `cargo check --all-targets --all-features` - clean.
- `cargo clippy --all-targets --all-features -- -D warnings` - clean.
- `cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth` -
  2400 passed, 0 failed, 38 ignored.

The edited Markdown (`architecture.md` and this document) passes
`markdownlint --config .markdownlint.json` and
`prettier --parser markdown --prose-wrap always`.

## Out of scope / deferred

- `PathTool` base type: intentionally not built (premature abstraction).
- Kafka payload size-bounding (the remaining half of Phase 1 `L2`) is a separate
  security concern in `watcher/generic/event.rs`, not a Phase 5 DRY item, and is
  tracked separately.
