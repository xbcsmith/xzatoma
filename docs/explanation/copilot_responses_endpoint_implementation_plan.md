# Copilot Responses Endpoint Implementation Plan

## Overview

This plan outlines the integration of GitHub Copilot's `/responses` endpoint alongside the existing `/chat/completions` endpoint. Different Copilot models support different endpoints, requiring the provider to detect and use the appropriate endpoint per model. The implementation will integrate the standalone `responses.rs` module into the existing provider architecture while maintaining backward compatibility.

## Current State Analysis

### Existing Infrastructure

- **Provider Architecture**: Base `Provider` trait in `src/providers/base.rs` defines standard interface for AI providers
- **Copilot Provider**: `src/providers/copilot.rs` implements authentication, model listing, and `/chat/completions` endpoint
- **Model Detection**: `ModelSupportedEndpoint` enum in `copilot_chat.rs` indicates which endpoint(s) each model supports
- **Standalone Implementation**: `responses.rs` contains complete `/responses` endpoint implementation but is not integrated
- **Streaming Support**: Both endpoints support streaming responses with different event structures

### Identified Issues

1. **Module Isolation**: `responses.rs` exists at project root instead of integrated into `src/providers/` structure
2. **No Endpoint Selection**: Current provider only uses `/chat/completions`, ignoring model endpoint preferences
3. **Separate Data Structures**: Two separate request/response type systems without unified mapping
4. **Missing Provider Integration**: `responses.rs` operates independently without Provider trait implementation
5. **No Unified Interface**: Agent must know which endpoint to call instead of provider handling it transparently
6. **Legacy Endpoint Default**: System defaults to older `/chat/completions` endpoint instead of modern `/responses`
7. **No Telemetry**: No metrics tracking endpoint usage, performance, or failure patterns

## Implementation Phases

### Phase 1: Module Integration and Foundation

**Objective**: Move responses module into proper architecture and establish foundation for dual-endpoint support. This phase removes backward compatibility with `/chat/completions` as the default.

#### Task 1.1: Restructure Module Location

- Move `responses.rs` from project root to `src/providers/copilot/responses.rs`
- Create `src/providers/copilot/mod.rs` to organize submodules
- Move chat completions logic from `src/providers/copilot.rs` to `src/providers/copilot/chat_completions.rs`
- Update `src/providers/copilot.rs` to import and re-export from submodules
- Mark `/chat/completions` code as legacy fallback only
- Verify all imports resolve correctly across codebase

#### Task 1.2: Add Endpoint Configuration

- Add `endpoint_type` field to `CopilotProvider` struct to track selected endpoint
- Implement `determine_endpoint_for_model()` method that checks `Model.supported_endpoints`
- Default to `/responses` endpoint when model supports both endpoints
- Add endpoint URL helpers: `get_responses_url()` and `get_chat_completions_url()`
- Update `api_endpoint()` method to return correct URL based on model's supported endpoint
- Remove configuration option for endpoint forcing (use model capabilities only)

#### Task 1.3: Create Type Mappings

- Create `src/providers/copilot/types.rs` module for shared/conversion types
- Implement `From<base::Message>` for `responses::ResponseInputItem`
- Implement `From<base::Message>` for `chat_completions::CopilotMessage`
- Implement `From<base::ToolCall>` for both endpoint-specific tool call formats
- Add bidirectional conversion tests for all type mappings

#### Task 1.4: Testing Requirements

- Unit test `determine_endpoint_for_model()` with models supporting different endpoints
- Test type conversions preserve all fields correctly
- Test URL generation for both endpoints
- Verify module structure compiles without errors

#### Task 1.5: Deliverables

- `src/providers/copilot/mod.rs` - Module organization
- `src/providers/copilot/responses.rs` - Relocated responses implementation
- `src/providers/copilot/chat_completions.rs` - Extracted chat completions logic
- `src/providers/copilot/types.rs` - Shared type conversions
- Updated `src/providers/copilot.rs` - Main provider file

#### Task 1.6: Success Criteria

- Update existing tests to expect `/responses` as default endpoint
- `cargo check --all-targets --all-features` succeeds
- `cargo clippy --all-targets --all-features -- -D warnings` shows zero warnings
- Module structure follows project conventions
- Legacy `/chat/completions` code clearly marked as fallback only

### Phase 2: Unified Streaming Interface

**Objective**: Create abstraction layer that handles streaming from either endpoint transparently.

#### Task 2.1: Define Unified Stream Events

- Create `StreamEvent` enum in `src/providers/copilot/types.rs` that unifies both endpoint events
- Prioritize `responses::StreamEvent` as primary format
- Add conversion: `From<responses::StreamEvent>` to unified `StreamEvent`
- Add conversion: `From<chat_completions::ResponseEvent>` to unified `StreamEvent` for legacy fallback
- Implement `to_completion_response()` method on unified `StreamEvent`
- Handle partial response accumulation during streaming
- Add telemetry hooks for tracking endpoint usage

#### Task 2.2: Implement Endpoint-Agnostic Streaming

- Create `stream_completion_unified()` method that:
  - Determines endpoint from model (prefer `/responses`)
  - Calls appropriate endpoint-specific streaming function
  - Converts endpoint-specific events to unified `StreamEvent`
  - Returns `BoxStream<Result<StreamEvent>>`
  - Logs endpoint selection decision with model ID
  - Records telemetry for endpoint usage
- Update `complete()` in `Provider` trait impl to use `stream_completion_unified()`
- Preserve existing error handling and retry logic

#### Task 2.3: Response Accumulation Logic

- Implement `ResponseAccumulator` struct to collect streaming events into final response
- Handle text delta accumulation for both endpoints
- Handle tool call chunk assembly (different formats per endpoint)
- Extract token usage from final events
- Convert accumulated response to `CompletionResponse`

#### Task 2.4: Testing Requirements

- Test streaming from `/responses` endpoint with mock responses
- Test streaming from `/chat/completions` endpoint with mock responses
- Test automatic endpoint selection based on model
- Test response accumulation produces identical results for both endpoints
- Test tool call streaming and assembly for both formats
- Test error handling during streaming

#### Task 2.5: Deliverables

- `StreamEvent` unified enum in `types.rs`
- `ResponseAccumulator` struct in `types.rs`
- `stream_completion_unified()` method in `CopilotProvider`
- Updated `complete()` implementation
- Comprehensive streaming tests

#### Task 2.6: Success Criteria

- Provider automatically selects correct endpoint per model
- Streaming responses work identically regardless of endpoint
- Tool calls serialize/deserialize correctly for both endpoints
- Token usage tracked accurately
- All streaming tests pass with >80% coverage

### Phase 3: Model-Aware Endpoint Selection

**Objective**: Implement intelligent model detection and endpoint routing based on model capabilities.

#### Task 3.1: Enhance Model Metadata

- Add `preferred_endpoint` field to cached model data
- Update `fetch_copilot_models()` to extract and store `supported_endpoints` from API
- Implement preference logic: if model supports both, prefer `/responses`; if model doesn't specify, default to `/responses`
- Only fallback to `/chat/completions` if model explicitly does not support `/responses`
- Cache endpoint preference with model metadata (TTL-based)
- Add method `get_model_endpoint_preference()` to query cached data

#### Task 3.2: Update Model Selection Logic

- Modify `set_model()` to validate endpoint compatibility
- When setting model, update provider's `endpoint_type` field
- If model supports both endpoints, automatically select `/responses`
- Return error if requested model doesn't support any available endpoint
- Log endpoint selection decisions with model metadata for debugging
- Emit telemetry event for model selection with endpoint type

#### Task 3.3: Dynamic Endpoint Switching

- Remove manual `switch_endpoint()` method (use model capabilities only)
- Endpoint is determined solely by model's supported endpoints
- Log automatic endpoint selection based on model capabilities
- Preserve model selection state across provider operations

#### Task 3.4: Testing Requirements

- Test model metadata extraction includes endpoint information
- Test automatic endpoint selection for models with single endpoint support
- Test preference for `/responses` when model supports both endpoints
- Test fallback to `/chat/completions` only when `/responses` not supported
- Test rejection of models with no supported endpoints
- Test caching behavior for model endpoint preferences
- Test telemetry emission for endpoint selection events

#### Task 3.5: Deliverables

- Enhanced model metadata structures
- Updated `fetch_copilot_models()` implementation
- `get_model_endpoint_preference()` method
- Enhanced `set_model()` with endpoint validation and telemetry
- Model endpoint compatibility tests with telemetry validation

#### Task 3.6: Success Criteria

- Models correctly report supported endpoints
- Provider automatically uses `/responses` as preferred endpoint
- Provider falls back to `/chat/completions` only when necessary
- Model cache includes endpoint metadata
- Validation prevents invalid model/endpoint combinations
- Telemetry accurately tracks endpoint selection decisions

### Phase 4: Configuration and Error Handling

**Objective**: Add user-facing configuration options and robust error handling for endpoint-specific scenarios.

#### Task 4.1: Configuration Schema

- Remove `copilot.endpoint` configuration field (automatic selection only)
- Update `CopilotConfig` struct in `src/config.rs` to remove endpoint options
- Add `copilot.telemetry.enabled` boolean for endpoint metrics
- Add `copilot.telemetry.log_endpoint_selection` boolean for verbose logging
- Add configuration validation on load
- Document telemetry options in comments

#### Task 4.2: Enhanced Error Handling

- Create `CopilotEndpointError` enum with variants:
  - `UnsupportedEndpoint` - model doesn't support requested endpoint
  - `EndpointNotAvailable` - API endpoint returns 404/unavailable
  - `EndpointMismatch` - response format doesn't match expected endpoint
  - `EndpointConfigurationError` - invalid configuration
- Implement automatic fallback: try `/chat/completions` if `/responses` fails
- Add detailed error messages indicating which endpoint failed and why
- Log endpoint errors with full context including telemetry data
- Record failure metrics for endpoint reliability tracking

#### Task 4.3: Request/Response Validation

- Validate request structure matches target endpoint schema before sending
- Validate response structure matches expected endpoint format
- Add schema version detection in responses
- Implement graceful degradation for unknown response fields
- Log validation failures with request/response details

#### Task 4.4: Testing Requirements

- Test configuration parsing for telemetry options
- Test error creation and messages for each error variant
- Test automatic fallback from `/responses` to `/chat/completions`
- Test request validation catches malformed requests
- Test response validation catches unexpected formats
- Test error propagation to Provider trait callers
- Test telemetry data collection during failures

#### Task 4.5: Deliverables

- Enhanced `CopilotConfig` with telemetry configuration
- `CopilotEndpointError` enum in error types
- Request/response validation functions
- Automatic fallback mechanism implementation
- Telemetry collection and logging functions
- Configuration and error handling tests with telemetry validation

#### Task 4.6: Success Criteria

- Configuration schema supports telemetry options
- Errors provide actionable information for debugging
- Automatic fallback mechanism prevents unnecessary failures
- Validation catches issues before API calls
- Error handling tests cover all failure modes
- Telemetry accurately captures endpoint usage patterns

### Phase 5: Documentation and Integration Testing

**Objective**: Complete documentation and validate end-to-end functionality across the system.

#### Task 5.1: API Documentation

- Add rustdoc comments to all public types in `copilot/` submodules
- Document endpoint differences and automatic preference for `/responses`
- Add examples showing automatic endpoint selection behavior
- Document telemetry configuration options with examples
- Add module-level documentation explaining architecture
- Document breaking changes from previous implementation

#### Task 5.2: Integration Tests

- Create integration test suite in `tests/copilot_endpoints_integration.rs`
- Test complete flow: authenticate → list models → select model → complete request
- Test with models supporting only `/responses`
- Test with models supporting only `/chat/completions`
- Test with models supporting both endpoints
- Test streaming responses end-to-end
- Test tool calling through both endpoints

#### Task 5.3: Update Documentation

- Create `docs/explanation/copilot_responses_endpoint_implementation.md` with:
  - Overview of dual-endpoint support with `/responses` preference
  - Architecture diagram showing request flow
  - Endpoint selection decision tree (prefer `/responses`)
  - Type conversion mappings
  - Telemetry and monitoring guide
  - Breaking changes from previous implementation
  - Troubleshooting common issues
- Update `docs/reference/providers.md` with Copilot endpoint details
- Update `docs/how-to/configure_providers.md` with endpoint configuration examples

#### Task 5.4: Migration Guide

- Document breaking changes:
  - Automatic endpoint selection (no manual override)
  - Default to `/responses` endpoint
  - Removal of endpoint configuration options
  - Addition of telemetry configuration
- Provide migration path for existing deployments
- Explain new automatic endpoint selection behavior
- Add FAQ section addressing common migration questions

#### Task 5.5: Testing Requirements

- Integration tests pass with real API calls (authenticated)
- Integration tests pass with mocked API calls (CI/CD)
- Documentation examples compile and run successfully
- Cargo doc builds without warnings
- All public APIs have documentation coverage

#### Task 5.6: Deliverables

- Comprehensive rustdoc comments on all public items
- Integration test suite
- `docs/explanation/copilot_responses_endpoint_implementation.md`
- Updated reference and how-to documentation
- Migration guide (if needed)

#### Task 5.7: Success Criteria

- `cargo doc --no-deps --open` generates complete documentation
- Integration tests validate real-world usage patterns
- Documentation provides clear guidance for endpoint selection
- Zero documentation warnings from cargo doc
- All examples in documentation are tested and working

## Breaking Changes

### User-Facing Changes

1. **Automatic Endpoint Selection**: Endpoint is now determined by model capabilities, not user configuration
2. **Default Endpoint Change**: System now prefers `/responses` over `/chat/completions`
3. **Configuration Removal**: `copilot.endpoint` and `copilot.endpoint_override` configuration options removed
4. **New Telemetry**: Endpoint usage metrics now collected by default (can be disabled)
5. **Behavior Change**: Models supporting both endpoints will use `/responses` instead of `/chat/completions`

### Migration Impact

- Users with endpoint-specific configurations must remove those settings
- Existing workflows relying on `/chat/completions` will automatically migrate to `/responses` (if supported)
- Monitor telemetry logs to verify endpoint selection meets expectations
- Models only supporting `/chat/completions` continue to work unchanged

## Dependencies and Risks

### External Dependencies

- GitHub Copilot API stability for both endpoints
- Model metadata accuracy in Copilot API responses
- No breaking changes to endpoint schemas during implementation
- Telemetry logging infrastructure availability

### Technical Risks

- **Risk**: Models may support both endpoints but behave differently
  - **Mitigation**: Extensive testing with both endpoints, document behavioral differences, use telemetry to detect issues
- **Risk**: Endpoint preference metadata may be missing/incorrect for some models
  - **Mitigation**: Implement safe default to `/responses`, fallback to `/chat/completions` with telemetry tracking
- **Risk**: Response format variations between models on same endpoint
  - **Mitigation**: Flexible parsing with graceful degradation for unknown fields

### Implementation Risks

- **Risk**: Breaking existing Copilot functionality during refactoring
  - **Mitigation**: Comprehensive test coverage before refactoring, incremental changes, telemetry to detect issues
- **Risk**: Type conversion losing information between formats
  - **Mitigation**: Bidirectional conversion tests, validate no data loss
- **Risk**: Immediate breaking changes without backward compatibility
  - **Mitigation**: Clear migration documentation, telemetry to track adoption, support for legacy endpoint as fallback

## Timeline Estimation

- **Phase 1**: 3-5 days (foundation and structure)
- **Phase 2**: 4-6 days (streaming unification)
- **Phase 3**: 3-4 days (model-aware selection)
- **Phase 4**: 2-3 days (configuration and errors)
- **Phase 5**: 3-4 days (documentation and testing)

**Total**: 15-22 days

## Success Metrics

1. Provider automatically selects correct endpoint for 100% of supported models
2. Zero regression in existing `/chat/completions` functionality
3. Streaming works identically for both endpoints from caller perspective
4. Test coverage >80% for all new code
5. All cargo quality checks pass (fmt, check, clippy, test)
6. Documentation coverage 100% for public APIs

## Implementation Decisions

1. **Manual Endpoint Override**: NO - Only use endpoints that models officially support to ensure reliability
2. **Endpoint Preference**: Prefer `/responses` when model supports both endpoints (newer, more feature-rich)
3. **Provider Trait Exposure**: Copilot-specific implementation - do not expose in base Provider trait
4. **Telemetry**: YES - Add metrics to track endpoint usage patterns for optimization and debugging
5. **Backward Compatibility**: NO - Enable automatic endpoint selection immediately, no deprecation period

## Telemetry Requirements

### Metrics to Track

- Endpoint usage distribution (responses vs chat_completions) per model
- Request success/failure rates per endpoint
- Average response latency per endpoint
- Token usage patterns per endpoint
- Endpoint fallback occurrences (if primary fails)
- Model/endpoint combination usage frequency

### Implementation

- Add lightweight metrics collection in `stream_completion_unified()`
- Log endpoint selection decisions with model identifier
- Track request timing for performance analysis
- Include endpoint type in error logging
- Emit structured logs parseable by log aggregation tools
