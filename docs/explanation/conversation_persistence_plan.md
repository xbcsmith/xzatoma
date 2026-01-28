# Conversation Persistence Implementation Plan

## Overview

This plan documents the conversation persistence feature for xzatoma and outlines the remaining work to achieve complete test coverage and documentation. The core implementation is COMPLETE, but tests and comprehensive documentation are REQUIRED per AGENTS.md Rule 4 and Rule 5.

**Feature**: Save, list, resume, and delete conversation sessions with SQLite persistence.

**Status**: Phase 1 (Core Implementation) ✅ COMPLETE | Phase 2 (Tests) ❌ TODO | Phase 3 (Documentation) ❌ TODO

## Current State Analysis

### Existing Infrastructure

The conversation persistence feature has been **fully implemented** with the following components:

| Component                    | File Path                   | Status      | Line Range |
| ---------------------------- | --------------------------- | ----------- | ---------- |
| Storage Backend              | `src/storage/mod.rs`        | ✅ COMPLETE | L1-243     |
| Storage Types                | `src/storage/types.rs`      | ✅ COMPLETE | L1-18      |
| Conversation ID/Title        | `src/agent/conversation.rs` | ✅ COMPLETE | L83-84     |
| Conversation History Loading | `src/agent/conversation.rs` | ✅ COMPLETE | L140-167   |
| CLI Resume Flag              | `src/cli.rs`                | ✅ COMPLETE | L47-49     |
| CLI History Command          | `src/cli.rs`                | ✅ COMPLETE | L85-96     |
| History Command Handler      | `src/commands/history.rs`   | ✅ COMPLETE | L1-73      |
| Main Handler                 | `src/main.rs`               | ✅ COMPLETE | L108-114   |
| Chat Resume Logic            | `src/commands/mod.rs`       | ✅ COMPLETE | L126-169   |
| Auto-save on Each Turn       | `src/commands/mod.rs`       | ✅ COMPLETE | L436-445   |

**Dependencies Added** (in `Cargo.toml`):

- `rusqlite = { version = "0.38.0", features = ["bundled"] }` (L47)
- `directories = "6.0.0"` (L48)
- `uuid = { version = "1.20.0", features = ["v4", "serde"] }` (L49)

**Database Schema** (Implemented at `src/storage/mod.rs` L43-52):

```sql
CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    model TEXT,
    messages JSON NOT NULL
)
```

**Database Location**:

- Platform-specific: `~/.local/share/xzatoma/history.db` (Linux/macOS)
- Uses `directories` crate for cross-platform compatibility

### Identified Issues

| Issue                                  | Severity    | AGENTS.md Rule Violated          |
| -------------------------------------- | ----------- | -------------------------------- |
| No unit tests for `src/storage/mod.rs` | 🔴 CRITICAL | Rule 4 - Quality Gates           |
| No integration tests for persistence   | 🔴 CRITICAL | Rule 4 - Test Coverage >80%      |
| No implementation documentation        | 🔴 CRITICAL | Rule 5 - Documentation Mandatory |
| Manual verification only               | 🟡 MEDIUM   | AI-Optimized Standards           |

## Implementation Phases

### Phase 1: Core Implementation ✅ COMPLETE

**Status**: All tasks completed

#### Task 1.1: Dependencies ✅ DONE

- Added `rusqlite` with bundled feature
- Added `directories` for cross-platform paths
- Added `uuid` with v4 and serde features

#### Task 1.2: Storage Layer ✅ DONE

- Created `src/storage/mod.rs` with `SqliteStorage` struct
- Created `src/storage/types.rs` with `StoredSession` struct
- Implemented CRUD operations: save, load, list, delete

#### Task 1.3: Conversation Integration ✅ DONE

- Added `id: Uuid` field to `Conversation` struct
- Added `title: String` field to `Conversation` struct
- Implemented `with_history()` constructor for loading saved conversations

#### Task 1.4: CLI Integration ✅ DONE

- Added `--resume <ID>` flag to `Chat` command
- Created `History` command with `List` and `Delete` subcommands
- Wired up command handlers in `main.rs`

#### Task 1.5: Chat Command Integration ✅ DONE

- Implemented resume logic with storage initialization
- Added auto-save after each conversation turn
- Implemented automatic title generation from first message

#### Task 1.6: History Command Implementation ✅ DONE

- Created `handle_history()` function
- Implemented list display with formatted table
- Implemented delete with confirmation

---

### Phase 2: Test Coverage Implementation ❌ TODO

**Objective**: Achieve >80% test coverage for all persistence-related code per AGENTS.md Rule 4.

**Prerequisites**: None (Phase 1 complete)

**Estimated Effort**: ~550 lines of test code

#### Task 2.1: Storage Layer Unit Tests

**File**: `src/storage/mod.rs`

**Location**: Add at end of file after L243

**Action**: Create `#[cfg(test)] mod tests` with comprehensive test suite

**Required Tests** (12 total):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // Helper function
    fn create_test_storage() -> (SqliteStorage, TempDir) {
        // Create temporary directory for test database
        // Return storage instance and temp dir guard
    }

    // Test 1: Database initialization
    #[test]
    fn test_sqlite_storage_init_creates_table() {
        // Verify table created with correct schema
        // Query sqlite_master to confirm structure
    }

    // Test 2: Save new conversation
    #[test]
    fn test_save_conversation_creates_new_record() {
        // Save conversation, verify it exists in DB
        // Check all fields match expected values
    }

    // Test 3: Save updates existing conversation
    #[test]
    fn test_save_conversation_updates_existing_record() {
        // Save conversation twice
        // Verify updated_at changed, created_at preserved
    }

    // Test 4: Load existing conversation
    #[test]
    fn test_load_conversation_returns_data() {
        // Save then load conversation
        // Verify title, model, messages match
    }

    // Test 5: Load non-existent conversation
    #[test]
    fn test_load_conversation_returns_none_for_missing_id() {
        // Try to load non-existent ID
        // Verify returns None
    }

    // Test 6: List conversations ordered by updated_at DESC
    #[test]
    fn test_list_sessions_returns_ordered_by_updated_at() {
        // Save 3 conversations with different times
        // Verify list returns in DESC order
    }

    // Test 7: List empty database
    #[test]
    fn test_list_sessions_returns_empty_for_new_db() {
        // New database, call list
        // Verify empty Vec returned
    }

    // Test 8: Delete existing conversation
    #[test]
    fn test_delete_conversation_removes_record() {
        // Save conversation, delete it
        // Verify load returns None after delete
    }

    // Test 9: Delete non-existent conversation (idempotent)
    #[test]
    fn test_delete_conversation_is_idempotent() {
        // Delete non-existent ID
        // Verify no error, graceful handling
    }

    // Test 10: Message count calculation
    #[test]
    fn test_stored_session_calculates_message_count() {
        // Save conversation with N messages
        // Verify StoredSession.message_count == N
    }

    // Test 11: Preserve created_at on update
    #[test]
    fn test_save_conversation_preserves_created_at_on_update() {
        // Save, wait, save again
        // Verify created_at unchanged, updated_at changed
    }

    // Test 12: JSON serialization roundtrip
    #[test]
    fn test_messages_serialize_deserialize_roundtrip() {
        // Create complex messages with tool calls
        // Save, load, verify exact match
    }
}
```

**Validation Commands**:

```bash
# Run storage tests only
cargo test --lib storage::tests

# Expected output:
# test storage::tests::test_sqlite_storage_init_creates_table ... ok
# test storage::tests::test_save_conversation_creates_new_record ... ok
# [... 10 more tests ...]
# test result: ok. 12 passed; 0 failed
```

**Deliverables**:

- `src/storage/mod.rs` - Unit test module (~200 lines)

**Success Criteria**:

- [ ] All 12 tests implemented
- [ ] All tests pass: `cargo test --lib storage::tests`
- [ ] Coverage >80% for `src/storage/mod.rs`
- [ ] Zero clippy warnings
- [ ] Tests use tempfile for isolation
- [ ] No test pollution (each test cleans up)

#### Task 2.2: Conversation Persistence Integration Tests

**File**: `tests/integration_conversation_persistence.rs` (NEW)

**Action**: Create comprehensive integration test suite

**Required Tests** (7 total):

```rust
use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

// Test 1: New conversation gets auto-saved
#[tokio::test]
async fn test_conversation_auto_saves_after_message() {
    // Start chat, send message, verify DB created
    // Verify conversation exists in history
}

// Test 2: Resume loads correct conversation state
#[tokio::test]
async fn test_resume_loads_conversation_history() {
    // Create conversation with messages
    // Resume by ID, verify context loaded
    // Send new message, verify appended to history
}

// Test 3: Resume with invalid ID starts new conversation
#[tokio::test]
async fn test_resume_invalid_id_starts_new() {
    // Try to resume non-existent ID
    // Verify new conversation started
    // Verify error message displayed
}

// Test 4: Title generated from first message
#[tokio::test]
async fn test_title_generated_from_first_user_message() {
    // Send first message "Hello world"
    // Verify title == "Hello world"
}

// Test 5: Title truncates long messages
#[tokio::test]
async fn test_title_truncates_long_first_message() {
    // Send first message >50 chars
    // Verify title == first 47 chars + "..."
}

// Test 6: History list command shows all sessions
#[tokio::test]
async fn test_history_list_displays_sessions() {
    // Create 3 conversations
    // Run history list
    // Verify all 3 displayed with correct info
}

// Test 7: History delete removes session
#[tokio::test]
async fn test_history_delete_removes_session() {
    // Create conversation
    // Delete by ID
    // Verify history list doesn't show it
    // Verify load returns None
}
```

**Validation Commands**:

```bash
# Run integration tests
cargo test --test integration_conversation_persistence

# Expected output:
# test test_conversation_auto_saves_after_message ... ok
# test test_resume_loads_conversation_history ... ok
# [... 5 more tests ...]
# test result: ok. 7 passed; 0 failed
```

**Deliverables**:

- `tests/integration_conversation_persistence.rs` (~300 lines)

**Success Criteria**:

- [ ] All 7 integration tests implemented
- [ ] All tests pass: `cargo test --test integration_conversation_persistence`
- [ ] Tests use temporary databases (no pollution)
- [ ] Zero clippy warnings
- [ ] Tests cover all user-facing workflows

#### Task 2.3: CLI Command Tests

**File**: `src/cli.rs` (MODIFY)

**Location**: Add to existing test module after L464

**Action**: Add 3 tests for new CLI flags and commands

**Required Tests**:

```rust
#[test]
fn test_cli_parse_chat_with_resume() {
    let cli = Cli::try_parse_from(["xzatoma", "chat", "--resume", "abc123"]);
    assert!(cli.is_ok());
    let cli = cli.unwrap();
    if let Commands::Chat { resume, .. } = cli.command {
        assert_eq!(resume, Some("abc123".to_string()));
    } else {
        panic!("Expected Chat command");
    }
}

#[test]
fn test_cli_parse_history_list() {
    let cli = Cli::try_parse_from(["xzatoma", "history", "list"]);
    assert!(cli.is_ok());
    let cli = cli.unwrap();
    if let Commands::History { command } = cli.command {
        assert!(matches!(command, HistoryCommand::List));
    } else {
        panic!("Expected History command");
    }
}

#[test]
fn test_cli_parse_history_delete() {
    let cli = Cli::try_parse_from(["xzatoma", "history", "delete", "--id", "abc123"]);
    assert!(cli.is_ok());
    let cli = cli.unwrap();
    if let Commands::History { command } = cli.command {
        if let HistoryCommand::Delete { id } = command {
            assert_eq!(id, "abc123");
        } else {
            panic!("Expected Delete command");
        }
    } else {
        panic!("Expected History command");
    }
}
```

**Validation Commands**:

```bash
# Run CLI tests
cargo test --lib cli::tests

# Expected output should include:
# test cli::tests::test_cli_parse_chat_with_resume ... ok
# test cli::tests::test_cli_parse_history_list ... ok
# test cli::tests::test_cli_parse_history_delete ... ok
```

**Deliverables**:

- `src/cli.rs` - 3 additional tests (~50 lines)

**Success Criteria**:

- [ ] All 3 CLI tests implemented
- [ ] All tests pass: `cargo test --lib cli::tests`
- [ ] Zero clippy warnings

#### Task 2.4: Validation Requirements

**Execute in order and verify all pass**:

```bash
# Step 1: Format all code
cargo fmt --all
# Expected: No output (all files formatted)

# Step 2: Check compilation
cargo check --all-targets --all-features
# Expected: "Finished dev [unoptimized + debuginfo] target(s) in X.XXs"
#           0 errors

# Step 3: Lint with zero warnings
cargo clippy --all-targets --all-features -- -D warnings
# Expected: "Finished dev [unoptimized + debuginfo] target(s) in X.XXs"
#           0 warnings

# Step 4: Run all tests
cargo test --all-features
# Expected: "test result: ok. X passed; 0 failed; Y ignored; 0 measured; 0 filtered out"
#           Where X >= previous test count + 22 new tests

# Step 5: Verify test coverage >80% (optional - requires tarpaulin)
cargo tarpaulin --out Stdout --exclude-files 'tests/*'
# Expected: Coverage >= 80.00%
```

#### Task 2.5: Deliverables

| File                                            | Type              | Lines    | Tests  |
| ----------------------------------------------- | ----------------- | -------- | ------ |
| `src/storage/mod.rs`                            | Unit tests        | ~200     | 12     |
| `tests/integration_conversation_persistence.rs` | Integration tests | ~300     | 7      |
| `src/cli.rs`                                    | Unit tests        | ~50      | 3      |
| **Total**                                       |                   | **~550** | **22** |

#### Task 2.6: Success Criteria

- [ ] `cargo fmt --all` - passes with no changes
- [ ] `cargo check --all-targets --all-features` - 0 errors
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` - 0 warnings
- [ ] `cargo test --all-features` - all tests pass, >80% coverage
- [ ] All 22 new tests implemented and passing
- [ ] No test pollution (each test cleans up)
- [ ] Tests run in parallel safely
- [ ] Test count increased by exactly 22

---

### Phase 3: Documentation Implementation ❌ TODO

**Objective**: Create comprehensive documentation per AGENTS.md Rule 5 and Diataxis framework.

**Prerequisites**: Phase 2 complete (tests validate implementation)

**Estimated Effort**: ~480 lines of documentation

#### Task 3.1: Implementation Documentation

**File**: `docs/explanation/conversation_persistence_implementation.md` (NEW)

**Category**: Explanation (Diataxis framework)

**Action**: Create comprehensive implementation documentation

**Required Structure**:

````markdown
# Conversation Persistence Implementation

## Overview

Brief description of the conversation persistence feature - what it does, why it exists, and how it fits into xzatoma architecture. (50-100 words)

## Components Delivered

- `src/storage/mod.rs` (243 lines) - SQLite storage backend
- `src/storage/types.rs` (18 lines) - Session metadata types
- `src/agent/conversation.rs` (Modified L83-84, L140-167) - ID/title fields and history loading
- `src/cli.rs` (Modified L47-49, L85-96) - CLI integration
- `src/commands/history.rs` (73 lines) - History command handlers
- `src/commands/mod.rs` (Modified L126-169, L436-445) - Resume and auto-save logic
- `tests/integration_conversation_persistence.rs` (300 lines) - Integration tests
- `src/storage/mod.rs#tests` (200 lines) - Unit tests

Total: ~850 lines

## Implementation Details

### Storage Architecture

#### Database Schema

[SQL schema with field explanations]

#### Storage Operations

[Document save, load, list, delete with code examples from mod.rs]

#### Platform-Specific Paths

[Explain directories crate usage and path resolution]

### Conversation Integration

#### ID and Title Fields

[Show Conversation struct fields at L83-84]

#### Loading Saved Conversations

[Show with_history() method implementation at L140-167]

#### Auto-Save Mechanism

[Show auto-save logic from commands/mod.rs L436-445]

### CLI Integration

#### Resume Flag

[Show --resume flag definition and usage]

#### History Commands

[Show History command enum and handlers]

### Data Flow Diagram

```text
┌─────────────────────────────────────────┐
│ User starts chat with --resume <ID>    │
└──────────────┬──────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────┐
│ SqliteStorage::load_conversation(id)    │
│ (src/storage/mod.rs L122-152)           │
└──────────────┬──────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────┐
│ Conversation::with_history()            │
│ (src/agent/conversation.rs L140-167)    │
└──────────────┬──────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────┐
│ Agent executes with restored context    │
└──────────────┬──────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────┐
│ After each turn:                        │
│ SqliteStorage::save_conversation()      │
│ (src/commands/mod.rs L436-445)          │
└─────────────────────────────────────────┘
```
````

## Testing

### Unit Test Coverage

- 12 unit tests in `src/storage/mod.rs`
- Test categories: CRUD operations, edge cases, error handling
- Coverage: >80%

### Integration Test Coverage

- 7 integration tests in `tests/integration_conversation_persistence.rs`
- Test categories: End-to-end workflows, CLI integration
- Coverage: All user-facing workflows tested

### Test Execution Results

[Show actual cargo test output]

## Usage Examples

### Starting a New Conversation

[Complete example with commands and expected output]

### Listing Saved Conversations

[Complete example showing history list output]

### Resuming a Conversation

[Complete example with resume flag]

### Deleting a Conversation

[Complete example with delete command]

## Validation Results

- ✅ `cargo fmt --all` - passed
- ✅ `cargo check --all-targets --all-features` - 0 errors
- ✅ `cargo clippy --all-targets --all-features -- -D warnings` - 0 warnings
- ✅ `cargo test --all-features` - all tests pass, >80% coverage
- ✅ Documentation complete in `docs/explanation/`

## References

- Architecture: `docs/explanation/overview.md`
- Storage Module: `src/storage/mod.rs`
- AGENTS.md: Project development guidelines

````

**Deliverables**:
- `docs/explanation/conversation_persistence_implementation.md` (~300 lines)

**Success Criteria**:
- [ ] Documentation file created with all required sections
- [ ] All code examples include file paths and line numbers
- [ ] Data flow diagram included
- [ ] Usage examples are complete and runnable
- [ ] No emojis used (except in validation checklist)
- [ ] Filename uses lowercase_with_underscores.md

#### Task 3.2: Update implementations.md

**File**: `docs/explanation/implementations.md` (MODIFY)

**Action**: Add entry for conversation persistence feature

**Location**: Append to existing implementations list

**Content to Add**:

```markdown
## Conversation Persistence

**Status**: ✅ Implemented
**Version**: 0.1.0
**Documentation**: [conversation_persistence_implementation.md](./conversation_persistence_implementation.md)

### Summary

Persistent conversation history using SQLite, allowing users to save, list, resume, and delete chat sessions.

### Key Features

- Auto-save conversations after each turn
- Resume previous conversations by ID
- List all saved conversations with metadata
- Delete conversations
- Cross-platform storage location
- RFC-3339 timestamps for created_at/updated_at

### Files Modified

- `src/storage/mod.rs` - Storage backend
- `src/storage/types.rs` - Storage types
- `src/agent/conversation.rs` - Added persistence fields
- `src/cli.rs` - CLI integration
- `src/commands/history.rs` - History commands
- `src/commands/mod.rs` - Resume and auto-save logic

### Dependencies Added

- `rusqlite` v0.38.0 (bundled SQLite)
- `directories` v6.0.0
- `uuid` v1.20.0
````

**Deliverables**:

- `docs/explanation/implementations.md` (Modified - add ~25 lines)

**Success Criteria**:

- [ ] Entry added to implementations.md
- [ ] Link to implementation doc is correct
- [ ] Status marked as ✅ Implemented
- [ ] File follows existing format in implementations.md

#### Task 3.3: Add How-To Guide

**File**: `docs/how-to/manage_conversation_history.md` (NEW)

**Category**: How-To Guide (Diataxis framework)

**Action**: Create task-oriented guide for managing conversations

**Required Structure**:

```markdown
# How to Manage Conversation History

## Starting a New Conversation

Step-by-step instructions for starting a new chat session...

## Resuming a Previous Conversation

### Step 1: List Available Conversations

Command and expected output...

### Step 2: Copy the Conversation ID

Instructions...

### Step 3: Resume with --resume Flag

Command example...

## Viewing Conversation History

Instructions for using history list command...

## Deleting Old Conversations

Instructions for using history delete command...

## Troubleshooting

### Database file not created

Problem description and solution steps...

### Cannot resume conversation

Problem description and solution steps...

### Permission errors

Problem description and solution steps...
```

**Deliverables**:

- `docs/how-to/manage_conversation_history.md` (~150 lines)

**Success Criteria**:

- [ ] How-to guide created in correct directory
- [ ] Follows task-oriented structure
- [ ] Includes troubleshooting section
- [ ] Filename uses lowercase_with_underscores.md

#### Task 3.4: Update README.md

**File**: `README.md` (MODIFY)

**Action**: Add conversation persistence to features list

**Location**: Find "Features" section in README, add entry

**Content to Add**:

```markdown
- **Persistent Conversations**: Save, resume, and manage chat sessions
  - Auto-save after each turn
  - List conversation history with metadata
  - Resume previous conversations by ID
  - Delete old conversations
  - Cross-platform storage location
```

**Deliverables**:

- `README.md` (Modified - add ~5 lines)

**Success Criteria**:

- [ ] Feature added to README.md
- [ ] Follows existing format
- [ ] Placed in Features section

#### Task 3.5: Validation Requirements

**Execute commands to verify documentation quality**:

```bash
# Verify all documentation files created
ls -la docs/explanation/conversation_persistence_implementation.md
ls -la docs/how-to/manage_conversation_history.md

# Expected: Files exist with correct permissions

# Check for emojis (should only be in checklist markers)
grep -rn "[😀-🙏]" docs/explanation/conversation_persistence_implementation.md
grep -rn "[😀-🙏]" docs/how-to/manage_conversation_history.md

# Expected: No matches (or only ✅/❌ in checklists)

# Verify lowercase filenames
find docs -name "*[A-Z]*persistence*.md"

# Expected: No matches (all lowercase)

# Verify links work
grep -n "conversation_persistence_implementation.md" docs/explanation/implementations.md

# Expected: Match found with correct path
```

#### Task 3.6: Deliverables

| File                                                          | Type   | Lines    | Purpose                   |
| ------------------------------------------------------------- | ------ | -------- | ------------------------- |
| `docs/explanation/conversation_persistence_implementation.md` | NEW    | ~300     | Explanation documentation |
| `docs/explanation/implementations.md`                         | MODIFY | +25      | Implementation index      |
| `docs/how-to/manage_conversation_history.md`                  | NEW    | ~150     | Task-oriented guide       |
| `README.md`                                                   | MODIFY | +5       | Feature visibility        |
| **Total**                                                     |        | **~480** | Complete documentation    |

#### Task 3.7: Success Criteria

- [ ] Implementation documentation complete with all required sections
- [ ] How-to guide created in correct Diataxis category
- [ ] implementations.md updated with entry
- [ ] README.md updated with feature
- [ ] All filenames use lowercase_with_underscores.md
- [ ] No emojis in documentation (except ✅/❌ markers)
- [ ] All code examples include file paths and line numbers
- [ ] All links between docs are functional

---

## Overall Success Criteria

### Phase 1: Core Implementation ✅

- [x] All components implemented
- [x] Dependencies added
- [x] CLI integration complete
- [x] Auto-save functional

### Phase 2: Test Coverage ❌

- [ ] 12 unit tests for storage layer
- [ ] 7 integration tests for persistence
- [ ] 3 additional CLI tests
- [ ] > 80% code coverage achieved
- [ ] All tests passing
- [ ] Zero clippy warnings

### Phase 3: Documentation ❌

- [ ] Implementation documentation created
- [ ] How-to guide created
- [ ] implementations.md updated
- [ ] README.md updated
- [ ] All documentation follows AGENTS.md rules
- [ ] All documentation follows Diataxis framework

## Quality Gates (MANDATORY - AGENTS.md Rule 4)

Before marking ANY phase complete, ALL of these MUST pass:

```bash
# 1. Format code
cargo fmt --all
# Expected: No output

# 2. Check compilation
cargo check --all-targets --all-features
# Expected: 0 errors

# 3. Lint with zero warnings
cargo clippy --all-targets --all-features -- -D warnings
# Expected: 0 warnings

# 4. Run all tests
cargo test --all-features
# Expected: All pass, >80% coverage
```

## Implementation Order

1. **Phase 2 First** - Add tests for existing implementation

   - Reason: Validates that current code works correctly
   - Priority: CRITICAL per AGENTS.md Rule 4

2. **Phase 3 Second** - Create documentation
   - Reason: Documents tested, validated implementation
   - Priority: CRITICAL per AGENTS.md Rule 5

## Notes

- Phase 1 is COMPLETE - all code implemented and functional
- Database location: Platform-specific via `directories` crate
  - Linux/macOS: `~/.local/share/xzatoma/history.db`
  - Windows: `%APPDATA%\xbcsmith\xzatoma\data\history.db`
- Messages stored as JSON in SQLite for simplicity
- Auto-save happens synchronously after each turn
- Title auto-generated from first 50 chars of first user message
- Resume uses full UUID, but display shows first 8 chars
- Delete is idempotent (no error if ID does not exist)

## Risks and Mitigations

| Risk                                | Impact   | Mitigation                                        |
| ----------------------------------- | -------- | ------------------------------------------------- |
| Tests reveal bugs in implementation | HIGH     | Fix bugs before documentation phase               |
| Coverage <80%                       | CRITICAL | Add tests until >80% achieved                     |
| Documentation becomes stale         | MEDIUM   | Generate from code examples, include line numbers |
| Platform-specific path issues       | LOW      | Already using `directories` crate                 |

---

**Last Updated**: 2025-01-15
**Status**: Phase 1 ✅ COMPLETE, Phase 2 ❌ TODO, Phase 3 ❌ TODO
**Next Action**: Begin Phase 2 Task 2.1 - Storage Layer Unit Tests
