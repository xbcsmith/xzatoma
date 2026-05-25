# Public API and Test Clutter Pruning Implementation

## Summary

Phase 5 narrows historical compatibility surfaces, removes deprecated model
capability paths, reduces dead-code suppressions, and converts several ignored
unit-test clusters back into normal test coverage.

## Public API pruning

The provider compatibility shim at `providers::base` was removed. Canonical
provider imports now come from `xzatoma::providers` or the focused provider
submodules.

The MCP root no longer wildcard re-exports every protocol type from
`mcp::types`. Callers should import MCP protocol structures from
`xzatoma::mcp::types` and lifecycle or transport behavior from the focused MCP
submodules.

The ACP compatibility modules `acp::events`, `acp::handlers`, `acp::routes`, and
`acp::run` were removed. The supported ACP surface is the root `acp` facade for
domain types plus focused modules such as `acp::runtime`, `acp::server`, and
`acp::stdio`.

The old root `xzepr` compatibility module was removed. XZepr remains supported
under the canonical watcher path, `xzatoma::watcher::xzepr`.

The command-level `commands::chat_mode` compatibility re-export was removed;
chat mode types remain available from `xzatoma::chat_mode` and the crate root
exports.

## Deprecated capability pruning

The deprecated `ModelCapability::Completion` and `ModelCapability::JsonMode`
variants were removed. Ollama still preserves raw capability strings such as
`completion` and `json` in provider metadata, but no active behavior maps those
legacy strings to first-class model capability variants.

Deprecated `assert_cmd::Command::cargo_bin` usages were replaced with shared
binary-path helpers that locate the compiled `xzatoma` binary without relying on
the deprecated helper.

## Dead-code suppression cleanup

Dead-code suppressions were removed from provider helpers, serde response
fields, MCP state, plan-format helpers, mention-cache APIs, and ACP queued
prompt execution. Test-only provider helpers are now compiled only for tests.
Serde-only fields were renamed with leading underscores and explicit serde
renames so their retention is local and documented.

One test-only module-level dead-code allowance remains in `tests/common/mod.rs`
because each integration test crate imports the shared helper module and uses a
different subset of those helpers.

## Ignored-test cleanup

ACP runtime and server tests now use their in-memory runtime state in normal
test runs. Environment-mutating config tests are serialized with
`serial_test::serial` instead of being ignored.

OpenAI wiremock provider tests are no longer ignored, so mocked HTTP behavior is
covered by normal test runs. Generic watcher dry-run and configuration-only
watcher tests are also unignored.

Tests that still require external services remain ignored with explicit reasons,
including system keyring tests and Kafka-broker integration tests.

## ACP queued prompt refactor

The ACP stdio `execute_queued_prompt` helper now accepts a private
`QueuedPromptExecution` context struct instead of a long argument list. The
refactor keeps the existing cancellation, observer, fallback execution, storage
checkpoint, and session-title update behavior while removing the Clippy
`too_many_arguments` suppression.

Additional tests cover direct queued-prompt cancellation, fallback execution
without a live ACP connection, and stop-reason mapping for cancelled,
max-iteration, and unhandled errors.
