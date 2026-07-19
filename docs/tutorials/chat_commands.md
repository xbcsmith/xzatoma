# Chat Commands Tutorial

## Overview

This tutorial walks you through the unified slash command UX in XZatoma's
interactive chat. By the end you will know how to discover available commands,
inspect current settings, change mode, safety, model, and system prompt, manage
context in long sessions, and understand streaming behavior in Zed ACP mode.

All model examples use `granite4:3b` running locally via Ollama.

## Prerequisites

- XZatoma built and on your `PATH` (or runnable via `cargo run --`)
- Ollama running locally with `granite4:3b` pulled:

  ```bash
  ollama pull granite4:3b
  ```

- XZatoma configured to use Ollama (see `../how-to/configure_providers.md`)

## Step 1 — Start a chat session

Open a terminal and start an interactive session:

```bash
xzatoma chat --provider ollama
```

Or if running from source:

```bash
cargo run -- chat --provider ollama
```

You will see the chat prompt:

```text
xzatoma> _
```

You are now in an interactive chat session. Type any message to converse with
the agent, or use a slash command to inspect or change session settings.

## Step 2 — Discover available commands with /help

Type `/help` to print the global command list:

```text
xzatoma> /help
```

Expected output:

```text
Available commands:

  /mode        Show help or switch the operation mode.
  /model       Show help or switch the active model.
  /safety      Show help or change the safety policy.
  /tools       List available agent tools.
  /context     Inspect or manage context window usage.
  /summarize   Summarize the current conversation.
  /skills      List active agent skills.
  /mcp         List connected MCP servers.
  /help        Show this command list.
  /status      Show mode, safety, model, and subagent state.
  /subagents   Show help or toggle subagent delegation.
  /system      Show help, inspect, or replace the system prompt.
  /streaming   Show streaming information (ACP mode).

Tip: run any command without arguments to see its help text.
     Run /<command> status to see the current value.
```

You can also run any command bare to see its own help. For example:

```text
xzatoma> /mode
```

```text
/mode — Show or switch the operation mode.

  /mode status    Show the current mode.
  /mode planning  Switch to planning mode (structured multi-step reasoning).
  /mode write     Switch to write mode (direct prose output).
```

## Step 3 — Check current settings with the status subcommand

Several commands accept `status` as a subcommand. Use this to inspect live
values without changing anything.

### Check the current mode

```text
xzatoma> /mode status
```

```text
Current mode: planning
Description:  Structured, multi-step reasoning mode. The agent plans before acting.
```

### Check the current safety policy

```text
xzatoma> /safety status
```

```text
Current safety policy: on
Dangerous operations require explicit confirmation before execution.
```

### Check the current model

```text
xzatoma> /model status
```

```text
Current model: granite4:3b
Provider:      ollama
```

### Check the current system prompt

```text
xzatoma> /system status
```

```text
No system prompt is active.
```

### Check subagent delegation

```text
xzatoma> /subagents status
```

```text
Subagent delegation: disabled
```

### See everything at once

```text
xzatoma> /status
```

```text
Mode:       planning
Safety:     on
Model:      granite4:3b (ollama)
Subagents:  disabled
```

## Step 4 — Change mode, safety, model, system prompt, and subagents

### Switch mode

```text
xzatoma> /mode write
```

```text
Mode switched to: write
```

Confirm the change:

```text
xzatoma> /mode status
```

```text
Current mode: write
Description:  Direct prose output mode. The agent responds without a planning phase.
```

Switch back to planning mode when you want structured reasoning:

```text
xzatoma> /mode planning
```

```text
Mode switched to: planning
```

### Disable safety

```text
xzatoma> /safety off
```

```text
Safety policy disabled. Dangerous operations will not prompt for confirmation.
```

Re-enable when you want confirmation guards back:

```text
xzatoma> /safety on
```

```text
Safety policy enabled.
```

### Switch model

```text
xzatoma> /model granite4:3b
```

```text
Model switched to: granite4:3b (ollama)
```

### Set a system prompt

```text
xzatoma> /system You are a concise assistant. Reply in one sentence.
```

```text
System prompt updated.
```

Verify the change:

```text
xzatoma> /system status
```

```text
Current system prompt:
  You are a concise assistant. Reply in one sentence.
```

To clear the system prompt, use an empty replacement or set a new one. To check
what an empty session looks like, start a new session without `--system`.

### Enable subagent delegation

```text
xzatoma> /subagents on
```

```text
Subagent delegation enabled.
```

Confirm:

```text
xzatoma> /subagents status
```

```text
Subagent delegation: enabled
```

Disable again:

```text
xzatoma> /subagents off
```

```text
Subagent delegation disabled.
```

## Step 5 — Manage long sessions with /context

In long conversations the context window fills up. Use `/context` to monitor
usage and summarize when needed.

### Inspect token usage

```text
xzatoma> /context info
```

```text
Context window usage:
  Used:      4 218 tokens
  Available: 123 782 tokens
  Maximum:   128 000 tokens
```

When the used token count approaches the maximum, summarize the conversation to
reclaim space.

### Summarize and reset the context window

```text
xzatoma> /context summary
```

```text
Conversation summarized. Context window reset.
```

The conversation history is replaced by a compact summary. You can verify by
running `/context info` again and observing that the used token count has
dropped.

You can also trigger summarization directly:

```text
xzatoma> /summarize
```

```text
Conversation summarized. Context window reset.
```

Both `/context summary` and `/summarize` produce the same result.

## Step 6 — Understand /streaming in Zed ACP mode

When XZatoma runs as an ACP server inside Zed, streaming is controlled by the
Zed client. You cannot toggle streaming from the chat session.

Running `/streaming` in Zed ACP mode:

```text
xzatoma> /streaming
```

```text
Streaming information:

  You are running in ACP mode (Zed).
  Streaming is controlled by the Zed client and cannot be changed from
  within this chat session.

  Use /streaming status for the same information.
```

Running `/streaming status` produces the same output:

```text
xzatoma> /streaming status
```

```text
Streaming information:

  You are running in ACP mode (Zed).
  Streaming is controlled by the Zed client and cannot be changed from
  within this chat session.
```

The following commands are also not available in ACP mode:

- `/auth` — use the Zed UI or your shell environment to manage credentials
- `/exit` — close the Zed chat panel to end the session

In CLI mode (not Zed), streaming behavior is configured via the provider
configuration file. See `../reference/configuration.md` for details.

## Step 7 — Exit the session

In CLI mode, exit the chat loop by typing `exit` or pressing `Ctrl-D`:

```text
xzatoma> exit
```

```text
Goodbye.
```

In Zed ACP mode, close the Zed chat panel. Do not use `/exit`; that command is
not available in ACP mode.

## Summary

| Goal                        | Command                                |
| --------------------------- | -------------------------------------- |
| List all commands           | `/help`                                |
| See current mode            | `/mode status`                         |
| Switch to write mode        | `/mode write`                          |
| See current model           | `/model status`                        |
| Switch model                | `/model granite4:3b`                   |
| See current safety policy   | `/safety status`                       |
| Disable safety              | `/safety off`                          |
| See current system prompt   | `/system status`                       |
| Set system prompt           | `/system You are a concise assistant.` |
| See subagent state          | `/subagents status`                    |
| Enable subagents            | `/subagents on`                        |
| Check context usage         | `/context info`                        |
| Summarize and reset context | `/context summary`                     |
| Full settings snapshot      | `/status`                              |
| Understand streaming in Zed | `/streaming` or `/streaming status`    |

## Next steps

- Full command reference: `../reference/chat_commands.md`
- How to configure providers: `../how-to/configure_providers.md`
- How to manage context window: `../how-to/manage_context_window.md`
- How to use chat modes: `../how-to/use_chat_modes.md`
- How to use subagents in chat: `../how-to/use_subagents_in_chat.md`
