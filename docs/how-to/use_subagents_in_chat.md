# Using Subagents in Chat Mode

This guide covers how to enable, control, and effectively use subagent delegation in XZatoma's interactive chat mode.

## Quick Start

1. Start chat mode:
   ```bash
   xzatoma chat
   ```

2. Enable subagents:
   ```
   /subagents on
   ```

3. Delegate work:
   ```
   Use subagents to process these 10 files in parallel
   ```

4. Check status:
   ```
   /status
   ```

## Understanding Subagents

Subagents are autonomous AI agent instances that can handle specialized tasks, parallel operations, and delegated work. When enabled, you can:

- **Delegate parallel tasks** to multiple subagents simultaneously
- **Specialize subagents** for different types of work
- **Reduce costs** by using cheaper models for routine subagent work
- **Improve speed** by distributing work across multiple agents

### Subagent vs. Main Agent

| Aspect | Main Agent | Subagent |
|--------|-----------|----------|
| **Provider** | Your configured main provider | Override with `agent.subagent.provider` |
| **Model** | Your main model | Override with `agent.subagent.model` |
| **Cost** | Higher (powerful model) | Usually lower (cheaper/faster model) |
| **Purpose** | Strategic reasoning, planning | Task execution, parallel work |
| **Control** | You manage directly | Main agent delegates automatically |

## Enabling and Disabling Subagents

### Enable Subagents

```
/subagents on
```

or

```
/subagents enable
```

**Result**: Subagent delegation tools become available.

### Disable Subagents

```
/subagents off
```

or

```
/subagents disable
```

**Result**: Subagent delegation tools are removed (uses less resources).

### Toggle Subagents

```
/subagents
```

Toggles between on and off. Shows current state.

## Checking Subagent Status

### View Full Status

```
/status
```

Shows:
- Chat mode (Planning or Write)
- Safety mode (Safe or Yolo)
- **Subagents state (Enabled or Disabled)**
- Available tools
- Conversation size
- Current prompt format

Example output:
```
╔══════════════════════════════════════════════════════════╗
║              XZatoma Session Status                      ║
╚══════════════════════════════════════════════════════════╝

Chat Mode:         WRITE (Read/Write mode)
Safety Mode:       SAFE (Requires confirmation)
Subagents:         ENABLED
Available Tools:   12
Conversation Size: 23 messages
Prompt Format:     Write mode + Safety mode
```

### Check Subagent Configuration

Configuration is loaded from your config file at startup:

```bash
# View current config
cat ~/.xzatoma/config.yaml

# Section: agent.subagent
```

## Automatic Subagent Activation

XZatoma automatically enables subagents when it detects relevant keywords in your prompts. This works even if subagents are disabled.

### Keywords That Trigger Automatic Enablement

Mention any of these phrases to auto-enable subagents:

- **"subagent"** - Direct mention
- **"delegate"** - Task delegation
- **"spawn agent"** - Agent creation
- **"parallel task"** - Parallel work
- **"parallel agent"** - Multi-agent execution
- **"agent delegation"** - Explicit delegation
- **"use agent"** - Agent instruction

### Examples of Auto-Enabling Prompts

```
# These automatically enable subagents:

"Use agents to process these files in parallel"

"Delegate the data transformation to subagents"

"Spawn multiple agents to analyze different sections"

"Create parallel agents for concurrent processing"

"I need agent delegation for this large task"
```

### Disable Auto-Enablement

If you don't want automatic activation, disable subagents:

```
/subagents off
```

When disabled, subagent tools are removed and auto-activation doesn't trigger.

## Using Subagent Tools

Once enabled, you can use subagent delegation in prompts. The main agent decides whether to delegate:

### Implicit Delegation

The main agent decides when to use subagents automatically:

```
Process these 50 CSV files and create a summary report
```

**What happens**:
1. Main agent receives your request
2. Sees complexity and number of files
3. Creates subagents to process files in parallel
4. Collects results from subagents
5. Creates summary report from combined results

### Explicit Delegation

You can explicitly request subagent involvement:

```
Use subagents to process each CSV file separately, then combine results
```

### Parallel Task Distribution

Request parallel execution across subagents:

```
Analyze these 10 log files for patterns. 
Use separate subagents for each file to run in parallel,
then combine findings into a unified report.
```

## Best Practices for Effective Delegation

### 1. Start Simple

Begin with basic delegation before complex scenarios:

```
# Good first delegation
Use subagents to read these 5 files

# Not good first delegation
Create a complex pipeline with 20 subagents, caching, and custom scheduling
```

### 2. Describe the Task Clearly

Be explicit about what you want subagents to do:

```
# Good: Clear, specific task
Process each JSON file in the data/ directory.
Extract the 'transactions' array from each file.
Sum all transaction amounts.
Return results grouped by date.

# Poor: Vague task
Process the files
```

### 3. Specify Parallelization Strategy

Tell subagents how to work together:

```
# Good: Clear parallelization
Use 4 subagents to process these 4 files in parallel.
Each subagent handles one file completely.
Collect results when all subagents finish.

# Poor: Unclear
Use subagents on the files
```

### 4. Set Boundaries

For large tasks, constrain the delegation:

```
# Good: Bounded
Use subagents to process up to 20 files at a time,
with maximum 5 subagents running in parallel.

# Poor: Unbounded
Use subagents on all the files in the directory
```

### 5. Provide Context When Needed

Supply information subagents need:

```
# Good: Context provided
These files are daily logs in JSON format.
Each log contains a "metrics" object with:
- cpu_usage: percentage
- memory_used: MB
- timestamp: ISO 8601

Use subagents to extract metrics from each file.

# Poor: No context
Process the files
```

## Performance and Cost Considerations

### Performance Factors

**Parallelization**:
- 1-2 subagents: Sequential/fallback performance
- 3-5 subagents: Excellent parallelism
- 6+ subagents: Diminishing returns, higher overhead

**Model Selection**:
- Small, fast models: Quick responses (1-5 seconds)
- Medium models: Balanced (5-15 seconds)
- Large models: Slower but higher quality (15-60 seconds)

### Cost Factors

If using a paid provider (e.g., Copilot):

| Configuration | Cost Impact | When to Use |
|---------------|------------|-----------|
| Cheap model for subagents | 70-80% less | High-volume work |
| Fast model for subagents | 90% less tokens | Many small tasks |
| Local Ollama subagents | Zero cost | Everything, if you can |

### Monitor Usage

Check resource consumption:

```
/status
```

For detailed token accounting:

```
/context
```

Shows context window and token usage for current session.

## Troubleshooting

### Issue: "Subagents are disabled"

**Problem**: You tried to use subagents but got an error saying they're disabled.

**Solution**:
```
/subagents on
```

Then try your request again.

### Issue: Subagents not responding

**Problem**: Subagents are enabled but don't seem to execute delegation requests.

**Possible causes**:
1. Provider not configured correctly
2. Model not available
3. Authentication failed
4. Resource limits exceeded

**Solutions**:
```
# Check configuration
/status

# Verify provider and model available
/models list

# Re-authenticate if needed
/auth

# Try a simpler delegation
Use a subagent to read one file
```

### Issue: Slow subagent responses

**Problem**: Subagent delegation is much slower than expected.

**Solutions**:
1. Use a faster model:
   ```
   /model gpt-3.5-turbo
   ```

2. Reduce subagent count:
   ```
   Limit to 2 subagents maximum for this task
   ```

3. Switch to local provider if available:
   - Ensure Ollama is running
   - Use smaller, faster models

### Issue: High costs from subagents

**Problem**: Token usage from subagents is unexpectedly high.

**Solutions**:
1. Check token consumption:
   ```
   /context
   ```

2. Use cheaper model for subagents:
   - Update config: `agent.subagent.model: gpt-3.5-turbo`

3. Reduce max tokens:
   - Explicitly limit in prompts: "Keep responses under 100 tokens"

4. Reduce parallelism:
   - "Use at most 2 subagents"

## Advanced Usage Patterns

### Pattern 1: Data Processing Pipeline

```
Read all CSV files in the data/ directory.
Use subagents to process each file:
1. Load the CSV
2. Normalize column names
3. Remove duplicates
4. Calculate summary statistics

Combine statistics from all files into a single report.
```

### Pattern 2: Content Analysis

```
Analyze these 10 articles for tone and sentiment.
Use 5 subagents to analyze 2 articles each in parallel.
For each article, determine:
- Primary sentiment (positive/negative/neutral)
- Tone (formal/casual/technical)
- Key themes

Return a summary with aggregate statistics.
```

### Pattern 3: Code Review

```
I have 8 Python functions that need review.
Use subagents to review 2 functions each in parallel.

Each subagent should:
1. Check for style violations
2. Find potential bugs
3. Suggest optimizations
4. Rate readability

Compile all feedback into a single comprehensive review.
```

### Pattern 4: Document Extraction

```
These 20 PDF documents contain contract terms.
Use subagents to extract key terms from each document:
- Party names
- Effective date
- Termination clause
- Payment terms
- Liability limits

Organize extracted terms in a comparison table.
```

## Configuration Integration

Subagent behavior in chat mode respects your configuration:

```yaml
agent:
  subagent:
    provider: ollama                # Provider for subagents
    model: llama3.2:latest          # Model for subagents
    chat_enabled: true              # Enable by default in chat
    max_executions: 5               # Max concurrent subagents
    max_depth: 3                    # Max nesting depth
```

### Override at Runtime

You can't override config in chat mode, but you can:

1. **Use `/model` to change main agent model**:
   ```
   /model gpt-4o-mini
   ```

2. **Update config and restart chat**:
   ```
   # Edit config
   nano ~/.xzatoma/config.yaml
   
   # Restart chat
   xzatoma chat
   ```

3. **Use different config at startup**:
   ```
   xzatoma chat --config custom.yaml
   ```

## Integration with Chat Modes

### Planning Mode with Subagents

```bash
# Start in planning mode
xzatoma chat
/mode planning

# Enable subagents
/subagents on

# Plan with delegation
Use subagents to brainstorm 5 different approaches to this problem
```

**Use case**: Strategic planning with parallel idea generation

### Write Mode with Subagents

```bash
# Switch to write mode
/mode write

# Enable subagents
/subagents on

# Delegate file operations
Use subagents to create these 10 files with specific content
```

**Use case**: Parallel file creation, content generation

## Safety Considerations

### Planning Mode

In planning mode, subagents cannot execute tools. They can only suggest delegated work.

```
/mode planning
/subagents on

# Subagents can suggest but not execute:
"Use subagents to analyze the file structure"  # OK: reads planning mode
# Main agent suggests, no execution happens
```

### Write Mode

In write mode, subagents can access file and terminal tools. Use safety mode:

```
/mode write
/safe                    # Enable safety mode
/subagents on

# Now subagents can delegate work that requires confirmation
Use subagents to create these files with content
```

Before each potentially dangerous operation, you'll confirm:

```
[Safety Check] Subagent wants to:
  - Create file: output.txt
Continue? (y/n)
```

## Session Persistence

Subagent state persists during a chat session:

```
/subagents on
# ... use subagents in various prompts ...

# State is remembered between prompts
"Delegate this task to subagents"  # Still enabled from earlier

# Switch modes (chat/write) - state persists
/mode write
/status
# Shows: Subagents enabled
```

To reset, explicitly disable:

```
/subagents off
/subagents on    # Fresh start
```

## Related Documentation

- **Configuration**: See `docs/how-to/configure_subagents.md` for setup and configuration
- **Examples**: See `docs/tutorials/subagent_usage.md` for practical workflow examples
- **Chat Modes**: See `docs/how-to/use_chat_modes.md` for planning vs. write mode details
- **Safety**: See `docs/how-to/use_chat_modes.md#safety-mode` for safety mode in write mode

---

**Last updated**: 2024  
**Feedback**: Check project issues or documentation for questions
```

Now I'll update the help text to include subagent commands:
