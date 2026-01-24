# Using Chat Modes in XZatoma

## Overview

XZatoma provides two interactive chat modes that let you control what the AI agent can do:

- **Planning Mode** - Read-only access to files (safe for exploration)
- **Write Mode** - Read and write access to files and terminal commands (full power)

Within each mode, you can also control safety confirmations with **Safety Modes**:

- **Safe Mode** - Requires confirmation before dangerous operations
- **YOLO Mode** - Operations proceed without confirmation (use with caution)

## When to Use Each Mode

### Planning Mode

Use Planning mode when you want the agent to:

- Explore your codebase without making changes
- Create plans or analyze existing code
- Understand project structure and dependencies
- Generate documentation or reports
- Search for specific patterns or TODOs

**Example workflow:**

```
$ xzatoma chat --mode planning
[PLANNING][SAFE] >> Analyze the src/ directory and tell me about the main modules
```

### Write Mode

Use Write mode when you want the agent to:

- Implement features and make code changes
- Create new files and directories
- Execute terminal commands (compile, test, run)
- Delete or rename files
- Apply automated refactoring

**Warning:** Write mode is powerful but dangerous. Always review changes before committing.

**Example workflow:**

```
$ xzatoma chat --mode write --safe
[WRITE][SAFE] >> Refactor all function names in src/ from snake_case to camelCase
```

## Starting Interactive Chat

### Basic Usage

```bash
# Start in Planning mode (default)
xzatoma chat

# Start in Write mode
xzatoma chat --mode write

# Start with safety confirmations enabled
xzatoma chat --safe

# Combine mode and safety options
xzatoma chat --mode write --safe
```

### Initial Configuration

When you start a chat session, you'll see:

```
╔══════════════════════════════════════════════════════════════╗
║     XZatoma Interactive Chat Mode - Welcome!       ║
╚══════════════════════════════════════════════════════════════╝

Mode:  PLANNING (Read-only mode for creating plans)
Safety: SAFE (Confirm dangerous operations)

Type '/help' for available commands, 'exit' to quit

[PLANNING][SAFE] >>
```

The prompt shows your current mode and safety setting:

- `[PLANNING]` or `[WRITE]` - current chat mode
- `[SAFE]` or `[YOLO]` - current safety mode

### Provider & Model Display

When a configured provider exposes a current model, the interactive prompt will also include a provider:model label after the mode/safety tags. The label is concise and intended to give immediate context about which provider and model are active.

Example (plain):

```text
[PLANNING][SAFE][Copilot: gpt-5-mini] >>>
```

Example (colored):

```text
[PLANNING][SAFE][Copilot: gpt-5-mini] >>>  (Provider is white, model is green)
```

Notes and behavior:

- The prompt queries the provider for its current model each time it renders and will update immediately when the active model changes (for example, after `/switch_model`).
- If the provider does not expose a current model or the query fails, the prompt falls back to the base prompt (no provider/model label), preserving prior behavior.
- The label is capitalized for the provider name and shows the model identifier as provided by the provider.

Quick validation steps:

1. Configure or authenticate the provider (see `docs/how-to/configure_providers.md`).
2. Start an interactive chat (`xzatoma chat --mode planning`) and confirm the provider/model label appears in the prompt.
3. Switch models (e.g., `/switch_model`) and verify the prompt updates on the next render.

## Switching Modes During Chat

### Switching Chat Modes

Switch between Planning and Write modes at any time:

```
[PLANNING][SAFE] >> /mode write
Warning: Switching to WRITE mode - agent can now modify files and execute commands!
Type '/safe' to enable confirmations, or '/yolo' to disable.

Switched from PLANNING to WRITE mode

[WRITE][SAFE] >>
```

Or use the shorthand:

```
[PLANNING][SAFE] >> /write
# Or
[WRITE][SAFE] >> /planning
```

### Switching Safety Modes

Toggle confirmations on and off:

```
[WRITE][SAFE] >> /yolo
Switched from SAFE to YOLO mode

[WRITE][YOLO] >>
```

Or use alternatives:

```
[WRITE][YOLO] >> /safe
# Or use explicit commands
[WRITE][SAFE] >> /safety off
[WRITE][YOLO] >> /safety on
```

### Important Note: Mode Switching Preserves Conversation

When you switch modes, your conversation history is preserved. The agent remembers:

- All previous messages and responses
- Context from earlier in the conversation
- Tool results from previous operations

This lets you work in Planning mode to explore, then switch to Write mode to execute plans based on your discoveries.

## Available Commands

Display all available commands:

```
[PLANNING][SAFE] >> /help
```

### Command Reference

| Command     | Aliases         | Purpose                      |
| ---------------- | ------------------------ | ------------------------------------------------- |
| `/mode planning` | `/planning`       | Switch to Planning mode (read-only)        |
| `/mode write`  | `/write`         | Switch to Write mode (read/write)         |
| `/safe`     | `/safety on`       | Enable safety mode (confirm dangerous operations) |
| `/yolo`     | `/safety off`      | Disable safety mode (YOLO mode)          |
| `/status`    | -            | Show current mode and safety setting       |
| `/help`     | `/?`           | Display all available commands          |
| `exit`      | `quit`, `/exit`, `/quit` | Exit the chat session               |

### Regular Commands

Any text that doesn't start with `/` is sent to the agent as a prompt:

```
[WRITE][SAFE] >> Create a new file called hello.rs with a simple main function
```

## Session Status

Check your current session status at any time:

```
[WRITE][SAFE] >> /status

╔══════════════════════════════════════════════════════════════╗
║           XZatoma Session Status          ║
╚══════════════════════════════════════════════════════════════╝

Chat Mode:     WRITE (Read/write mode for executing tasks)
Safety Mode:    SAFE (Confirm dangerous operations)
Available Tools:  5
Conversation Size: 12 messages
Prompt Format:   [WRITE][SAFE] >>

```

## Safety Confirmations

### What Gets Confirmed in Safe Mode?

When Safety Mode is enabled, the agent will ask for confirmation before:

- Executing terminal commands (especially dangerous ones like `rm -rf`)
- Modifying files (especially large changes or deletions)
- Writing to sensitive locations
- Running scripts or executables

### Confirming Operations

When the agent requests confirmation:

```
Agent: I'm about to run: rm -rf old_build/

Do you want to proceed? (yes/no)
```

Type `yes` to confirm or `no` to cancel.

### YOLO Mode Caution

YOLO mode disables these confirmations. Use it only when:

- You've thoroughly reviewed the agent's plan
- You're working on a test repository or non-critical files
- You have recent backups
- You're confident in what the agent will do

## Example Workflows

### Workflow 1: Safe Code Exploration

```bash
$ xzatoma chat --mode planning --safe

[PLANNING][SAFE] >> What does the Agent struct do?
Agent: [reads files and explains]

[PLANNING][SAFE] >> Show me all public functions in Agent
Agent: [lists and describes functions]

[PLANNING][SAFE] >> Create a document describing the architecture
Agent: [creates docs/architecture.md]
```

### Workflow 2: Feature Implementation with Review

```bash
$ xzatoma chat --mode planning --safe

[PLANNING][SAFE] >> What would I need to add to support JSON output?
Agent: [analyzes code and suggests changes]

[PLANNING][SAFE] >> Create a plan for implementing JSON support
Agent: [creates implementation_plan.md]

[PLANNING][SAFE] >> /mode write
Warning: Switching to WRITE mode...

[WRITE][SAFE] >> Let's implement JSON output support based on the plan
Agent: [modifies files]
Agent: Please confirm: Writing 500 lines to src/output.rs
[You review and confirm]

[WRITE][SAFE] >> Run the tests to verify everything works
Agent: [executes tests and shows results]
```

### Workflow 3: Quick Script Creation

```bash
$ xzatoma chat --mode write --safe

[WRITE][SAFE] >> Create a shell script that lists all Rust files with >1000 lines
Agent: [creates and configures the script]
Agent: Please confirm: Writing 45 lines to find_large_files.sh
[You confirm]

[WRITE][SAFE] >> Run the script and show me the results
Agent: [executes and displays results]
```

## Best Practices

### Do's

- Start in Planning mode to understand the codebase
- Use `/status` to check your current mode and safety setting
- Keep `/safe` enabled when working with important files
- Review the agent's plans before switching to Write mode
- Use `/help` if you forget a command

### Don'ts

- Don't use YOLO mode on production repositories
- Don't forget to check your current mode before giving risky commands
- Don't mix Planning mode expectations with Write mode capabilities
- Don't ignore warnings when switching to Write mode

## Troubleshooting

### "I accidentally switched to Write mode"

Switch back to Planning mode:

```
[WRITE][SAFE] >> /planning
Switched from WRITE to PLANNING mode
```

Your conversation history is preserved, so you can continue from where you were.

### "The agent won't modify files in Planning mode"

That's correct! Planning mode is intentionally read-only. Switch to Write mode:

```
[PLANNING][SAFE] >> /mode write
```

### "I need to confirm operations"

Enable Safe mode:

```
[WRITE][YOLO] >> /safe
Switched from YOLO to SAFE mode
```

### "Commands are taking too long"

Large operations (reading entire directory trees, running slow tests) can take time. The agent will display progress as it works.

## Tips and Tricks

### Mode-Specific Workflows

**For code review:** Start in Planning mode to read and understand code before deciding if changes are needed.

**For rapid prototyping:** Use Write mode with Safe mode enabled to quickly implement and test changes with safety confirmations.

**For batch operations:** Use Write mode with YOLO mode disabled to prevent accidental bulk modifications.

### Efficient Mode Switching

You can optimize your workflow by:

1. Gathering requirements in Planning mode
2. Creating a plan while still in Planning mode
3. Switching to Write mode only when ready to implement
4. Switching back to Planning mode for code review if needed

### Conversation Context

Your full conversation is preserved during mode switches. This means:

- The agent remembers earlier analysis from Planning mode
- You can refer to previous decisions in Write mode
- Context from earlier messages influences future responses

Use this to your advantage by establishing context in Planning mode before switching to Write mode.

## Related Topics

- [Chat Modes Architecture](../explanation/chat_modes_architecture.md) - How chat modes work internally
- [System Prompts Reference](../archive/implementation_summaries/phase4_system_prompts_and_plan_format_implementation.md) - How system prompts guide agent behavior
- [CLI Reference](../reference/cli.md) - Complete command-line options

## Getting Help

- Type `/help` in any chat session to see available commands
- Check `/status` to verify your current mode and settings
- Read the [Architecture Documentation](../explanation/chat_modes_architecture.md) for technical details

```

Now let me create the architecture explanation document:
```
