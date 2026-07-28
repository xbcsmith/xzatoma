# System Prompt Reference

## Overview

The system prompt is the first instruction message sent to the AI provider at
the start of every session. It sets the model's persona, constraints, and tone
before any task content is delivered. XZatoma supports five independent
configuration channels for the system prompt, each with a defined precedence so
that values supplied closer to execution always win over values set further
away.

This document covers every configuration channel, the full precedence order,
validation rules, the interactive `/system` command, and trace-logging
behaviour.

## Precedence

The table below lists all channels from highest to lowest priority. The first
channel that supplies a non-blank value wins; all lower-priority channels are
ignored for that session.

| Priority | Channel                            | Applies to                          |
| -------- | ---------------------------------- | ----------------------------------- |
| 1        | Plan file `system_prompt` field    | `run`, `watch`                      |
| 2        | `--system-prompt` CLI flag         | all modes                           |
| 3        | `XZATOMA_SYSTEM_PROMPT` env var    | all modes                           |
| 4        | `acp.system_prompt` config field   | `agent`, `acp serve` (ACP contexts) |
| 5        | `agent.system_prompt` config field | global fallback                     |

In ACP contexts (`agent` and `acp serve`), `acp.system_prompt` takes precedence
over `agent.system_prompt` when both are set in the config file.

## Configuration File Fields

Both fields are optional. When absent they default to `None` and are omitted
from serialised output. A blank (whitespace-only) value is rejected at startup
with a config validation error.

```yaml
agent:
  system_prompt: "You are a helpful assistant."

acp:
  system_prompt: "You are an ACP assistant."
```

- `agent.system_prompt` — global fallback used by `chat`, `run`, `watch`, and
  `agent` modes.
- `acp.system_prompt` — ACP-specific override; takes precedence over
  `agent.system_prompt` in `agent` and `acp serve` modes.

## Environment Variable

`XZATOMA_SYSTEM_PROMPT` is read during config load and writes its value into
both `config.agent.system_prompt` and `config.acp.system_prompt`. Any subsequent
`--system-prompt` CLI flag overrides both.

```bash
export XZATOMA_SYSTEM_PROMPT="You are a senior Rust engineer."
xzatoma chat
```

The env var applies to all modes:

```bash
XZATOMA_SYSTEM_PROMPT="You are a code reviewer." xzatoma run --plan plan.yaml
XZATOMA_SYSTEM_PROMPT="You are a deployment assistant." xzatoma watch
```

## CLI Flag

`--system-prompt` is available on every LLM-facing subcommand. It overrides the
environment variable and both config file fields for that invocation.

```bash
xzatoma chat      --system-prompt "You are a concise assistant."
xzatoma run       --system-prompt "You are a refactoring expert."
xzatoma agent     --system-prompt "You are a deployment agent."
xzatoma watch     --system-prompt "You are an event processor."
xzatoma acp serve --system-prompt "You are an ACP service agent."
```

A whitespace-only value passed via the CLI is treated as absent and does not
override lower-priority channels.

## Plan File Field

A plan file can embed a `system_prompt` field at the top level. This value
applies only to that single plan execution and takes the highest precedence of
all channels.

```yaml
name: code-review
system_prompt: "You are a code review bot. Focus on correctness."
steps:
  - name: Review
    action: Review all .rs files in src/
```

- Available in `run` and `watch` modes.
- Overrides `--system-prompt`, the env var, and both config file fields.
- A blank value is rejected during plan parsing.

## Interactive `/system` Command

In `xzatoma chat` interactive mode and in Zed (ACP mode), the `/system` command
has three forms:

| Form             | Behaviour                                       |
| ---------------- | ----------------------------------------------- |
| `/system` (bare) | Shows help text for the `/system` command       |
| `/system status` | Displays the current active system prompt       |
| `/system <text>` | Replaces the active system prompt with `<text>` |

### Replacing the system prompt

```text
/system You are a strict code reviewer. Reject anything without tests.
System prompt updated.
```

Behaviour:

- Replaces the first system message in the active conversation with `<text>`.
- If no system message exists, prepends one.
- Skill disclosure messages are left untouched.
- Produces `System prompt updated.` on success.

### Inspecting the current system prompt

```text
/system status
Current system prompt:
You are a strict code reviewer. Reject anything without tests.
```

When no system prompt is active:

```text
/system status
No system prompt is active for this session.
```

### Bare invocation

A bare `/system` (no text, no subcommand) now shows the help text for the
`/system` command instead of returning an error. This matches the unified UX
contract: bare command = help, `status` = inspect, text argument = change.

## Injection Order in Agent Conversations

Regardless of which configuration channel supplied the system prompt, messages
are injected into every agent conversation in this order:

1. User-defined system prompt (from any channel above).
2. Skill disclosure message (if active skills are loaded).
3. Mode-specific base prompt (transient, per-call; not stored in conversation
   history).
4. Active skill prompt injection (transient, per-call).

Items 3 and 4 are appended at call time and are never persisted to the
conversation log.

## Trace Logging

When `--trace` (or `XZATOMA_TRACE=true`) is active, the resolved system prompt
and its source are logged once at session start:

```text
TRACE source="CliFlag" system_prompt="You are a pirate" "Session system prompt"
```

Possible `source` values:

| Value         | Meaning                                           |
| ------------- | ------------------------------------------------- |
| `PlanFile`    | Resolved from the plan file `system_prompt` field |
| `CliFlag`     | Resolved from `--system-prompt`                   |
| `EnvVar`      | Resolved from `XZATOMA_SYSTEM_PROMPT`             |
| `AcpConfig`   | Resolved from `acp.system_prompt`                 |
| `AgentConfig` | Resolved from `agent.system_prompt`               |

At normal log levels (info or debug), only the prompt length is recorded, not
the content, to avoid leaking sensitive instructions into logs.

## Validation Rules

| Channel                    | Blank value behaviour                           |
| -------------------------- | ----------------------------------------------- |
| `agent.system_prompt`      | Rejected at startup with a config error         |
| `acp.system_prompt`        | Rejected at startup with a config error         |
| Plan file `system_prompt`  | Rejected during plan parsing with a parse error |
| `--system-prompt` CLI flag | Treated as absent; no override applied          |
| `XZATOMA_SYSTEM_PROMPT`    | Rejected at startup with a config error         |

"Blank" means an empty string or a string containing only whitespace.

## Related Documentation

- [Configuration Reference](configuration.md) — full `agent` and `acp` config
  sections
- [CLI Reference](cli.md) — `--system-prompt` flag placement per subcommand
- [Workflow Format Reference](workflow_format.md) — plan file schema including
  the `system_prompt` field
- [Logging Reference](logging.md) — `--trace` flag and `XZATOMA_TRACE` env var
