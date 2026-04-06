# Use OpenAI and OpenAI-Compatible Providers

## Overview

XZatoma supports OpenAI's hosted API and any server that implements the OpenAI
chat completions API. This how-to guide covers six common configuration tasks:

1. [Configure XZatoma to use OpenAI's hosted API](#1-configure-xzatoma-to-use-openais-hosted-api)
2. [Configure XZatoma to use a local llama.cpp server](#2-configure-xzatoma-to-use-a-local-llamacpp-server)
3. [Configure XZatoma to use a local vLLM server](#3-configure-xzatoma-to-use-a-local-vllm-server)
4. [Configure XZatoma to use a local Mistral.rs server](#4-configure-xzatoma-to-use-a-local-mistralrs-server)
5. [Set the API key via environment variable](#5-set-the-api-key-via-environment-variable)
6. [Switch model without editing the config file](#6-switch-model-without-editing-the-config-file)

For a complete field reference see
[docs/reference/configuration.md](../reference/configuration.md). For a list of
commonly used OpenAI model names and their context windows see
[docs/reference/models.md](../reference/models.md).

---

## 1. Configure XZatoma to use OpenAI's hosted API

### Via config file

Create or edit `config/config.yaml`:

```yaml
provider:
  type: openai
  openai:
    # Set your API key here or via XZATOMA_OPENAI_API_KEY (recommended)
    api_key: ""
    base_url: "https://api.openai.com/v1"
    model: gpt-4o-mini
    # organization_id: org-replace-with-your-org-id
    enable_streaming: true

agent:
  max_turns: 50
  timeout_seconds: 300
```

### Via environment variables

```bash
export XZATOMA_OPENAI_API_KEY="sk-..."
export XZATOMA_OPENAI_BASE_URL="https://api.openai.com/v1"
export XZATOMA_OPENAI_MODEL="gpt-4o-mini"
xzatoma chat --provider openai
```

Environment variables override values in the config file. You can combine a
config file for non-sensitive settings and an environment variable for the API
key.

### Verify connectivity

```bash
xzatoma models list --provider openai
```

A successful response lists the models available on your account. If you see a
`401 Unauthorized` error, check that `XZATOMA_OPENAI_API_KEY` is set correctly
and contains no stray characters.

---

## 2. Configure XZatoma to use a local llama.cpp server

llama.cpp's built-in HTTP server exposes an OpenAI-compatible endpoint at `/v1`.
Start the server before configuring XZatoma:

```bash
llama-server --model /path/to/model.gguf --port 8080
```

### Via config file

```yaml
provider:
  type: openai
  openai:
    api_key: ""
    base_url: "http://localhost:8080/v1"
    # Use "local-model" or whatever name llama-server reports for your model
    model: local-model
    enable_streaming: true

agent:
  max_turns: 50
  timeout_seconds: 300
```

### Via environment variables

```bash
export XZATOMA_OPENAI_API_KEY=""
export XZATOMA_OPENAI_BASE_URL="http://localhost:8080/v1"
export XZATOMA_OPENAI_MODEL="local-model"
xzatoma chat --provider openai
```

### Notes

- Leave `api_key` empty. llama.cpp does not require authentication.
- The model name `local-model` is the default reported by `llama-server`. Some
  builds report a different name; run `curl http://localhost:8080/v1/models` to
  see what your server reports.
- Increase `agent.timeout_seconds` for large models that respond slowly.

---

## 3. Configure XZatoma to use a local vLLM server

vLLM exposes a fully OpenAI-compatible API. Start the server before configuring
XZatoma:

```bash
python -m vllm.entrypoints.openai.api_server \
  --model meta-llama/Llama-3.2-3B-Instruct \
  --port 8000
```

### Via config file

```yaml
provider:
  type: openai
  openai:
    api_key: ""
    base_url: "http://localhost:8000/v1"
    # Use the Hugging Face model ID passed to vLLM
    model: meta-llama/Llama-3.2-3B-Instruct
    enable_streaming: true

agent:
  max_turns: 50
  timeout_seconds: 300
```

### Via environment variables

```bash
export XZATOMA_OPENAI_API_KEY=""
export XZATOMA_OPENAI_BASE_URL="http://localhost:8000/v1"
export XZATOMA_OPENAI_MODEL="meta-llama/Llama-3.2-3B-Instruct"
xzatoma chat --provider openai
```

### Notes

- The model name must exactly match the `--model` argument passed to vLLM.
- vLLM supports tool calls for models that have been fine-tuned for them. If
  tool calls fail, set `enable_streaming: false` as a diagnostic step.
- If you start vLLM with an API key (`--api-key mykey`), set
  `XZATOMA_OPENAI_API_KEY="mykey"` to match.

---

## 4. Configure XZatoma to use a local Mistral.rs server

Mistral.rs exposes an OpenAI-compatible HTTP server. Start the server before
configuring XZatoma:

```bash
mistralrs-server --port 1234 plain -m /path/to/mistral-7b-instruct
```

### Via config file

```yaml
provider:
  type: openai
  openai:
    api_key: ""
    base_url: "http://localhost:1234/v1"
    model: mistral-7b-instruct
    enable_streaming: true

agent:
  max_turns: 50
  timeout_seconds: 300
```

### Via environment variables

```bash
export XZATOMA_OPENAI_API_KEY=""
export XZATOMA_OPENAI_BASE_URL="http://localhost:1234/v1"
export XZATOMA_OPENAI_MODEL="mistral-7b-instruct"
xzatoma chat --provider openai
```

### Notes

- Leave `api_key` empty. Mistral.rs does not require authentication by default.
- The model name should match the identifier Mistral.rs uses for the loaded
  model. Run `curl http://localhost:1234/v1/models` to confirm.

---

## 5. Set the API key via environment variable

Storing API keys in config files risks accidental exposure in version control.
The recommended approach is to keep the config file free of secrets and supply
the key at runtime:

```bash
export XZATOMA_OPENAI_API_KEY="sk-..."
xzatoma chat
```

The environment variable always overrides the `api_key` field in the config
file. You can leave `api_key` empty or omit it entirely in the config file:

```yaml
provider:
  type: openai
  openai:
    # api_key is intentionally omitted here; set XZATOMA_OPENAI_API_KEY at runtime
    base_url: "https://api.openai.com/v1"
    model: gpt-4o-mini
    enable_streaming: true

agent:
  max_turns: 50
  timeout_seconds: 300
```

For organizational accounts, also set the organization ID via environment
variable:

```bash
export XZATOMA_OPENAI_API_KEY="sk-..."
export XZATOMA_OPENAI_ORG_ID="org-..."
xzatoma chat
```

---

## 6. Switch model without editing the config file

Use `XZATOMA_OPENAI_MODEL` to override the model at runtime without changing the
config file:

```bash
export XZATOMA_OPENAI_MODEL="gpt-4o"
xzatoma chat
```

Or pass the model name inline for a single command:

```bash
XZATOMA_OPENAI_MODEL="gpt-4o" xzatoma chat
```

Equivalent config file approach (for a persistent change):

```yaml
provider:
  type: openai
  openai:
    api_key: ""
    base_url: "https://api.openai.com/v1"
    model: gpt-4o
    enable_streaming: true

agent:
  max_turns: 50
  timeout_seconds: 300
```

### Verify the active model

```bash
xzatoma models current --provider openai
```

### Available model names

The full list of models available on your account is returned by:

```bash
xzatoma models list --provider openai
```

For a reference table of commonly used OpenAI model names and context windows
see [docs/reference/models.md](../reference/models.md).

---

## Environment Variable Summary

| Variable                   | Purpose                                   | Default                       |
| -------------------------- | ----------------------------------------- | ----------------------------- |
| `XZATOMA_OPENAI_API_KEY`   | Bearer token for API authentication       | `""`                          |
| `XZATOMA_OPENAI_BASE_URL`  | API base URL (override for local servers) | `"https://api.openai.com/v1"` |
| `XZATOMA_OPENAI_MODEL`     | Model name                                | `"gpt-4o-mini"`               |
| `XZATOMA_OPENAI_ORG_ID`    | Organization ID (optional)                | (none)                        |
| `XZATOMA_OPENAI_STREAMING` | Enable SSE streaming for text responses   | `"true"`                      |

---

## Troubleshooting

### 401 Unauthorized

Verify that `XZATOMA_OPENAI_API_KEY` is set and correct:

```bash
echo $XZATOMA_OPENAI_API_KEY
```

Ensure there are no trailing spaces or stray quote characters. For local servers
that do not require authentication, leave the variable unset or set it to an
empty string:

```bash
export XZATOMA_OPENAI_API_KEY=""
```

### Connection refused / server not reachable

Confirm the local inference server is running and listening on the expected
port:

```bash
curl http://localhost:8080/v1/models
```

Check that `XZATOMA_OPENAI_BASE_URL` points to the correct address and includes
the `/v1` path suffix.

### Model not found

Run `xzatoma models list --provider openai` to see what model names your server
reports and set `XZATOMA_OPENAI_MODEL` to one of those names.

### Slow responses

Increase the agent timeout in your config file:

```yaml
agent:
  timeout_seconds: 600
```

---

## Related Documentation

- Configuration reference:
  [docs/reference/configuration.md](../reference/configuration.md)
- Model reference: [docs/reference/models.md](../reference/models.md)
- Provider abstraction reference:
  [docs/reference/provider_abstraction.md](../reference/provider_abstraction.md)
- Configure providers how-to:
  [docs/how-to/configure_providers.md](configure_providers.md)
