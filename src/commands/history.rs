use crate::cli::HistoryCommand;
use crate::error::Result;
use crate::storage::SqliteStorage;
use colored::Colorize;
use prettytable::{format, Table};

/// Handle history commands
pub fn handle_history(command: HistoryCommand) -> Result<()> {
    // Initialize storage
    // Note: We use the default location. If we need custom paths, we'd need to thread config here.
    let storage = SqliteStorage::new()?;
    handle_history_with_storage(&storage, command)
}

/// Helper that performs history operations using a provided storage instance.
///
/// This is intentionally separate from `handle_history(...)` so the behavior
/// can be tested by passing a test-local `SqliteStorage` (e.g., via `new_with_path`).
fn handle_history_with_storage(storage: &SqliteStorage, command: HistoryCommand) -> Result<()> {
    match command {
        HistoryCommand::List => {
            let sessions = storage.list_sessions()?;

            if sessions.is_empty() {
                println!("{}", "No conversation history found.".yellow());
                return Ok(());
            }

            let mut table = Table::new();
            table.set_format(*format::consts::FORMAT_BORDERS_ONLY);

            table.add_row(prettytable::row![
                "ID".bold(),
                "Title".bold(),
                "Model".bold(),
                "Messages".bold(),
                "Last Updated".bold()
            ]);

            for session in sessions {
                let id_short = &session.id[..8];
                let title = if session.title.len() > 40 {
                    format!("{}...", &session.title[..37])
                } else {
                    session.title
                };
                let model = session.model.unwrap_or_else(|| "-".to_string());
                let updated = session.updated_at.format("%Y-%m-%d %H:%M").to_string();

                table.add_row(prettytable::row![
                    id_short.cyan(),
                    title,
                    model,
                    session.message_count,
                    updated
                ]);
            }

            println!("\nConversation History:");
            table.printstd();
            println!();
            println!(
                "Use {} to resume a session.",
                "xzatoma chat --resume <ID>".cyan()
            );
            println!();
        }
        HistoryCommand::Delete { id } => {
            // Delete is idempotent; report to user for feedback.
            storage.delete_conversation(&id)?;
            println!("{}", format!("Deleted conversation {}", id).green());
        }
    }

    Ok(())
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;
    use crate::cli::HistoryCommand;
    use crate::providers::Message;
    use crate::storage::SqliteStorage;
    use assert_cmd::Command;
    use predicates::prelude::*;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[test]
    fn test_handle_history_list_displays_sessions() {
        // Setup temporary storage and populate it
        let tmp = tempdir().expect("failed to create tempdir");
        let db_path = tmp.path().join("history.db");
        let storage = SqliteStorage::new_with_path(&db_path).expect("failed to create storage");

        storage
            .save_conversation("session-1", "First", None, &[Message::user("one")])
            .expect("save1 failed");
        storage
            .save_conversation("session-2", "Second", None, &[Message::user("two")])
            .expect("save2 failed");

        // Run the CLI binary, pointing it at our temp DB, and assert output contains expected rows
        let mut cmd = Command::cargo_bin("xzatoma").expect("failed to find binary");
        cmd.arg("--storage-path")
            .arg(db_path.to_string_lossy().to_string())
            .arg("history")
            .arg("list");

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Conversation History"))
            .stdout(predicate::str::contains("First"))
            .stdout(predicate::str::contains("Second"));
    }

    #[test]
    fn test_handle_history_delete_removes_session() {
        // Setup temporary storage and create a session to delete
        let tmp = tempdir().expect("failed to create tempdir");
        let db_path = tmp.path().join("history.db");
        let storage = SqliteStorage::new_with_path(&db_path).expect("failed to create storage");

        let id = Uuid::new_v4().to_string();
        storage
            .save_conversation(&id, "ToDelete", None, &[Message::user("x")])
            .expect("save failed");

        // Ensure the session exists before delete
        assert!(storage
            .load_conversation(&id)
            .expect("load failed")
            .is_some());

        // Run the CLI binary to delete the session
        let mut cmd = Command::cargo_bin("xzatoma").expect("failed to find binary");
        cmd.arg("--storage-path")
            .arg(db_path.to_string_lossy().to_string())
            .arg("history")
            .arg("delete")
            .arg("--id")
            .arg(&id);

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Deleted conversation"));

        // Verify the session no longer exists in the DB
        assert!(storage
            .load_conversation(&id)
            .expect("load failed")
            .is_none());
    }
}
