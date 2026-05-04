# Configure Zed to use XZatoma as an ACP agent

This guide shows you how to configure the Zed editor to launch XZatoma as an ACP
stdio agent subprocess. XZatoma will appear in Zed's agent panel and can receive
text and vision prompts from the active workspace.

## Before you begin

You need:

- Zed installed and updated to a version that supports custom ACP agent servers
- XZatoma built from source (`cargo build --release`) or installed via
  `cargo install --git https://github.com/xbcsmith/xzatoma`
- At least one working provider configured in your XZatoma config file. See
  `docs/how-to/configure_providers.md` for setup instructions.
- The `xzatoma` binary on your system `PATH`, or the full path to the binary
  ready to use in the Zed settings JSON

Confirm XZatoma is on your PATH:

```sh
xzatoma --version
```

## Important: stdout is reserved for JSON-RPC

When running in agent mode, XZatoma writes all JSON-RPC protocol traffic to
stdout. Any non-JSON bytes on stdout will corrupt the protocol stream and break
the Zed connection. XZatoma forces all tracing, logging, and diagnostic output
to stderr automatically in agent mode. Do not set environment variables that
write additional output to stdout.

## Step 1: Add XZatoma to Zed agent_servers

Open your Zed settings file (`~/.config/zed/settings.json` on macOS and Linux,
`%AppData%\Zed\settings.json` on Windows) and add or update the `agent_servers`
array:

```json
{
  "agent_servers": [
    {
      "name": "xzatoma",
      "command": "xzatoma",
      "args": ["agent"],
      "env": {}
    }
  ]
}
```

If `xzatoma` is not on your PATH, replace `"xzatoma"` with the full path:

```json
{
  "agent_servers": [
    {
      "name": "xzatoma",
      "command": "/home/yourname/.cargo/bin/xzatoma",
      "args": ["agent"],
      "env": {}
    }
  ]
}
```

After saving the settings file, restart Zed or reload the window. XZatoma should
appear in the agent panel.

## Step 2: Choose a provider

XZatoma reads its provider configuration from `config/config.yaml` in the
current working directory, or from the location set by the `XZATOMA_CONFIG`
environment variable.

### GitHub Copilot

If you have GitHub Copilot credentials stored by `xzatoma auth`, the default
configuration works without changes. Verify authentication:

```sh
xzatoma auth --provider copilot
```

To force the Copilot provider in Zed without changing your config file:

```json
{
  "agent_servers": [
    {
      "name": "xzatoma",
      "command": "xzatoma",
      "args": ["agent", "--provider", "copilot"],
      "env": {}
    }
  ]
}
```

### Ollama

Start Ollama and pull your preferred model:

```sh
ollama serve
ollama pull granite4:3b
```

Then configure the Zed settings to use the Ollama provider and model:

```json
{
  "agent_servers": [
    {
      "name": "xzatoma",
      "command": "xzatoma",
      "args": ["agent", "--provider", "ollama", "--model", "granite4:3b"],
      "env": {}
    }
  ]
}
```

### OpenAI-compatible providers

For providers that expose an OpenAI-compatible API (including local inference
servers), set the provider to `openai` and supply the relevant environment
variables:

```json
{
  "agent_servers": [
    {
      "name": "xzatoma",
      "command": "xzatoma",
      "args": ["agent", "--provider", "openai", "--model", "gpt-4o"],
      "env": {
        "OPENAI_API_KEY": "sk-..."
      }
    }
  ]
}
```

## Step 3: Optional CLI flags

The `xzatoma agent` command accepts these flags:

| Flag                   | Description                                                       |
| ---------------------- | ----------------------------------------------------------------- |
| `--provider <name>`    | Override the provider from config (`copilot`, `ollama`, `openai`) |
| `--model <name>`       | Override the model for the selected provider                      |
| `--allow-dangerous`    | Allow terminal commands without confirmation prompts              |
| `--working-dir <path>` | Fallback workspace root when Zed does not provide one             |

## Step 4: Enable or disable vision support

XZatoma accepts image content blocks by default
(`acp.stdio.vision_enabled: true`). The provider and model must also support
vision for image prompts to succeed.

### Vision-capable configurations

| Provider | Vision models                                                          |
| -------- | ---------------------------------------------------------------------- |
| Ollama   | `granite3.2-vision:2b`, `llava:7b`, and other multimodal Ollama models |
| OpenAI   | `gpt-4o`, `gpt-4-turbo`                                                |
| Copilot  | Model-dependent; check current Copilot model availability              |

If you use a text-only model, disable vision to get a clear error instead of a
provider-level failure:

```yaml
acp:
  stdio:
    vision_enabled: false
```

## Troubleshooting

### XZatoma does not appear in Zed

Check that:

- the `xzatoma` binary is on your PATH (run `which xzatoma` or `where xzatoma`)
- the Zed settings JSON is valid (no trailing commas, correct braces)
- Zed has been restarted after editing the settings

### Authentication errors

For Copilot, re-authenticate:

```sh
xzatoma auth --provider copilot
```

For Ollama, verify the server is running:

```sh
curl http://localhost:11434/api/tags
```

For OpenAI-compatible providers, verify the `OPENAI_API_KEY` environment
variable or the `api_key` field in your config.

### Ollama connection refused

XZatoma defaults to `http://localhost:11434` for Ollama. If your Ollama server
runs on a different host or port, set this in your config:

```yaml
provider:
  provider_type: ollama
  ollama_base_url: "http://127.0.0.1:11434"
```

### Corrupted stdout / broken JSON-RPC

If Zed shows JSON parse errors or the connection breaks immediately:

1. Check that no shell profile (`.bashrc`, `.zshrc`, `.profile`) prints output
   to stdout unconditionally. Banners, `echo` statements, and `fortune` calls
   can corrupt the stdio stream before XZatoma starts.
2. Run `xzatoma agent` directly in a terminal to see what goes to stdout and
   stderr. Valid output on stdout is newline-delimited JSON only.

### Session resume not working

Workspace resume requires `acp.stdio.persist_sessions: true` and
`acp.stdio.resume_by_workspace: true` in your config. Verify the SQLite storage
path is writable by the XZatoma process.

### Queue backpressure

If Zed reports that a prompt could not be queued, the session's prompt queue is
full. The default queue capacity is 8. If your workflow submits many prompts in
rapid succession, increase the capacity:

```yaml
acp:
  stdio:
    prompt_queue_capacity: 16
```

### Unsupported vision model

If an image prompt fails with a provider error, the model does not support
vision. Either switch to a vision-capable model or disable vision:

```yaml
acp:
  stdio:
    vision_enabled: false
```

## Related documentation

- `docs/reference/acp_configuration.md` -- full `acp.stdio` field reference
- `docs/explanation/zed_acp_agent_command_implementation.md` -- implementation
  overview
- `docs/how-to/configure_providers.md` -- provider setup instructions
- `demos/zed_acp/README.md` -- self-contained demo with example prompts
