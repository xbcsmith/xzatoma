# Dynamic System Prompts Implementation Plan

## Overview

Add support for configuring a dynamic system prompt across all xzatoma modes that
talk to an LLM: `chat`, `run`, `agent`, `watch`, and `serve` (ACP). The system
prompt is injected as a `role: "system"` message at the head of each session's
message array, allowing callers to steer the model's persona or behavior before
any task-specific content is sent.

System prompts are configurable through four channels with a clear precedence
order: plan file (highest), `--system-prompt` CLI flag, `XZATOMA_SYSTEM_PROMPT`
environment variable, and the `agent.system_prompt` config file field (lowest).
In chat mode an interactive `/system <text>` command lets users change the prompt
mid-session. When `--trace` is active the resolved prompt is logged at session
start.

## Current State Analysis

### Existing Infrastructure

- `src/prompts/mod.rs` — `build_system_prompt()` and related helpers generate
  mode-specific base prompts (planning, write, watcher).
- `src/agent/conversation.rs` — `Conversation::add_system_message()` inserts a
  system-role message.
- `src/agent/core.rs` — `Agent` carries `transient_system_messages: Vec<String>`
  for ephemeral per-call injections (currently used by active skills).
- `src/commands/mod.rs` — `run_chat`, `run_plan_with_options` each call
  `agent.conversation_mut().add_system_message(disclosure.clone())` for the skill
  disclosure; no general-purpose system prompt hook exists yet.
- `src/commands/special_commands.rs` — `SpecialCommand` enum handles `/mode`,
  `/safety`, `/context`, etc.; no `/system` variant exists.
- `src/tools/plan.rs` — `Plan` struct has no `system_prompt` field.
- `src/config.rs` — `AgentConfig` has no `system_prompt` field; no env var
  handler for a system prompt exists.
- `src/cli.rs` — no `--system-prompt` flag on any sub-command.

### Identified Issues

- No mechanism to inject a user-defined system prompt without editing source.
- Plan files cannot carry a session-level persona instruction.
- Resumed chat sessions have no way to enforce a new system prompt from the CLI.
- Trace logging does not surface the active system prompt.

## Implementation Phases

### Phase 1: Foundation — Config, Plan Schema, and CLI Flags

#### Task 1.1 Add `system_prompt` to `AgentConfig`

In [`src/config.rs`](../../src/config.rs), add an optional field to `AgentConfig`:

```
pub system_prompt: Option<String>
```

- Default is `None` (no field in YAML means no override).
- Add `#[serde(skip_serializing_if = "Option::is_none")]` so existing config files
  remain valid.
- In `apply_env_vars`, map `XZATOMA_SYSTEM_PROMPT` to
  `config.agent.system_prompt`.
- Add `validate` guard: if present, reject blank (whitespace-only) values.

#### Task 1.2 Add `system_prompt` to `Plan`

In [`src/tools/plan.rs`](../../src/tools/plan.rs), add an optional field to the
`Plan` struct:

```
pub system_prompt: Option<String>
```

- Annotate with `#[serde(skip_serializing_if = "Option::is_none")]`.
- Update `Plan::new()` to initialize the field to `None`.
- In `PlanParser::validate`, reject blank system prompts the same way other
  optional string fields are validated.

#### Task 1.3 Add `--system-prompt` CLI flag to all LLM-facing sub-commands

In [`src/cli.rs`](../../src/cli.rs), add the following field to each variant
listed below:

```
/// Override the system prompt for this session.
///
/// On resumed chat sessions this flag always replaces any stored system
/// message. When running a plan the plan's own system_prompt field takes
/// precedence over this flag.
#[arg(long)]
system_prompt: Option<String>,
```

Affected variants: `Commands::Chat`, `Commands::Run`, `Commands::Agent`,
`Commands::Watch`, and `AcpCommand::Serve`.

Update the corresponding function signatures:

- `chat::run_chat` — add `system_prompt: Option<String>` parameter.
- `run::run_plan_with_options` — add `system_prompt: Option<String>` parameter.
- `commands::agent` runner — add `system_prompt: Option<String>` parameter.
- `watch::run_watch` — add `system_prompt: Option<String>` parameter.
- `acp::serve` runner — add `system_prompt: Option<String>` parameter.

Update `main.rs` (or wherever the `Commands` match arm dispatches) to extract and
forward the new field for each sub-command.

#### Task 1.4 Testing Requirements

- Unit tests in `config.rs`: deserialization with and without
  `system_prompt`; env var override sets the field; blank value fails
  validation.
- Unit tests in `tools/plan.rs`: round-trip YAML with `system_prompt` present
  and absent; blank value fails `validate`.
- Unit tests in `cli.rs`: `--system-prompt` flag is parsed correctly for each
  sub-command; flag is absent produces `None`.

#### Task 1.5 Deliverables

- [ ] `AgentConfig.system_prompt` field with env var and validation.
- [ ] `Plan.system_prompt` field with validation.
- [ ] `--system-prompt` flag on `chat`, `run`, `agent`, `watch`, and
      `AcpCommand::Serve`.
- [ ] Updated function signatures to accept the new parameter.
- [ ] All new code passes `cargo fmt`, `cargo check`, `cargo clippy`, and
      `cargo test`.

#### Task 1.6 Success Criteria

`cargo test --all-features` passes with no new failures. `--system-prompt hello`
is accepted without error on every LLM sub-command (even if not yet injected).
`XZATOMA_SYSTEM_PROMPT=hello xzatoma chat` sets `config.agent.system_prompt`.

---

### Phase 2: Resolution Logic and Trace Logging

#### Task 2.1 Implement `SystemPromptResolver`

Create [`src/agent/system_prompt.rs`](../../src/agent/system_prompt.rs) with a
`resolve` free function and a `SystemPromptSource` enum:

```
pub enum SystemPromptSource {
    Plan,
    CliFlag,
    EnvVar,   // surfaced through config; kept for trace logging clarity
    Config,
    Default,
}

pub struct ResolvedSystemPrompt {
    pub text: String,
    pub source: SystemPromptSource,
}

pub fn resolve(
    plan_prompt: Option<&str>,
    cli_flag: Option<&str>,
    config_prompt: Option<&str>,
) -> Option<ResolvedSystemPrompt>
```

Precedence (highest to lowest):

1. `plan_prompt` — `SystemPromptSource::Plan`
2. `cli_flag` — `SystemPromptSource::CliFlag`
3. `config_prompt` (covers env var, because `apply_env_vars` already wrote it) —
   `SystemPromptSource::Config`
4. `None` when no source supplies a value.

The `Default` source variant is reserved for callers that build a fallback from
the mode-specific base prompt when resolution returns `None`.

Expose the new module in [`src/agent/mod.rs`](../../src/agent/mod.rs).

#### Task 2.2 Add trace-level logging of resolved system prompt

In each mode's startup path (after resolution), add:

```rust
if tracing::enabled!(tracing::Level::TRACE) {
    tracing::trace!(
        source = ?resolved.source,
        system_prompt = %resolved.text,
        "Session system prompt"
    );
}
```

This satisfies the requirement that `--trace` surfaces the full system prompt at
session start without logging sensitive content at lower verbosity levels.

#### Task 2.3 Testing Requirements

- Unit tests for `resolve` covering all precedence combinations: plan wins over
  CLI, CLI wins over config, config wins over absent, all absent returns `None`.
- Test that blank strings from any source are treated as absent (consistent with
  validation in Phase 1).

#### Task 2.4 Deliverables

- [ ] `src/agent/system_prompt.rs` with `resolve` and `SystemPromptSource`.
- [ ] Module re-exported from `src/agent/mod.rs`.
- [ ] Trace-level logging helper wired into each mode (stubs acceptable; full
      wiring done in Phase 3 and 4).

#### Task 2.5 Success Criteria

All unit tests for the resolver pass. `cargo clippy` reports no warnings.

---

### Phase 3: `run` and `chat` Mode Integration

#### Task 3.1 Inject system prompt in `run_plan_with_options`

In [`src/commands/mod.rs`](../../src/commands/mod.rs) `run::run_plan_with_options`:

1. After parsing the plan file, call `resolve(plan.system_prompt.as_deref(),
cli_flag.as_deref(), config.agent.system_prompt.as_deref())`.
2. If resolution returns `Some(resolved)`, call
   `agent.conversation_mut().add_system_message(resolved.text)` before any skill
   disclosure message.
3. Emit the trace log.

Precedence note: the plan's `system_prompt` overrides the CLI flag; document this
in the field doc comment on `Plan`.

#### Task 3.2 Inject system prompt in `run_chat`

In `chat::run_chat`:

1. Accept the new `system_prompt: Option<String>` parameter.
2. Resolve: `cli_flag > config.agent.system_prompt`.
3. For a **new session**: if resolved, inject before skill disclosure.
4. For a **resumed session**: if the CLI flag is set, locate any existing
   `role == "system"` message in the loaded conversation and replace it with the
   new text; otherwise leave history intact. This implements the rule that
   `--system-prompt` always overwrites on resume.
5. Emit trace log after resolution.

#### Task 3.3 Add `/system` special command to interactive chat

In [`src/commands/special_commands.rs`](../../src/commands/special_commands.rs):

1. Add variant to `SpecialCommand`:
   ```
   SetSystemPrompt(String),
   ```
2. In `parse_special_command`, handle the prefix `/system ` (with a trailing
   space): the remainder of the line (trimmed) becomes the prompt text. An empty
   remainder returns `Err(CommandError::MissingArgument { command: "system",
usage: "/system <text>" })`.
3. In `print_help`, document the new command.

In the `run_chat` loop in `commands/mod.rs`:

1. Match `Ok(SpecialCommand::SetSystemPrompt(text))`.
2. Replace only the **first** system message in the conversation: locate the
   index of the first `role == "system"` entry and overwrite its content
   in-place. If no system message exists yet, prepend one. Skill disclosure
   messages (any subsequent system messages) are left untouched.
   Add a `Conversation::replace_first_system_message(text: String)` helper to
   encapsulate this logic.
3. Print a confirmation line, e.g. `System prompt updated.`.

#### Task 3.4 Testing Requirements

- `run_plan_with_options`: test that plan system prompt takes precedence over CLI
  flag; test that CLI flag is used when plan has none; test that config value is
  used as fallback.
- `run_chat`: test that `--system-prompt` on a resumed session replaces the stored
  system message; test that without the flag the stored message is preserved.
- Special command parser: `test_parse_set_system_prompt_with_text`,
  `test_parse_set_system_prompt_empty_returns_missing_argument_error`,
  `test_parse_set_system_prompt_with_leading_whitespace`.

#### Task 3.5 Deliverables

- [ ] `run_plan_with_options` injects resolved system prompt.
- [ ] `run_chat` injects system prompt and handles resume overwrite.
- [ ] `SetSystemPrompt` variant and `/system` parser.
- [ ] `/system` handled in `run_chat` loop.
- [ ] `Conversation::replace_first_system_message()` helper.
- [ ] All tests pass.

#### Task 3.6 Success Criteria

`xzatoma run --system-prompt "you are a pirate" --prompt "say hello"` sends the
system message as the first message in the conversation. `xzatoma chat` interactive
session responds to `/system you are a pirate` by updating the active system
prompt. A plan YAML with `system_prompt: "you are a pirate"` causes that prompt to
override `--system-prompt`.

---

### Phase 4: `agent`, `watch`, and `serve` Mode Integration

#### Task 4.1 Inject system prompt in `agent` command

In [`src/commands/agent.rs`](../../src/commands/agent.rs), locate the agent
construction site. Accept `system_prompt: Option<String>` from the CLI, resolve
against `config.agent.system_prompt`, and inject via `add_system_message` before
execution begins. Emit trace log.

#### Task 4.2 Inject system prompt in `watch` command

In [`src/commands/mod.rs`](../../src/commands/mod.rs) `watch::run_watch` and the
watcher execution path in
[`src/watcher/`](../../src/watcher/): resolve the system prompt from
`WatchCliOverrides` (add a `system_prompt` field) or config, and inject into each
per-event agent instance before it runs. Emit trace log per agent execution.

Add `system_prompt: Option<String>` to `WatchCliOverrides` and propagate it
through `apply_cli_overrides`.

When the watcher dispatches an individual plan file that contains a
`system_prompt` field, that plan's prompt takes precedence over the
CLI/config-level prompt for that specific execution, consistent with the global
rule that the plan system prompt always wins.

#### Task 4.3 Inject system prompt in ACP serve

In [`src/commands/acp.rs`](../../src/commands/acp.rs) and
[`src/acp/`](../../src/acp/), locate where agent instances are constructed per
incoming run request. Accept `system_prompt` from `AcpCommand::Serve` (CLI) and
from `AcpConfig` (config file). Add `system_prompt: Option<String>` to
`AcpConfig`. Inject via `add_system_message` before the run executes. Emit trace
log.

Update `apply_env_vars` in `config.rs` to map `XZATOMA_SYSTEM_PROMPT` to
`config.acp.system_prompt`. No separate `XZATOMA_ACP_SYSTEM_PROMPT` env var is
introduced; one env var covers all modes.

#### Task 4.4 Testing Requirements

- `agent` command: test system prompt injection via CLI flag and config.
- `watch` command: test `WatchCliOverrides.system_prompt` is applied to agent
  instances; test config fallback.
- ACP serve: unit test that the per-run agent receives the configured system
  message.

#### Task 4.5 Deliverables

- [ ] `agent` command injects resolved system prompt.
- [ ] `WatchCliOverrides.system_prompt` field and wiring through `run_watch`.
- [ ] `AcpConfig.system_prompt` field with env var support.
- [ ] ACP serve injects system prompt per run.
- [ ] All tests pass.

#### Task 4.6 Success Criteria

`xzatoma agent --system-prompt "act as a senior Rust engineer"` injects the system
message. `xzatoma watch --system-prompt "..."` applies the prompt to each
watcher-triggered agent run. ACP serve configured with `system_prompt` in the YAML
config or via `--system-prompt` applies the prompt to every run session.

---

### Phase 5: Documentation

#### Task 5.1 Update reference documentation

Create or update the following files:

- `docs/reference/system_prompt.md` — reference page describing all configuration
  channels (plan field, CLI flag, env var, config file), precedence rules, the
  `/system` chat command, and trace logging behavior.

#### Task 5.2 Add how-to guide

Create `docs/how-to/configure_system_prompt.md` — task-oriented guide with
concrete examples for each mode.

#### Task 5.3 Update existing docs

- `docs/explanation/chat_modes_architecture.md` — note the new `/system` command
  and resolution logic.
- `docs/explanation/overview.md` — mention dynamic system prompts as a feature.

#### Task 5.4 Deliverables

- [ ] `docs/reference/system_prompt.md`
- [ ] `docs/how-to/configure_system_prompt.md`
- [ ] Updated `chat_modes_architecture.md` and `overview.md`
- [ ] All Markdown files pass `markdownlint` and `prettier` checks.

#### Task 5.5 Success Criteria

A new engineer can read `docs/how-to/configure_system_prompt.md` and successfully
configure a custom system prompt for each mode without additional help.
