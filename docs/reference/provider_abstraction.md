# Provider Abstraction - Quick Reference

## Overview

This quick reference describes the provider abstraction patterns used by XZatoma
and the user-facing configuration and behavioral differences between providers.

**Implemented providers** (available in `src/providers/`):

- GitHub Copilot
- Ollama (local / remote)

**API reference only** (documented for comparison purposes; not implemented as
separate providers in XZatoma):

- OpenAI
- Anthropic

OpenAI and Anthropic are included in this document because their API conventions
inform the provider trait design and are useful when evaluating compatibility or
extending XZatoma in the future. They are **not** shipped as built-in provider
implementations.

Use this document when working with the implemented providers or when comparing
API conventions across providers. For implementation plans and historical notes,
see:
`../archive/implementation_summaries/provider_abstraction_implementation_plan.md`
and
`../archive/implementation_summaries/implementation_plan_refactoring_summary.md`.

---

## Provider Comparison

| Provider  | Status             | Auth Method    | Base URL              | Default Model     | Context Limit   | Streaming  | Tool Calls        |
| --------- | ------------------ | -------------- | --------------------- | ----------------- | --------------- | ---------- | ----------------- |
| Copilot   | Implemented        | OAuth Device   | api.githubcopilot.com | gpt-5.3-codex     | 128K            | SSE        | OpenAI-compatible |
| Ollama    | Implemented        | None (local)   | localhost:11434       | User-configured   | Model-dependent | JSON Lines | Limited           |
| OpenAI    | API Reference Only | Bearer Token   | api.openai.com        | gpt-4o            | 128K            | SSE        | Native            |
| Anthropic | API Reference Only | API Key Header | api.anthropic.com     | claude-sonnet-4-0 | 200K            | SSE        | Content blocks    |

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

GitHub Copilot

```bash
# Preferred: run the CLI device OAuth flow (xzatoma auth --provider copilot)
export GITHUB_TOKEN="ghp_..."      # Optional for some flows
export COPILOT_API_KEY="..."      # Alternative (if supported)
```

Ollama (local)

```bash
export OLLAMA_HOST="http://localhost:11434"
export OLLAMA_MODEL="llama3.2:latest"
```

OpenAI (not implemented -- shown for API comparison only)

```bash
export OPENAI_API_KEY="sk-..."     # Required
export OPENAI_HOST="https://api.openai.com" # Optional
export OPENAI_TIMEOUT="600"       # Optional (seconds)
```

Anthropic (not implemented -- shown for API comparison only)

```bash
export ANTHROPIC_API_KEY="sk-ant-..."  # Required
export ANTHROPIC_HOST="https://api.anthropic.com" # Optional
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
- Anthropic: tool usage is embedded in content array with special
  `tool_use`/`tool_result` blocks.
- Ollama: format can be provider-specific (JSON lines, model-dependent).

### Tool Results

- OpenAI/Copilot: tool results often appear as a `role: "tool"` message with an
  identifier.
- Anthropic: tool results can be `tool_result` entries inside a user/content
  array.

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

```text
data: {"choices":[{"delta":{"content":"Hello"}}]}

data: {"choices":[{"delta":{"content":" world"}}]}

data: [DONE]
```

JSON Lines (Ollama)

```text
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

- Anthropic expects separate `system` field (don't send system as first message
  blindly).
- Tool result formats differ (OpenAI vs Anthropic) — write provider adapters
  that normalize tool results early.
- Streaming JSON may split arguments or message pieces across chunks; implement
  chunk reassembly.
- Token accounting differences: providers vary in token reporting; consider
  consistent internal accounting.

---

## Security Reminders

- Never log API keys or secrets.
- Use HTTPS for remote providers (OpenAI, Anthropic).
- Validate and honor `retry-after` and rate limits to avoid throttling.
- Store secrets in environment variables or secure keyring backends, not source
  control.
- Implement timeouts to avoid hanging requests.

---

## File Layout

```text
providers/
├── base.rs         # Provider trait and base types
├── copilot.rs       # GitHub Copilot (OAuth) implementation
├── ollama.rs        # Ollama local/remote provider
└── mod.rs          # Module root and re-exports
```

---

## References

- Provider implementation plan (archived):
  `../archive/implementation_summaries/provider_abstraction_implementation_plan.md`
- Refactor audit & details:
  `../archive/implementation_summaries/implementation_plan_refactoring_summary.md`
- OpenAI API: <https://platform.openai.com/docs/api-reference>
- Anthropic API: <https://docs.anthropic.com/en/api>
- Ollama API: <https://github.com/ollama/ollama/blob/main/docs/api.md>

---
