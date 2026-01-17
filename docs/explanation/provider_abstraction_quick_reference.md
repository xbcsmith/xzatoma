# Provider Abstraction - Quick Reference

## Overview

This is a quick reference for implementing a provider abstraction layer that supports OpenAI, Anthropic, GitHub Copilot, and Ollama. See `provider_abstraction_implementation_plan.md` for full details.

## Provider Comparison

| Provider  | Auth Method    | Base URL              | Default Model     | Context Limit   | Streaming  | Tool Calls        |
| --------- | -------------- | --------------------- | ----------------- | --------------- | ---------- | ----------------- |
| OpenAI    | Bearer Token   | api.openai.com        | gpt-4o            | 128K            | SSE        | Native            |
| Anthropic | API Key Header | api.anthropic.com     | claude-sonnet-4-0 | 200K            | SSE        | Tool Use Blocks   |
| Copilot   | OAuth Device   | api.githubcopilot.com | gpt-5-mini        | 128K            | SSE        | OpenAI-compatible |
| Ollama    | None           | localhost:11434       | User-configured   | Model-dependent | JSON Lines | Limited           |

## Core Interface (Pseudo-code)

```
interface Provider {
    async complete(messages: Message[], tools: Tool[]) -> Result<Response>
    async stream(messages: Message[], tools: Tool[]) -> Stream<ResponseChunk>
    get_model_config() -> ModelConfig
    get_name() -> string
    supports_streaming() -> bool
}

struct Response {
    content: string
    tool_calls: ToolCall[]
    usage: Usage
}

struct Message {
    role: "system" | "user" | "assistant" | "tool"
    content: string | ContentBlock[]
}

struct Tool {
    name: string
    description: string
    parameters: JsonSchema
}
```

## Environment Variables

### OpenAI

```bash
OPENAI_API_KEY=sk-...          # Required
OPENAI_HOST=...                # Optional, default: https://api.openai.com
OPENAI_TIMEOUT=600             # Optional, seconds
```

### Anthropic

```bash
ANTHROPIC_API_KEY=sk-ant-...   # Required
ANTHROPIC_HOST=...             # Optional, default: https://api.anthropic.com
```

### GitHub Copilot

```bash
GITHUB_TOKEN=ghp_...           # Optional, for OAuth
COPILOT_API_KEY=...            # Alternative auth
```

### Ollama

```bash
OLLAMA_HOST=http://localhost:11434  # Optional
OLLAMA_MODEL=qwen3                  # Required
```

## Key Differences Between Providers

### System Prompt Handling

**OpenAI/Copilot/Ollama**: System prompt as first message

```json
{ "role": "system", "content": "You are a helpful assistant" }
```

**Anthropic**: System prompt as separate field

```json
{
  "system": "You are a helpful assistant",
  "messages": [...]
}
```

### Tool Call Format

**OpenAI/Copilot**: Tool calls in separate field

```json
{
  "message": {
    "tool_calls": [
      {
        "id": "call_123",
        "type": "function",
        "function": {
          "name": "tool_name",
          "arguments": "{...}"
        }
      }
    ]
  }
}
```

**Anthropic**: Tool calls in content array

```json
{
  "content": [
    {"type": "text", "text": "..."},
    {
      "type": "tool_use",
      "id": "toolu_123",
      "name": "tool_name",
      "input": {...}
    }
  ]
}
```

### Tool Results

**OpenAI/Copilot**: Tool role message

```json
{ "role": "tool", "tool_call_id": "call_123", "content": "result" }
```

**Anthropic**: User message with tool_result

```json
{
  "role": "user",
  "content": [
    {
      "type": "tool_result",
      "tool_use_id": "toolu_123",
      "content": "result"
    }
  ]
}
```

## Error Types

```
AuthenticationError      -> 401 Unauthorized
RateLimitError          -> 429 Too Many Requests (includes retry_after)
ContextLengthError      -> 400 Bad Request (context too long)
ServerError             -> 500, 502, 503, 504
RequestError            -> 400, 404 (invalid request)
NetworkError            -> Connection timeout, DNS failure
NotImplementedError     -> Feature not supported
```

## Streaming

### SSE Format (OpenAI/Anthropic/Copilot)

```
data: {"choices":[{"delta":{"content":"Hello"}}]}

data: {"choices":[{"delta":{"content":" world"}}]}

data: [DONE]
```

### JSON Lines Format (Ollama)

```
{"message":{"content":"Hello"}}
{"message":{"content":" world"}}
{"done":true}
```

## Retry Configuration

```
max_retries: 3
initial_delay_ms: 1000
max_delay_ms: 60000
backoff_multiplier: 2.0
retry_on_status: [429, 500, 502, 503, 504]
```

## Implementation Phases

1. **Phase 1**: Core abstractions (Provider interface, types, errors) - 700 LOC
2. **Phase 2**: HTTP client & retry logic - 1,250 LOC
3. **Phase 3**: Provider implementations (4 providers) - 2,100 LOC
4. **Phase 4**: Streaming support - 950 LOC
5. **Phase 5**: Factory & registry - 700 LOC
6. **Phase 6**: Usage tracking & advanced features - 650 LOC

**Total**: ~6,350 LOC + ~2,200 LOC tests = ~8,550 LOC

## Minimal Example (Language-agnostic)

```
# Create provider from config
config = ProviderConfig {
    provider_type: "openai",
    model: "gpt-4o",
    api_key: env("OPENAI_API_KEY")
}
provider = ProviderFactory.create(config)

# Make completion request
messages = [
    Message { role: "user", content: "Hello!" }
]
tools = [
    Tool {
        name: "get_weather",
        description: "Get weather for location",
        parameters: { type: "object", properties: {...} }
    }
]

response = await provider.complete(messages, tools)
print(response.content)

# Check for tool calls
if response.tool_calls:
    for tool_call in response.tool_calls:
        result = execute_tool(tool_call.name, tool_call.arguments)
        # Add tool result to messages and continue conversation
```

## Testing Checklist

- [ ] Request formatting for each provider
- [ ] Response parsing for each provider
- [ ] Tool call extraction
- [ ] Error mapping for all HTTP status codes
- [ ] Retry logic (success after N retries)
- [ ] Streaming chunk accumulation
- [ ] Authentication header construction
- [ ] Configuration loading from env/file
- [ ] Mock HTTP server integration tests
- [ ] Test coverage >80%

## Common Pitfalls

1. **System Prompt**: Anthropic requires separate `system` field, not a message
2. **Tool Results**: Format differs between OpenAI (tool message) and Anthropic (user message with tool_result)
3. **Streaming JSON**: Tool call arguments may be split across chunks - must accumulate
4. **Token Counting**: Anthropic doesn't return `total_tokens` - calculate as input + output
5. **OAuth Flow**: Copilot requires device flow - can't use simple API key
6. **Local Ollama**: Check if service is running before making requests
7. **Model Names**: Verify model exists/is pulled for Ollama before use

## Security Reminders

- Never log API keys
- Use HTTPS for all providers (except localhost Ollama)
- Validate token limits before sending requests
- Respect rate limit retry-after headers
- Store API keys in environment variables or secure keychain
- Implement timeouts to prevent hangs

## File Structure

```
providers/
├── base.rs/py/go              # Provider interface
├── types.rs/py/go             # Message, Tool, Response
├── errors.rs/py/go            # Error types
├── config.rs/py/go            # Configuration
├── api_client.rs/py/go        # HTTP client
├── retry.rs/py/go             # Retry logic
├── streaming.rs/py/go         # Streaming types
├── factory.rs/py/go           # Provider factory
├── registry.rs/py/go          # Provider registry
├── openai.rs/py/go            # OpenAI implementation
├── anthropic.rs/py/go         # Anthropic implementation
├── copilot.rs/py/go           # Copilot implementation
├── ollama.rs/py/go            # Ollama implementation
├── oauth.rs/py/go             # OAuth utilities
├── formats/
│   ├── openai.rs/py/go        # OpenAI request/response format
│   └── anthropic.rs/py/go     # Anthropic request/response format
└── tests/
    └── ...
```

## References

- Full plan: `provider_abstraction_implementation_plan.md`
- Architecture: `../reference/architecture.md`
- OpenAI API: https://platform.openai.com/docs/api-reference
- Anthropic API: https://docs.anthropic.com/en/api
- Ollama API: https://github.com/ollama/ollama/blob/main/docs/api.md
