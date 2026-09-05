# Phase 4: ACP Agent Tool Implementation

## Overview

`src/tools/acp_agent.rs` adds the `call_acp_agent` tool to XZatoma. It enables
an agent running inside XZatoma to call a remote ACP-compatible server, either
waiting for the run to finish (`sync` mode) or returning a run ID immediately
(`async` mode).

## Design Decisions

### SSRF Allow-list Enforcement

All outbound URLs are validated against `acp.client.allowed_base_urls` before
any network connection is attempted. An empty list blocks every call. This
prevents server-side request forgery (SSRF) by requiring operators to explicitly
enumerate the remote agents that XZatoma is permitted to contact.

Trailing slashes are normalised on both the incoming URL and each allow-list
entry before comparison, so `http://agent:8765` and `http://agent:8765/` are
treated as equivalent.

### Deserialization of Remote Responses

`AcpRunId` on the server side is a newtype tuple struct (`AcpRunId(String)`)
that `serde` serialises as a bare JSON string. The local `RemoteRun` struct
therefore uses `id: String` rather than a wrapper type to avoid an unnecessary
layer of deserialisation.

The `output.messages` field is kept as `Vec<serde_json::Value>` rather than a
typed struct. This decouples the tool from server-side schema evolution: only
the `type` and `text` fields inside each part are accessed, and any unknown
fields or future part types are silently ignored.

### Polling Interval

The `poll_until_terminal` helper sleeps 500 ms between each `GET /runs/{id}`
request. This is a simple fixed interval chosen to balance responsiveness
against unnecessary load on the remote server.

### HTTP Client Lifecycle

A new `reqwest::Client` is built per `execute` call rather than being shared
across calls. This keeps the tool stateless and avoids holding open connection
pools when the tool is idle.

## Module Structure

```text
src/tools/acp_agent.rs
    AcpAgentTool          -- public struct, implements ToolExecutor
    AcpAgentParams        -- serde-deserialised call arguments
    RemoteRunResponse     -- top-level JSON wrapper
    RemoteRun             -- run record (id, status, output)
    RemoteRunStatus       -- { state: String }
    RemoteRunOutput       -- { messages: Vec<Value> }
    TOOL_CALL_ACP_AGENT   -- tool name constant
```

## Error Handling

All errors are returned as `XzatomaError::Tool(...)`. Operational failures
visible to the model (e.g. a run ending in `failed` state) are returned as
`Ok(ToolResult::error(...))` rather than `Err(...)`, following the project
convention.

## Tests

The unit tests cover:

- URL validation: empty allow-list, URL not in list, URL in list, trailing slash
  normalisation, multi-entry allow-list, different port rejection
- Execute path: invalid mode returns error, blocked URL returns allow-list error
  without making a network call (verified by asserting the error message
  content)
- Output extraction: single text part, multiple messages joined by newline,
  artifact parts ignored, empty output
- Tool definition: required fields present, tool name matches constant
- Debug impl: struct name appears in output
