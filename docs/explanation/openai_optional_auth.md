# OpenAI Optional Authentication Implementation

## Overview

The OpenAI provider in XZatoma targets any server that implements the OpenAI
Chat Completions API: the hosted service at `https://api.openai.com/v1` as well
as local inference servers such as llama.cpp, vLLM, Mistral.rs, and Candle-vLLM.
Local servers typically run without authentication while the hosted service
always requires a Bearer token.

This document explains three layered fixes that were made to support local
servers reliably, what the root cause of the original bug was, and why each
design decision was taken.

---

## Root Cause

The primary bug was that `build_request_headers()` unconditionally inserted
`Content-Type: application/json` into **every** outgoing request, including GET
requests used for model discovery.

The llama.cpp model-router (started with `--models-preset`) treats the presence
of `Content-Type: application/json` on a GET request as a signal that the caller
is an authenticated API client. When no `Authorization: Bearer` header
accompanies it, the router responds with `401 Unauthorized`:

```text
Error listing models: Provider error: HTTP 401 Unauthorized: {
  "error": {
    "message": "Missing bearer authentication in header",
    "type": "invalid_request_error",
    "param": null,
    "code": null
  }
}
```

A plain `curl http://localhost:8000/v1/models` succeeds because curl sends no
`Content-Type` header on GET requests. xzatoma always sent one, so the server
demanded a Bearer token that was not configured.

---

## Changes Made

### 1. `build_get_headers()` — new method, no Content-Type

A new `build_get_headers()` method was added to `OpenAIProvider`. It builds the
same `Authorization` and `OpenAI-Organization` headers as
`build_request_headers()` but omits `Content-Type: application/json`. All GET
requests now use this method.

```rust
fn build_get_headers(&self) -> Result<reqwest::header::HeaderMap> {
    // Authorization + OpenAI-Organization only.
    // Content-Type is intentionally absent: its presence on GET requests
    // causes some local servers (e.g. llama.cpp with --models-preset) to
    // require authentication even when no --api-key was configured.
    ...
}
```

`build_request_headers()` (which includes `Content-Type: application/json`) is
kept for POST requests only: `post_completions` and
`post_completions_streaming`.

### 2. `is_authenticated()` — local servers are always considered authenticated

Before this change `is_authenticated()` returned `false` when `api_key` was
empty. For local servers this was semantically wrong: a llama.cpp instance
started without `--api-key` is fully usable with no credentials.

The updated implementation returns `true` when either an `api_key` is configured
or the `base_url` is not the default OpenAI endpoint:

```rust
fn is_authenticated(&self) -> bool {
    self.config
        .read()
        .map(|c| {
            !c.api_key.is_empty() || c.base_url != "https://api.openai.com/v1"
        })
        .unwrap_or(false)
}
```

This mirrors how the Ollama provider works: when no credentials are required,
the provider reports itself as ready to use.

### 3. `http_error()` — contextual 401 error messages

A private `http_error()` helper replaces the three separate
`format!("HTTP {}: {}", status, body)` constructions in `post_completions`,
`post_completions_streaming`, and `list_models`. When the server returns
`401 Unauthorized` and no `api_key` is configured, the message includes an
actionable hint:

```text
HTTP 401 Unauthorized: ... -- server requires authentication; set api_key in
the OpenAI provider configuration or start the server without requiring
authentication
```

### 4. Graceful fallback in `list_models` on 401 with no api_key

As a resilience measure for servers that genuinely gate `/v1/models` behind
authentication while leaving `/chat/completions` open, `list_models` now falls
back to returning the currently configured model name when it receives
`401 Unauthorized` and `api_key` is empty:

```rust
if status == reqwest::StatusCode::UNAUTHORIZED {
    let (api_key_empty, model) = self
        .config
        .read()
        .map(|c| (c.api_key.is_empty(), c.model.clone()))
        .unwrap_or_else(|_| (true, String::new()));
    if api_key_empty && !model.is_empty() {
        tracing::warn!(
            "GET /models returned 401 Unauthorized with no api_key configured; \
             falling back to the configured model '{}'",
            model
        );
        let mut info = ModelInfo::new(model.clone(), model.clone(), 0);
        for cap in build_capabilities_from_id(&info.name) {
            info.add_capability(cap);
        }
        return Ok(vec![info]);
    }
}
```

This fallback only triggers when `api_key` is empty. A wrong or expired key
still propagates as an error.

### 5. `get_model_info` — consistent GET header and error handling

`get_model_info` was updated to use `build_get_headers()` (same fix as
`list_models`) and to use `http_error()` for its non-success status handling,
making the behaviour consistent with the rest of the provider.

---

## Behavior Matrix

### Header sent per request type

| Method                       | `Content-Type` sent | `Authorization` sent          |
| ---------------------------- | ------------------- | ----------------------------- |
| `post_completions`           | yes                 | yes, when `api_key` non-empty |
| `post_completions_streaming` | yes                 | yes, when `api_key` non-empty |
| `list_models` (GET)          | no                  | yes, when `api_key` non-empty |
| `get_model_info` (GET)       | no                  | yes, when `api_key` non-empty |

### `is_authenticated()` return value

| `base_url`                  | `api_key` | `is_authenticated()` |
| --------------------------- | --------- | -------------------- |
| `https://api.openai.com/v1` | set       | true                 |
| `https://api.openai.com/v1` | empty     | false                |
| `http://localhost:8000/v1`  | set       | true                 |
| `http://localhost:8000/v1`  | empty     | true                 |
| any other custom URL        | set       | true                 |
| any other custom URL        | empty     | true                 |

### `list_models()` on non-2xx response

| `api_key` | Server returns | Result                                  |
| --------- | -------------- | --------------------------------------- |
| empty     | 401            | `Ok(vec![configured_model])` + WARN log |
| set       | 401            | `Err(...)` with hint to check the key   |
| any       | other 4xx/5xx  | `Err(...)` with raw HTTP status         |
| any       | 200            | `Ok(filtered_and_sorted_model_list)`    |

---

## Tests Added

| Test                                                                  | What it verifies                                                |
| --------------------------------------------------------------------- | --------------------------------------------------------------- |
| `test_is_authenticated_with_key_returns_true`                         | Provider with `api_key` is authenticated regardless of URL      |
| `test_is_authenticated_without_key_local_url_returns_true`            | Local URL with no key is considered authenticated               |
| `test_is_authenticated_without_key_default_url_returns_false`         | Default OpenAI URL with no key is not authenticated             |
| `test_list_models_get_request_omits_content_type`                     | GET /models request carries no `Content-Type` header            |
| `test_list_models_401_without_api_key_falls_back_to_configured_model` | 401 with no key returns configured model instead of failing     |
| `test_list_models_401_with_api_key_set_returns_error`                 | 401 with a key set still propagates as an error                 |
| `test_post_completions_401_without_api_key_includes_hint`             | 401 from completions includes `api_key` hint when no key is set |

The existing `test_no_auth_header_when_api_key_empty` and
`test_bearer_token_sent_in_header` tests remain unchanged.

---

## Why Not a Single Toggle Field

An alternative design considered adding an explicit `require_auth: bool` field
to `OpenAIConfig`. This was rejected because:

1. The `Content-Type` fix solves the root cause; the toggle would only treat the
   symptom.
2. Most users would never think to set it.
3. The `base_url` field already carries sufficient signal: any URL that differs
   from the default OpenAI endpoint implies a self-hosted server where auth is
   optional by convention.

---

## Configuration Example

To use xzatoma with a local llama.cpp server started with `--models-preset` and
no `--api-key`:

```yaml
provider:
  provider_type: openai
  openai:
    base_url: "http://127.0.0.1:8000/v1"
    model: "ibm-granite/granite-4.0-h-small-GGUF:Q4_K_M"
    # api_key is intentionally omitted
```

If the server was started with `--api-key`, supply the matching value:

```yaml
provider:
  provider_type: openai
  openai:
    base_url: "http://127.0.0.1:8000/v1"
    model: "ibm-granite/granite-4.0-h-small-GGUF:Q4_K_M"
    api_key: "your-local-key"
```

The key can also be supplied at runtime without modifying the configuration
file:

```bash
XZATOMA_OPENAI_API_KEY=your-local-key xzatoma models list
```
