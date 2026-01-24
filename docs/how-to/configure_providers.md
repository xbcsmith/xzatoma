# Configure AI Providers

## Overview

This how-to explains how to configure AI providers for XZatoma so the agent can make completions, stream responses, and (when allowed) call tools. It covers supported providers (OpenAI, Anthropic, GitHub Copilot, and Ollama), environment variables, CLI authentication flows, quick validation steps, and troubleshooting tips.

Intended audience: users who want to set up a provider for interactive chat sessions, model discovery, or running plans that require provider completions.

## Quickstart (one-liners)

- OpenAI (env var)
```bash
export OPENAI_API_KEY="sk-..."
xzatoma chat --provider openai
```

- Anthropic (env var)
```bash
export ANTHROPIC_API_KEY="sk-ant-..."
xzatoma chat --provider anthropic
```

- GitHub Copilot (device OAuth)
```bash
xzatoma auth --provider copilot
# Follow the interactive instructions in your terminal/browser
xzatoma chat --provider copilot
```

- Ollama (local host)
```bash
export OLLAMA_HOST="http://localhost:11434"
export OLLAMA_MODEL="llama3.2:latest"
xzatoma chat --provider ollama
```

## Supported Providers & Auth Methods

- OpenAI
  - Auth: Bearer API key via `OPENAI_API_KEY`
  - Optional: `OPENAI_HOST`, `OPENAI_TIMEOUT`
  - Typical use: cloud-hosted API (HTTPS)

- Anthropic
  - Auth: API key via `ANTHROPIC_API_KEY`
  - Optional: `ANTHROPIC_HOST`

- GitHub Copilot
  - Auth: OAuth device flow (recommended) via `xzatoma auth --provider copilot`
  - Alternatives: `GITHUB_TOKEN` (when available) or `COPILOT_API_KEY` (if supported by your deployment)
  - Tokens are typically cached in the system keyring for convenience.

- Ollama
  - Auth: Typically none (local model server)
  - Host & model: `OLLAMA_HOST` and `OLLAMA_MODEL` (Ollama model names are runtime-specific)
  - Common host: `http://localhost:11434`

For provider implementation details and differences (tool formats, streaming formats, request/response shapes) see the reference:
- Provider reference: ../reference/provider_abstraction.md
- Model management: ../reference/model_management.md

## Environment Variables

Example environment variables and brief explanations:

- OpenAI
```bash
export OPENAI_API_KEY="sk-..."
# Optional
export OPENAI_HOST="https://api.openai.com"
export OPENAI_TIMEOUT="600"  # seconds
```

- Anthropic
```bash
export ANTHROPIC_API_KEY="sk-ant-..."
export ANTHROPIC_HOST="https://api.anthropic.com"
```

- GitHub Copilot (alternatives)
```bash
export GITHUB_TOKEN="ghp_..."
# or (if supported for direct API key usage)
export COPILOT_API_KEY="..."
```

- Ollama (local)
```bash
export OLLAMA_HOST="http://localhost:11434"
export OLLAMA_MODEL="llama3.2:latest"
```

Notes:
- Prefer environment variables or system keyring for secrets. Do not commit API keys to source control.
- `xzatoma` will prefer explicit CLI flags, then environment variables, then config file values, then defaults (CLI overrides env overrides file overrides defaults).

## CLI Authentication & Validation

- Authenticate (Copilot device flow)
```bash
xzatoma auth --provider copilot
```
Follow the interactive instructions (you will be asked to open a URL and enter a code to authorize). The resulting credentials are stored (usually in the system keyring).

- Check available models (useful to validate provider connectivity)
```bash
xzatoma models list --provider copilot
xzatoma models list --provider ollama
xzatoma models list --provider openai
```

- Show current model for a provider
```bash
xzatoma models current --provider copilot
```

- Start an interactive chat with a specific provider
```bash
xzatoma chat --provider ollama  # or openai, copilot, anthropic
```

If the above commands return errors (authentication or network), consult the troubleshooting section below.

## Configuration File Example (config/config.yaml)

You can also configure providers in the YAML config file. Example:
```yaml
provider:
  provider_type: copilot         # default provider: copilot|ollama|openai|anthropic
  copilot:
    model: "gpt-5-mini"
  ollama:
    host: "http://localhost:11434"
    model: "llama3.2:latest"
```

When using a config file, sensitive values (API keys) are still recommended to be set via environment variables or a secure keyring.

## Troubleshooting

- 401 / Unauthorized
  - Verify the API key is set (`echo $OPENAI_API_KEY`) and is correct.
  - Ensure there are no stray characters or quotes.
  - For Copilot, confirm OAuth completed successfully: re-run `xzatoma auth --provider copilot`.

- Provider not reachable / network errors
  - Verify `OPENAI_HOST`/`ANTHROPIC_HOST`/`OLLAMA_HOST` are reachable.
  - For Ollama ensure the local service is running. Example: `curl $OLLAMA_HOST` (should respond).

- Ollama model not found
  - Confirm `OLLAMA_MODEL` is correct and available in your local Ollama instance.
  - Use the Ollama CLI (outside the scope of XZatoma) to list or pull models, e.g. `ollama pull <model>`.

- Copilot device flow hangs in CI / headless environments
  - Use `COPILOT_API_KEY` or `GITHUB_TOKEN` if available and supported, or run the device flow in an environment where you can complete the browser-based step.
  - Some integration tests that rely on the system keyring are ignored in CI (they require an interactive keyring).

- Rate-limited / 429
  - Check provider-specific rate limits and throttle accordingly. Consider adding retry/backoff in your usage patterns.

## Security Best Practices

- Never store API keys in plaintext in the repository.
- Use environment variables or secure key storage (system keyring or secrets manager).
- For remote providers use HTTPS endpoints (OpenAI, Anthropic).
- Avoid printing secrets in logs or error messages.
- When authorizing Copilot, be mindful of token scopes and expiration.

## Testing & Validation Checklist

1. Set environment variables for the provider you want to use.
2. Run `xzatoma models list --provider <provider>` — expect a list of models or a helpful error.
3. Run `xzatoma models current --provider <provider>` — confirm the model in use.
4. Start `xzatoma chat --provider <provider>` and run a sample prompt to verify completions.
5. If using Copilot, run `xzatoma auth --provider copilot` to authenticate and then repeat steps 2–4.

## When to Consult Reference Docs

- For implementation details, error types, provider differences, tool calling formats, and streaming specifics, refer to:
  - Provider reference: ../reference/provider_abstraction.md
  - Model management reference: ../reference/model_management.md
  - Implementation plans and archived notes: ../archive/implementation_summaries/

## Examples & Notes

- Example: Quick local Ollama test
```bash
export OLLAMA_HOST="http://localhost:11434"
export OLLAMA_MODEL="llama3.2:latest"
xzatoma chat --provider ollama --mode planning
```

- Example: OpenAI quick test
```bash
export OPENAI_API_KEY="sk-..."
xzatoma chat --provider openai --mode planning
```

## References

- Provider abstraction quick reference: ../reference/provider_abstraction.md
- Model management: ../reference/model_management.md
- Project documentation index: ../README.md

---

If you encounter a provider-specific issue that isn't covered here, please open an issue with the provider name, the commands you ran, and the error output so maintainers can help and update this document.
