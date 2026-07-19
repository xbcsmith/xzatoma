# Chat Commands Reference

## Overview

XZatoma exposes a set of slash commands in interactive chat sessions. All
commands follow a unified UX contract described below. This reference covers all
thirteen commands, their arguments, and any ACP-specific notes.

| Command      | Description                                  | Bare behavior            | Status behavior                    | Example action             |
| ------------ | -------------------------------------------- | ------------------------ | ---------------------------------- | -------------------------- |
| `/mode`      | Switch the operation mode                    | Shows mode help          | Shows current mode and description | `/mode write`              |
| `/model`     | Switch the active AI model                   | Shows model help         | Shows current model and provider   | `/model granite4:3b`       |
| `/safety`    | Change the safety confirmation policy        | Shows safety help        | Shows current safety policy        | `/safety off`              |
| `/tools`     | List available agent tools                   | Lists tools              | n/a                                | n/a                        |
| `/context`   | Inspect or manage context window usage       | Shows token stats        | n/a                                | `/context summary`         |
| `/summarize` | Summarize the current conversation           | Summarizes conversation  | n/a                                | n/a                        |
| `/skills`    | List active agent skills                     | Lists skills             | n/a                                | n/a                        |
| `/mcp`       | List connected MCP servers                   | Lists MCP servers        | n/a                                | n/a                        |
| `/help`      | Show the global command list                 | Shows command list       | n/a                                | n/a                        |
| `/status`    | Show mode, safety, model, and subagent state | Shows full status        | n/a                                | n/a                        |
| `/subagents` | Toggle subagent delegation                   | Shows subagents help     | Shows enabled or disabled          | `/subagents on`            |
| `/system`    | Inspect or replace the active system prompt  | Shows system prompt help | Shows current system prompt        | `/system You are concise.` |
| `/streaming` | Show streaming information                   | Shows streaming help     | n/a (ACP read-only)                | n/a                        |

## Unified UX Contract

Every command follows this three-tier contract:

- **Bare command** — typing a command with no arguments (for example, `/mode`)
  prints per-command help text. This is the discovery path: you can always find
  out what a command does by running it without arguments.
- **`/<command> status`** — prints the current live value for the setting
  controlled by that command. Not every command supports a status subcommand;
  see the table above and the section below.
- **`/<command> <action>`** — applies a change. The accepted values are
  command-specific and described in each section below.

## Command Reference

### /mode

**Purpose:** Show or switch the XZatoma operation mode. Available modes are
`planning` (structured, multi-step reasoning) and `write` (direct prose output).

**Usage:**

```text
/mode
/mode status
/mode planning
/mode write
```

**Arguments:**

- `status` — print the current mode and a short description
- `planning` — switch to planning mode
- `write` — switch to write mode

**ACP notes:** None. Mode switching works the same in ACP (Zed) and CLI modes.

---

### /model

**Purpose:** Show or switch the active AI model used by the current provider.

**Usage:**

```text
/model
/model status
/model <model_name>
```

**Arguments:**

- `status` — print the current model name and the active provider
- `<model_name>` — any model identifier accepted by the active provider, for
  example `granite4:3b` (Ollama) or `gpt-4o` (OpenAI-compatible)

**ACP notes:** None. Model switching works the same in ACP and CLI modes.

---

### /safety

**Purpose:** Show or change the safety confirmation policy that governs whether
dangerous operations require explicit user approval.

**Usage:**

```text
/safety
/safety status
/safety on
/safety off
```

**Arguments:**

- `status` — print the current safety policy
- `on` — enable safety confirmations
- `off` — disable safety confirmations

**ACP notes:** None.

---

### /tools

**Purpose:** List the file and terminal tools currently available to the agent.

**Usage:**

```text
/tools
```

**Arguments:** None. `/tools status` is not supported; bare `/tools` always
prints the full tool list.

**ACP notes:** None.

---

### /context

**Purpose:** Inspect token usage for the current context window or summarize the
conversation to reclaim context space.

**Usage:**

```text
/context
/context info
/context summary
```

**Arguments:**

- `info` — print token counts for the current conversation (used, available, and
  maximum)
- `summary` — ask the model to summarize the conversation and reset the context
  window to the summary

**ACP notes:** None.

---

### /summarize

**Purpose:** Request an immediate summarization of the current conversation.
Equivalent to `/context summary` but available as a standalone command.

**Usage:**

```text
/summarize
```

**Arguments:** None.

**ACP notes:** None.

---

### /skills

**Purpose:** List the agent skills that are currently active for this session.

**Usage:**

```text
/skills
```

**Arguments:** None. `/skills status` is not supported.

**ACP notes:** None.

---

### /mcp

**Purpose:** List the MCP (Model Context Protocol) servers that are currently
connected.

**Usage:**

```text
/mcp
```

**Arguments:** None. `/mcp status` is not supported.

**ACP notes:** None.

---

### /help

**Purpose:** Print the global command list with a one-line description of each
command.

**Usage:**

```text
/help
```

**Arguments:** None. `/help status` is not supported.

**ACP notes:** In Zed ACP mode, the Zed completion menu also surfaces commands
through its autocomplete mechanism. `/help` remains available as an in-chat
fallback.

---

### /status

**Purpose:** Show a combined status snapshot: current mode, safety policy,
active model, and subagent delegation state. Use this as a quick health check
when you want all settings at once instead of running each command individually.

**Usage:**

```text
/status
```

**Arguments:** None. `/status` itself is the status command; there is no
`/status status` subcommand.

**ACP notes:** None.

---

### /subagents

**Purpose:** Show or toggle subagent delegation. When enabled, the agent may
spawn or delegate to subagent processes; when disabled, all work is done in the
primary agent loop.

**Usage:**

```text
/subagents
/subagents status
/subagents on
/subagents off
```

**Arguments:**

- `status` — print whether subagent delegation is currently enabled or disabled
- `on` — enable subagent delegation
- `off` — disable subagent delegation

**ACP notes:** None.

---

### /system

**Purpose:** Show, inspect, or replace the active system prompt. The system
prompt is prepended to every conversation turn and shapes agent behavior.

**Usage:**

```text
/system
/system status
/system <new prompt text>
```

**Arguments:**

- `status` — print the current system prompt, or report that no system prompt is
  active
- `<new prompt text>` — any non-empty string replaces the current system prompt
  immediately

**ACP notes:** None.

---

### /streaming

**Purpose:** Show information about the streaming configuration for the current
session.

**Usage:**

```text
/streaming
/streaming status
```

**Arguments:**

- `status` — displays the ACP note described below

**ACP notes:** In ACP mode (Zed), streaming is controlled by the Zed client and
cannot be toggled from within the chat session. The `/streaming` command and
`/streaming status` both report this constraint rather than offering an on/off
toggle. The following commands are also not supported in ACP mode:

- `/auth` — authentication is managed outside the ACP session
- `/exit` — use the Zed UI to close the chat session

## Commands Without a Status Subcommand

The following commands perform their entire function when run bare. They do not
accept a `status` argument:

| Command      | Reason                                                                |
| ------------ | --------------------------------------------------------------------- |
| `/tools`     | Always lists all tools; no single current value to inspect            |
| `/context`   | Bare shows token stats; use `info` or `summary` for specific actions  |
| `/summarize` | Performs an action; there is no summarization state to inspect        |
| `/skills`    | Always lists all active skills; no on/off toggle                      |
| `/mcp`       | Always lists connected servers; no single value to inspect            |
| `/help`      | Always shows the global list; there is no help state                  |
| `/status`    | Is itself a status command; no nested status subcommand               |
| `/streaming` | ACP read-only; status prints the ACP note rather than a mutable value |
