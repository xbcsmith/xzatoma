# Configure Zed to use XZatoma as an ACP agent

This guide shows you how to configure the Zed editor to launch XZatoma as a
custom External Agent using the Agent Client Protocol (ACP). XZatoma will appear
in Zed's agent panel and can receive text and vision prompts from the active
workspace.

For Zed's official documentation on External Agents, see
[Zed External Agents](https://zed.dev/docs/ai/external-agents).

## Before you begin

You need:

- Zed installed and updated to a version that supports External Agents and the
  ACP Registry
- XZatoma built from source (`cargo build --release`) or installed via
  `cargo install --git https://github.com/xbcsmith/xzatoma`
- At least one working provider configured in your XZatoma config file. See
  `docs/how-to/configure_providers.md` for setup instructions.
- The `xzatoma` binary on your system `PATH`, or the full path to the binary
  ready to use in the Zed settings JSON

Confirm XZatoma is on your PATH:

```bash
xzatoma --version
```

## Important: stdout is reserved for JSON-RPC

When running in agent mode, XZatoma writes all JSON-RPC protocol traffic to
stdout. Any non-JSON bytes on stdout will corrupt the protocol stream and break
the Zed connection. XZatoma forces all tracing, logging, and diagnostic output
to stderr automatically in agent mode. Do not set environment variables that
write additional output to stdout.

## Logging verbosity in Zed

Zed launches XZatoma as a subprocess and passes environment variables from the
`env` block in your agent server configuration. Because Zed provides env vars
rather than CLI arguments, use `RUST_LOG` to control log verbosity instead of
the `--debug` or `--trace` CLI flags:

```json
{
  "agent_servers": {
    "xzatoma": {
      "type": "custom",
      "command": "xzatoma",
      "args": ["agent"],
      "env": {
        "RUST_LOG": "xzatoma=debug"
      }
    }
  }
}
```

`RUST_LOG=xzatoma=debug` is equivalent to passing `--debug` on the CLI.
`RUST_LOG=xzatoma=trace` is equivalent to passing `--trace` on the CLI.

For targeted module-level filtering, use the standard `RUST_LOG` module syntax:

```json
{
  "agent_servers": {
    "xzatoma": {
      "type": "custom",
      "command": "xzatoma",
      "args": ["agent"],
      "env": {
        "RUST_LOG": "xzatoma::acp=debug,xzatoma::agent=trace"
      }
    }
  }
}
```

When `RUST_LOG` is set explicitly it takes precedence over any `--debug` or
`--trace` flag that might be present in the `args` array.

### File logging

Write a second log stream to a file while XZatoma runs as a Zed agent. The file
is always written in JSON (NDJSON) format and is opened in append mode, so log
lines accumulate across session restarts.

Create the log directory once before first use:

```bash
mkdir -p ~/.local/xzatoma
```

**Testing outside Zed (terminal):**

```bash
# Debug level: provider round-trips, tool execution, iteration counts
xzatoma agent --debug --logfile ~/.local/xzatoma/agent.log

# Trace level: full conversation transcript, tool arguments and results
xzatoma agent --trace --logfile ~/.local/xzatoma/agent.log
```

**Inside Zed via `agent_servers`:**

Zed does not perform shell expansion on values in the `args` array, so `~` is
not resolved. Use the `XZATOMA_LOG_FILE` env var with the full absolute path
instead. Replace `/Users/yourname` with your actual home directory.

Debug level:

```json
{
  "agent_servers": {
    "xzatoma": {
      "type": "custom",
      "command": "xzatoma",
      "args": ["agent", "--debug"],
      "env": {
        "XZATOMA_LOG_FILE": "/Users/yourname/.local/xzatoma/agent.log"
      }
    }
  }
}
```

Trace level:

```json
{
  "agent_servers": {
    "xzatoma": {
      "type": "custom",
      "command": "xzatoma",
      "args": ["agent", "--trace"],
      "env": {
        "XZATOMA_LOG_FILE": "/Users/yourname/.local/xzatoma/agent.log"
      }
    }
  }
}
```

After a few prompt turns, inspect the log:

```bash
# Follow in real time with jq pretty-printing
tail -f ~/.local/xzatoma/agent.log | jq .

# Show only ERROR and WARN events
jq 'select(.level == "ERROR" or .level == "WARN")' ~/.local/xzatoma/agent.log
```

See `docs/reference/logging.md` for the full logging reference.

## Step 1: Add XZatoma as a custom agent

XZatoma is not in the ACP Registry, so add it as a custom agent.

### Option A: Use the Zed settings UI (recommended)

1. Open the Command Palette and run `agent: open settings`.
2. Go to the **External Agents** page.
3. Click **Add Agent** and choose **Add Custom Agent**.
4. Zed opens your settings file with a pre-filled `agent_servers` entry. Replace
   the placeholder values with:

```json
{
  "agent_servers": {
    "xzatoma": {
      "type": "custom",
      "command": "xzatoma",
      "args": ["agent"],
      "env": {}
    }
  }
}
```

### Option B: Edit settings.json directly

Open your Zed settings file (`~/.config/zed/settings.json` on macOS and Linux,
`%AppData%\Zed\settings.json` on Windows) and add or update the `agent_servers`
object:

```json
{
  "agent_servers": {
    "xzatoma": {
      "type": "custom",
      "command": "xzatoma",
      "args": ["agent"],
      "env": {}
    }
  }
}
```

If `xzatoma` is not on your PATH, replace `"xzatoma"` in `"command"` with the
full path to the binary:

```json
{
  "agent_servers": {
    "xzatoma": {
      "type": "custom",
      "command": "/home/yourname/.cargo/bin/xzatoma",
      "args": ["agent"],
      "env": {}
    }
  }
}
```

After saving the settings file, restart Zed or reload the window. XZatoma should
appear in the new-thread menu in the Agent Panel and Threads Sidebar.

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
  "agent_servers": {
    "xzatoma": {
      "type": "custom",
      "command": "xzatoma",
      "args": ["agent", "--provider", "copilot"],
      "env": {}
    }
  }
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
  "agent_servers": {
    "xzatoma": {
      "type": "custom",
      "command": "xzatoma",
      "args": ["agent", "--provider", "ollama", "--model", "granite4:3b"],
      "env": {}
    }
  }
}
```

### OpenAI-compatible providers

For providers that expose an OpenAI-compatible API (including local inference
servers), set the provider to `openai` and supply the relevant environment
variables:

```json
{
  "agent_servers": {
    "xzatoma": {
      "type": "custom",
      "command": "xzatoma",
      "args": ["agent", "--provider", "openai", "--model", "gpt-4o"],
      "env": {
        "OPENAI_API_KEY": "sk-..."
      }
    }
  }
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

## Configuration boundaries

XZatoma runs as a separate process that communicates with Zed over ACP. This
creates a clear boundary between Zed configuration and XZatoma configuration.

| Capability           | Who owns it                                                   |
| -------------------- | ------------------------------------------------------------- |
| Model and provider   | XZatoma (via config file or `--provider`/`--model` CLI flags) |
| Auth and API keys    | XZatoma (via `xzatoma auth` or environment variables)         |
| Zed Agent profiles   | Zed only; do not apply to XZatoma threads                     |
| Zed Skills           | Zed only; do not apply as Zed Skills in XZatoma threads       |
| XZatoma agent skills | XZatoma; configured in the XZatoma skills directory           |
| Zed MCP servers      | May be forwarded to XZatoma over ACP (see MCP section below)  |
| Tool permissions     | XZatoma manages its own tool registry per session mode        |

## Session Mode Selector

XZatoma advertises four session modes to Zed. Use the mode selector dropdown in
the Zed agent panel to switch between them without restarting the session.

| Mode ID         | Display Name    | File writes | Terminal     | Confirmations |
| --------------- | --------------- | ----------- | ------------ | ------------- |
| planning        | Planning        | No          | None         | Always        |
| write           | Write           | Yes         | Safe only    | Always        |
| safe            | Safe            | Yes         | Safe only    | Always (Zed)  |
| full_autonomous | Full Autonomous | Yes         | Unrestricted | Never         |

### Planning

Read-only analysis mode. No file writes or destructive terminal commands are
permitted. Use this mode to explore, research, and plan work before making
changes. This is the default mode.

### Write

File editing and safe terminal execution are allowed. Dangerous terminal
operations require confirmation before proceeding. Use this mode for day-to-day
coding tasks.

### Safe

Write-capable mode with Zed user approval required for any risky action. All
potentially destructive operations trigger a confirmation prompt in Zed. Use
this mode when you want explicit control over every destructive operation.

### Full Autonomous

Unrestricted write and terminal access within configured resource limits. No
confirmations are requested. Use with care, and only in sandboxed or trusted
environments.

To switch modes, click the mode selector in the Zed agent panel header and
choose from the dropdown. The mode change takes effect immediately for the next
prompt in that session.

You can also switch modes mid-session using the `/mode` slash command, or change
the default via the `session_mode` config option. See
`docs/reference/acp_configuration.md` for details.

To confirm mode changes are applied, run XZatoma with debug logging:

```json
{
  "agent_servers": {
    "xzatoma": {
      "type": "custom",
      "command": "xzatoma",
      "args": ["agent"],
      "env": {
        "RUST_LOG": "xzatoma::acp=debug"
      }
    }
  }
}
```

Mode changes emit `ConfigOptionUpdate` and `CurrentModeUpdate` notifications
visible in the debug log.

## Session Config Dropdowns

XZatoma advertises seven session config dropdowns to Zed. They appear in the
agent panel toolbar in this order (left to right):

| Position | Dropdown            | What it controls                                                     | Default          |
| -------- | ------------------- | -------------------------------------------------------------------- | ---------------- |
| 1        | Thinking Effort     | Reasoning depth for extended-thinking models                         | `none` (config)  |
| 2        | Tool Routing        | Whether tools run via the IDE or locally                             | `prefer_ide`     |
| 3        | Subagent Delegation | Whether XZatoma can spawn subagent workers                           | from config      |
| 4        | MCP Tools           | Whether MCP server tools are active this session                     | from config      |
| 5        | Safety Policy       | When to ask for confirmation before risky actions                    | `always_confirm` |
| 6        | Session Mode        | Overall capability level (planning / write / safe / full_autonomous) | `planning`       |
| 7        | Model               | Active AI model (fetched from provider at startup)                   | from provider    |

All dropdowns take effect immediately for the next prompt. They can also be
changed via slash commands (`/mode`, `/model`, `/safety`, `/subagents`).

### Thinking Effort

Controls how much reasoning the model performs per turn. The default at session
start is set by `acp.stdio.default_thinking_effort` in your config file (default
`"none"`). Set it to `"medium"` or higher to enable reasoning on models that
support extended thinking (for example `deepseek-r1` via Ollama or `o3-mini` via
OpenAI-compatible providers). Has no effect on models that do not support
extended thinking.

```yaml
acp:
  stdio:
    default_thinking_effort: "medium"
```

### Streaming

Response streaming in ACP stdio mode is controlled by the Zed client, not by
XZatoma. To signal that XZatoma prefers streaming responses, set
`acp.default_run_mode` to `streaming` in your config:

```yaml
acp:
  default_run_mode: streaming
```

See `docs/reference/acp_configuration.md` for the full list of advertised
session config options and their accepted values.

## Context Window Bar

XZatoma reports token usage to Zed so the context window bar in the agent panel
stays current. Two separate mechanisms keep the bar updated:

1. After every completed prompt turn, XZatoma sends a `UsageUpdate` notification
   with the current token count and maximum context window size.
2. `PromptResponse.usage` is populated with the same counts in the response
   payload.

The bar shows `used / max` tokens. When the bar is nearly full, consider
starting a new session or reducing the context with `/context summary`.

### What counts as context

XZatoma uses a two-tier approach for token counting:

- Provider-reported usage: when the provider returns token counts in its
  response (for example, the OpenAI `usage` field), those counts are used
  directly.
- Heuristic fallback: when the provider does not return usage, XZatoma uses an
  internal character-based estimate from the conversation history.

The `total_tokens` figure in the context bar is the most reliable field. The
`input_tokens` value is set equal to `total_tokens` because XZatoma does not
split per-turn input versus output counts without provider-level token
accounting. `output_tokens` is reported as zero.

### Interpreting the bar

| Bar fill | Meaning                                                         |
| -------- | --------------------------------------------------------------- |
| < 50%    | Context window is comfortable. Normal operation.                |
| 50-80%   | Context is filling. Consider pruning or starting a new session. |
| > 80%    | Context is nearly full. Long prompts may be truncated or fail.  |

### Debugging context window updates

To confirm `UsageUpdate` notifications are being sent, run with debug logging
and check for log lines containing
`"ACP stdio: sending initial context window usage update"` at session creation
and `"post-turn usage update"` after each prompt.

## MCP server forwarding

Zed-configured MCP servers may be forwarded to XZatoma over ACP. XZatoma may
also read its own native MCP configuration. If an MCP tool does not appear in an
XZatoma thread, check both Zed's MCP server configuration and the `mcp_servers`
section of the XZatoma config file.

See `docs/reference/mcp_configuration.md` for XZatoma-native MCP setup.

## Thread import

Zed can import existing XZatoma threads into your Thread History. Open the
Threads Sidebar, click the clock icon at the bottom to open Thread History, then
click **Import Threads** and select XZatoma. Sessions without an associated
working directory are skipped; re-importing is safe because existing threads are
not duplicated.

## Troubleshooting

### Inspect ACP messages

Run `dev: open acp logs` from the Command Palette to inspect the raw messages
exchanged between Zed and XZatoma. Include the ACP log output when reporting
issues.

### XZatoma does not appear in Zed

Check that:

- The `xzatoma` binary is on your PATH (run `which xzatoma` or `where xzatoma`).
- The `agent_servers` entry uses the object format with `"type": "custom"` (not
  the old array format).
- The Zed settings JSON is valid (no trailing commas, correct braces). Open the
  Command Palette and run `zed: open settings` to verify the file opens without
  a parse error.
- Zed has been restarted after editing the settings.

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
3. Open the ACP log with `dev: open acp logs` to see the raw message exchange.

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
- `docs/explanation/acp_features_implementation.md` -- implementation overview
- `docs/how-to/configure_providers.md` -- provider setup instructions
- `docs/reference/mcp_configuration.md` -- MCP server configuration reference
- `demos/zed_acp/README.md` -- self-contained demo with example prompts
