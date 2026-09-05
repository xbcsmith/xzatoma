# Using Chat Commands in XZatoma

How to use the unified slash commands available in both terminal chat mode and
ACP/Zed mode. Each task shows the exact command to type, the expected response,
and any relevant difference between the two modes.

## Before You Begin

In **terminal chat mode** (`xzatoma chat`), commands are typed at the
`[MODE][SAFETY] >>` prompt.

In **ACP/Zed mode** (`xzatoma agent`), commands are typed in the Zed chat input
box.

The commands and their responses are identical in both modes unless noted.

## 1. Check the Current Mode

Use `/mode status` to print the active chat mode and its description.

**Command:**

```text
[PLANNING][SAFE] >> /mode status
```

**Expected response:**

```text
Current mode: planning
<description>
```

**Mode note:** In terminal chat mode, the prompt indicator shows the mode
bracket. In ACP/Zed mode, the prompt line is not visible, but the response text
is the same.

## 2. Switch Modes Mid-Session

Switch to planning mode (read-only) or write mode (read/write) without
restarting the session. Switching modes preserves the full conversation history.

**Switch to planning mode:**

```text
[WRITE][SAFE] >> /mode planning
```

**Expected response:**

```text
Mode switched to planning.
```

**Switch to write mode:**

```text
[PLANNING][SAFE] >> /mode write
```

**Expected response:**

```text
Mode switched to write.
```

**Mode note:** In terminal chat mode, the prompt bracket updates immediately
after the switch (for example, `[WRITE][SAFE] >>` becomes
`[PLANNING][SAFE] >>`). In ACP/Zed mode, the mode change is confirmed in the
response text.

## 3. Inspect and Change the Safety Policy

Use `/safety` to view or change whether the agent requests confirmation before
potentially dangerous operations.

**Check the current safety policy:**

```text
[PLANNING][SAFE] >> /safety status
```

**Expected response:**

```text
Current safety policy: confirm
<description>
```

**Disable confirmations:**

```text
[PLANNING][SAFE] >> /safety off
```

**Expected response:**

```text
Safety policy set to: never_confirm
```

**Re-enable confirmations:**

```text
[PLANNING][YOLO] >> /safety on
```

**Expected response:**

```text
Safety policy set to: confirm
```

**Mode note:** In terminal chat mode, the prompt bracket updates from `[SAFE]`
to `[YOLO]` (or back) after each change. In ACP/Zed mode, the safety state is
only reflected in the response text; there is no visible prompt indicator.

## 4. Replace the System Prompt Mid-Session

Use `/system <text>` to replace the active system prompt without restarting the
session. Use `/system status` to confirm the change took effect.

**Replace the system prompt:**

```text
[PLANNING][SAFE] >> /system You are a concise assistant.
```

**Expected response:**

```text
System prompt updated.
```

**Verify the new system prompt is active:**

```text
[PLANNING][SAFE] >> /system status
```

**Expected response:**

```text
Current system prompt:
You are a concise assistant.
```

The new prompt takes effect immediately for all subsequent messages in the
session. It does not modify `config.yaml`.

**Mode note:** Works identically in both terminal chat mode and ACP/Zed mode. To
set a persistent system prompt that survives restarts, edit `config.yaml` or see
[Configuring the System Prompt](configure_system_prompt.md).

## 5. Enable and Disable Subagents Per-Session

Use `/subagents` to check whether subagent delegation is active and to toggle it
on or off for the current session.

**Check subagent delegation status:**

```text
[PLANNING][SAFE] >> /subagents status
```

**Expected response:**

```text
Subagent delegation: enabled
```

**Enable subagents:**

```text
[PLANNING][SAFE] >> /subagents on
```

**Expected response:**

```text
Subagent delegation enabled.
```

**Disable subagents:**

```text
[PLANNING][SAFE] >> /subagents off
```

**Expected response:**

```text
Subagent delegation disabled.
```

**Mode note:** The `agent.subagent.chat_enabled` key in `config.yaml` sets the
default. The `/subagents` command overrides this default for the current session
only; it does not persist to the config file.

## 6. Inspect the Active Model and Switch It Without Restarting

Use `/model` and `/models` to check the current model, list available models,
and switch to a different model without restarting the session.

**Check the active model:**

```text
[PLANNING][SAFE] >> /model status
```

**Expected response:**

```text
Current model: granite4:3b
Provider: ollama
```

**List all available models:**

```text
[PLANNING][SAFE] >> /models list
```

**Expected response:**

```text
Available models:
- granite4:3b
...
```

**Switch to a different model:**

```text
[PLANNING][SAFE] >> /model llama3:8b
```

**Expected response:**

```text
Model switched to: llama3:8b
```

The switch takes effect on the next message. The full conversation history is
preserved and sent to the new model.

**Mode note:** In ACP/Zed mode, `/model <name>` changes the model for the
current session. The provider:model label in the Zed prompt updates on the next
render.

## 7. Manage Context Window Pressure

Use `/context info` to see how much of the context window is in use. Use
`/context summary` to compress the conversation and free up space.

**Check context window usage:**

```text
[PLANNING][SAFE] >> /context info
```

**Expected response:**

```text
Context window: 500/32000 tokens used (1.6% full)
Remaining: 31500 tokens
```

**Summarize and reset the context window:**

```text
[PLANNING][SAFE] >> /context summary
```

**Expected response:**

```text
Conversation summarized. Context window reset.
```

After `/context summary`, the agent replaces the full message history with a
compact summary. The conversation continues from that summary. Use this command
when the context window approaches its limit and response quality begins to
degrade.

**Mode note:** `/context info` is useful in both modes. In ACP/Zed mode, Zed
also shows token counts in its UI, but `/context info` reports the values
tracked by XZatoma's own conversation manager, which may differ.

## 8. Reload Configuration Without Restarting

Use `/config reload` to re-read `config.yaml` and apply it to the current
session without restarting the process. This is useful after editing
provider settings, skills, MCP servers, or agent behavior mid-session.

**Check the active config file path:**

```text
[PLANNING][SAFE] >> /config status
```

**Expected response:**

```text
Active config file: config/config.yaml
```

**Reload after editing `config.yaml`:**

```text
[PLANNING][SAFE] >> /config reload
```

**Expected response:**

```text
Config reloaded. Changed: provider, skills.
```

`/config reload` rebuilds the provider, tool registry, skills, and MCP
connections from the new config while preserving conversation history. Log
level/format and persistence storage paths cannot be applied this way; if
either changed, the response calls it out by name and still requires a
restart.

**Mode note:** Works identically in terminal chat mode and ACP/Zed mode (no
Zed dropdown needed — type `/config reload` directly in the Agent Panel
thread). In ACP/Zed mode, the reload also applies to any new session created
afterward in the same subprocess, but sibling sessions already open at
reload time keep their existing agent until they also run `/config reload`.

## Related Topics

- [Managing Context Window](manage_context_window.md) - Detailed context
  management strategies
- [Using Chat Modes](use_chat_modes.md) - Full reference for chat mode behavior
- [Managing Models](manage_models.md) - Configuring and switching providers and
  models
- [Configuring the System Prompt](configure_system_prompt.md) - Persistent
  system prompt configuration
- [Using Subagents in Chat](use_subagents_in_chat.md) - Subagent delegation
  details
