# UUID Prefix Matching Fix for Conversation Resume

## Overview

Fixed two critical bugs related to conversation resume functionality:

1. **UUID Prefix Matching**: `xzatoma chat --resume <ID>` failed to resume conversations because of a mismatch between the displayed 8-character ID and the full UUID stored in the database.
2. **Readline History**: When resuming a conversation, the up/down arrow keys didn't work to recall previous user inputs because the readline history buffer wasn't populated from the loaded conversation.

## Problem Description

### Symptoms

When running `xzatoma history list`, conversations were displayed with truncated 8-character IDs (e.g., `21173421`). However, attempting to resume a conversation using this displayed ID would fail:

```bash
./target/debug/xzatoma chat --resume 21173421
# Output: "Conversation 21173421 not found, starting new one."
```

Yet the conversation was clearly visible in the history list with 2 messages.

### Root Cause

The conversation ID storage and display logic had an inconsistency:

1. **In the database**: Full UUIDs were stored (36 characters, e.g., `21173421-201f-4e56-87a0-8e13fc02f7e5`)
2. **In the display** (`src/commands/history.rs`): Only the first 8 characters were shown (`21173421`)
3. **In the lookup** (`src/storage/mod.rs`): The `load_conversation()` method performed exact matching on the full UUID, not prefix matching

When a user copied the 8-character ID from the display and used it with `--resume`, the lookup failed because it was searching for a conversation with `id = '21173421'` instead of `id LIKE '21173421%'`.

## Solution

Updated the `load_conversation()` and `delete_conversation()` methods in `src/storage/mod.rs` to support both full UUID matching and 8-character prefix matching:

### Implementation Details

```rust
/// Load a conversation by ID (supports full UUID or 8-char prefix)
pub fn load_conversation(&self, id: &str) -> Result<Option<LoadedConversation>> {
    // ...
    // Support both full UUID and 8-char prefix matching
    let query = if id.len() == 36 {
        // Full UUID provided
        "SELECT title, model, messages FROM conversations WHERE id = ?"
    } else {
        // Prefix matching (e.g., first 8 chars)
        "SELECT title, model, messages FROM conversations WHERE id LIKE ?"
    };

    let search_param = if id.len() == 36 {
        id.to_string()
    } else {
        format!("{}%", id)
    };
    // ...
}
```

The same pattern was applied to `delete_conversation()` to ensure consistency.

### Behavior

- **Full UUID input** (36 chars): Uses exact matching (`WHERE id = ?`)
- **Prefix input** (8 chars): Uses LIKE pattern matching (`WHERE id LIKE '8chars%'`)
- **Other lengths**: Falls back to LIKE pattern matching

This allows users to:

- Use the displayed 8-character ID: `xzatoma chat --resume 21173421`
- Use the full UUID: `xzatoma chat --resume 21173421-201f-4e56-87a0-8e13fc02f7e5`
- Show conversation details by prefix: `xzatoma history show --id 21173421`
- Delete conversations by prefix: `xzatoma history delete --id 21173421`

## Readline History Population Fix

### Additional Issue

Even after fixing the UUID prefix matching, users noticed that when resuming a conversation, the up/down arrow keys didn't show previous user inputs from that conversation. This is because the readline editor's history buffer was empty, even though the conversation messages were loaded into the agent.

### Root Cause

In `src/commands/mod.rs`, the readline editor was created fresh for each chat session:

```rust
// Create readline instance
let mut rl = DefaultEditor::new()?;
```

When resuming a conversation, the agent's conversation object contained all the previous messages, but these were never added to the readline history buffer. The readline library maintains its own separate history that enables up/down arrow navigation.

### Solution

Added code to populate the readline history with user messages from the resumed conversation:

```rust
// Create readline instance
let mut rl = DefaultEditor::new()?;

// Populate readline history with previous user inputs when resuming
if resume.is_some() {
    for msg in agent.conversation().messages() {
        if msg.role == "user" {
            if let Some(content) = &msg.content {
                // Add each user message to readline history so up/down arrows work
                let _ = rl.add_history_entry(content);
            }
        }
    }
}
```

### Behavior

Now when resuming a conversation:

- Press UP arrow → Shows most recent user input from that conversation
- Press UP again → Shows previous user input, and so on
- Users can easily re-run or modify previous prompts

## Components Delivered

### UUID Prefix Matching

- `src/storage/mod.rs` (260+ lines) - Updated `load_conversation()` and `delete_conversation()` methods with prefix matching support
- Three new tests validating the prefix matching behavior:
  - `test_load_conversation_by_full_uuid()` - Verify full UUID lookup still works
  - `test_load_conversation_by_8char_prefix()` - Verify prefix matching works
  - `test_delete_conversation_by_8char_prefix()` - Verify deletion by prefix works

### Readline History Population

- `src/commands/mod.rs` (12 lines added) - Populate readline history when resuming conversations
- Extracts user messages from loaded conversation and adds them to readline history buffer

## Testing

All 81 tests pass, including the new prefix matching tests:

```
test result: ok. 81 passed; 0 failed; 15 ignored
```

### Validation Results

- ✅ `cargo fmt --all` passed (no formatting issues)
- ✅ `cargo check --all-targets --all-features` passed (no compilation errors)
- ✅ `cargo clippy --all-targets --all-features -- -D warnings` shows zero warnings
- ✅ `cargo test --all-features` passed with 81 tests
- ✅ Manual testing confirms resume functionality works with 8-character IDs

### Manual Verification

**Before fix:**

```bash
./target/debug/xzatoma chat --resume 21173421
# Output: "Conversation 21173421 not found, starting new one."
```

**After fix:**

```bash
./target/debug/xzatoma chat --resume 21173421
# Output: "Resuming conversation: Explain what @src/commands/special_commands.rs ..."
# Press UP arrow: Shows "explain what @src/storage/types.rs does"
# Press UP again: Shows any earlier user inputs
```

The conversation now resumes successfully, with all messages loaded, and the readline history allows browsing previous user inputs with arrow keys.

## Usage Examples

### Resume using displayed ID (8 characters)

```bash
$ ./target/release/xzatoma history list
Conversation History:
+---------+-----------------------------------------+-----------+---------+-------+
| ID      | Title                                   | Model     | Messages | Time  |
| 21173421| Explain what @src/commands/special_... | llama3.2  | 2        | 22:10 |
+---------+-----------------------------------------+-----------+---------+-------+

$ ./target/release/xzatoma chat --resume 21173421
Resuming conversation: Explain what @src/commands/special_commands.rs ...
```

### Show conversation details by prefix

```bash
$ ./target/release/xzatoma history show --id 21173421
Conversation:
Explain what @src/commands/special_commands.rs ...
ID:
21173421
Model:
llama3.2:latest
Messages:
2 total
```

### Delete conversation by prefix

```bash
$ ./target/release/xzatoma history delete --id 21173421
Deleted conversation 21173421
```

## Design Rationale

### Why Prefix Matching?

1. **User Experience**: Users see 8-character IDs in the list output, so they expect those IDs to work with `--resume`
2. **Backward Compatibility**: Full UUID input still works for programmatic usage
3. **Ambiguity Handling**: If multiple conversations share the same 8-character prefix (extremely unlikely with UUIDs), the first match is returned; in practice, UUID prefix collisions are astronomically improbable
4. **Simplicity**: Avoids storing redundant ID fields or changing the database schema

### Length-Based Selection

The implementation uses ID length to determine the matching strategy:

- `len == 36`: Full UUID (standard UUID4 format: `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`)
- `len != 36`: Treat as prefix and use LIKE matching

This is simple, efficient, and requires no additional configuration.

## Impact

### Who Benefits

- All users resuming conversations (the `history --resume` feature was completely broken before)
- Users who need to manipulate conversation history by ID

### Scope

Internal change to storage layer; no public API changes to the Agent or Provider interfaces. The fix is transparent to external callers.

## References

- Storage implementation: `src/storage/mod.rs`
- History command: `src/commands/history.rs`
- Chat command resume logic: `src/commands/mod.rs` (lines 125-155)
- Architecture: `docs/explanation/architecture.md`

## Future Improvements

1. **Add configuration option**: Allow users to set preferred ID format (full UUID vs. prefix)
2. **Enhance ambiguity handling**: Return multiple matches if a prefix is ambiguous, allowing user selection
3. **Shorter prefixes**: Consider supporting shorter prefixes (4 chars) if needed for ergonomics
4. **Search by title**: Add ability to search/resume conversations by title substring in addition to ID
