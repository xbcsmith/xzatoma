# How to Manage Conversation History

This how-to guide explains how to use XZatoma's conversation persistence features from the command line:
- List saved sessions
- Resume a saved session into an interactive chat
- Delete old sessions
- Troubleshoot common issues

Quick reference (commands):
- `xzatoma history list` — List saved sessions
- `xzatoma chat --resume <ID>` — Resume a saved session by ID
- `xzatoma history delete --id <ID>` — Delete a saved session

All CLI commands in this guide are implemented in the repository; implementation references are shown alongside examples for convenience.

---

## Starting a new conversation

1. Start an interactive session:
```bash
# Implemented in: src/commands/mod.rs (chat handling)
xzatoma chat
```

2. Send your first user message. After the assistant responds, the session is automatically persisted to the history database.

Notes:
- On first-turn conversations (≤ 2 messages) the saved session title is generated from your first user message and will be trimmed and truncated to 50 characters if necessary (see implementation: `src/commands/mod.rs` L417-430).
- The conversation is stored in a local SQLite database; the storage implementation is in `src/storage/mod.rs` (see `new`, `save_conversation`, `load_conversation`).

---

## Listing saved conversations

To see saved sessions:
```bash
# Implemented in: src/commands/history.rs (L1-60)
xzatoma history list
```

What you will see:
- A table containing a short 8-character ID (prefix of the UUID), the title, model, message count, and last updated timestamp.
- The table includes a hint showing how to resume a session: `xzatoma chat --resume <ID>`.

Important: The table displays only the first 8 characters of the full UUID. To resume a session you will need the full UUID (see "Resuming a previous conversation" below).

---

## Resuming a previous conversation

Steps:

1. List sessions to find the ID prefix:
```bash
xzatoma history list
# (ID column will show an 8-character prefix)
```

2. To resume you must supply the full UUID. If you don't have the full ID, retrieve it directly from the SQLite DB:

```bash
# DB path determined by ProjectDirs::from("com", "xbcsmith", "xzatoma")
# Example commands (pick the one that matches your OS):

# macOS
sqlite3 ~/Library/Application\ Support/xzatoma/history.db "SELECT id, title FROM conversations;"

# Linux
sqlite3 ~/.local/share/xzatoma/history.db "SELECT id, title FROM conversations;"

# Windows (PowerShell)
sqlite3 "$env:APPDATA\\xbcsmith\\xzatoma\\history.db" "SELECT id, title FROM conversations;"
```

(See storage creation: `src/storage/mod.rs` L24-39)

3. Resume the session using the full ID:
```bash
# Implemented in: src/commands/mod.rs (resume handling, L112-140)
xzatoma chat --resume 7f3b2aef-2c9e-4c2e-b7f4-a4585fbfa585
```

Behavior:
- If the ID is found, the conversation is reconstructed into the interactive chat (using `Conversation::with_history(...)`).
- If the ID is not found, XZatoma will start a new empty conversation.

---

## Deleting a conversation

To remove a saved session:

```bash
# Implemented in: src/commands/history.rs (Delete branch, L50-59)
xzatoma history delete --id 7f3b2aef-2c9e-4c2e-b7f4-a4585fbfa585
```

Notes:
- The delete operation is idempotent: deleting a missing ID is a no-op (it will not cause errors).
- Delete removes the record from the `conversations` table in `history.db`.

---

## Troubleshooting

### Database file not created
Symptoms: `xzatoma chat` runs but neither `xzatoma history list` shows sessions nor does the DB file exist.

Checks:
- Verify the data directory exists and is writable:
  - macOS: `ls -la ~/Library/Application\ Support/xzatoma`
  - Linux: `ls -la ~/.local/share/xzatoma`
  - Windows (PowerShell): `ls $env:APPDATA\\xbcsmith\\xzatoma`

- The storage initialization is implemented in `src/storage/mod.rs` (constructor `new` creates the directory; see L24-39).

If the directory is missing, create it and ensure permissions are correct:
```bash
# Example (Linux/macOS)
mkdir -p ~/.local/share/xzatoma
chmod 700 ~/.local/share/xzatoma
```

### Cannot resume a conversation
If `xzatoma chat --resume <ID>` starts a new session:
- Ensure you are using the full UUID (not the 8-char short prefix shown in `history list`). Use the sqlite command above to retrieve full IDs.
- Confirm the DB you are inspecting is the same DB that XZatoma uses (same user and same app data path).

Advanced: inspect the DB directly to see stored rows:
```bash
# Example (Linux)
sqlite3 ~/.local/share/xzatoma/history.db "SELECT id, title, updated_at FROM conversations ORDER BY updated_at DESC;"
```

### Permission errors
If XZatoma cannot write to the app data directory:
- Fix ownership: `chown $(whoami) ~/.local/share/xzatoma`
- Fix permissions: `chmod 700 ~/.local/share/xzatoma`

### Title missing or unexpectedly truncated
- Title generation logic: the first user message is used as the title only for first-turn conversations (≤ 2 messages). The title is trimmed and truncated to 50 characters with an ellipsis.
  - See the save/title logic: `src/commands/mod.rs` L417-430.

---

## Advanced: export or inspect messages
Messages are stored as JSON in the `messages` column. You can inspect them with `sqlite3` and `json_extract` functions:
```bash
# Example: show JSON for one conversation
sqlite3 ~/.local/share/xzatoma/history.db "SELECT id, title, json_extract(messages, '$') FROM conversations LIMIT 1;"
```

---

## Security & Privacy
- Conversation history is stored locally in plaintext JSON inside the SQLite database.
- If you store sensitive information in conversations, consider deleting those sessions (`xzatoma history delete --id <ID>`) or removing the DB entirely.
- Backup/export with `sqlite3` if you need to move or archive sessions.

---

## Examples (quick)
```bash
# List sessions
# Implemented in: src/commands/history.rs L25-39
xzatoma history list

# Resume a session (requires full UUID)
# Implemented in: src/commands/mod.rs L112-140
xzatoma chat --resume 7f3b2aef-2c9e-4c2e-b7f4-a4585fbfa585

# Delete a session
# Implemented in: src/commands/history.rs L50-59
xzatoma history delete --id 7f3b2aef-2c9e-4c2e-b7f4-a4585fbfa585
```

---

## Where to find implementation details & tests
- Storage: `src/storage/mod.rs` (table schema, save/load/list/delete) — see L67-77, L85-135, L148-163, L180-242, L245-255
- Chat save/resume: `src/commands/mod.rs` (resume and auto-save logic) — see L112-140 and L417-430
- History commands: `src/commands/history.rs` — list & delete handling
- Integration tests: `tests/conversation_persistence_integration.rs` — full end-to-end test coverage

---

If you run into a problem not covered here, please open an issue with:
- The command you ran
- Any error output (copy/paste)
- The output of `xzatoma history list` (if available)
- Platform (Linux/macOS/Windows) and the path to your `history.db` (if known)
