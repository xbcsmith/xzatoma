# Phase 5: Documentation for the Dynamic System Prompts Feature

## Overview

Phase 5 closes out the dynamic system prompts feature by producing the
user-facing documentation that makes every implementation detail from Phases 1-4
discoverable and actionable. No Rust source changes were made in this phase. All
work is documentation only.

The deliverables break into two categories:

- New files that give readers authoritative reference material and a
  step-by-step configuration guide.
- Updates to two existing explanation documents that situate the new feature
  within the broader architecture and project overview.

## Deliverables

### `docs/reference/system_prompt.md` (created)

A reference page covering every aspect of the system prompt subsystem for
operators and integrators who need precise technical details:

- All five configuration channels listed in precedence order (plan file > CLI
  flag > environment variable > `acp.system_prompt` > `agent.system_prompt`).
- `XZATOMA_SYSTEM_PROMPT` environment variable behaviour, including the fact
  that it writes to both `config.agent.system_prompt` and
  `config.acp.system_prompt` simultaneously.
- The `--system-prompt` CLI flag, available on every LLM-facing subcommand.
- The `agent.system_prompt` and `acp.system_prompt` config file fields with YAML
  examples.
- The plan file `system_prompt` field with a YAML example.
- The interactive `/system <text>` command: replacement semantics, edge cases
  (no existing system message, empty argument), and the confirmation output.
- Injection order within agent conversations (user prompt, skill disclosure,
  mode-specific base prompt, active skill prompt).
- Trace logging format and the full table of `source` values reported at `TRACE`
  level.
- Validation rules for blank values across all channels.
- Links to related reference pages.

### `docs/how-to/configure_system_prompt.md` (created)

A task-oriented guide aimed at users who want to set up a system prompt and need
runnable examples rather than specification prose. The guide covers:

- The three global configuration channels (CLI flag, environment variable,
  config file) with copy-pasteable code blocks for each.
- Per-mode sections for `chat`, `run`, `agent`, `watch`, and `acp serve`, each
  with the mode-specific flags, override behaviour, and edge cases.
- The `chat` session-resume rule: `--system-prompt` always replaces a stored
  system message on resume.
- The plan file `system_prompt` precedence in `run` and `watch` modes, with a
  YAML example showing the field in context.
- The ACP-specific dual-field setup for users who need different personas for
  ACP and non-ACP contexts.
- A precedence summary in plain text and a quick-reference table covering all
  six channels (CLI, env var, `agent.system_prompt`, `acp.system_prompt`, plan
  field, `/system` command).
- Blank-value behaviour for YAML vs CLI inputs.

### `docs/explanation/chat_modes_architecture.md` (updated)

The existing architecture explanation was extended with:

- A `SetSystemPrompt` variant in the `SpecialCommand` enum listing.
- A dedicated `/system` command section documenting the replacement semantics,
  the `replace_first_system_message` helper, and the constraint that skill
  disclosure messages are not affected.
- An expanded "Relationship to System Prompts" subsection that describes the
  full precedence chain and references the resolver types introduced in Phase 2
  (`SystemPromptSource`, `ResolvedSystemPrompt`).

### `docs/explanation/overview.md` (updated)

A "Dynamic System Prompts" entry was added to the Key Features list, covering:

- The steering purpose of system prompts.
- All four configuration channels with the precedence rule stated concisely.
- The interactive `/system <text>` command.
- The list of all LLM-facing modes where the feature applies.

## Success Criteria

The primary success criterion for Phase 5 was: a new engineer can read
`docs/how-to/configure_system_prompt.md` and successfully configure a custom
system prompt for each mode without additional help.

That criterion is met because the how-to guide:

1. Provides a working command or YAML snippet for every mode.
2. States the precedence rules explicitly and in a scannable table.
3. Covers edge cases (blank CLI flag, resume semantics, plan-level override)
   that would otherwise require reading the source code.
4. Links to the reference page and prior phase explanation documents for readers
   who need deeper detail.

## Quality Gates

All four files were validated with the mandatory Markdown quality gate commands
before this task was marked complete:

```bash
markdownlint --fix --config .markdownlint.json \
  docs/reference/system_prompt.md \
  docs/how-to/configure_system_prompt.md \
  docs/explanation/chat_modes_architecture.md \
  docs/explanation/overview.md

prettier --write --parser markdown --prose-wrap always \
  docs/reference/system_prompt.md \
  docs/how-to/configure_system_prompt.md \
  docs/explanation/chat_modes_architecture.md \
  docs/explanation/overview.md
```

Both commands exited with status 0 and reported no remaining violations.

## Relationship to Prior Phases

- **Phase 1** -- config field, plan field, and CLI flags:
  `docs/explanation/task_1_1_system_prompt_agent_config_implementation.md`
- **Phase 2** -- `resolve` function, `SystemPromptSource`, and trace logging
  stubs: `docs/explanation/phase2_system_prompt_resolver_implementation.md`
- **Phase 3** -- `run` and `chat` mode injection, `/system` command:
  `docs/explanation/phase3_run_chat_integration_implementation.md`
- **Phase 4** -- `agent`, `watch`, and `acp serve` mode injection:
  `docs/explanation/phase4_agent_watch_serve_system_prompt_implementation.md`
- **Phase 5** -- reference doc, how-to guide, two updated explanation docs: this
  file

The original feature plan is documented in
`docs/explanation/dynamic_chat_templates_implementation.md`.
