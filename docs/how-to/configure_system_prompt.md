# Configure a Custom System Prompt

## Overview

This guide shows how to set a custom system prompt for each of the five
LLM-facing modes in XZatoma: `chat`, `run`, `agent`, `watch`, and `acp serve`.

## Prerequisites

- XZatoma installed and on your `PATH`.
- At least one provider configured and authenticated. See
  [Configure AI Providers](configure_providers.md).

## Three Global Configuration Channels

All five modes read the system prompt from the same three sources. The
`--system-prompt` CLI flag takes precedence over the environment variable, which
takes precedence over the config file.

**1. CLI flag** — inline, one invocation only:

```bash
xzatoma chat --system-prompt "You are a senior Rust engineer."
```

**2. Environment variable** — applies to every invocation in the current shell:

```bash
export XZATOMA_SYSTEM_PROMPT="You are a senior Rust engineer."
xzatoma chat
```

**3. Config file** — persistent default for all modes:

```yaml
# config/config.yaml
agent:
  system_prompt: "You are a senior Rust engineer."
```

The sections below cover mode-specific flags, overrides, and edge cases.

## `chat` — Interactive Mode

The `--system-prompt` flag overrides the environment variable and the config
file. Once a session is running, the `/system` command updates the prompt
without restarting.

Start a session with a custom prompt:

```bash
xzatoma chat --system-prompt "You are a helpful assistant."
```

Change the prompt mid-session at the interactive prompt:

```bash
/system You are now a concise code reviewer.
```

Output:

```text
System prompt updated.
```

**Session resume:** when you resume a previous session, the `--system-prompt`
flag always replaces any system message that was stored with that session. Pass
the flag again on resume to keep your preferred prompt active.

## `run` — Plan File or One-shot Prompt

Use `--system-prompt` for ad-hoc runs. When you execute a plan file, a
`system_prompt` field inside the plan takes the highest priority and silently
overrides the CLI flag for that run.

One-shot run with a CLI system prompt:

```bash
xzatoma run --prompt "Summarise this project." \
            --system-prompt "Be terse; reply in plain text only."
```

Plan file with an embedded system prompt:

```yaml
name: code-review
system_prompt:
  "You are a code review bot. Focus on correctness and performance."
steps:
  - name: Review
    action: Review all .rs files in src/ and list issues.
```

Run the plan:

```bash
xzatoma run --plan review.yaml
# The plan's system_prompt is used.
# Any --system-prompt flag is ignored for this run.
```

## `agent` — ACP stdio Subprocess (Zed Integration)

The `agent` command starts an ACP stdio subprocess used by Zed. The
`--system-prompt` flag writes its value into `agent.system_prompt` in the
resolved config. When the subprocess creates a session it reads
`acp.system_prompt` first and falls back to `agent.system_prompt` when the
ACP-specific field is absent.

For Zed integration, prefer the config file so the prompt persists across
invocations:

```yaml
# config/config.yaml
agent:
  system_prompt: "Act as a senior Rust engineer."
```

For a one-off override, use the CLI flag:

```bash
xzatoma agent --system-prompt "Act as a senior Rust engineer."
```

## `watch` — Kafka-triggered Watcher

The `--system-prompt` flag sets the prompt for all agent runs triggered during
that watcher invocation. A plan event whose YAML includes a `system_prompt`
field overrides the CLI or config value for that single execution only.

Start the watcher with a system prompt:

```bash
xzatoma watch --system-prompt "You are a deployment automation bot."
```

Or set it in the config file so it applies every time the watcher starts:

```yaml
# config/config.yaml
agent:
  system_prompt: "You are a deployment automation bot."
```

A triggered plan event that overrides the watcher prompt for one run:

```yaml
name: deploy-check
system_prompt: "You are a deployment verifier. Be brief."
steps:
  - name: Check
    action: Verify that all deployment targets are healthy.
```

## `acp serve` — ACP HTTP Server

The HTTP server supports a dedicated `acp.system_prompt` config field that
applies only in ACP contexts. It takes precedence over `agent.system_prompt`
when both are present. The `--system-prompt` CLI flag writes to
`acp.system_prompt` and mirrors the value into `agent.system_prompt` for shared
code paths. `XZATOMA_SYSTEM_PROMPT` writes to both fields simultaneously.

Start the server with a prompt from the CLI:

```bash
xzatoma acp serve --system-prompt "You are a CI/CD pipeline assistant."
```

Configure distinct prompts for the HTTP server and other modes:

```yaml
# config/config.yaml
acp:
  system_prompt: "You are a CI/CD pipeline assistant."
agent:
  system_prompt: "You are a helpful assistant."
# acp.system_prompt takes precedence in ACP contexts.
```

Use the environment variable to apply the same prompt to both contexts:

```bash
export XZATOMA_SYSTEM_PROMPT="You are a CI/CD pipeline assistant."
xzatoma acp serve
```

## Precedence Rules

Full resolution order, highest priority first:

```text
plan.system_prompt
  > --system-prompt flag
  > XZATOMA_SYSTEM_PROMPT environment variable
  > acp.system_prompt  (ACP contexts only: agent, acp serve)
  > agent.system_prompt
```

In ACP contexts (`agent`, `acp serve`), `acp.system_prompt` takes precedence
over `agent.system_prompt` when both are set in the config file.

## Blank Values

- Blank (whitespace-only) values in a YAML config file or plan file are rejected
  at startup with a validation error.
- A blank string passed via `--system-prompt` on the command line is silently
  ignored and treated as absent.

## Quick Reference

| Channel               | How to set it                          | Applies to                        |
| --------------------- | -------------------------------------- | --------------------------------- |
| CLI flag              | `--system-prompt "..."`                | All five modes                    |
| Environment variable  | `export XZATOMA_SYSTEM_PROMPT="..."`   | All five modes                    |
| `agent.system_prompt` | `agent.system_prompt` in `config.yaml` | All five modes (lowest fallback)  |
| `acp.system_prompt`   | `acp.system_prompt` in `config.yaml`   | `agent`, `acp serve` only         |
| Plan field            | `system_prompt: "..."` in plan YAML    | `run`, `watch` (highest priority) |
| Interactive command   | `/system <text>` at the chat prompt    | `chat` only                       |

## References

- [Configure AI Providers](configure_providers.md)
- [System Prompt Resolver Implementation](../explanation/phase2_system_prompt_resolver_implementation.md)
- [Phase 4: agent, watch, and serve System Prompt Integration](../explanation/phase4_agent_watch_serve_system_prompt_implementation.md)
- [Run and Chat Integration](../explanation/phase3_run_chat_integration_implementation.md)
