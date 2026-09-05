# Phase 4: ACP Multi-Agent Config Infrastructure

This document describes the configuration changes made to support Phase 4 ACP
multi-agent deployments in `src/config.rs`.

## Overview

Two new structs and one new method were added to enable multi-agent ACP
deployments: `AcpAgentConfig`, `AcpClientConfig`, and
`AcpConfig::effective_agents`.

## New Types

### `AcpAgentConfig`

Represents a single named agent in a multi-agent deployment. Each entry in
`AcpConfig::agents` can override the global provider, system prompt, and
thinking mode for runs targeting that named agent.

Fields:

- `name` - RFC 1123 label (required, non-blank)
- `description` - Human-readable purpose (defaults to empty string)
- `provider` - Optional provider override (`"copilot"`, `"ollama"`, `"openai"`)
- `input_content_types` - Declared accepted content types
- `output_content_types` - Declared produced content types
- `thinking_mode` - Optional thinking mode override
- `system_prompt` - Optional system prompt override (non-blank when present)

### `AcpClientConfig`

Controls the outbound HTTP client used for inter-agent tool calls
(`call_acp_agent`, `discover_acp_agents`).

Fields:

- `default_timeout_seconds` - Per-request timeout; `0` disables the tools
  entirely (default: `30`)
- `allowed_base_urls` - SSRF allow-list; empty list blocks all outbound calls

## New `AcpConfig` Fields

Two fields were appended to `AcpConfig`:

```rust
pub agents: Vec<AcpAgentConfig>,
pub client: AcpClientConfig,
```

Both default to empty / default values via `#[serde(default)]`, so existing
config files continue to work without modification.

## `AcpConfig::effective_agents`

```rust
pub fn effective_agents(&self, provider_type: &str) -> Vec<AcpAgentConfig>
```

Returns the configured agent list. When the list is empty (the common single-
agent case), synthesises a single entry with `name = "xzatoma"` and
`provider = Some(provider_type)` so callers always receive at least one agent.

## Validation

`validate_acp_config` enforces two new rules:

1. Every `agents[]` entry must have a non-blank `name`.
2. If `agents[].system_prompt` is set it must not be blank.
3. Every entry in `client.allowed_base_urls` must not be blank.

## Backward Compatibility

All new fields carry `#[serde(default)]` annotations. An existing YAML config
that does not mention `agents` or `client` deserializes cleanly to the same
defaults as `AcpConfig::default()`, and `effective_agents` will return the
synthesised single-agent entry as before.
