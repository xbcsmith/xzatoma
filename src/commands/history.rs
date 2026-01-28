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
            // Check if it exists/try delete
            // Since we don't have an "exists" check exposed easily, delete is idempotent in SQL usually.
            // But we might want to know if it actually deleted anything.
            // For now, just run delete.
            storage.delete_conversation(&id)?;
            println!("{}", format!("Deleted conversation {}", id).green());
        }
    }

    Ok(())
}
