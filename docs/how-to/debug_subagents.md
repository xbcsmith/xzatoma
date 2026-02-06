# How to Debug Subagent Executions

This guide explains how to use conversation persistence to debug and analyze subagent executions.

## Overview

XZatoma can save all subagent conversations to a persistent database for later inspection, debugging, and analysis. This is useful for understanding what subagents did, how many tokens they consumed, and troubleshooting failed executions.

## Enable Conversation Persistence

Edit your `config.yaml` file to enable persistence:

```yaml
agent:
  subagent:
    persistence_enabled: true
    persistence_path: ~/.xzatoma/conversations.db
```

## Run Your Workflow

Execute tasks that use subagents:

```bash
xzatoma chat
# In chat mode, invoke subagents as needed
# Example: @subagent label="analyze_code" task_prompt="Review this code for bugs"
```

## List All Conversations

View all recorded subagent conversations:

```bash
xzatoma replay --list
```

Example output:

```
Conversations (showing 10 starting at 0):

ID:     01ARZ3NDEKTSV4RRFFQ69G5FAV
Label:  analyze_code
Depth:  1
Status: complete
Turns:  7
Start:  2025-11-07T18:12:07.982682+00:00

ID:     01ARZ3NDEKTSV4RRFFQ69G5FAW
Label:  search_docs
Depth:  1
Status: incomplete
Turns:  10
Start:  2025-11-07T18:12:15.123456+00:00
Parent: 01ARZ3NDEKTSV4RRFFQ69G5FAV
```

## Replay Specific Conversation

View full conversation history for a specific subagent:

```bash
xzatoma replay --id 01ARZ3NDEKTSV4RRFFQ69G5FAV
```

Example output:

```
=== Conversation 01ARZ3NDEKTSV4RRFFQ69G5FAV ===
Label: analyze_code
Depth: 1
Started: 2025-11-07T18:12:07.982682+00:00
Completed: 2025-11-07T18:12:25.123456+00:00

Task: Review this code for bugs

=== Messages ===

--- Message 1 (user) ---
Review this code for bugs

--- Message 2 (assistant) ---
I'll analyze the code for potential issues...

... (additional messages) ...

=== Metadata ===
Turns Used: 7
Tokens Consumed: 1250
Status: complete
Max Turns Reached: false
Allowed Tools: ["file_ops", "grep"]
```

## View Conversation Tree

For nested subagents, visualize the parent-child relationship:

```bash
xzatoma replay --tree --id 01ARZ3NDEKTSV4RRFFQ69G5FAV
```

Example output:

```
Conversation tree:
├─ 01ARZ3NDEKTSV4RRFFQ69G5FAV [root_analysis] (depth=1, turns=7)
  ├─ 01ARZ3NDEKTSV4RRFFQ69G5FAW [code_review] (depth=2, turns=5)
  ├─ 01ARZ3NDEKTSV4RRFFQ69G5FAX [security_check] (depth=2, turns=6)
```

## Pagination

List conversations with custom pagination:

```bash
# Show 20 conversations starting at offset 10
xzatoma replay --list --limit 20 --offset 10
```

## Custom Database Path

Use a specific database instead of the default:

```bash
xzatoma replay --list --db-path /custom/path/conversations.db
```

## Common Debugging Scenarios

### Why did subagent fail?

1. Replay the conversation:
   ```bash
   xzatoma replay --id <conversation_id>
   ```

2. Look for error messages in the final messages or metadata

3. Check `Status: incomplete` and `Max Turns Reached: true` to see if execution was cut short

### Why was output truncated?

Output is truncated when it exceeds `agent.subagent.output_max_size` (default: 4096 bytes).

1. Check your config:
   ```bash
   grep -A 5 "output_max_size" config.yaml
   ```

2. Increase the limit if needed:
   ```yaml
   agent:
     subagent:
       output_max_size: 8192
   ```

3. Re-run the task

### How many tokens did subagent consume?

Check token usage in the metadata:

```bash
xzatoma replay --id <conversation_id> | grep "Tokens"
```

Example output:
```
Tokens Consumed: 1250
```

Use this to:
- Track API costs
- Identify inefficient subagents
- Debug token limit issues

### Subagent took too long or hit max turns

Check the turn count and completion status:

```bash
xzatoma replay --id <conversation_id> | grep -E "Turns|Status|Max Turns"
```

If `Max Turns Reached: true`:

1. Increase `agent.subagent.default_max_turns` in config (default: 10)
2. Or specify `max_turns` in the subagent input when invoking it

### Which tools did subagent use?

Check the allowed tools in metadata:

```bash
xzatoma replay --id <conversation_id> | grep "Allowed Tools"
```

This helps verify tool filtering worked correctly.

## Configuration Reference

### Persistence Settings

```yaml
agent:
  subagent:
    # Enable/disable conversation persistence
    persistence_enabled: true

    # Path to conversation database (created if doesn't exist)
    persistence_path: ~/.xzatoma/conversations.db

    # Maximum recursion depth for nested subagents
    max_depth: 3

    # Default max turns per subagent
    default_max_turns: 10

    # Maximum output size before truncation (bytes)
    output_max_size: 4096

    # Enable telemetry logging of subagent events
    telemetry_enabled: true
```

## Telemetry Events

When `telemetry_enabled: true`, subagent events are logged with structured fields:

- `spawn` - Subagent created and starting execution
- `complete` - Subagent finished successfully
- `error` - Subagent execution failed
- `max_turns_exceeded` - Reached turn limit before completion
- `truncation` - Output was truncated due to size limit
- `depth_limit` - Cannot spawn due to recursion depth limit

Example log output:
```
INFO xzatoma: subagent.event=spawn subagent.label=analyze_code subagent.depth=1 Spawning subagent
INFO xzatoma: subagent.event=complete subagent.label=analyze_code subagent.depth=1 subagent.turns_used=7 Subagent completed
```

## Next Steps

- Review the [Subagent Architecture](../explanation/subagent_implementation.md) for technical details
- Check the [API Reference](../reference/subagent_api.md) for input/output formats
- See [Advanced Patterns](../explanation/advanced_subagent_patterns.md) for complex scenarios
```

Now let me run the integration tests:
