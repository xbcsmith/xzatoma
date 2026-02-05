# Using Subagents for Task Delegation

## Overview

Subagents are autonomous agent instances that you can spawn to delegate focused tasks without polluting your main conversation history. This tutorial teaches you how to use subagents to break down complex problems into manageable sub-tasks and execute them in parallel or sequence.

**Key Benefits:**
- Isolate sub-problems from main conversation context
- Delegate specialized tasks (research, analysis, code review)
- Control execution with tool whitelisting and turn limits
- Monitor execution with structured telemetry
- Maintain conversation history for debugging

## Prerequisites

- XZatoma installed and configured with a working AI provider (Copilot or Ollama)
- Basic familiarity with XZatoma chat mode
- A text editor to create configuration files

## What You'll Learn

By the end of this tutorial, you'll be able to:

1. Spawn subagents with the `subagent` tool
2. Configure subagent execution limits and behavior
3. Filter tools available to subagents for security
4. Understand subagent output and metadata
5. Debug subagent execution with logs

## Step 1: Basic Subagent Invocation

The simplest use case: delegate a self-contained task to a subagent.

### Start interactive chat mode

```bash
xzatoma chat
```

### Invoke a subagent in chat

Ask the agent to research something using the subagent tool:

```
Research the history of artificial intelligence using the fetch and grep tools.
Focus on key milestones and breakthroughs. Create a comprehensive summary.
```

Or directly invoke the subagent tool:

```json
{
  "label": "research_ai_history",
  "task_prompt": "Research the history of artificial intelligence. Focus on: Turing (1950), Expert Systems (1980s), Deep Learning (2012+). Return key dates and achievements.",
  "allowed_tools": ["fetch", "grep"]
}
```

### What happens

1. A new subagent spawns at depth=1
2. It receives your task and can only use `fetch` and `grep` tools
3. It executes independently with its own conversation history
4. You receive structured output with metadata about execution

### Output structure

```json
{
  "success": true,
  "output": "AI History Research:\n- 1950: Turing Test proposed...",
  "metadata": {
    "subagent_label": "research_ai_history",
    "recursion_depth": "1",
    "completion_status": "complete",
    "turns_used": "3",
    "tokens_consumed": "1250",
    "max_turns_reached": "false"
  }
}
```

## Step 2: Tool Filtering for Security

Restrict which tools a subagent can access using `allowed_tools` whitelist.

### Example: Code analysis with limited tools

```json
{
  "label": "analyze_security",
  "task_prompt": "Analyze this code for security vulnerabilities: [code snippet]",
  "allowed_tools": ["file_ops"],
  "max_turns": 5
}
```

Only `file_ops` is available (no terminal execution for safety).

### Tool filtering rules

- If `allowed_tools` is omitted: subagent gets ALL parent tools (except "subagent")
- If `allowed_tools` is specified: subagent gets ONLY listed tools
- "subagent" tool is always blocked (prevents infinite recursion)
- Unknown tool names in whitelist cause an error

### Why filter tools?

- **Security**: Prevent untrusted subagents from running dangerous commands
- **Focus**: Force subagent to use specific tools for a task
- **Cost**: Some tools have usage limits; filtering avoids waste

## Step 3: Controlling Execution Length

Limit subagent execution with `max_turns` to prevent runaway tasks.

### Example: Quick research (limited turns)

```json
{
  "label": "quick_summary",
  "task_prompt": "Summarize the latest news on quantum computing in 2024",
  "allowed_tools": ["fetch"],
  "max_turns": 3
}
```

This subagent can make at most 3 turns (interactions with the AI provider) before stopping.

### Turn counting

- Each `execute()` call = 1 turn (1 user message)
- Initial task = 1 turn
- Summary request = 1 turn
- Additional assistant responses = counted turns
- Example: basic invocation uses ~2 turns (task + summary)

### When to use max_turns

- Quick information lookups: `max_turns: 2-3`
- Problem analysis: `max_turns: 5-10` (default)
- Complex research: `max_turns: 15-20`
- Never: too-high limits (>50) risk excessive costs

### What happens when max_turns is exceeded?

```json
{
  "success": true,
  "output": "Incomplete research results...",
  "metadata": {
    "completion_status": "incomplete",
    "max_turns_reached": "true",
    "turns_used": "10",
    "max_turns": "10"
  }
}
```

The subagent stops and returns early, marked as "incomplete".

## Step 4: Custom Summary Prompts

By default, the subagent receives a summary request: "Summarize your findings concisely". You can customize this.

### Example: Structured summary

```json
{
  "label": "code_review",
  "task_prompt": "Review this pull request for code quality issues: [code]",
  "summary_prompt": "Provide a structured review with: Issues Found, Severity (Critical/High/Medium/Low), Recommendations, Overall Quality Score",
  "allowed_tools": ["file_ops"]
}
```

### Default behavior

If `summary_prompt` is omitted: `"Summarize your findings concisely"`

### Custom prompts are useful for

- Enforcing specific output format
- Requesting metrics or scores
- Asking for next steps or recommendations
- Creating structured reports

## Step 5: Configuration

Control global subagent behavior via YAML configuration.

### Edit your config file

```yaml
# config/config.yaml
provider:
  type: copilot

agent:
  max_turns: 50

  # Subagent settings
  subagent:
    # Maximum nesting depth (0=root, 1=first subagent, etc.)
    max_depth: 3

    # Default max_turns if not specified in tool call
    default_max_turns: 10

    # Output truncation threshold (bytes)
    output_max_size: 4096

    # Emit structured telemetry logs
    telemetry_enabled: true

    # Save conversations for debugging (Phase 4+)
    persistence_enabled: false
```

### Configuration validation

All settings are validated on startup:

```bash
xzatoma --config config/config.yaml chat
# Error: agent.subagent.max_depth must be greater than 0
```

### Recommended settings

**Development (permissive):**
```yaml
subagent:
  max_depth: 5
  default_max_turns: 20
  output_max_size: 8192
  telemetry_enabled: true
```

**Production (conservative):**
```yaml
subagent:
  max_depth: 2
  default_max_turns: 5
  output_max_size: 4096
  telemetry_enabled: true
```

## Step 6: Monitoring and Debugging

Enable structured logging to monitor subagent execution.

### Enable verbose logging

```bash
export RUST_LOG=info
xzatoma chat
```

### Telemetry events

When `telemetry_enabled: true`, you'll see logs like:

```
subagent.event=spawn subagent.label=research_ai_history subagent.depth=1 subagent.max_turns=10
Spawning subagent

subagent.event=complete subagent.label=research_ai_history subagent.depth=1 subagent.turns_used=3 subagent.tokens_consumed=1250 subagent.status=complete
Subagent completed
```

### Common telemetry events

- `spawn`: Subagent created and ready to execute
- `complete`: Subagent finished successfully
- `error`: Subagent execution failed (includes error message)
- `max_turns_exceeded`: Subagent hit turn limit
- `truncation`: Output was truncated
- `depth_limit`: Recursion limit enforced

### Disable telemetry if needed

```yaml
subagent:
  telemetry_enabled: false
```

## Common Patterns

### Pattern 1: Parallel Analysis

Execute multiple independent analyses using separate subagents:

```
Task 1: Create a subagent to analyze financial documents
Task 2: Create another subagent to check legal compliance
Task 3: Combine results in main conversation
```

Each subagent runs independently with its own conversation history.

### Pattern 2: Iterative Refinement

Use subagents in a loop to refine results:

```
Iteration 1: Subagent drafts code
Iteration 2: Subagent reviews for issues
Iteration 3: Subagent optimizes for performance
Iteration 4: Main agent integrates final version
```

### Pattern 3: Security Isolation

Spawn restricted subagents for untrusted tasks:

```json
{
  "label": "analyze_user_input",
  "task_prompt": "[untrusted user input]",
  "allowed_tools": ["file_ops"],
  "max_turns": 2
}
```

Only file operations allowed, limited execution time.

## Troubleshooting

### "Maximum subagent depth N reached"

**Cause:** Tried to spawn a subagent at or beyond `max_depth` limit

**Solution:** Reduce nesting or increase `max_depth` in config (max 10)

```yaml
subagent:
  max_depth: 5  # Allow deeper nesting if needed
```

### "Subagent output truncated"

**Cause:** Output exceeded `output_max_size` bytes

**Solution:** Check metadata for original size, increase limit if needed

```yaml
subagent:
  output_max_size: 8192  # Increase from 4096
```

### Subagent not completing

**Cause:** Hit `max_turns` limit before finishing task

**Solution:** Increase `max_turns` in tool call or config

```json
{
  "label": "research",
  "task_prompt": "...",
  "max_turns": 20  // Increase from default 10
}
```

### Subagent using wrong tools

**Cause:** Tool not in `allowed_tools` whitelist

**Solution:** Add tool to whitelist or remove filter

```json
{
  "label": "research",
  "task_prompt": "...",
  "allowed_tools": ["fetch", "grep", "file_ops"]  // Add missing tool
}
```

## Next Steps

- Learn about parallel execution (Phase 5)
- Set up conversation persistence for debugging (Phase 4)
- Implement resource quotas for production (Phase 5)
- Monitor metrics and performance (Phase 5)

## See Also

- [Subagent API Reference](../reference/subagent_api.md)
- [Configuration Reference](../reference/api_specification.md)
- [How to Debug Subagents](../how-to/debug_subagents.md)
