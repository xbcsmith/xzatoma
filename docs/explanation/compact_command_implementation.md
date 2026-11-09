# `/compact` Command Implementation

## Summary

This document describes the implementation of the `/compact` slash command,
which compacts chat history so that a session can continue after the context
window fills up. The command is available in both interactive chat mode and ACP
(Zed agent) mode.

## Motivation

Long-running conversations accumulate many tokens. When the context window
approaches its limit the provider begins to drop earlier messages, causing the
agent to lose important context. `/compact` gives the user an explicit,
low-friction way to summarize the accumulated conversation and reset the message
history so that more turns can fit within the window.

The existing `/context summary` subcommand and its `/summarize` shorthand
already provided this capability. However, neither name matched the mental model
that users familiar with other AI tools (e.g. Claude Code) bring to the
workflow. Adding `/compact` as a second alias removes that friction.

## Architecture

### Alias approach

`/compact` is implemented as a pure alias: `parse_special_command("/compact")`
returns `SpecialCommand::ContextSummary { model: None }`, the same variant that
`/context summary` and `/summarize` produce. This means:

- No new enum variant or handler function is needed.
- All dispatch paths (chat mode, ACP stdio, `resolve_special_command_response`)
  handle `/compact` automatically through the existing `ContextSummary` arms.
- Behavior is identical to `/summarize` — the conversation is compacted via
  `Conversation::summarize_and_reset()` and a confirmation message is returned.

### Execution path

```text
User types /compact
  -> parse_special_command("/compact")
     -> Ok(SpecialCommand::ContextSummary { model: None })
  -> dispatch_stdio_command  (ACP / Zed mode)
     -> handle_context_summary(None, session)
        -> agent.conversation_mut().summarize_and_reset()
        -> returns "Conversation summarized. Context window reset."
```

In interactive chat mode the `ContextSummary` variant is handled by the existing
command dispatch loop in the CLI chat handler — no changes required there.

## Files changed

| File                               | Change                                                                                     |
| ---------------------------------- | ------------------------------------------------------------------------------------------ |
| `src/commands/special_commands.rs` | Added `/compact` match arm in `parse_special_command`; updated `format_help_text`          |
| `src/acp/available_commands.rs`    | Added `build_compact_command`; updated count to 15                                         |
| `src/acp/stdio.rs`                 | Updated command count constant in test; added `test_dispatch_compact_returns_confirmation` |

## Help text

The CONTEXT WINDOW MANAGEMENT section of `/help` now reads:

```text
CONTEXT WINDOW MANAGEMENT:
  /context info              - Show context window usage and token statistics
  /context summary           - Summarize conversation and reset context window
  /context summary -m MODEL  - Summarize using a specific model (for cost optimization)
  /compact                   - Compact conversation history (alias for /context summary)
  /summarize                 - Same as /compact
```

## ACP command discovery

`/compact` is advertised to Zed via the ACP available-commands list so it
appears in the slash-command completion menu. It takes no arguments (no `input`
field), matching the shape of `/summarize`, `/context`, `/tools`, and other
no-argument commands.

## Tests added

| Test                                         | File                    | Description                                                                                 |
| -------------------------------------------- | ----------------------- | ------------------------------------------------------------------------------------------- |
| `test_parse_compact_returns_context_summary` | `special_commands.rs`   | Confirms `/compact` parses to `ContextSummary { model: None }`                              |
| `test_parse_compact_is_case_insensitive`     | `special_commands.rs`   | Confirms `/COMPACT` works via the lowercasing path                                          |
| `test_compact_command_is_present`            | `available_commands.rs` | Confirms `build_available_commands` includes `"compact"`                                    |
| `test_dispatch_compact_returns_confirmation` | `stdio.rs`              | End-to-end ACP dispatch test: `/compact` returns `EndTurn` with a "summarized" confirmation |

## Design decisions

- **Alias over new variant**: Adding a third enum variant for essentially the
  same operation would create dead dispatch arms and require parallel
  maintenance. The alias approach keeps the variant count stable and the
  dispatch logic DRY.

- **No model argument**: `/compact` takes no arguments. Users who need to select
  a summarization model can use `/context summary --model <name>`. Keeping
  `/compact` argument-free maximises discoverability and matches the
  one-word-command expectation.

- **Placement in completion menu**: `/compact` is listed after `/summarize` in
  the ACP available-commands vector. Both commands appear in the menu so that
  users who know either name can find the feature without consulting
  documentation.
