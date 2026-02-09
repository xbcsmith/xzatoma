//! Replay subagent conversations for debugging and analysis
//!
//! This module provides commands to list, replay, and visualize
//! conversation history stored by the persistence system.

use crate::agent::ConversationStore;
use crate::error::Result;
use clap::Args;
use std::path::PathBuf;

/// Arguments for the replay command
///
/// Supports listing all conversations, replaying specific conversations,
/// and visualizing conversation trees (showing parent-child relationships).
#[derive(Debug, Args)]
pub struct ReplayArgs {
    /// Conversation ID to replay
    #[arg(long, short = 'i')]
    pub id: Option<String>,

    /// List all conversations
    #[arg(long, short = 'l')]
    pub list: bool,

    /// Path to conversation database
    #[arg(long, default_value = "~/.xzatoma/conversations.db")]
    pub db_path: PathBuf,

    /// Limit for list results
    #[arg(long, default_value = "10")]
    pub limit: usize,

    /// Offset for pagination
    #[arg(long, default_value = "0")]
    pub offset: usize,

    /// Show conversation tree (with nested subagents)
    #[arg(long, short = 't')]
    pub tree: bool,
}

/// Run the replay command
///
/// # Arguments
///
/// * `args` - Parsed command-line arguments
///
/// # Returns
///
/// Returns Ok(()) on success, or an error if replay operations fail
///
/// # Examples
///
/// ```rust
/// use xzatoma::commands::replay;
/// use std::path::PathBuf;
///
/// // Construct the args directly for documentation/testing purposes
/// let args = replay::ReplayArgs {
///     id: None,
///     list: true,
///     db_path: PathBuf::from("~/.xzatoma/conversations.db"),
///     limit: 10,
///     offset: 0,
///     tree: false,
/// };
/// assert!(args.list);
/// ```
pub async fn run_replay(args: ReplayArgs) -> Result<()> {
    // Expand tilde in path
    let db_path = if args.db_path.to_string_lossy().starts_with('~') {
        let home = std::env::var("HOME")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join(args.db_path.to_string_lossy().trim_start_matches("~/"))
    } else {
        args.db_path
    };

    let store = ConversationStore::new(&db_path)?;

    if args.list {
        list_conversations(&store, args.limit, args.offset)?;
    } else if let Some(id) = args.id {
        if args.tree {
            show_conversation_tree(&store, &id)?;
        } else {
            replay_conversation(&store, &id)?;
        }
    } else {
        eprintln!("Error: Must specify --list or --id");
        std::process::exit(1);
    }

    Ok(())
}

/// List all conversations with pagination
///
/// # Arguments
///
/// * `store` - Conversation store
/// * `limit` - Maximum number of records to display
/// * `offset` - Number of records to skip
///
/// # Returns
///
/// Returns Ok(()) on success
fn list_conversations(store: &ConversationStore, limit: usize, offset: usize) -> Result<()> {
    let records = store.list(limit, offset)?;

    println!("Conversations (showing {} starting at {}):", limit, offset);
    println!();

    for record in records {
        println!("ID:     {}", record.id);
        println!("Label:  {}", record.label);
        println!("Depth:  {}", record.depth);
        println!("Status: {}", record.metadata.completion_status);
        println!("Turns:  {}", record.metadata.turns_used);
        println!("Start:  {}", record.started_at);
        if let Some(parent_id) = &record.parent_id {
            println!("Parent: {}", parent_id);
        }
        println!();
    }

    Ok(())
}

/// Replay a specific conversation with full message history
///
/// # Arguments
///
/// * `store` - Conversation store
/// * `id` - Conversation ID to replay
///
/// # Returns
///
/// Returns Ok(()) on success, or error if conversation not found
fn replay_conversation(store: &ConversationStore, id: &str) -> Result<()> {
    match store.get(id)? {
        Some(record) => {
            println!("=== Conversation {} ===", record.id);
            println!("Label: {}", record.label);
            println!("Depth: {}", record.depth);
            println!("Started: {}", record.started_at);
            if let Some(completed_at) = &record.completed_at {
                println!("Completed: {}", completed_at);
            }
            println!();
            println!("Task: {}", record.metadata.task_prompt);
            println!();
            println!("=== Messages ===");
            println!();

            for (i, message) in record.messages.iter().enumerate() {
                println!("--- Message {} ({}) ---", i + 1, message.role);
                if let Some(content) = &message.content {
                    println!("{}", content);
                } else {
                    println!("(no content)");
                }
                if let Some(tool_calls) = &message.tool_calls {
                    println!("Tool calls: {:?}", tool_calls);
                }
                println!();
            }

            println!("=== Metadata ===");
            println!("Turns Used: {}", record.metadata.turns_used);
            println!("Tokens Consumed: {}", record.metadata.tokens_consumed);
            println!("Status: {}", record.metadata.completion_status);
            println!("Max Turns Reached: {}", record.metadata.max_turns_reached);
            println!("Allowed Tools: {:?}", record.metadata.allowed_tools);
        }
        None => {
            eprintln!("Error: Conversation {} not found", id);
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Display conversation tree showing parent-child relationships
///
/// # Arguments
///
/// * `store` - Conversation store
/// * `id` - Root conversation ID
///
/// # Returns
///
/// Returns Ok(()) on success
fn show_conversation_tree(store: &ConversationStore, id: &str) -> Result<()> {
    println!("Conversation tree:");
    print_tree(store, id, 0)?;
    Ok(())
}

/// Recursively print conversation tree structure
///
/// # Arguments
///
/// * `store` - Conversation store
/// * `id` - Conversation ID to print
/// * `indent` - Current indentation level
///
/// # Returns
///
/// Returns Ok(()) on success
fn print_tree(store: &ConversationStore, id: &str, indent: usize) -> Result<()> {
    let record = store.get(id)?.ok_or_else(|| {
        crate::error::XzatomaError::Storage(format!("Conversation {} not found", id))
    })?;

    let prefix = "  ".repeat(indent);
    println!(
        "{}├─ {} [{}] (depth={}, turns={})",
        prefix, record.id, record.label, record.depth, record.metadata.turns_used
    );

    let children = store.find_by_parent(id)?;
    for child in children {
        print_tree(store, &child.id, indent + 1)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replay_args_list_defaults() {
        let args = ReplayArgs {
            id: None,
            list: true,
            db_path: PathBuf::from("test.db"),
            limit: 10,
            offset: 0,
            tree: false,
        };
        assert!(args.list);
        assert!(args.id.is_none());
        assert!(!args.tree);
    }

    #[test]
    fn test_replay_args_replay_with_id() {
        let args = ReplayArgs {
            id: Some("test_id".to_string()),
            list: false,
            db_path: PathBuf::from("test.db"),
            limit: 10,
            offset: 0,
            tree: false,
        };
        assert!(!args.list);
        assert_eq!(args.id, Some("test_id".to_string()));
        assert!(!args.tree);
    }

    #[test]
    fn test_replay_args_tree_view() {
        let args = ReplayArgs {
            id: Some("test_id".to_string()),
            list: false,
            db_path: PathBuf::from("test.db"),
            limit: 10,
            offset: 0,
            tree: true,
        };
        assert!(args.tree);
        assert_eq!(args.id, Some("test_id".to_string()));
    }

    #[test]
    fn test_replay_args_pagination() {
        let args = ReplayArgs {
            id: None,
            list: true,
            db_path: PathBuf::from("test.db"),
            limit: 20,
            offset: 5,
            tree: false,
        };
        assert_eq!(args.limit, 20);
        assert_eq!(args.offset, 5);
    }
}
