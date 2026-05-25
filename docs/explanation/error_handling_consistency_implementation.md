# Error Handling Consistency Implementation

Phase 2 of the codebase cleanup plan standardizes expected validation errors,
source-preserving infrastructure failures, and model-visible tool failures.

## Typed validation errors

Chat mode parsing now returns `ChatModeParseError` and safety mode parsing now
returns `SafetyModeParseError`, replacing public `Result<_, String>` parsing
paths. Provider prompt validation now returns `PromptInputError` and
`ImagePromptError`, including typed cases for empty prompts, unusable prompt
content, invalid image MIME types, empty image sources, and text-only conversion
of image input.

ACP stdio prompt conversion maps prompt validation failures to JSON-RPC invalid
parameter errors instead of internal errors, so client input issues remain
protocol validation failures.

## Source-preserving crate errors

`XzatomaError` now includes structured variants for provider HTTP request
failures, provider HTTP status failures, provider response parse failures,
watcher failures, runtime timeouts, and storage failures. The storage variants
cover database open, migration, query, row decoding, serialization, and
persistence path failures while preserving source errors through `#[source]`
fields.

The agent timeout path now returns `XzatomaError::RuntimeTimeout` instead of a
configuration error. Storage and ACP runtime persistence paths now use
structured variants where source errors are available.

## XZepr watcher errors

The XZepr watcher no longer uses `anyhow!` for expected security configuration
or extraction errors. Watcher construction and start-up now use `WatcherError`,
and plan extraction uses `PlanExtractionError`. Conversions into `XzatomaError`
happen at command and crate boundaries while retaining source chains.

## Best-effort send and cleanup policy

Required ACP stdio responses now propagate response send failures. Best-effort
session notifications are sent through a shared log-on-error helper. Cleanup and
notification failures in ACP prompt workers, IDE terminal cleanup, MCP stdio
transport tasks, MCP elicitation prompts, and MCP sampling prompts are logged
instead of silently dropped.

## Tool error boundary

`ToolExecutor` and `ToolResult` documentation now defines the boundary:
`Err(XzatomaError)` is for tool infrastructure failure, while
`Ok(ToolResult::error(...))` is for model-visible operational failure after the
tool ran. `ToolResult` includes retryability and recoverability metadata helpers
for structured model-visible failures.

MCP resource and prompt bridge manager failures now propagate as infrastructure
errors instead of being downgraded to plain tool text; user rejections and
missing server registrations remain model-visible `ToolResult::error` outcomes.
