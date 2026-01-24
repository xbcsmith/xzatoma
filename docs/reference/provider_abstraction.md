# Provider Abstraction - Quick Reference

## Overview

This quick reference describes the provider abstraction patterns used by XZatoma and the
user-facing configuration and behavioral differences between supported providers:

- OpenAI
- Anthropic
- GitHub Copilot
- Ollama (local / remote)

Use this document when implementing provider adapters or configuring providers for
your environment. For implementation plans and historical notes, see:
`../archive/implementation_summaries/provider_abstraction_implementation_plan.md`
and `../archive/implementation_summaries/implementation_plan_refactoring_summary.md`.

---

## Provider Comparison

| Provider  | Auth Method    | Base URL              | Default Model     | Context Limit   | Streaming  | Tool Calls        |
| --------- | -------------- | --------------------- | ----------------- | --------------- | ---------- | ----------------- |
| OpenAI    | Bearer Token   | api.openai.com        | gpt-4o            | 128K            | SSE        | Native            |
| Anthropic | API Key Header | api.anthropic.com     | claude-sonnet-4-0 | 200K            | SSE        | Content blocks    |
| Copilot   | OAuth Device   | api.githubcopilot.com | gpt-5-mini        | 128K            | SSE        | OpenAI-compatible |
| Ollama    | None (local)   | localhost:11434       | User-configured   | Model-dependent | JSON Lines | Limited           |

---

## Core Interface (Pseudo-code)

```rust
// Language-agnostic pseudo-signature
interface Provider {
    async fn complete(messages: &[Message], tools: &[Tool]) -> Result<CompletionResponse>;
    async fn stream(messages: &[Message], tools: &[Tool]) -> Stream<CompletionChunk>;
    fn get_model_config(&self) -> ModelConfig;
    fn name(&self) -> &str;
    fn supports_streaming(&self) -> bool;
}
```

Types (conceptual):

```rust
struct Message { role: Role, content: Content }
struct Tool { name: String, description: String, parameters: JsonSchema }
struct CompletionResponse { message: Message, tool_calls: Vec<ToolCall>, usage: TokenUsage }
struct ToolCall { id: String, name: String, arguments: serde_json::Value }
```

---

## Environment Variables

OpenAI

```bash
export OPENAI_API_KEY="sk-..."          # Required
export OPENAI_HOST="https://api.openai.com"  # Optional
export OPENAI_TIMEOUT="600"             # Optional (seconds)
```

Anthropic

```bash
export ANTHROPIC_API_KEY="sk-ant-..."   # Required
export ANTHROPIC_HOST="https://api.anthropic.com"  # Optional
```

GitHub Copilot

```bash
# Preferred: run the CLI device OAuth flow (xzatoma auth --provider copilot)
export GITHUB_TOKEN="ghp_..."           # Optional for some flows
export COPILOT_API_KEY="..."            # Alternative (if supported)
```

Ollama (local)

```bash
export OLLAMA_HOST="http://localhost:11434"
export OLLAMA_MODEL="llama3.2:latest"
```

---

## Key Differences Between Providers

### System Prompt Handling

- OpenAI / Copilot / Ollama: system prompt as the first message:
```json
{ "role": "system", "content": "You are a helpful assistant" }
```

- Anthropic: system prompt is a separate field in the request payload:
```json
{ "system": "You are a helpful assistant", "messages": [...] }
```

### Tool Call Format

- OpenAI / Copilot: tool calls appear in a `tool_calls` field or functions list.
- Anthropic: tool usage is embedded in content array with special `tool_use`/`tool_result` blocks.
- Ollama: format can be provider-specific (JSON lines, model-dependent).

### Tool Results

- OpenAI/Copilot: tool results often appear as a `role: "tool"` message with an identifier.
- Anthropic: tool results can be `tool_result` entries inside a user/content array.

---

## Error Types (Common)

- AuthenticationError -> 401 Unauthorized
- RateLimitError -> 429 Too Many Requests (observe `retry_after`)
- ContextLengthError -> 400 Bad Request (context too long)
- ServerError -> 500, 502, 503, 504
- RequestError -> 400, 404 (invalid request)
- NetworkError -> Connection timeout, DNS failure
- NotImplementedError -> Feature not supported by provider

---

## Streaming Formats

SSE (OpenAI/Anthropic/Copilot)

```
data: {"choices":[{"delta":{"content":"Hello"}}]}

data: {"choices":[{"delta":{"content":" world"}}]}

data: [DONE]
```

JSON Lines (Ollama)

```
{"message":{"content":"Hello"}}
{"message":{"content":" world"}}
{"done":true}
```

When implementing streaming clients, accumulate partial chunks carefully and
handle reassembly (arguments or tool calls may be split across chunks).

---

## Retry & Backoff (Suggested Defaults)

```yaml
max_retries: 3
initial_delay_ms: 1000
max_delay_ms: 60000
backoff_multiplier: 2.0
retry_on_status: [429, 500, 502, 503, 504]
```

- Respect `Retry-After` headers where provided.
- For idempotent or retry-safe requests, apply exponential backoff.

---

## Minimal Example (Language-agnostic)

```text
# Create provider from config
config = ProviderConfig {
    provider_type: "openai",
    model: "gpt-4o",
    api_key: env("OPENAI_API_KEY")
}
provider = ProviderFactory.create(config)

# Make completion request
messages = [ Message{ role: \"user\", content: \"Hello\" } ]
response = await provider.complete(messages, tools=[])
print(response.message.content)

# If tool calls present, execute tools and continue the conversation loop
```

---

## Testing Checklist (Recommended)

- Request formatting validation per provider (headers, body schema)
- Response parsing and tool call extraction for each provider
- Error mapping & status code handling
- Retry behavior correctness (succeeds after N retries)
- Streaming chunk accumulation and correctness
- Authentication handling: env vars and CLI device flow for Copilot
- Mock HTTP server integration tests
- Test coverage target: >80%

---

## Common Pitfalls

- Anthropic expects separate `system` field (don't send system as first message blindly).
- Tool result formats differ (OpenAI vs Anthropic) — write provider adapters that normalize tool results early.
- Streaming JSON may split arguments or message pieces across chunks; implement chunk reassembly.
- Token accounting differences: providers vary in token reporting; consider consistent internal accounting.

---

## Security Reminders

- Never log API keys or secrets.
- Use HTTPS for remote providers (OpenAI, Anthropic).
- Validate and honor `retry-after` and rate limits to avoid throttling.
- Store secrets in environment variables or secure keyring backends, not source control.
- Implement timeouts to avoid hanging requests.

---

## Typical File Layout (implementation hint)

```
providers/
├── base.rs                 # Provider trait and base types
├── types.rs                # Message, Tool, Response types
├── errors.rs               # Error types & conversions
├── config.rs               # Provider configuration
├── api_client.rs           # Shared HTTP client & retry
├── openai.rs               # OpenAI implementation
├── anthropic.rs            # Anthropic implementation
├── copilot.rs              # GitHub Copilot (OAuth) implementation
├── ollama.rs               # Ollama local/remote provider
└── tests/                  # Provider unit/integration tests with mocks
```

---

## References

- Provider implementation plan (archived): `../archive/implementation_summaries/provider_abstraction_implementation_plan.md`
- Refactor audit & details: `../archive/implementation_summaries/implementation_plan_refactoring_summary.md`
- OpenAI API: https://platform.openai.com/docs/api-reference
- Anthropic API: https://docs.anthropic.com/en/api
- Ollama API: https://github.com/ollama/ollama/blob/main/docs/api.md

---
