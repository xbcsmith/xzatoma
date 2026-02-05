# Subagent API Reference

## Overview

The subagent tool enables task delegation to autonomous agent instances with isolated conversation contexts. This reference documents the complete API for invoking subagents, understanding outputs, and configuring behavior.

## Tool Definition

### Name
```
subagent
```

### Description
```
Delegate a focused task to a recursive agent instance with isolated conversation
context. Use this when you need to explore a sub-problem independently without
polluting the main conversation.
```

## Input Schema

### Parameters Type
```
object
```

### Required Fields

#### `label` (string, required)

Unique identifier for tracking this subagent instance.

- **Type:** `string`
- **Min length:** 1
- **Example:** `"research_api_docs"`, `"analyze_error_logs"`
- **Purpose:** Logging, telemetry, and debugging identification

#### `task_prompt` (string, required)

The specific task for the subagent to complete. Should be self-contained and explicit.

- **Type:** `string`
- **Min length:** 1
- **Example:** `"Research the latest PyTorch API changes in 2024"`
- **Purpose:** Initial user message to subagent

### Optional Fields

#### `summary_prompt` (string, optional)

How to summarize the subagent's findings. If omitted, default is used.

- **Type:** `string`
- **Default:** `"Summarize your findings concisely"`
- **Example:** `"Provide a structured summary with: Key Findings, Implementation Details, Recommendations"`
- **Purpose:** Shape final output format before returning to parent

#### `allowed_tools` (array of strings, optional)

Whitelist of tool names the subagent can access. If omitted, all parent tools are available (except "subagent").

- **Type:** `array[string]`
- **Items:** Tool names (e.g., "fetch", "grep", "file_ops", "terminal")
- **Special:** "subagent" is always blocked (prevents infinite recursion)
- **Example:** `["fetch", "grep", "file_ops"]`
- **Purpose:** Security isolation and task focus

#### `max_turns` (integer, optional)

Maximum conversation turns (user messages) allowed for this subagent execution.

- **Type:** `integer`
- **Minimum:** 1
- **Maximum:** 50
- **Default:** (uses `agent.subagent.default_max_turns` from config, typically 10)
- **Example:** `5` (quick task), `20` (complex task)
- **Purpose:** Prevent runaway execution and control costs

## Complete Input Example

```json
{
  "label": "research_quantum_computing",
  "task_prompt": "Research quantum computing breakthroughs in 2024. Focus on: new algorithms, hardware improvements, and companies leading the field.",
  "summary_prompt": "Create an executive summary with sections: Breakthroughs, Technical Details, Commercial Impact, Future Outlook",
  "allowed_tools": ["fetch", "grep"],
  "max_turns": 8
}
```

## Output Schema

### Response Format

```json
{
  "success": true,
  "output": "...",
  "error": null,
  "metadata": {
    "key": "value"
  }
}
```

### Success Response

#### `success` (boolean)

Whether the subagent executed successfully.

- **true:** Subagent completed, output is in `output` field
- **false:** Subagent failed, error details in `error` field

#### `output` (string)

The final result from the subagent's execution.

- **Type:** `string`
- **Max size:** `agent.subagent.output_max_size` (default 4096 bytes)
- **Truncation:** Output is automatically truncated if exceeded, with "[Output truncated]" notice appended
- **Content:** Summary of subagent's findings or work product

#### `error` (string or null)

Error message if execution failed.

- **null** if `success: true`
- **string** containing error description if `success: false`

### Metadata Fields

#### `subagent_label` (string)

Echo of the input `label` for correlation.

```
"subagent_label": "research_quantum_computing"
```

#### `recursion_depth` (string)

Nesting depth of this subagent (0=root, 1=first subagent, 2=nested subagent).

```
"recursion_depth": "1"
```

#### `completion_status` (string)

Whether execution completed normally or was interrupted.

- `"complete"`: Subagent finished task normally
- `"incomplete"`: Hit max_turns limit or error occurred

```
"completion_status": "complete"
```

#### `turns_used` (string)

Number of conversation turns (user messages) consumed.

```
"turns_used": "3"
```

#### `max_turns_reached` (string)

Whether the subagent hit its max_turns limit.

- `"true"`: Stopped due to turn limit (see `completion_status: incomplete`)
- `"false"`: Completed normally within turn limit

```
"max_turns_reached": "false"
```

#### `tokens_consumed` (string)

Total tokens used by the subagent's AI provider calls.

```
"tokens_consumed": "1250"
```

### Complete Output Example

```json
{
  "success": true,
  "output": "Quantum Computing 2024 Breakthroughs:\n\nKey Advances:\n- Google's Willow chip achieves quantum advantage in new domains\n- IBM rolls out 1000+ qubit systems\n- Atom Computing demonstrates 24-qubit neutral atom system\n\nTechnical Details:\nQuantum error correction (QEC) remains the bottleneck...",
  "error": null,
  "metadata": {
    "subagent_label": "research_quantum_computing",
    "recursion_depth": "1",
    "completion_status": "complete",
    "turns_used": "4",
    "max_turns_reached": "false",
    "tokens_consumed": "3200"
  }
}
```

## Error Response

If the subagent fails, the response contains an error message:

```json
{
  "success": false,
  "output": "",
  "error": "Subagent execution failed: Invalid tool request",
  "metadata": {
    "subagent_label": "research_quantum_computing",
    "recursion_depth": "1"
  }
}
```

## Common Error Messages

### "Maximum subagent depth N reached"

**Cause:** Attempted to spawn a subagent at or beyond the configured `max_depth` limit.

**Example:**
```
Maximum subagent recursion depth (3) exceeded. Current depth: 3.
Cannot spawn nested subagent.
```

**Solution:** Reduce nesting level or increase `agent.subagent.max_depth` in config (max 10).

### "Invalid input: missing field `label`"

**Cause:** The `label` field is missing or empty.

**Solution:** Include a non-empty `label` string.

### "Tool 'subagent' cannot be used by subagents"

**Cause:** Attempted to add "subagent" to the `allowed_tools` list.

**Solution:** Remove "subagent" from allowed_tools (it's always blocked).

### "Unknown tool: X"

**Cause:** Tool name in `allowed_tools` doesn't exist in parent registry.

**Example:**
```
Unknown tool in allowed_tools: nonexistent_tool
```

**Solution:** Use valid tool names available in the parent agent.

### "task_prompt cannot be empty"

**Cause:** The `task_prompt` is missing or contains only whitespace.

**Solution:** Provide a non-empty task prompt.

### "max_turns must be between 1 and 50"

**Cause:** `max_turns` value is outside the valid range.

**Solution:** Use a value between 1 and 50.

## Configuration Reference

All settings are in the `agent.subagent` section of the configuration file.

### `agent.subagent.max_depth`

**Type:** `integer`
**Valid Range:** 1-10
**Default:** 3
**Description:** Maximum recursion depth for nested subagents

```yaml
agent:
  subagent:
    max_depth: 5
```

### `agent.subagent.default_max_turns`

**Type:** `integer`
**Valid Range:** 1-100
**Default:** 10
**Description:** Default max_turns if not specified in subagent invocation

```yaml
agent:
  subagent:
    default_max_turns: 15
```

### `agent.subagent.output_max_size`

**Type:** `integer`
**Valid Range:** >= 1024
**Default:** 4096
**Description:** Maximum output size before truncation (bytes)

```yaml
agent:
  subagent:
    output_max_size: 8192
```

### `agent.subagent.telemetry_enabled`

**Type:** `boolean`
**Default:** true
**Description:** Emit structured telemetry logs for subagent lifecycle

```yaml
agent:
  subagent:
    telemetry_enabled: true
```

### `agent.subagent.persistence_enabled`

**Type:** `boolean`
**Default:** false
**Description:** Save conversations to persistent storage for debugging (Phase 4+)

```yaml
agent:
  subagent:
    persistence_enabled: false
```

## Telemetry Events

When `telemetry_enabled: true`, the following events are logged with structured fields:

### Event: `spawn`

Emitted when a subagent is created and ready to execute.

**Fields:**
- `subagent.event`: "spawn"
- `subagent.label`: Subagent identifier
- `subagent.depth`: Recursion depth
- `subagent.max_turns`: Maximum turns for this execution
- `subagent.allowed_tools`: List of available tools

**Example:**
```
subagent.event=spawn subagent.label=research_quantum subagent.depth=1
subagent.max_turns=8 subagent.allowed_tools=["fetch","grep"]
Spawning subagent
```

### Event: `complete`

Emitted when a subagent finishes successfully.

**Fields:**
- `subagent.event`: "complete"
- `subagent.label`: Subagent identifier
- `subagent.depth`: Recursion depth
- `subagent.turns_used`: Turns consumed
- `subagent.tokens_consumed`: Tokens used
- `subagent.status`: "complete" or "incomplete"

**Example:**
```
subagent.event=complete subagent.label=research_quantum subagent.depth=1
subagent.turns_used=4 subagent.tokens_consumed=3200 subagent.status=complete
Subagent completed
```

### Event: `error`

Emitted when a subagent fails.

**Fields:**
- `subagent.event`: "error"
- `subagent.label`: Subagent identifier
- `subagent.depth`: Recursion depth
- `subagent.error`: Error message

**Example:**
```
subagent.event=error subagent.label=research_quantum subagent.depth=1
subagent.error="Tool execution failed: fetch timeout"
Subagent execution failed
```

### Event: `max_turns_exceeded`

Emitted when a subagent hits its max_turns limit.

**Fields:**
- `subagent.event`: "max_turns_exceeded"
- `subagent.label`: Subagent identifier
- `subagent.depth`: Recursion depth
- `subagent.max_turns`: Limit that was reached

**Example:**
```
subagent.event=max_turns_exceeded subagent.label=research_quantum
subagent.depth=1 subagent.max_turns=8
Subagent exceeded max turns
```

### Event: `truncation`

Emitted when output is truncated due to size limit.

**Fields:**
- `subagent.event`: "truncation"
- `subagent.label`: Subagent identifier
- `subagent.original_size`: Original output size in bytes
- `subagent.truncated_size`: Size after truncation

**Example:**
```
subagent.event=truncation subagent.label=research_quantum
subagent.original_size=8192 subagent.truncated_size=4096
Subagent output truncated
```

### Event: `depth_limit`

Emitted when a subagent cannot be spawned due to depth limit.

**Fields:**
- `subagent.event`: "depth_limit"
- `subagent.label`: Subagent identifier (if available)
- `subagent.current_depth`: Current recursion depth
- `subagent.max_depth`: Maximum allowed depth

**Example:**
```
subagent.event=depth_limit subagent.label=research_quantum
subagent.current_depth=3 subagent.max_depth=3
Subagent recursion depth limit enforced
```

## Enable Logging

To see telemetry events, set environment variable:

```bash
export RUST_LOG=info
xzatoma chat
```

Or with filtering:

```bash
export RUST_LOG=xzatoma::tools::subagent=debug
xzatoma chat
```

## Usage Examples

### Simple Information Lookup

Delegate a quick research task:

```json
{
  "label": "weather_research",
  "task_prompt": "What was the highest temperature recorded on Earth in 2024?",
  "allowed_tools": ["fetch"],
  "max_turns": 3
}
```

**Expected execution:** ~2 turns, quick completion

### Complex Problem Solving

Allow full access for complex analysis:

```json
{
  "label": "architecture_review",
  "task_prompt": "Review this microservices architecture for scalability issues: [architecture description]",
  "summary_prompt": "Provide recommendations organized by: Critical Issues, Medium Priority, Low Priority, Implementation Timeline",
  "max_turns": 15
}
```

**Expected execution:** ~8-10 turns, detailed analysis

### Security-Isolated Task

Restrict tools for untrusted input:

```json
{
  "label": "validate_user_code",
  "task_prompt": "Analyze this user-provided code for syntax errors and security issues: [code]",
  "allowed_tools": ["file_ops"],
  "max_turns": 2
}
```

**Expected execution:** Quick scan, no terminal access

### Iterative Refinement

Use subagents in sequence:

```
Main Agent: "Create a Python function to sort a list"
└─> Subagent 1: Generate implementation
└─> Subagent 2: Review for bugs
└─> Subagent 3: Optimize for performance
Main Agent: Integrate final version
```

Each step uses separate subagent invocations.

## Rate Limits and Quotas

Subagents consume tokens from your AI provider. No built-in rate limits yet, but Phase 5 adds:

- `max_executions`: Maximum subagent instances per session
- `max_total_tokens`: Total token budget for all subagents
- `max_total_time`: Total execution time limit

## Limitations

- Maximum depth: 10 levels (prevents stack overflow)
- Maximum turns: 50 per subagent (prevents runaway execution)
- Maximum output: Configurable, default 4096 bytes
- Tool access: Must be available in parent agent

## Related Documentation

- [Subagent Tutorial](../tutorials/subagent_usage.md)
- [How to Debug Subagents](../how-to/debug_subagents.md)
- [Configuration Reference](../reference/api_specification.md)
