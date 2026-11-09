# OpenAI `allow_http` Field Implementation

## Summary

Added `allow_http: bool` to `OpenAIConfig` so that users who run an
OpenAI-compatible inference server (llama.cpp, vLLM, LM Studio, etc.) on a
trusted local network can explicitly opt in to plain HTTP connections.

## Motivation

By default xzatoma should only allow HTTPS connections to remote
OpenAI-compatible hosts. This prevents accidental transmission of API keys and
prompt content over an unencrypted channel. However, inference servers running
on a home or office LAN often do not have TLS configured, so a blanket rejection
of HTTP would prevent legitimate use cases.

## Design Decisions

- `allow_http` mirrors the identical field already present on `OllamaConfig`,
  keeping provider configuration consistent.
- The field defaults to `false` via `#[serde(default)]` so existing config files
  and programmatic `OpenAIConfig::default()` calls are unaffected.
- The env-var `XZATOMA_OPENAI_ALLOW_HTTP` uses the existing `parse_env_bool`
  helper, accepting `true`/`false`/`1`/`0` and warning on unrecognized values.

## Files Changed

- `src/config.rs` - Added field, updated `Default` impl, `apply_env_vars`, and
  tests.
- `src/providers/openai.rs` - Added `allow_http: false` to all explicit
  `OpenAIConfig` struct literals in tests.
- `src/providers/factory.rs` - Added `allow_http: false` to the
  `unreachable_openai_config` helper.
- `tests/integration_provider_factory.rs` - Added `allow_http: false` to the
  `create_test_provider_config` helper.

## Usage

Via YAML:

```yaml
provider:
  openai:
    base_url: http://192.168.1.100:8080/v1
    allow_http: true
```

Via environment variable:

```sh
XZATOMA_OPENAI_ALLOW_HTTP=true xzatoma run "summarize this file"
```

## Tests Added

- `test_openai_config_allow_http_defaults_false` - default value
- `test_openai_config_allow_http_deserializes_true` - YAML round-trip
- `test_openai_config_allow_http_absent_gives_false` - absent field defaults
- `test_apply_env_vars_overrides_openai_allow_http` - env-var override
- `test_openai_config_defaults` updated to assert `allow_http` is `false`
