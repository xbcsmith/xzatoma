# Provider API Comparison Reference

## Overview

This document provides a detailed comparison of API specifications for OpenAI, Anthropic, GitHub Copilot, and Ollama. Use this as a reference when implementing provider-specific request/response handling.

## Quick Comparison Matrix

| Feature          | OpenAI        | Anthropic           | GitHub Copilot    | Ollama      |
| ------------------------- | -------------------- | ----------------------------- | --------------------- | ----------------- |
| **Authentication**    | Bearer Token     | API Key Header        | OAuth Device Flow   | None       |
| **Base URL**       | api.openai.com    | api.anthropic.com       | api.githubcopilot.com | localhost:11434  |
| **API Path**       | /v1/chat/completions | /v1/messages         | /chat/completions   | /api/chat     |
| **System Prompt**     | First message    | Separate field        | First message     | First message   |
| **Tool Call Location**  | Separate field    | Content array         | Separate field    | Separate field  |
| **Tool Result Format**  | Tool role message  | User message with tool_result | Tool role message   | Tool role message |
| **Streaming Format**   | SSE         | SSE              | SSE          | JSON Lines    |
| **Streaming Done Marker** | `[DONE]`       | `event: message_stop`     | `[DONE]`       | `"done": true`  |
| **Usage Tokens**     | All three      | Input + Output only      | All three       | Token count only |
| **Max Context**      | 128K-200K      | 200K             | 128K         | Model-dependent  |
| **Image Support**     | Yes (base64/URL)   | Yes (base64)         | Yes          | Limited      |

## Authentication

### OpenAI

**Method**: Bearer token in Authorization header

**Request Headers**:

```http
Authorization: Bearer sk-proj-...
Content-Type: application/json
```

**Environment Variable**: `OPENAI_API_KEY`

**Optional Headers**:

- `OpenAI-Organization`: Organization ID
- `OpenAI-Project`: Project ID

### Anthropic

**Method**: API key in custom header

**Request Headers**:

```http
x-api-key: sk-ant-api03-...
anthropic-version: 2023-06-01
Content-Type: application/json
```

**Environment Variable**: `ANTHROPIC_API_KEY`

**Required Header**: `anthropic-version` must be included

### GitHub Copilot

**Method**: OAuth 2.0 Device Flow

**OAuth Flow**:

1. POST to `https://github.com/login/device/code` with client_id
2. User visits verification_uri and enters user_code
3. Poll `https://github.com/login/oauth/access_token` until authorized
4. Use access token as Bearer token

**Request Headers**:

```http
Authorization: Bearer ghu_...
Content-Type: application/json
```

**Environment Variable**: `GITHUB_TOKEN` (after OAuth flow)

### Ollama

**Method**: No authentication

**Request Headers**:

```http
Content-Type: application/json
```

**Notes**: Local deployment, no auth required

## Request Format

### OpenAI Request

```json
{
 "model": "gpt-4o",
 "messages": [
  {
   "role": "system",
   "content": "You are a helpful assistant."
  },
  {
   "role": "user",
   "content": "Hello!"
  },
  {
   "role": "assistant",
   "content": "Hi there!"
  },
  {
   "role": "user",
   "content": "What's the weather?"
  }
 ],
 "tools": [
  {
   "type": "function",
   "function": {
    "name": "get_weather",
    "description": "Get current weather for a location",
    "parameters": {
     "type": "object",
     "properties": {
      "location": {
       "type": "string",
       "description": "City name"
      }
     },
     "required": ["location"]
    }
   }
  }
 ],
 "temperature": 0.7,
 "max_tokens": 4096,
 "stream": false
}
```

**Key Points**:

- System prompt is a message with `role: "system"`
- Tools use `function` type wrapper
- Tool results use `role: "tool"` with `tool_call_id`

### Anthropic Request

```json
{
 "model": "claude-sonnet-4-0",
 "system": "You are a helpful assistant.",
 "messages": [
  {
   "role": "user",
   "content": "Hello!"
  },
  {
   "role": "assistant",
   "content": "Hi there!"
  },
  {
   "role": "user",
   "content": "What's the weather?"
  }
 ],
 "tools": [
  {
   "name": "get_weather",
   "description": "Get current weather for a location",
   "input_schema": {
    "type": "object",
    "properties": {
     "location": {
      "type": "string",
      "description": "City name"
     }
    },
    "required": ["location"]
   }
  }
 ],
 "temperature": 0.7,
 "max_tokens": 4096
}
```

**Key Points**:

- System prompt is separate `system` field (NOT a message)
- Messages array must start with `user` role
- Tools use `input_schema` instead of `parameters`
- No `stream` field for non-streaming (omit it)

### GitHub Copilot Request

```json
{
 "model": "gpt-5-mini",
 "messages": [
  {
   "role": "system",
   "content": "You are a helpful assistant."
  },
  {
   "role": "user",
   "content": "Hello!"
  }
 ],
 "tools": [
  {
   "type": "function",
   "function": {
    "name": "get_weather",
    "description": "Get current weather",
    "parameters": {
     "type": "object",
     "properties": {
      "location": { "type": "string" }
     },
     "required": ["location"]
    }
   }
  }
 ],
 "temperature": 0.7,
 "max_tokens": 4096,
 "stream": false
}
```

**Key Points**:

- Identical to OpenAI format
- Can reuse OpenAI request formatter

### Ollama Request

```json
{
 "model": "qwen3",
 "messages": [
  {
   "role": "system",
   "content": "You are a helpful assistant."
  },
  {
   "role": "user",
   "content": "Hello!"
  }
 ],
 "tools": [
  {
   "type": "function",
   "function": {
    "name": "get_weather",
    "description": "Get current weather",
    "parameters": {
     "type": "object",
     "properties": {
      "location": { "type": "string" }
     },
     "required": ["location"]
    }
   }
  }
 ],
 "stream": false
}
```

**Key Points**:

- Similar to OpenAI format
- Model must be pulled locally first (`ollama pull qwen3`)
- Tool support depends on model version
- Temperature and max_tokens are optional

## Response Format

### OpenAI Response

```json
{
 "id": "chatcmpl-abc123",
 "object": "chat.completion",
 "created": 1677858242,
 "model": "gpt-4o-2024-08-06",
 "choices": [
  {
   "index": 0,
   "message": {
    "role": "assistant",
    "content": "The weather is sunny!",
    "tool_calls": null
   },
   "finish_reason": "stop"
  }
 ],
 "usage": {
  "prompt_tokens": 56,
  "completion_tokens": 31,
  "total_tokens": 87
 }
}
```

**With Tool Calls**:

```json
{
 "choices": [
  {
   "message": {
    "role": "assistant",
    "content": null,
    "tool_calls": [
     {
      "id": "call_abc123",
      "type": "function",
      "function": {
       "name": "get_weather",
       "arguments": "{\"location\":\"San Francisco\"}"
      }
     }
    ]
   },
   "finish_reason": "tool_calls"
  }
 ],
 "usage": {
  "prompt_tokens": 82,
  "completion_tokens": 18,
  "total_tokens": 100
 }
}
```

**Key Points**:

- Tool calls are in `message.tool_calls` array
- `finish_reason` is `"tool_calls"` when tools are used
- `arguments` is JSON string, must parse

### Anthropic Response

```json
{
 "id": "msg_01XFDUDYJgAACzvnptvVoYEL",
 "type": "message",
 "role": "assistant",
 "content": [
  {
   "type": "text",
   "text": "The weather is sunny!"
  }
 ],
 "model": "claude-sonnet-4-0",
 "stop_reason": "end_turn",
 "usage": {
  "input_tokens": 56,
  "output_tokens": 31
 }
}
```

**With Tool Calls**:

```json
{
 "id": "msg_01XFDUDYJgAACzvnptvVoYEL",
 "type": "message",
 "role": "assistant",
 "content": [
  {
   "type": "text",
   "text": "I'll check the weather for you."
  },
  {
   "type": "tool_use",
   "id": "toolu_01A09q90qw90lq917835lq9",
   "name": "get_weather",
   "input": {
    "location": "San Francisco"
   }
  }
 ],
 "stop_reason": "tool_use",
 "usage": {
  "input_tokens": 82,
  "output_tokens": 18
 }
}
```

**Key Points**:

- Content is always an array of blocks
- Tool calls are `tool_use` blocks in content array
- `input` is JSON object, not string
- No `total_tokens` field (calculate as input + output)
- `stop_reason` is `"tool_use"` when tools are used

### GitHub Copilot Response

Identical to OpenAI response format.

### Ollama Response

```json
{
 "model": "qwen3",
 "created_at": "2024-01-01T12:00:00Z",
 "message": {
  "role": "assistant",
  "content": "The weather is sunny!"
 },
 "done": true,
 "total_duration": 5000000000,
 "load_duration": 1000000000,
 "prompt_eval_count": 56,
 "prompt_eval_duration": 2000000000,
 "eval_count": 31,
 "eval_duration": 2000000000
}
```

**Key Points**:

- Message structure similar to OpenAI
- Token counts use `prompt_eval_count` and `eval_count`
- No `total_tokens` field
- Durations in nanoseconds

## Tool Result Format

### OpenAI Tool Result

**As next message in conversation**:

```json
{
 "role": "tool",
 "tool_call_id": "call_abc123",
 "content": "Temperature: 72°F, Sunny"
}
```

### Anthropic Tool Result

**As user message with tool_result content**:

```json
{
 "role": "user",
 "content": [
  {
   "type": "tool_result",
   "tool_use_id": "toolu_01A09q90qw90lq917835lq9",
   "content": "Temperature: 72°F, Sunny"
  }
 ]
}
```

**With error**:

```json
{
 "role": "user",
 "content": [
  {
   "type": "tool_result",
   "tool_use_id": "toolu_01A09q90qw90lq917835lq9",
   "content": "Error: Location not found",
   "is_error": true
  }
 ]
}
```

### GitHub Copilot Tool Result

Same as OpenAI (use `role: "tool"`).

### Ollama Tool Result

Same as OpenAI (use `role: "tool"`).

## Streaming Format

### OpenAI Streaming

**Request**: Set `"stream": true`

**Response**: Server-Sent Events (SSE)

```
data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1694268190,"model":"gpt-4o","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1694268190,"model":"gpt-4o","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1694268190,"model":"gpt-4o","choices":[{"index":0,"delta":{"content":"!"},"finish_reason":null}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1694268190,"model":"gpt-4o","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]
```

**Tool Call Streaming** (JSON may be split across chunks):

```
data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_abc","type":"function","function":{"name":"get_weather","arguments":""}}]}}]}

data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\\"loc"}}]}}]}

data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"ation\\""}}]}}]}

data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":":"San"}}]}}]}

data: [DONE]
```

### Anthropic Streaming

**Request**: Set `"stream": true`

**Response**: Server-Sent Events with event types

```
event: message_start
data: {"type":"message_start","message":{"id":"msg_123","type":"message","role":"assistant","content":[],"model":"claude-sonnet-4-0","usage":{"input_tokens":25,"output_tokens":1}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"!"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":10}}

event: message_stop
data: {"type":"message_stop"}
```

**Key Points**:

- Events have types: `message_start`, `content_block_start`, `content_block_delta`, etc.
- Must track content blocks by index
- Final usage in `message_delta` event

### GitHub Copilot Streaming

Same as OpenAI streaming (SSE with `data:` prefix, `[DONE]` marker).

### Ollama Streaming

**Request**: Set `"stream": true`

**Response**: Newline-delimited JSON (not SSE)

```
{"model":"qwen3","created_at":"2024-01-01T12:00:00Z","message":{"role":"assistant","content":"Hello"},"done":false}
{"model":"qwen3","created_at":"2024-01-01T12:00:00Z","message":{"role":"assistant","content":"!"},"done":false}
{"model":"qwen3","created_at":"2024-01-01T12:00:00Z","message":{"role":"assistant","content":""},"done":true,"total_duration":5000000000,"prompt_eval_count":25,"eval_count":10}
```

**Key Points**:

- Each line is complete JSON object
- No `data:` prefix (unlike SSE)
- `done: true` in final chunk
- Usage stats only in final chunk

## Error Responses

### OpenAI Errors

```json
{
 "error": {
  "message": "Incorrect API key provided",
  "type": "invalid_request_error",
  "param": null,
  "code": "invalid_api_key"
 }
}
```

**Status Codes**:

- 401: Invalid API key
- 429: Rate limit exceeded
- 400: Invalid request (context too long, etc.)
- 500: Server error

### Anthropic Errors

```json
{
 "type": "error",
 "error": {
  "type": "invalid_request_error",
  "message": "Your credit balance is too low to access the Claude API"
 }
}
```

**Status Codes**:

- 401: Authentication error
- 429: Rate limit exceeded
- 400: Invalid request
- 500: Internal error

### GitHub Copilot Errors

Similar to OpenAI error format.

### Ollama Errors

```json
{
 "error": "model 'qwen3' not found, try pulling it first"
}
```

**Common Errors**:

- Model not found (need to `ollama pull`)
- Connection refused (Ollama not running)
- Invalid JSON format

## Image Support

### OpenAI Images

**In message content** (array format):

```json
{
 "role": "user",
 "content": [
  {
   "type": "text",
   "text": "What's in this image?"
  },
  {
   "type": "image_url",
   "image_url": {
    "url": "https://example.com/image.jpg"
   }
  }
 ]
}
```

**Or base64**:

```json
{
 "type": "image_url",
 "image_url": {
  "url": "data:image/jpeg;base64,/9j/4AAQSkZJRg..."
 }
}
```

### Anthropic Images

**In message content**:

```json
{
 "role": "user",
 "content": [
  {
   "type": "text",
   "text": "What's in this image?"
  },
  {
   "type": "image",
   "source": {
    "type": "base64",
    "media_type": "image/jpeg",
    "data": "/9j/4AAQSkZJRg..."
   }
  }
 ]
}
```

**Key Differences**:

- Anthropic uses `type: "image"` (not `image_url`)
- Base64 only (no URL support)
- Must specify `media_type`

### GitHub Copilot Images

Same as OpenAI (supports both URL and base64).

### Ollama Images

Limited support, depends on model (e.g., llava models).

## Rate Limits

| Provider | Rate Limit        | Header                  |
| --------- | ------------------------ | ---------------------------------------- |
| OpenAI  | Varies by tier      | `x-ratelimit-remaining-requests`     |
| Anthropic | Varies by tier      | `anthropic-ratelimit-requests-remaining` |
| Copilot  | Included in subscription | N/A                   |
| Ollama  | No limits (local)    | N/A                   |

**Retry-After**: Both OpenAI and Anthropic return `retry-after` header on 429 errors.

## Implementation Recommendations

1. **Request Formatting**: Create separate formatters for OpenAI-compatible (OpenAI/Copilot/Ollama) and Anthropic
2. **System Prompt**: Abstract system prompt handling (message vs field)
3. **Tool Calls**: Create unified tool call structure, convert to/from provider format
4. **Streaming**: Abstract SSE vs JSON Lines parsing
5. **Error Mapping**: Map provider-specific errors to common error types
6. **Token Counting**: Handle missing `total_tokens` field (Anthropic, Ollama)

## References

- OpenAI API Docs: https://platform.openai.com/docs/api-reference/chat
- Anthropic API Docs: https://docs.anthropic.com/en/api/messages
- GitHub Copilot: https://docs.github.com/en/copilot
- Ollama API: https://github.com/ollama/ollama/blob/main/docs/api.md
