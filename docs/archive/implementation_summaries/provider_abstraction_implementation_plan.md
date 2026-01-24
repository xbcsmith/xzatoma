# Provider Abstraction Layer - Language-Agnostic Implementation Plan

## Overview

This document provides a language-agnostic implementation plan for building a provider abstraction layer that supports multiple AI providers (OpenAI, Anthropic, GitHub Copilot, and Ollama). The plan is designed to work with Rust, Python, Go, or other languages, following common patterns observed in production implementations.

## Current State Analysis

### Target Providers

1. **OpenAI** - Industry standard, REST API, streaming support, function calling
2. **Anthropic** - Claude models, REST API, streaming support, tool use
3. **GitHub Copilot** - Developer-focused, OAuth authentication, OpenAI-compatible
4. **Ollama** - Local deployment, OpenAI-compatible API, no authentication

### Common Provider Patterns

- REST API communication over HTTPS
- JSON request/response format
- Streaming via Server-Sent Events (SSE)
- Tool/function calling support
- Token-based authentication (except Ollama)
- Rate limiting and retry logic
- Usage tracking (input/output tokens)

### Identified Requirements

- Unified interface across all providers
- Authentication abstraction (API keys, OAuth, none)
- Streaming response support
- Tool call handling
- Error handling with provider-specific mapping
- Configuration management
- Retry logic with exponential backoff
- Usage/cost tracking

## Implementation Phases

### Phase 1: Core Abstractions (Foundation)

Define the base types and interfaces that all providers must implement.

#### Task 1.1: Define Provider Interface

Create the core provider trait/interface with minimal required methods:

**Interface Definition** (language-agnostic pseudo-code):

```
interface Provider {
  // Required methods
  async complete(messages: Message[], tools: Tool[]) -> Result<Response>
  get_model_config() -> ModelConfig
  get_name() -> string

  // Optional methods with default implementations
  async complete_fast(messages: Message[], tools: Tool[]) -> Result<Response>
  supports_streaming() -> bool
  supports_embeddings() -> bool
}
```

**Key Structures**:

- `Message`: role (system/user/assistant/tool), content (text/image), metadata
- `Tool`: name, description, parameters (JSON schema)
- `Response`: message content, tool calls, usage statistics
- `ModelConfig`: model name, context limit, temperature, max tokens

**Files to Create**:

- `providers/base` - Core provider interface
- `providers/types` - Shared types (Message, Tool, Response, ModelConfig)

#### Task 1.2: Define Error Types

Create provider-specific error types that map to common categories:

**Error Categories**:

- `AuthenticationError` - Invalid credentials, expired tokens
- `RateLimitError` - Rate limit exceeded, includes retry-after duration
- `ContextLengthError` - Input too long for model
- `ServerError` - Provider service issues (5xx errors)
- `RequestError` - Invalid request format (4xx errors)
- `NetworkError` - Connection issues, timeouts
- `NotImplementedError` - Feature not supported by provider

**Files to Create**:

- `providers/errors` - Error types and conversion utilities

#### Task 1.3: Define Configuration Structure

Create configuration structure for provider settings:

**Configuration Fields**:

```
struct ProviderConfig {
  provider_type: string // "openai", "anthropic", "copilot", "ollama"
  model: string
  fast_model: string (optional)
  api_key: string (optional, from env/config)
  base_url: string (optional, custom endpoints)
  timeout_seconds: int (default: 600)
  max_retries: int (default: 3)
  custom_headers: map<string, string> (optional)
}
```

**Files to Create**:

- `providers/config` - Configuration types and loading

#### Task 1.4: Testing Requirements

- Unit tests for Message/Tool/Response serialization
- Error type conversion tests
- Configuration loading tests (env vars, config files)

#### Task 1.5: Deliverables

- Provider interface definition (~50 lines)
- Type definitions (~200 lines)
- Error types (~100 lines)
- Configuration types (~150 lines)
- Unit tests (~200 lines)

Total: ~700 lines

#### Task 1.6: Success Criteria

- [ ] Provider interface compiles/validates
- [ ] All core types serialize/deserialize correctly
- [ ] Error types cover all HTTP status codes
- [ ] Configuration loads from environment and files
- [ ] Test coverage >80%

### Phase 2: HTTP Client Abstraction

Create a reusable HTTP client for provider API calls.

#### Task 2.1: Implement API Client

Build HTTP client with authentication and header management:

**API Client Structure**:

```
struct ApiClient {
  base_url: string
  auth_method: AuthMethod
  headers: map<string, string>
  timeout: duration
  http_client: HttpClient
}

enum AuthMethod {
  BearerToken(string)
  ApiKey { header_name: string, key: string }
  OAuth { token: string, refresh_token: string }
  None
}

impl ApiClient {
  async request(method: HttpMethod, path: string, body: json) -> Result<ApiResponse>
  async post(path: string, body: json) -> Result<ApiResponse>
  async get(path: string) -> Result<ApiResponse>
  with_header(key: string, value: string) -> Self
}
```

**Features**:

- Connection pooling
- Timeout configuration
- Custom headers
- Request/response logging (debug mode)

**Files to Create**:

- `providers/api_client` - HTTP client implementation

#### Task 2.2: Implement Retry Logic

Add retry mechanism with exponential backoff:

**Retry Configuration**:

```
struct RetryConfig {
  max_retries: int (default: 3)
  initial_delay_ms: int (default: 1000)
  max_delay_ms: int (default: 60000)
  backoff_multiplier: float (default: 2.0)
  retry_on_status: set<int> (default: [429, 500, 502, 503, 504])
}

interface Retryable {
  async execute_with_retry<T>(operation: async () -> Result<T>) -> Result<T>
}
```

**Files to Create**:

- `providers/retry` - Retry logic with exponential backoff

#### Task 2.3: Implement Request Formatting

Create utilities for formatting provider requests:

**Format Utilities**:

- Message list to provider-specific JSON
- Tool definitions to provider schema format
- Image content encoding (base64, URLs)
- System prompt handling (varies by provider)

**Files to Create**:

- `providers/formats/openai` - OpenAI request/response format
- `providers/formats/anthropic` - Anthropic request/response format

#### Task 2.4: Testing Requirements

- Mock HTTP server for testing API client
- Retry logic tests (success after N retries, max retries exceeded)
- Request formatting tests for each provider format
- Authentication header tests

#### Task 2.5: Deliverables

- API client implementation (~300 lines)
- Retry logic (~150 lines)
- Request formatters (~400 lines total)
- Unit tests (~400 lines)

Total: ~1,250 lines

#### Task 2.6: Success Criteria

- [ ] API client handles all auth methods
- [ ] Retry logic respects backoff configuration
- [ ] Request formatters produce valid provider JSON
- [ ] Mock tests verify HTTP behavior
- [ ] Test coverage >80%

### Phase 3: Provider Implementations

Implement each provider using the abstractions.

#### Task 3.1: Implement OpenAI Provider

**OpenAI Specifics**:

- Base URL: `https://api.openai.com/v1/chat/completions`
- Authentication: Bearer token (`Authorization: Bearer <key>`)
- Default model: `gpt-4o`
- Fast model: `gpt-4o-mini`
- Context limits: 128K tokens (most models)
- Streaming: SSE format
- Tool calls: Native function calling support

**Environment Variables**:

- `OPENAI_API_KEY` (required)
- `OPENAI_HOST` (optional, default: `https://api.openai.com`)
- `OPENAI_BASE_PATH` (optional, default: `v1/chat/completions`)
- `OPENAI_ORGANIZATION` (optional)
- `OPENAI_PROJECT` (optional)
- `OPENAI_TIMEOUT` (optional, default: 600)

**Request Format**:

```json
{
 "model": "gpt-4o",
 "messages": [
  { "role": "system", "content": "..." },
  { "role": "user", "content": "..." }
 ],
 "tools": [
  {
   "type": "function",
   "function": {
    "name": "tool_name",
    "description": "...",
    "parameters": {
     /* JSON schema */
    }
   }
  }
 ],
 "temperature": 0.7,
 "max_tokens": 4096,
 "stream": false
}
```

**Response Format**:

```json
{
 "id": "chatcmpl-...",
 "object": "chat.completion",
 "model": "gpt-4o",
 "choices": [
  {
   "message": {
    "role": "assistant",
    "content": "...",
    "tool_calls": [
     /* if tools used */
    ]
   },
   "finish_reason": "stop"
  }
 ],
 "usage": {
  "prompt_tokens": 100,
  "completion_tokens": 50,
  "total_tokens": 150
 }
}
```

**Files to Create**:

- `providers/openai` - OpenAI provider implementation

#### Task 3.2: Implement Anthropic Provider

**Anthropic Specifics**:

- Base URL: `https://api.anthropic.com/v1/messages`
- Authentication: API key header (`x-api-key: <key>`)
- API Version header: `anthropic-version: 2023-06-01`
- Default model: `claude-sonnet-4-0`
- Fast model: `claude-3-7-sonnet-latest`
- Context limits: 200K tokens
- Streaming: SSE format
- Tool calls: Tool use blocks in response

**Environment Variables**:

- `ANTHROPIC_API_KEY` (required)
- `ANTHROPIC_HOST` (optional, default: `https://api.anthropic.com`)
- `ANTHROPIC_TIMEOUT` (optional, default: 600)

**Request Format** (note: system prompt is separate):

```json
{
 "model": "claude-sonnet-4-0",
 "system": "System prompt here",
 "messages": [
  { "role": "user", "content": "..." },
  { "role": "assistant", "content": "..." }
 ],
 "tools": [
  {
   "name": "tool_name",
   "description": "...",
   "input_schema": {
    /* JSON schema */
   }
  }
 ],
 "temperature": 0.7,
 "max_tokens": 4096
}
```

**Response Format**:

```json
{
 "id": "msg_...",
 "type": "message",
 "role": "assistant",
 "content": [
  {
   "type": "text",
   "text": "..."
  },
  {
   "type": "tool_use",
   "id": "toolu_...",
   "name": "tool_name",
   "input": {
    /* tool arguments */
   }
  }
 ],
 "model": "claude-sonnet-4-0",
 "usage": {
  "input_tokens": 100,
  "output_tokens": 50
 }
}
```

**Key Differences from OpenAI**:

- System prompt is separate field, not a message
- Tool calls are in content array, not separate field
- Tool results sent as user messages with `tool_result` type
- No `total_tokens` in usage (calculate as sum)

**Files to Create**:

- `providers/anthropic` - Anthropic provider implementation

#### Task 3.3: Implement GitHub Copilot Provider

**Copilot Specifics**:

- Base URL: `https://api.githubcopilot.com/chat/completions`
- Authentication: OAuth device flow (complex)
- Uses OpenAI-compatible format
- Default model: `gpt-5-mini`
- Context limits: 128K tokens
- Streaming: SSE format
- Tool calls: OpenAI-compatible

**Environment Variables**:

- `GITHUB_TOKEN` (optional, for OAuth)
- `COPILOT_API_KEY` (alternative auth method)

**OAuth Device Flow** (required for Copilot):

1. Request device code from GitHub
2. User visits URL and enters code
3. Poll for access token
4. Store token for future use

**Note**: OpenAI-compatible format, can reuse OpenAI formatters.

**Files to Create**:

- `providers/copilot` - Copilot provider with OAuth
- `providers/oauth` - OAuth device flow utilities

#### Task 3.4: Implement Ollama Provider

**Ollama Specifics**:

- Base URL: `http://localhost:11434/api/chat` (default)
- Authentication: None (local deployment)
- Uses OpenAI-compatible format (with variations)
- Default model: User-configured (e.g., `qwen3`, `llama3`)
- Context limits: Model-dependent
- Streaming: JSON lines format
- Tool calls: Supported in recent versions

**Environment Variables**:

- `OLLAMA_HOST` (optional, default: `http://localhost:11434`)
- `OLLAMA_MODEL` (required, no default)

**Request Format** (OpenAI-compatible):

```json
{
 "model": "qwen3",
 "messages": [
  { "role": "system", "content": "..." },
  { "role": "user", "content": "..." }
 ],
 "tools": [
  /* OpenAI format */
 ],
 "stream": false
}
```

**Key Differences**:

- No authentication required
- Model must exist locally (pulled via `ollama pull`)
- May have limited tool calling support (model-dependent)
- Errors are simpler (connection refused if not running)

**Files to Create**:

- `providers/ollama` - Ollama provider implementation

#### Task 3.5: Testing Requirements

- Unit tests for each provider's request formatting
- Mock HTTP tests for each provider's API calls
- Error handling tests (rate limits, auth failures, etc.)
- Tool call parsing tests
- Usage statistics extraction tests
- Integration tests with test provider/mock responses

#### Task 3.6: Deliverables

- OpenAI provider (~300 lines)
- Anthropic provider (~350 lines)
- Copilot provider (~400 lines, includes OAuth)
- Ollama provider (~250 lines)
- OAuth utilities (~200 lines)
- Unit tests (~600 lines)

Total: ~2,100 lines

#### Task 3.7: Success Criteria

- [ ] All four providers implement Provider interface
- [ ] OpenAI provider handles standard and custom endpoints
- [ ] Anthropic provider correctly formats system prompts
- [ ] Copilot provider completes OAuth device flow
- [ ] Ollama provider works with local installations
- [ ] All providers parse tool calls correctly
- [ ] Error mapping works for all HTTP status codes
- [ ] Test coverage >80%

### Phase 4: Streaming Support

Add streaming response support for real-time output.

#### Task 4.1: Define Streaming Interface

**Streaming Types**:

```
struct ResponseChunk {
  delta: string (incremental content)
  tool_call_delta: ToolCallDelta (optional)
  finish_reason: string (optional, "stop", "tool_calls", etc.)
  usage: Usage (optional, only in final chunk)
}

struct ToolCallDelta {
  index: int
  id: string (optional)
  name: string (optional)
  arguments_delta: string (partial JSON)
}

interface StreamingProvider extends Provider {
  async stream(messages: Message[], tools: Tool[]) -> Stream<ResponseChunk>
}
```

**Files to Create**:

- `providers/streaming` - Streaming types and utilities

#### Task 4.2: Implement SSE Parsing

Server-Sent Events (SSE) parser for OpenAI/Anthropic/Copilot:

**SSE Format**:

```
data: {"id":"1","choices":[{"delta":{"content":"Hello"}}]}

data: {"id":"1","choices":[{"delta":{"content":" world"}}]}

data: [DONE]
```

**Parser Features**:

- Line-by-line parsing
- JSON deserialization per chunk
- Handle `[DONE]` marker
- Error recovery (skip malformed chunks)

**Files to Create**:

- `providers/sse_parser` - SSE stream parsing

#### Task 4.3: Implement Streaming for Each Provider

Add streaming methods to each provider:

**OpenAI Streaming**:

- Set `"stream": true` in request
- Parse SSE response
- Accumulate deltas for tool calls (JSON may be split)

**Anthropic Streaming**:

- Set `"stream": true` in request
- Parse SSE response (different event types)
- Handle `content_block_start`, `content_block_delta`, `content_block_stop`

**Copilot Streaming**:

- Same as OpenAI (compatible format)

**Ollama Streaming**:

- Set `"stream": true` in request
- Parse newline-delimited JSON (not SSE)
- Each line is a complete JSON object

**Files to Update**:

- `providers/openai` - Add stream method
- `providers/anthropic` - Add stream method
- `providers/copilot` - Add stream method
- `providers/ollama` - Add stream method

#### Task 4.4: Testing Requirements

- Mock SSE stream tests
- Chunk accumulation tests (ensure deltas merge correctly)
- Tool call streaming tests (JSON parsing across chunks)
- Error handling in streams (network interruption)

#### Task 4.5: Deliverables

- Streaming interface (~50 lines)
- SSE parser (~200 lines)
- Streaming implementations (~400 lines total)
- Unit tests (~300 lines)

Total: ~950 lines

#### Task 4.6: Success Criteria

- [ ] SSE parser handles all event types
- [ ] Streaming works for all providers
- [ ] Deltas accumulate correctly
- [ ] Tool calls parse correctly from streamed JSON
- [ ] Stream errors are handled gracefully
- [ ] Test coverage >80%

### Phase 5: Provider Registry and Factory

Create factory pattern for provider instantiation.

#### Task 5.1: Implement Provider Factory

**Factory Pattern**:

```
struct ProviderFactory {
  registered_providers: map<string, ProviderConstructor>
}

impl ProviderFactory {
  register(name: string, constructor: ProviderConstructor)
  create(config: ProviderConfig) -> Result<Box<Provider>>
  list_providers() -> vec<ProviderMetadata>
}

struct ProviderMetadata {
  name: string
  display_name: string
  description: string
  default_model: string
  known_models: vec<ModelInfo>
  config_keys: vec<ConfigKey>
}

struct ConfigKey {
  name: string (env var name)
  required: bool
  secret: bool (store securely)
  default: string (optional)
}
```

**Files to Create**:

- `providers/factory` - Provider factory implementation
- `providers/registry` - Provider registration

#### Task 5.2: Implement Provider Metadata

Add metadata to each provider for discovery:

**Metadata Examples**:

- OpenAI: Known models (gpt-4o, gpt-4o-mini, etc.), context limits
- Anthropic: Claude models, 200K context
- Copilot: OpenAI models via GitHub
- Ollama: User-configured, local models

**Files to Update**:

- Each provider implementation - Add metadata method

#### Task 5.3: Testing Requirements

- Factory creation tests for each provider
- Metadata retrieval tests
- Configuration validation tests
- Provider not found error tests

#### Task 5.4: Deliverables

- Provider factory (~200 lines)
- Registry implementation (~100 lines)
- Metadata definitions (~200 lines)
- Unit tests (~200 lines)

Total: ~700 lines

#### Task 5.5: Success Criteria

- [ ] Factory creates all four providers from config
- [ ] Metadata is complete for all providers
- [ ] Invalid provider names return errors
- [ ] Missing configuration keys are detected
- [ ] Test coverage >80%

### Phase 6: Advanced Features (Optional)

Additional features for production use.

#### Task 6.1: Usage Tracking and Cost Estimation

**Usage Tracking**:

```
struct ProviderUsage {
  model: string
  input_tokens: int
  output_tokens: int
  total_tokens: int
  estimated_cost: float (optional)
  currency: string (default: "USD")
}

interface CostEstimator {
  estimate_cost(usage: ProviderUsage) -> float
}
```

**Cost Data** (approximate, update regularly):

- OpenAI gpt-4o: $2.50/1M input, $10/1M output
- Anthropic Claude Sonnet: $3/1M input, $15/1M output
- Copilot: Included in subscription
- Ollama: Free (local)

**Files to Create**:

- `providers/usage_tracking` - Usage types and tracking
- `providers/pricing` - Cost estimation utilities

#### Task 6.2: Caching Support

**Cache Types**:

- Prompt caching (Anthropic, OpenAI beta)
- Response caching (for identical requests)
- Tool definition caching

**Files to Create**:

- `providers/cache` - Caching utilities (optional)

#### Task 6.3: Testing Requirements

- Usage calculation tests
- Cost estimation tests
- Cache hit/miss tests

#### Task 6.4: Deliverables

- Usage tracking (~150 lines)
- Cost estimation (~100 lines)
- Caching (if implemented) (~200 lines)
- Unit tests (~200 lines)

Total: ~650 lines

#### Task 6.5: Success Criteria

- [ ] Usage statistics are accurate
- [ ] Cost estimation within 5% of actual
- [ ] Cache reduces API calls
- [ ] Test coverage >80%

## Open Questions

1. **Authentication Storage**: Should API keys be stored in environment variables only, or support config files / system keychains? _Recommendation: Environment variables + optional keychain (platform-specific)._

2. **Fast Model Fallback**: Should providers automatically fall back to regular model if fast model fails? _Recommendation: Yes, with warning log._

3. **Tool Call Limits**: Should we limit the number of tool calls per response to prevent loops? _Recommendation: Yes, configurable limit (default: 10)._

4. **Streaming Buffer Size**: What buffer size for streaming responses? _Recommendation: 8KB chunks, configurable._

5. **Retry on Streaming Errors**: Should streaming requests be retried if they fail mid-stream? _Recommendation: No, return error to caller._

6. **Model Validation**: Should we validate that the configured model exists before making requests? _Recommendation: No, let provider return error (models change frequently)._

## Implementation Estimates

### Total Lines of Code

- Phase 1 (Core Abstractions): ~700 lines
- Phase 2 (HTTP Client): ~1,250 lines
- Phase 3 (Provider Implementations): ~2,100 lines
- Phase 4 (Streaming Support): ~950 lines
- Phase 5 (Factory & Registry): ~700 lines
- Phase 6 (Advanced Features): ~650 lines

**Total Core Implementation**: ~6,350 lines
**Total Tests**: ~2,200 lines
**Grand Total**: ~8,550 lines

### Timeline Estimate

- Phase 1: 3-5 days
- Phase 2: 5-7 days
- Phase 3: 10-14 days
- Phase 4: 5-7 days
- Phase 5: 3-5 days
- Phase 6: 3-5 days

**Total**: 29-43 days (6-9 weeks) for complete implementation with tests

## Language-Specific Notes

### Rust Implementation

- Use `async-trait` crate for async trait methods
- Use `reqwest` for HTTP client
- Use `serde_json` for JSON serialization
- Use `tokio` for async runtime
- Use `thiserror` for error types
- Use `Pin<Box<dyn Stream>>` for streaming responses

### Python Implementation

- Use `abc.ABC` for abstract base classes
- Use `httpx` or `aiohttp` for async HTTP
- Use `pydantic` for data validation
- Use `asyncio` for async operations
- Use typed exceptions for error handling
- Use `AsyncIterator` for streaming responses

### Go Implementation

- Use interfaces for provider abstraction
- Use `net/http` for HTTP client
- Use `encoding/json` for JSON
- Use goroutines for async operations
- Use typed errors with `errors` package
- Use channels for streaming responses

## Security Considerations

1. **API Key Storage**: Never log or print API keys
2. **HTTPS Only**: Enforce HTTPS for all providers (except localhost Ollama)
3. **Timeout Configuration**: Prevent indefinite hangs
4. **Rate Limit Respect**: Honor retry-after headers
5. **Input Validation**: Validate all user inputs before sending to provider
6. **Token Limits**: Enforce maximum token counts to prevent excessive costs

## Testing Strategy

### Unit Tests

- Mock HTTP responses for each provider
- Test error mapping for all status codes
- Test request formatting for each provider
- Test response parsing for each provider
- Test streaming chunk accumulation

### Integration Tests

- Real API calls to test providers (with test accounts)
- End-to-end tool calling tests
- Streaming integration tests
- OAuth flow tests (manual/automated)

### Test Coverage Requirements

- Minimum 80% code coverage
- All error paths tested
- All providers tested with same test suite (compliance tests)

## Documentation Requirements

### API Documentation

- Provider interface documentation
- Configuration reference
- Error handling guide
- Streaming guide

### Examples

- Basic completion example for each provider
- Streaming example
- Tool calling example
- OAuth setup guide (Copilot)
- Ollama local setup guide

### Migration Guides

- Guide for adding new providers
- Guide for custom provider implementations

## References

- OpenAI API Docs: https://platform.openai.com/docs/api-reference
- Anthropic API Docs: https://docs.anthropic.com/en/api
- GitHub Copilot API: https://docs.github.com/en/copilot
- Ollama API Docs: https://github.com/ollama/ollama/blob/main/docs/api.md
- Goose Provider Implementation: Reference for production patterns

---

_This plan is designed to be language-agnostic. Adapt types, syntax, and patterns to your chosen language while maintaining the overall architecture and phasing strategy._
