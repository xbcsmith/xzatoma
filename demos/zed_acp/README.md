# Zed ACP Demo

This demo shows how to configure Zed to use XZatoma as an ACP stdio agent
subprocess. It includes provider-specific configuration files, a safe fixture
workspace, and example prompts for text and vision scenarios.

## Prerequisites

1. Build XZatoma:

   ```sh
   cargo build --release
   cp target/release/xzatoma ~/.cargo/bin/xzatoma
   ```

2. Configure at least one provider. This demo includes example config files for
   Copilot, Ollama, and OpenAI-compatible providers. See
   `docs/how-to/configure_providers.md` for general setup instructions.

3. For Ollama-based usage, start Ollama and pull a model:

   ```sh
   ollama serve
   ollama pull granite4:3b
   ```

   For vision prompts with Ollama, also pull a vision model:

   ```sh
   ollama pull granite3.2-vision:2b
   ```

4. Install Zed and ensure it supports custom ACP agent servers.

## Demo directory layout

```text
zed_acp/
  README.md                    This walkthrough
  config.yaml                  Default configuration (Ollama, text-only)
  config_copilot.yaml          Copilot provider configuration
  config_ollama_vision.yaml    Ollama with vision support enabled
  config_openai.yaml           OpenAI-compatible provider configuration
  input/
    text_prompts.txt           Example prompts for text-only usage
    vision_prompts.txt         Example prompts for image analysis
    fixture/                   Safe fixture workspace for tool testing
      hello.txt                A simple text fixture file
      sample.rs                A simple Rust snippet for analysis
  tmp/
    .gitignore                 Excludes generated output from version control
    output/                    Demo output artifacts are written here
```

## Step 1: Set up demo files

Run the setup script to create the `tmp/` directory structure:

```sh
cd demos/zed_acp
bash setup.sh
```

## Step 2: Configure Zed

Add XZatoma to your Zed settings. Open `~/.config/zed/settings.json` and add:

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

To use a specific configuration file for the demo:

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

## Step 3: Try text prompts

Open Zed with this demo directory as the workspace. In the agent panel, try the
prompts from `input/text_prompts.txt`:

- Ask XZatoma to list the files in `input/fixture/`
- Ask XZatoma to read `input/fixture/hello.txt` and summarize it
- Ask XZatoma to read `input/fixture/sample.rs` and describe what it does

All tool output goes to `tmp/output/` to keep the fixture workspace clean.

## Step 4: Try vision prompts (requires a vision model)

Vision prompts require a model that supports image input. See
`docs/how-to/zed_acp_agent_setup.md` for provider-specific setup.

Use the prompts from `input/vision_prompts.txt` to test image understanding
through the Zed agent panel.

## Step 5: Reset

To remove all generated state and start fresh:

```sh
cd demos/zed_acp
bash reset.sh
```

## Configuration files

| File                        | Provider                        | Vision          |
| --------------------------- | ------------------------------- | --------------- |
| `config.yaml`               | Ollama (`granite4:3b`)          | Disabled        |
| `config_copilot.yaml`       | GitHub Copilot                  | Model-dependent |
| `config_ollama_vision.yaml` | Ollama (`granite3.2-vision:2b`) | Enabled         |
| `config_openai.yaml`        | OpenAI (`gpt-4o`)               | Enabled         |

## Notes

- stdout is reserved for JSON-RPC. Do not add output to stdout in agent mode.
- Session state is persisted to `~/.local/share/xzatoma/` by default. Each
  workspace gets its own conversation history.
- The fixture workspace in `input/fixture/` is read-only by convention. All
  agent-generated output goes to `tmp/output/`.

## Related documentation

- `docs/how-to/zed_acp_agent_setup.md` -- detailed Zed setup guide
- `docs/reference/acp_configuration.md` -- full `acp.stdio` field reference
- `docs/explanation/zed_acp_agent_command_implementation.md` -- implementation
  overview
