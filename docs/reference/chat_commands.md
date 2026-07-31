# Chat Commands Reference

## Overview

XZatoma exposes a set of slash commands in interactive chat sessions. All
commands follow a unified UX contract described below. This reference covers all
fifteen commands, their arguments, aliases, and any ACP-specific notes.

| Command      | Description                                  | Bare behavior            | Status behavior                    | Example action             |
| ------------ | -------------------------------------------- | ------------------------ | ---------------------------------- | -------------------------- |
| `/mode`      | Switch the operation mode                    | Shows mode help          | Shows current mode and description | `/mode write`              |
| `/model`     | Switch the active AI model                   | Shows model help         | Shows current model and provider   | `/model granite4:3b`       |
| `/models`    | List models or show model info               | Shows models help        | n/a                                | `/models list`             |
| `/safety`    | Change the safety confirmation policy        | Shows safety help        | Shows current safety policy        | `/safety off`              |
| `/tools`     | List available agent tools                   | Lists tools              | n/a                                | n/a                        |
| `/context`   | Inspect or manage context window usage       | Shows token stats        | n/a                                | `/context summary`         |
| `/skills`    | List active agent skills                     | Lists skills             | n/a                                | n/a                        |
| `/mcp`       | List connected MCP servers                   | Lists MCP servers        | n/a                                | n/a                        |
| `/mentions`  | Show available @-mention references          | Lists mentions           | n/a                                | n/a                        |
| `/auth`      | Authenticate with a provider                 | Authenticates default    | n/a                                | `/auth copilot`            |
| `/help`      | Show the global command list                 | Shows command list       | n/a                                | n/a                        |
| `/status`    | Show mode, safety, model, and subagent state | Shows full status        | n/a                                | n/a                        |
| `/subagents` | Toggle subagent delegation                   | Shows subagents help     | Shows enabled or disabled          | `/subagents on`            |
| `/system`    | Inspect or replace the active system prompt  | Shows system prompt help | Shows current system prompt        | `/system You are concise.` |
| `/streaming` | Toggle or show streaming state               | Shows streaming help     | Shows current streaming state      | `/streaming on`            |

Several commands also accept short aliases: `/planning` and `/write` for
`/mode planning` and `/mode write`; `/safe` and `/yolo` for `/safety on` and
`/safety off`; and `/?` for `/help`. Typing `exit`, `quit`, `/exit`, or `/quit`
ends the session.

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
- `planning` — switch to planning mode (alias: `/planning`)
- `write` — switch to write mode (alias: `/write`)

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

### /models

**Purpose:** List the models available from the active provider or show details
for a specific model. This is distinct from `/model`, which switches the active
model.

**Usage:**

```text
/models
/models list
/models info <model_name>
```

**Arguments:**

- (bare) — print models help
- `list` — list the models available from the active provider
- `info <model_name>` — show details for the named model

**ACP notes:** None.

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
- `on` — enable safety confirmations (alias: `/safe`)
- `off` — disable safety confirmations (alias: `/yolo`)

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
/context summary --model <model_name>
/context summary -m <model_name>
```

**Arguments:**

- `info` — print token counts for the current conversation (used, available, and
  maximum). Bare `/context` is equivalent to `/context info`.
- `summary` — ask the model to summarize the conversation and reset the context
  window to the summary. Optionally pass `--model <name>` or `-m <name>` to use
  a specific model for the summary.

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

### /mentions

**Purpose:** List the available `@`-mention references that can be used to pull
files or other context into the conversation.

**Usage:**

```text
/mentions
```

**Arguments:** None. `/mentions status` is not supported.

**ACP notes:** None.

---

### /auth

**Purpose:** Authenticate with an AI provider. With no argument, authenticates
the default provider; with a provider name, authenticates that provider.

**Usage:**

```text
/auth
/auth <provider>
```

**Arguments:**

- (bare) — authenticate the default provider
- `<provider>` — authenticate the named provider (for example, `copilot`)

**ACP notes:** Not supported in ACP mode. Authentication is managed outside the
ACP session.

---

### /help

**Purpose:** Print the global command list with a one-line description of each
command.

**Usage:**

```text
/help
```

**Arguments:** None. `/help status` is not supported. Alias: `/?`.

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
- `on` — enable subagent delegation (alias: `/subagents enable`)
- `off` — disable subagent delegation (alias: `/subagents disable`)

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

**Purpose:** Show or toggle whether responses are streamed incrementally.

**Usage:**

```text
/streaming
/streaming status
/streaming on
/streaming off
```

**Arguments:**

- `status` — print the current streaming state
- `on` — enable streaming (alias: `/streaming enable`)
- `off` — disable streaming (alias: `/streaming disable`)

**ACP notes:** In ACP mode (Zed), streaming is controlled by the Zed client. The
following commands are also not supported in ACP mode:

- `/auth` — authentication is managed outside the ACP session
- `/exit` — use the Zed UI to close the chat session

## Commands Without a Status Subcommand

The following commands perform their entire function when run bare. They do not
accept a `status` argument:

| Command     | Reason                                                               |
| ----------- | -------------------------------------------------------------------- |
| `/tools`    | Always lists all tools; no single current value to inspect           |
| `/context`  | Bare shows token stats; use `info` or `summary` for specific actions |
| `/skills`   | Always lists all active skills; no on/off toggle                     |
| `/mcp`      | Always lists connected servers; no single value to inspect           |
| `/mentions` | Always lists available mentions; no single value to inspect          |
| `/help`     | Always shows the global list; there is no help state                 |
| `/status`   | Is itself a status command; no nested status subcommand              |
