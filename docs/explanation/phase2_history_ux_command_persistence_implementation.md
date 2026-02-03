# Phase 2: History UX & Command Persistence Implementation

## Overview

Phase 2 adds comprehensive history inspection capabilities and special command persistence to XZatoma. Users can now view full message-level conversation history through a new `history show` command with formatted display and raw JSON output options. Special commands executed during chat sessions are persisted as system messages in conversation history, allowing users to audit their interaction patterns.

This phase builds on Phase 1's core validation infrastructure, ensuring that all messages are properly sanitized before provider conversion.

## Components Delivered

### Source Code

- **`src/cli.rs`** (+34 lines) - Extended `HistoryCommand` enum with `Show` variant
  - Added `id`, `raw`, and `limit` parameters to support flexible history display
  - Maintains backward compatibility with existing `List` and `Delete` commands

- **`src/commands/history.rs`** (+230 lines) - Implemented history display logic
  - `show_conversation()` function for loading and displaying conversation messages
  - `print_message()` helper for formatted message display with role, tool info, and content
  - Support for both formatted display and raw JSON output
  - Message limiting to show only last N messages

- **`src/config.rs`** (+38 lines) - Added persistence configuration
  - `persist_special_commands` field in `ChatConfig` with default enabled
  - `should_persist_commands()` method on `Config` for checking persistence state
  - Default value: `true` (special commands are persisted by default)

### Tests

- **3 CLI parsing tests** in `src/cli.rs`:
  - `test_cli_parse_history_show_parses_id` - Verifies ID parameter parsing
  - `test_cli_parse_history_show_parses_raw_flag` - Verifies raw JSON flag parsing
  - `test_cli_parse_history_show_parses_limit` - Verifies message limit parsing

- **4 command handler tests** in `src/commands/history.rs`:
  - `test_show_conversation_formatted` - Verifies formatted output generation
  - `test_show_conversation_raw_json` - Verifies JSON serialization
  - `test_show_conversation_with_limit` - Verifies message limiting works correctly
  - `test_show_conversation_not_found` - Verifies error handling for missing conversations

- **3 configuration tests** in `src/config.rs`:
  - `test_config_should_persist_commands_default_on` - Verifies persistence defaults to true
  - `test_config_should_persist_commands_can_disable` - Verifies can be disabled
  - `test_chat_config_persist_special_commands_default` - Verifies field defaults to true

### Documentation

This document provides comprehensive implementation details for Phase 2 components.

## Implementation Details

### CLI Enhancement: `history show` Command

The `HistoryCommand` enum now includes a `Show` variant with three parameters:

```rust
Show {
    /// Conversation ID to display
    #[arg(short, long)]
    id: String,

    /// Output raw JSON format instead of formatted display
    #[arg(short, long)]
    raw: bool,

    /// Show only the last N messages (default: all)
    #[arg(short = 'n', long)]
    limit: Option<usize>,
}
```

This allows users to:
- View full conversation history: `xzatoma history show --id <id>`
- Export as JSON: `xzatoma history show --id <id> --raw`
- View recent messages only: `xzatoma history show --id <id> --limit 10`

### History Display Implementation

The `show_conversation()` function provides two output modes:

**Formatted Mode** (default):
- Human-readable display with clear section headers
- Colored output using `colored` crate
- Message index, role, tool information, and truncated content
- Helpful formatting for interactive inspection

**Raw JSON Mode**:
- Full JSON serialization of conversation metadata and messages
- Suitable for programmatic processing
- Includes message count and all message details

Each message displays:
- Index in conversation
- Role (user, assistant, system, tool)
- Tool call ID (for tool result messages)
- Tool calls summary with function names and IDs (for assistant messages)
- Message content (with truncation at 500 chars in formatted mode)

### Configuration: Special Command Persistence

The `ChatConfig` struct now includes:

```rust
pub struct ChatConfig {
    // ... existing fields
    
    /// Persist special commands in conversation history
    #[serde(default = "default_persist_special_commands")]
    pub persist_special_commands: bool,
}
```

Default behavior:
- Enabled by default (`default_persist_special_commands()` returns `true`)
- Can be disabled via YAML config: `chat.persist_special_commands: false`
- Can be overridden via environment variables or CLI (if implemented)

Helper method on `Config`:
```rust
pub fn should_persist_commands(&self) -> bool {
    self.agent.chat.persist_special_commands
}
```

### Design Decisions

**Decision 1: Placement in ChatConfig**
- Special command persistence is logically part of chat behavior
- Placed in `AgentConfig::chat` structure
- Provides natural grouping with other chat settings

**Decision 2: Default Enabled**
- Users benefit from audit trail of special commands
- Can be explicitly disabled if desired
- Non-invasive for message context (commands recorded as `system` role)

**Decision 3: Storage as System Messages**
- Special commands recorded as `system` role messages
- Does not pollute user/assistant message flow
- Can be filtered in future if needed
- Integrates seamlessly with existing message storage

## Testing

### Test Coverage

Phase 2 adds 10 new tests:

- 3 CLI tests for `history show` command parsing
- 4 handler tests for `show_conversation` function
- 3 configuration tests for persistence settings

All tests pass successfully:
```
test result: ok. 81 passed; 0 failed; 15 ignored
```

### Test Categories

**CLI Tests** verify argument parsing:
- ID parameter extraction
- Raw flag detection
- Limit parameter handling

**Handler Tests** verify display logic:
- Formatted output generation (colors, structure)
- JSON serialization and pretty-printing
- Message limiting (shows only last N)
- Error handling for missing conversations

**Configuration Tests** verify defaults:
- Persistence enabled by default
- Can be disabled via field modification
- ChatConfig field defaults correctly

### Test Patterns

Tests use temporary SQLite databases to avoid side effects:
```rust
let tmp = tempdir().expect("failed to create tempdir");
let db_path = tmp.path().join("history.db");
let storage = SqliteStorage::new_with_path(&db_path).expect("failed to create storage");
```

### Coverage

All new code paths are covered by tests:
- Success cases (formatted, JSON, with/without limit)
- Error cases (conversation not found)
- Configuration defaults and overrides
- CLI argument combinations

## Validation Results

### Quality Gates (All Passing)

```
cargo fmt --all
Result: SUCCESS - All code formatted

cargo check --all-targets --all-features
Result: SUCCESS - No compilation errors

cargo clippy --all-targets --all-features -- -D warnings
Result: SUCCESS - Zero warnings

cargo test --all-features
Result: SUCCESS - 81 tests passed, 15 ignored
```

### Manual Validation

Commands tested:
- `xzatoma history show --id <id>` - Displays formatted conversation
- `xzatoma history show --id <id> --raw` - Outputs JSON
- `xzatoma history show --id <id> --limit 5` - Shows last 5 messages
- `xzatoma history show --id nonexistent` - Returns clear error

Output verified:
- Formatting includes colors and clear headers
- JSON is properly formatted and valid
- Message limiting correctly shows appropriate subset
- Error messages are clear and actionable

## Next Steps

### Phase 3: Pruning Integrity

Will implement atomic removal of tool call/result pairs during pruning:
- Add `find_tool_results_for_call()` helper
- Ensure orphans are not created during pruning
- Insert summary messages when needed

### Future Enhancements

- Search functionality within history (`history search --query <pattern>`)
- Export to different formats (markdown, CSV)
- Statistics on conversation tokens and message distribution
- Integration with other analysis tools

## References

- Phase 1 Documentation: `docs/explanation/phase1_core_validation_implementation.md`
- Architecture Overview: `docs/explanation/architecture.md` (future)
- Configuration Reference: `docs/reference/configuration.md` (future)
- Implementation Plan: `docs/explanation/history_and_tool_integrity_implementation.md`
