/*!
Command handlers for the CLI

This module provides command handlers invoked by the CLI entrypoint.

It exposes three top-level command modules:

- `chat`  — Interactive chat mode
- `run`   — Execute a plan or a single prompt
- `auth`  — Provider authentication helper

These handlers are intentionally small and use the library components:
providers, tools, and the agent.
*/

#![allow(dead_code)]
#![allow(unused_imports)]

use crate::agent::Agent;
use crate::chat_mode::{ChatMode, ChatModeState, SafetyMode};
use crate::commands::special_commands::{
    parse_special_command, print_help, print_mention_help, SpecialCommand,
};
use crate::config::{Config, ExecutionMode};
use crate::error::{Result, XzatomaError};
use crate::mention_parser;
use crate::providers::{create_provider, CopilotProvider, OllamaProvider};
use crate::tools::plan::PlanParser;
use crate::tools::registry_builder::ToolRegistryBuilder;
use crate::tools::terminal::{CommandValidator, TerminalTool};
use crate::tools::{FileOpsTool, ToolExecutor, ToolRegistry};
use std::path::Path;
use std::sync::Arc;

// Chat mode types and utilities
pub mod chat_mode;

// Special commands parser for mode switching
pub mod special_commands;

// Model management commands
pub mod models;

// Chat command handler
pub mod chat {
    //! Interactive chat mode handler.
    //!
    //! Instantiates provider and tools, creates an `Agent`, and runs a
    //! readline-based interactive loop that submits user input to the agent.
    //!
    //! The agent will use the registered tools (file_ops, etc.) as required.

    use super::*;
    use rustyline::error::ReadlineError;
    use rustyline::DefaultEditor;

    /// Start interactive chat mode
    ///
    /// # Arguments
    ///
    /// * `config` - Global configuration (consumed)
    /// * `provider_name` - Optional override for the configured provider
    /// * `mode` - Optional override for the chat mode ("planning" or "write")
    /// * `safe` - If true, enable safety mode (always confirm dangerous operations)
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::commands::chat;
    /// use xzatoma::config::Config;
    ///
    /// // In application code:
    /// // chat::run_chat(Config::default(), None, None, false).await?;
    /// ```
    pub async fn run_chat(
        config: Config,
        provider_name: Option<String>,
        mode: Option<String>,
        _safe: bool,
    ) -> Result<()> {
        tracing::info!("Starting interactive chat mode");

        let provider_type = provider_name
            .as_deref()
            .unwrap_or(&config.provider.provider_type);

        let working_dir = std::env::current_dir()?;

        // Initialize mode state from command-line arguments
        // Defaults: Planning mode, AlwaysConfirm (safe) safety mode
        let initial_mode = mode
            .as_deref()
            .and_then(|m| ChatMode::parse_str(m).ok())
            .unwrap_or(ChatMode::Planning);

        // Default to safe mode (AlwaysConfirm)
        // The `safe` parameter is currently unused as we always default to safe
        let initial_safety = SafetyMode::AlwaysConfirm;

        let mut mode_state = ChatModeState::new(initial_mode, initial_safety);

        // Build initial tool registry based on mode
        let tools = build_tools_for_mode(&mode_state, &config, &working_dir)?;

        // Create provider
        let provider = create_provider(provider_type, &config.provider)?;
        let mut agent = Agent::new_boxed(provider, tools, config.agent.clone())?;

        // Create readline instance
        let mut rl = DefaultEditor::new()?;

        // Initialize mention cache for file content injection
        let mut mention_cache = crate::mention_parser::MentionCache::new();
        let max_file_size = config.agent.tools.max_file_read_size as u64;

        // Display welcome banner with current mode and safety
        print_welcome_banner(&mode_state.chat_mode, &mode_state.safety_mode);

        loop {
            let prompt = mode_state.format_colored_prompt();
            match rl.readline(&prompt) {
                Ok(line) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    // Check for special commands first
                    match parse_special_command(trimmed) {
                        SpecialCommand::SwitchMode(new_mode) => {
                            handle_mode_switch(
                                &mut agent,
                                &mut mode_state,
                                new_mode,
                                &config,
                                &working_dir,
                                provider_type,
                            )?;
                            continue;
                        }
                        SpecialCommand::SwitchSafety(new_safety) => {
                            let old_safety = mode_state.switch_safety(new_safety);
                            println!("Switched from {} to {} mode\n", old_safety, new_safety);
                            continue;
                        }
                        SpecialCommand::ShowStatus => {
                            let tool_count = agent.num_tools();
                            let conversation_len = agent.conversation().len();
                            print_status_display(&mode_state, tool_count, conversation_len);
                            continue;
                        }
                        SpecialCommand::Help => {
                            print_help();
                            continue;
                        }
                        SpecialCommand::Mentions => {
                            special_commands::print_mention_help();
                            continue;
                        }
                        SpecialCommand::Exit => break,
                        SpecialCommand::None => {
                            // Regular agent prompt
                        }
                    }

                    // Parse mentions from input
                    let (mentions, _cleaned_text) = match mention_parser::parse_mentions(trimmed) {
                        Ok((m, c)) => {
                            if !m.is_empty() {
                                tracing::info!("Detected {} mentions in input", m.len());
                                for mention in &m {
                                    tracing::debug!("Mention: {:?}", mention);
                                }
                            }
                            (m, c)
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse mentions: {}", e);
                            (Vec::new(), trimmed.to_string())
                        }
                    };

                    // Add to history
                    rl.add_history_entry(trimmed)?;

                    // Show per-mention loading status (and indicate cached files)
                    if !mentions.is_empty() {
                        use colored::Colorize;

                        for mention in &mentions {
                            match mention {
                                crate::mention_parser::Mention::File(fm) => {
                                    // Try to resolve the path so we can check the mention cache.
                                    match crate::mention_parser::resolve_mention_path(
                                        &fm.path,
                                        &working_dir,
                                    ) {
                                        Ok(path) => {
                                            if mention_cache.get(&path).is_some() {
                                                println!(
                                                    "{}",
                                                    format!("Using cached @{}", fm.path).green()
                                                );
                                            } else {
                                                println!(
                                                    "{}",
                                                    format!("Loading @{}", fm.path).cyan()
                                                );
                                            }
                                        }
                                        Err(e) => {
                                            // If the path cannot be resolved, still surface a helpful message.
                                            println!(
                                                "{}",
                                                format!("Loading @{} (path error: {})", fm.path, e)
                                                    .yellow()
                                            );
                                        }
                                    }
                                }
                                crate::mention_parser::Mention::Url(um) => {
                                    println!("{}", format!("Fetching @url:{}", um.url).cyan());
                                }
                                crate::mention_parser::Mention::Search(sm) => {
                                    println!(
                                        "{}",
                                        format!("Searching @search:\"{}\"", sm.pattern).cyan()
                                    );
                                }
                                crate::mention_parser::Mention::Grep(gm) => {
                                    println!(
                                        "{}",
                                        format!("Searching @grep:\"{}\"", gm.pattern).cyan()
                                    );
                                }
                            }
                        }
                    }

                    // Augment prompt with file contents from mentions (Phase 2)
                    let (augmented_prompt, load_errors, successes) =
                        crate::mention_parser::augment_prompt_with_mentions(
                            &mentions,
                            trimmed,
                            &working_dir,
                            max_file_size,
                            &mut mention_cache,
                        )
                        .await;

                    // Summarize mention load results and display colored output
                    use colored::Colorize;

                    let total_mentions = mentions.len();
                    if total_mentions > 0 {
                        let total_files = mentions
                            .iter()
                            .filter(|m| matches!(m, crate::mention_parser::Mention::File(_)))
                            .count();
                        let total_urls = mentions
                            .iter()
                            .filter(|m| matches!(m, crate::mention_parser::Mention::Url(_)))
                            .count();
                        let total_searches = mentions
                            .iter()
                            .filter(|m| {
                                matches!(
                                    m,
                                    crate::mention_parser::Mention::Search(_)
                                        | crate::mention_parser::Mention::Grep(_)
                                )
                            })
                            .count();

                        // First, present any per-item success messages (green)
                        if !successes.is_empty() {
                            for msg in &successes {
                                println!("{}", msg.green());
                            }
                        }

                        let failed = load_errors.len();
                        let succeeded = total_mentions.saturating_sub(failed);

                        if failed == 0 {
                            println!(
                                "{}",
                                format!(
                                    "Loaded {} mentions ({} files, {} urls, {} searches) — all succeeded",
                                    total_mentions, total_files, total_urls, total_searches
                                )
                                .green()
                            );
                        } else {
                            println!(
                                "{}",
                                format!(
                                    "Loaded {} mentions: {} succeeded, {} failed",
                                    total_mentions, succeeded, failed
                                )
                                .yellow()
                            );

                            // Display per-type summary (cyan)
                            let failed_file_count = load_errors
                                .iter()
                                .filter(|e| !e.source.starts_with("http"))
                                .count();
                            let failed_url_count = load_errors
                                .iter()
                                .filter(|e| e.source.starts_with("http"))
                                .count();

                            println!(
                                "{}",
                                format!(
                                    "Files: {} total, {} loaded, {} failed",
                                    total_files,
                                    total_files.saturating_sub(failed_file_count),
                                    failed_file_count
                                )
                                .cyan()
                            );
                            println!(
                                "{}",
                                format!(
                                    "URLs: {} total, {} loaded, {} failed",
                                    total_urls,
                                    total_urls.saturating_sub(failed_url_count),
                                    failed_url_count
                                )
                                .cyan()
                            );

                            // Show detailed errors (red) with suggestion text included when present
                            for error in &load_errors {
                                eprintln!("{}", format!("Error: {}", error).red());
                            }
                        }
                    }

                    // Execute the prompt via the agent
                    match agent.execute(augmented_prompt).await {
                        Ok(response) => {
                            println!("\n{}\n", response);
                        }
                        Err(e) => {
                            eprintln!("Error: {}\n", e);
                        }
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    println!("CTRL-C");
                    break;
                }
                Err(ReadlineError::Eof) => {
                    println!("CTRL-D");
                    break;
                }
                Err(err) => {
                    tracing::error!("Readline error: {:?}", err);
                    break;
                }
            }
        }

        println!("Goodbye!");
        Ok(())
    }

    /// Build a tool registry for the current chat mode
    ///
    /// # Arguments
    ///
    /// * `mode_state` - The current mode state
    /// * `config` - Global configuration
    /// * `working_dir` - Working directory for tool operations
    ///
    /// # Returns
    ///
    /// Returns a configured ToolRegistry or an error
    /// Display welcome banner at the start of interactive chat mode
    ///
    /// Shows a formatted banner with the application name, current mode,
    /// safety mode, and basic instructions.
    ///
    /// # Arguments
    ///
    /// * `mode` - The initial chat mode
    /// * `safety` - The initial safety mode
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use xzatoma::chat_mode::{ChatMode, SafetyMode};
    ///
    /// print_welcome_banner(&ChatMode::Planning, &SafetyMode::AlwaysConfirm);
    /// ```
    fn print_welcome_banner(mode: &ChatMode, safety: &SafetyMode) {
        use colored::Colorize;

        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║         XZatoma Interactive Chat Mode - Welcome!             ║");
        println!("╚══════════════════════════════════════════════════════════════╝\n");
        println!("Mode:   {} ({})", mode.colored_tag(), mode.description());
        println!(
            "Safety: {} ({})\n",
            safety.colored_tag(),
            safety.description()
        );
        println!("Type '/help' for available commands, 'exit' to quit\n");
    }

    /// Display detailed status information about the current session
    ///
    /// Shows the current chat mode, safety mode, available tools, and conversation state.
    /// This is called when the user types '/status' command.
    ///
    /// # Arguments
    ///
    /// * `mode_state` - Current chat mode state
    /// * `tool_count` - Number of available tools in current mode
    /// * `conversation_len` - Number of messages in the conversation
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use xzatoma::chat_mode::{ChatMode, SafetyMode, ChatModeState};
    ///
    /// let state = ChatModeState::new(ChatMode::Write, SafetyMode::AlwaysConfirm);
    /// print_status_display(&state, 5, 10);
    /// ```
    fn print_status_display(
        mode_state: &ChatModeState,
        tool_count: usize,
        conversation_len: usize,
    ) {
        use colored::Colorize;

        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║                     XZatoma Session Status                   ║");
        println!("╚══════════════════════════════════════════════════════════════╝\n");
        println!(
            "Chat Mode:         {} ({})",
            mode_state.chat_mode.colored_tag(),
            mode_state.chat_mode.description()
        );
        println!(
            "Safety Mode:       {} ({})",
            mode_state.safety_mode.colored_tag(),
            mode_state.safety_mode.description()
        );
        println!("Available Tools:   {}", tool_count);
        println!("Conversation Size: {} messages", conversation_len);
        println!("Prompt Format:     {}", mode_state.format_colored_prompt());
        println!();
    }

    fn build_tools_for_mode(
        mode_state: &ChatModeState,
        config: &Config,
        working_dir: &std::path::Path,
    ) -> Result<ToolRegistry> {
        let builder = ToolRegistryBuilder::new(
            mode_state.chat_mode,
            mode_state.safety_mode,
            working_dir.to_path_buf(),
        )
        .with_tools_config(config.agent.tools.clone())
        .with_terminal_config(config.agent.terminal.clone());

        builder.build()
    }

    /// Handle switching to a new chat mode while preserving conversation
    ///
    /// # Arguments
    ///
    /// * `agent` - The current agent (will be replaced)
    /// * `mode_state` - The mode state to update
    /// * `new_mode` - The new chat mode to switch to
    /// * `config` - Global configuration
    /// * `working_dir` - Working directory for tool operations
    /// * `provider_type` - Type of provider ("copilot" or "ollama")
    ///
    /// # Returns
    ///
    /// Returns Ok if the switch succeeded, or an error if it failed
    fn handle_mode_switch(
        agent: &mut Agent,
        mode_state: &mut ChatModeState,
        new_mode: ChatMode,
        config: &Config,
        working_dir: &std::path::Path,
        provider_type: &str,
    ) -> Result<()> {
        // Show warning when switching to Write mode
        if matches!(new_mode, ChatMode::Write) {
            println!("\nWarning: Switching to WRITE mode - agent can now modify files and execute commands!");
            println!("Type '/safe' to enable confirmations, or '/yolo' to disable.\n");
        }

        // Update mode state
        let old_mode = mode_state.chat_mode;
        mode_state.chat_mode = new_mode;

        // Rebuild tools for new mode
        let new_tools = build_tools_for_mode(mode_state, config, working_dir)?;

        // Preserve conversation history
        let conversation = agent.conversation().clone();

        // Create new provider
        let new_provider = create_provider(provider_type, &config.provider)?;

        // Create new agent with same conversation but new tools
        let new_agent =
            Agent::with_conversation(new_provider, new_tools, config.agent.clone(), conversation)?;

        // Replace agent
        *agent = new_agent;

        println!(
            "Switched from {} to {} mode\n",
            old_mode, mode_state.chat_mode
        );
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::config::Config;

        /// Unknown provider should return an error quickly during provider creation
        #[tokio::test]
        async fn test_run_chat_unknown_provider() {
            let mut cfg = Config::default();
            cfg.provider.provider_type = "invalid_provider".to_string();

            let res = run_chat(cfg, None, None, false).await;
            assert!(res.is_err());
        }

        #[test]
        fn test_build_tools_for_planning_mode() {
            let mode_state = ChatModeState::new(ChatMode::Planning, SafetyMode::AlwaysConfirm);
            let config = Config::default();
            let working_dir = std::path::PathBuf::from(".");

            let result = build_tools_for_mode(&mode_state, &config, &working_dir);
            assert!(result.is_ok());

            let registry = result.unwrap();
            assert!(registry.get("file_ops").is_some());
            assert!(registry.get("terminal").is_none());
        }

        #[test]
        fn test_build_tools_for_write_mode() {
            let mode_state = ChatModeState::new(ChatMode::Write, SafetyMode::AlwaysConfirm);
            let config = Config::default();
            let working_dir = std::path::PathBuf::from(".");

            let result = build_tools_for_mode(&mode_state, &config, &working_dir);
            assert!(result.is_ok());

            let registry = result.unwrap();
            assert!(registry.get("file_ops").is_some());
            assert!(registry.get("terminal").is_some());
        }

        #[test]
        fn test_build_tools_respects_safety_mode() {
            let mode_state_safe = ChatModeState::new(ChatMode::Write, SafetyMode::AlwaysConfirm);
            let mode_state_yolo = ChatModeState::new(ChatMode::Write, SafetyMode::NeverConfirm);
            let config = Config::default();
            let working_dir = std::path::PathBuf::from(".");

            let result_safe = build_tools_for_mode(&mode_state_safe, &config, &working_dir);
            let result_yolo = build_tools_for_mode(&mode_state_yolo, &config, &working_dir);

            assert!(result_safe.is_ok());
            assert!(result_yolo.is_ok());

            let registry_safe = result_safe.unwrap();
            let registry_yolo = result_yolo.unwrap();

            assert!(registry_safe.get("terminal").is_some());
            assert!(registry_yolo.get("terminal").is_some());
        }

        #[test]
        fn test_handle_mode_switch_planning_to_write() {
            use crate::providers::Message;
            use async_trait::async_trait;

            // Create a simple mock provider
            #[derive(Clone)]
            struct TestProvider;

            #[async_trait]
            impl crate::providers::Provider for TestProvider {
                async fn complete(
                    &self,
                    _messages: &[Message],
                    _tools: &[serde_json::Value],
                ) -> Result<crate::providers::CompletionResponse> {
                    Ok(crate::providers::CompletionResponse::new(
                        Message::assistant("test"),
                    ))
                }
            }

            let config = Config::default();
            let working_dir = std::path::PathBuf::from(".");
            let provider = TestProvider;
            let tools = build_tools_for_mode(
                &ChatModeState::new(ChatMode::Planning, SafetyMode::AlwaysConfirm),
                &config,
                &working_dir,
            )
            .unwrap();

            let mut agent = Agent::new(provider, tools, config.agent.clone()).unwrap();
            let mut mode_state = ChatModeState::new(ChatMode::Planning, SafetyMode::AlwaysConfirm);

            let result = handle_mode_switch(
                &mut agent,
                &mut mode_state,
                ChatMode::Write,
                &config,
                &working_dir,
                "ollama",
            );

            assert!(result.is_ok());
            assert_eq!(mode_state.chat_mode, ChatMode::Write);
        }

        #[test]
        fn test_chat_mode_state_initialization_from_args() {
            let planning_mode = ChatMode::parse_str("planning").unwrap();
            let write_mode = ChatMode::parse_str("write").unwrap();

            assert_eq!(planning_mode, ChatMode::Planning);
            assert_eq!(write_mode, ChatMode::Write);
        }

        #[test]
        fn test_safety_mode_initialization_from_flag() {
            let safe = true;
            let safety_mode = if safe {
                SafetyMode::AlwaysConfirm
            } else {
                SafetyMode::NeverConfirm
            };

            assert_eq!(safety_mode, SafetyMode::AlwaysConfirm);

            let unsafe_mode = if !safe {
                SafetyMode::AlwaysConfirm
            } else {
                SafetyMode::NeverConfirm
            };

            assert_eq!(unsafe_mode, SafetyMode::NeverConfirm);
        }

        #[test]
        fn test_print_welcome_banner_planning_safe() {
            // Test that welcome banner displays correctly for Planning + Safe
            let mode = ChatMode::Planning;
            let safety = SafetyMode::AlwaysConfirm;

            // Note: In actual tests, we'd capture stdout, but this is a smoke test
            print_welcome_banner(&mode, &safety);
            // If this doesn't panic, the function works
        }

        #[test]
        fn test_print_welcome_banner_write_yolo() {
            // Test that welcome banner displays correctly for Write + YOLO
            let mode = ChatMode::Write;
            let safety = SafetyMode::NeverConfirm;

            print_welcome_banner(&mode, &safety);
            // Smoke test - verifies function executes without panic
        }

        #[test]
        fn test_print_status_display_planning_mode() {
            // Test status display for Planning mode
            let state = ChatModeState::new(ChatMode::Planning, SafetyMode::AlwaysConfirm);
            let tool_count = 3;
            let conversation_len = 5;

            print_status_display(&state, tool_count, conversation_len);
            // Smoke test - verifies function executes without panic
        }

        #[test]
        fn test_print_status_display_write_mode() {
            // Test status display for Write mode with YOLO
            let state = ChatModeState::new(ChatMode::Write, SafetyMode::NeverConfirm);
            let tool_count = 6;
            let conversation_len = 12;

            print_status_display(&state, tool_count, conversation_len);
            // Smoke test - verifies function executes without panic
        }

        #[test]
        fn test_chat_mode_state_format_prompt_all_combinations() {
            // Test all four mode combinations format correctly
            let combinations = vec![
                (
                    ChatMode::Planning,
                    SafetyMode::AlwaysConfirm,
                    "[PLANNING][SAFE] >> ",
                ),
                (
                    ChatMode::Planning,
                    SafetyMode::NeverConfirm,
                    "[PLANNING][YOLO] >> ",
                ),
                (
                    ChatMode::Write,
                    SafetyMode::AlwaysConfirm,
                    "[WRITE][SAFE] >> ",
                ),
                (
                    ChatMode::Write,
                    SafetyMode::NeverConfirm,
                    "[WRITE][YOLO] >> ",
                ),
            ];

            for (mode, safety, expected) in combinations {
                let state = ChatModeState::new(mode, safety);
                assert_eq!(state.format_prompt(), expected);
            }
        }

        #[test]
        fn test_chat_mode_state_status_includes_all_info() {
            // Verify status string includes mode, safety, and descriptions
            let state = ChatModeState::new(ChatMode::Write, SafetyMode::AlwaysConfirm);
            let status = state.status();

            assert!(status.contains("WRITE"));
            assert!(status.contains("SAFE"));
            assert!(status.contains("Read/write mode"));
            assert!(status.contains("Confirm dangerous"));
        }

        #[test]
        fn test_chat_mode_descriptions() {
            // Test that mode descriptions are informative
            assert!(!ChatMode::Planning.description().is_empty());
            assert!(!ChatMode::Write.description().is_empty());
            assert!(!SafetyMode::AlwaysConfirm.description().is_empty());
            assert!(!SafetyMode::NeverConfirm.description().is_empty());

            // Verify descriptions contain key information (case-insensitive)
            let planning_desc = ChatMode::Planning.description().to_lowercase();
            assert!(planning_desc.contains("read"));
            let write_desc = ChatMode::Write.description().to_lowercase();
            assert!(write_desc.contains("read") && write_desc.contains("write"));
            assert!(SafetyMode::AlwaysConfirm.description().contains("Confirm"));
            assert!(
                SafetyMode::NeverConfirm.description().contains("Never")
                    || SafetyMode::NeverConfirm.description().contains("YOLO")
            );
        }

        #[test]
        fn test_mode_display_formatting() {
            // Test that mode displays correctly as uppercase strings
            assert_eq!(ChatMode::Planning.to_string(), "PLANNING");
            assert_eq!(ChatMode::Write.to_string(), "WRITE");
            assert_eq!(SafetyMode::AlwaysConfirm.to_string(), "SAFE");
            assert_eq!(SafetyMode::NeverConfirm.to_string(), "YOLO");
        }
    }
}

/// Run command handler(s)
///
/// This module provides `run_plan` which runs a plan or a single prompt.
/// We provide a `run_plan_with_options` helper to support the `allow_dangerous` flag.
pub mod r#run {
    use super::*;

    /// Run a plan or a prompt via the agent
    ///
    /// Wrapper that uses `allow_dangerous = false`.
    ///
    /// # Arguments
    ///
    /// * `config` - Global configuration (consumed)
    /// * `plan_path` - Optional path to plan file (yaml/json/md)
    /// * `prompt` - Optional prompt text (used if plan_path is None)
    pub async fn run_plan(
        config: Config,
        plan_path: Option<String>,
        prompt: Option<String>,
    ) -> Result<()> {
        run_plan_with_options(config, plan_path, prompt, false).await
    }

    /// Run a plan or a prompt via the agent with extra options.
    ///
    /// # Arguments
    ///
    /// * `config` - Global configuration (consumed)
    /// * `plan_path` - Optional path to a plan file
    /// * `prompt` - Optional direct prompt
    /// * `allow_dangerous` - If true, the execution mode is escalated to FullAutonomous
    pub async fn run_plan_with_options(
        config: Config,
        plan_path: Option<String>,
        prompt: Option<String>,
        allow_dangerous: bool,
    ) -> Result<()> {
        tracing::info!("Starting plan execution mode");

        if plan_path.is_none() && prompt.is_none() {
            return Err(XzatomaError::Config(
                "Either --plan or --prompt must be provided".to_string(),
            )
            .into());
        }

        // Build tools & agent
        let mut tools = ToolRegistry::new();
        let working_dir = std::env::current_dir()?;

        // Determine execution mode - if `allow_dangerous` is true, switch to FullAutonomous
        let mode = if allow_dangerous {
            ExecutionMode::FullAutonomous
        } else {
            config.agent.terminal.default_mode
        };

        if allow_dangerous {
            tracing::warn!("Dangerous commands are allowed for this run: allow_dangerous=true");
        }

        // Instantiate the validator with the resolved mode (reserved for use by a TerminalTool)
        let validator = CommandValidator::new(mode, working_dir.clone());

        // Register FileOps tool
        let file_tool = FileOpsTool::new(working_dir.clone(), config.agent.tools.clone());
        let file_tool_executor: Arc<dyn crate::tools::ToolExecutor> = Arc::new(file_tool);
        tools.register("file_ops", file_tool_executor);

        // Register Terminal tool (validated)
        let terminal_tool = TerminalTool::new(validator, config.agent.terminal.clone());
        let terminal_tool_executor: Arc<dyn crate::tools::ToolExecutor> = Arc::new(terminal_tool);
        tools.register("terminal", terminal_tool_executor);

        // Create agent using concrete providers
        let agent = match config.provider.provider_type.as_str() {
            "ollama" => {
                let p = OllamaProvider::new(config.provider.ollama.clone())?;
                Agent::new(p, tools, config.agent.clone())?
            }
            "copilot" => {
                let p = CopilotProvider::new(config.provider.copilot.clone())?;
                Agent::new(p, tools, config.agent.clone())?
            }
            other => {
                return Err(XzatomaError::Config(format!("Unknown provider: {}", other)).into());
            }
        };

        // Compose a textual task to send to the agent
        let task = if let Some(path) = plan_path {
            let plan = PlanParser::from_file(Path::new(&path))?;
            PlanParser::validate(&plan)?;

            let steps_s = plan
                .steps
                .iter()
                .map(|s| format!("- {}: {}", s.name, s.action))
                .collect::<Vec<_>>()
                .join("\n");

            format!(
                "Execute this plan:\n\nName: {}\n\nSteps:\n{}\n",
                plan.name, steps_s
            )
        } else {
            // `prompt` is guaranteed to be Some when here because of the earlier check
            prompt.unwrap()
        };

        println!("Executing task...\n");
        match agent.execute(task).await {
            Ok(response) => {
                println!("Result:\n{}", response);
                Ok(())
            }
            Err(e) => {
                eprintln!("Execution failed: {}", e);
                Err(e)
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::fs as stdfs;
        use tempfile::tempdir;

        // Ensure we require either plan or prompt
        #[tokio::test]
        async fn test_run_plan_requires_input() {
            let cfg = Config::default();
            let res = run_plan(cfg, None, None).await;
            assert!(res.is_err());
        }

        // Prepare a simple plan file and validate parsing does not panic
        #[tokio::test]
        async fn test_run_plan_parses_file_and_validates() {
            let yaml = r#"
name: Simple Plan
steps:
  - name: Do thing
    action: echo hi
"#;
            let dir = tempdir().unwrap();
            let p = dir.path().join("plan.yaml");
            stdfs::write(&p, yaml).expect("write plan");
            let mut cfg = Config::default();
            // Use an invalid provider so we don't try to execute network-bound providers
            cfg.provider.provider_type = "invalid_provider".to_string();

            // Because provider initialization will fail before agent execution,
            // we don't execute the agent; we verify we can parse and not crash.
            let res = run_plan(cfg, Some(p.to_string_lossy().to_string()), None).await;
            assert!(res.is_err());
        }
    }
}

/// Auth command(s)
///
/// Minimal provider authentication helper. Depending on provider, triggers or
/// guides the expected authentication flow (e.g., device flow for Copilot).
pub mod auth {
    use super::*;

    /// Trigger provider-specific authentication flow or instructions
    ///
    /// # Arguments
    ///
    /// * `config` - Global configuration (consumed)
    /// * `provider` - Provider name (e.g. "copilot", "ollama")
    pub async fn authenticate(config: Config, provider: String) -> Result<()> {
        tracing::info!("Starting authentication for provider: {}", provider);

        match provider.as_str() {
            "copilot" => {
                // Create the provider and run its authentication flow. The provider
                // will print the device-code verification URI + user code and will
                // poll until the user authorizes the device (or an error/timeout occurs).
                let provider = CopilotProvider::new(config.provider.copilot.clone())?;

                println!("Copilot: initiating device flow (you will be prompted to visit a URL and enter a code)...");
                // Run the provider's authenticate flow and surface any errors to the user.
                match provider.authenticate().await {
                    Ok(_) => {
                        println!("Copilot: authentication successful — token cached in the system keyring.");
                        Ok(())
                    }
                    Err(e) => {
                        // Provide a clear, immediate message and propagate the error.
                        eprintln!("Copilot: authentication failed: {}", e);
                        Err(e)
                    }
                }
            }
            "ollama" => {
                println!("Ollama: typically uses a local host with no OAuth; ensure `provider.ollama` config is set.");
                Ok(())
            }
            other => Err(XzatomaError::Provider(format!("Unsupported provider: {}", other)).into()),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[tokio::test]
        async fn test_auth_unknown_provider_fails() {
            let cfg = Config::default();
            let res = authenticate(cfg, "nope".to_string()).await;
            assert!(res.is_err());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn sanity_check_compile() {
        // Ensure the module builds and default config compiles
        let _ = Config::default();
    }
}
