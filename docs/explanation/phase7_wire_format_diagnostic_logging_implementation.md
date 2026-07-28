# Phase 7: Wire-Format Diagnostic Logging

## Overview

Phase 7 adds TRACE-level wire-format logging for `NewSessionResponse` in
`src/acp/stdio.rs`. After `create_session` constructs the response struct it
serializes it to JSON and emits a single `tracing` event at `TRACE` level. This
lets operators inspect the exact JSON payload sent to Zed when a new ACP session
is created, without modifying source code or attaching a debugger.

## Problem

Without this logging, diagnosing ACP protocol issues requires either attaching a
debugger or inserting temporary `println!` statements. Common classes of
protocol bugs that are invisible without the wire payload include:

- Wrong `configOptions` key names (Zed silently ignores unknown keys)
- Missing or misspelled entries in the `modes` array
- Incorrect `session_id` format (Zed expects a specific UUID string form)

These bugs produce no error at the Rust layer; the session is created
successfully from Rust's perspective but Zed behaves incorrectly at runtime.
Observing the literal JSON before it reaches Zed is the only reliable way to
confirm the payload is correct.

## Solution

After `create_session` builds the `NewSessionResponse`, it:

1. Serializes the response to a JSON string using `serde_json::to_string`.
2. Emits a `tracing` event at `TRACE` level with two structured fields:
   - `session_id` - the session identifier echoed for quick grep/filtering
   - `response_json` - the full serialized JSON string

The log emission is guarded by `tracing::enabled!(tracing::Level::TRACE)`. When
TRACE is not active the guard short-circuits before calling
`serde_json::to_string`, making the check zero-cost in production builds where
TRACE is not configured.

## Usage

Enable the log by setting `RUST_LOG` to include the `xzatoma::acp` module at
`trace` level:

```bash
RUST_LOG=xzatoma::acp=trace xzatoma agent 2>trace.log
```

Redirect stderr to a file because `tracing` writes to stderr by default. After
the agent starts a session the log file will contain a line matching:

```text
TRACE xzatoma::acp::stdio: ACP stdio: NewSessionResponse wire format session_id="<uuid>" response_json="{...}"
```

The `response_json` field contains the complete JSON object. You can extract and
pretty-print it with:

```bash
grep "NewSessionResponse wire format" trace.log \
  | sed 's/.*response_json="\(.*\)"/\1/' \
  | python3 -m json.tool
```

## Note on Task 7.2

The implementation spec also described a `LoadSessionResponse` log for session
resume paths. In XZatoma, session resume is controlled by
`AcpStdioConfig::resume_by_workspace` and is handled inside `create_session`
itself. There is no separate `LoadSessionRequest` handler at the protocol level;
the single `NewSessionRequest` handler covers both fresh-session and
resume-session code paths.

The Task 7.1 log therefore covers both code paths. Task 7.2 will apply if a
dedicated `LoadSessionRequest` handler is introduced in a future phase.

## Files Changed

- `src/acp/stdio.rs` - TRACE log emitted inside `create_session` after the
  `NewSessionResponse` is constructed

## Tests

| Test name                                                    | What it verifies                                                                                                                                                                               |
| ------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `test_new_session_response_is_serializable_to_json`          | Constructs a `NewSessionResponse` with modes and config options, serializes with `serde_json::to_string`, verifies the JSON contains the session_id value and that config_options is non-empty |
| `test_new_session_response_json_contains_config_options_key` | Verifies the serialized JSON string contains the literal key `"configOptions"`                                                                                                                 |

## Design Decision: TRACE vs DEBUG

TRACE level is reserved for protocol wire bytes and internal loop iterations
that produce very high output volume. Using TRACE for the full JSON payload
keeps DEBUG output readable for everyday troubleshooting.

DEBUG events in `xzatoma::acp` describe state transitions such as session
creation, tool dispatch, and error recovery. These are useful in normal
development. The full JSON payload of every response is useful only when
diagnosing a protocol conformance issue, which is an infrequent and deliberate
diagnostic act.

Operators who need the wire format explicitly opt in with
`RUST_LOG=xzatoma::acp=trace`. Operators who do not need it see no overhead: the
`tracing::enabled!` guard prevents the `serde_json::to_string` call entirely
when TRACE is not active.
