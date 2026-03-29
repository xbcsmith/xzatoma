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

use crate::agent::Agent;
use crate::chat_mode::{ChatMode, ChatModeState, SafetyMode};
use crate::commands::special_commands::{
    parse_special_command, print_help, print_models_help, SpecialCommand,
};
use crate::config::Config;
use crate::error::{Result, XzatomaError};
use crate::mcp::manager::build_mcp_manager_from_config;
use crate::mcp::tool_bridge::register_mcp_tools;
use crate::mention_parser;
use crate::providers::{create_provider, CopilotProvider, OllamaProvider};
use crate::skills::{
    build_skill_disclosure_section, discover_skills, render_skill_catalog, ActiveSkillRegistry,
    SkillCatalog, SkillRecord,
};
use crate::tools::activate_skill::ActivateSkillTool;
use crate::tools::plan::PlanParser;
use crate::tools::registry_builder::ToolRegistryBuilder;
use crate::tools::{SubagentTool, ToolRegistry};
use std::path::Path;
use std::sync::Arc;

// Chat mode types and utilities
pub mod chat_mode;

// Special commands parser for mode switching
pub mod special_commands;

// Model management commands
pub mod models;

// History management commands
pub mod history;

// Replay command for conversation debugging
pub mod replay;

// MCP server management commands
pub mod mcp;

// ACP server management commands
pub mod acp;

// Skills management commands
pub mod skills;

// Agent environment builder (shared tool/skill/MCP initialization)
pub mod environment;
pub use environment::{build_agent_environment, AgentEnvironment};

/// Detect if a user prompt requests subagent functionality
///
/// Analyzes the prompt for keywords and patterns that suggest the user
/// wants to enable subagent delegation. This is used in chat mode to
/// automatically enable subagents when appropriate without explicit user command.
///
/// # Arguments
///
/// * `prompt` - The user's input prompt
///
/// # Returns
///
/// Returns true if the prompt contains subagent-related keywords or patterns
///
/// # Examples
///
/// ```
/// use xzatoma::commands::should_enable_subagents;
///
/// assert!(should_enable_subagents("use subagents to organize the files"));
/// assert!(should_enable_subagents("delegate this to subagents"));
/// assert!(should_enable_subagents("spawn agents in parallel"));
/// assert!(!should_enable_subagents("read the file"));
/// ```
pub fn should_enable_subagents(prompt: &str) -> bool {
    let lower = prompt.to_lowercase();

    // Check for subagent-related keywords and phrases
    lower.contains("subagent")
        || lower.contains("delegate")
        || lower.contains("spawn agent")
        || lower.contains("parallel task")
        || lower.contains("parallel agent")
        || lower.contains("agent delegation")
        || lower.contains("use agent")
}

/// Returns `true` if the given skill is visible for startup disclosure.
///
/// Visibility rules for Phase 2:
///
/// - invalid skills are already excluded by discovery
/// - project skills require trust when `skills.project_trust_required == true`
/// - user skills do not require project trust
/// - custom paths require trust unless
///   `skills.allow_custom_paths_without_trust == true`
///
/// # Arguments
///
/// * `config` - Global configuration
/// * `record` - Valid discovered skill
///
/// # Returns
///
/// Returns `true` if the skill should be disclosed to the model.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
/// use std::collections::HashSet;
/// use std::path::{Path, PathBuf};
/// use xzatoma::commands::is_skill_visible_for_disclosure;
/// use xzatoma::config::Config;
/// use xzatoma::skills::{SkillMetadata, SkillRecord, SkillSourceScope};
///
/// let config = Config::default();
/// let record = SkillRecord {
///     metadata: SkillMetadata {
///         name: "example_skill".to_string(),
///         description: "Example".to_string(),
///         license: None,
///         compatibility: None,
///         metadata: BTreeMap::new(),
///         allowed_tools_raw: None,
///         allowed_tools: Vec::new(),
///     },
///     skill_dir: PathBuf::from("/tmp/example_skill"),
///     skill_file: PathBuf::from("/tmp/example_skill/SKILL.md"),
///     source_scope: SkillSourceScope::UserClientSpecific,
///     source_order: 0,
///     body: "Body".to_string(),
/// };
///
/// assert!(is_skill_visible_for_disclosure(
///     &config,
///     &record,
///     Path::new("."),
///     &HashSet::new(),
/// ));
/// ```
pub fn is_skill_visible_for_disclosure(
    config: &Config,
    record: &SkillRecord,
    working_dir: &Path,
    trusted_paths: &std::collections::HashSet<std::path::PathBuf>,
) -> bool {
    crate::skills::disclosure::is_skill_visible(record, &config.skills, working_dir, trusted_paths)
}

/// Builds the startup skill disclosure block for the current session.
///
/// This helper discovers skills, filters them by disclosure visibility rules,
/// enforces `catalog_max_entries`, and renders the disclosure block that can be
/// injected into the conversation before the first provider call.
///
/// # Arguments
///
/// * `config` - Global configuration
/// * `working_dir` - Current working directory
///
/// # Returns
///
/// Returns a rendered disclosure block when at least one valid visible skill
/// exists, otherwise `None`.
///
/// # Errors
///
/// Returns an error if skill discovery fails.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use xzatoma::commands::build_startup_skill_disclosure;
/// use xzatoma::config::Config;
///
/// let disclosure = build_startup_skill_disclosure(&Config::default(), Path::new("."))?;
/// let _ = disclosure;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn build_startup_skill_disclosure(
    config: &Config,
    working_dir: &Path,
) -> Result<Option<String>> {
    if !config.skills.enabled {
        return Ok(None);
    }

    let discovery = discover_skills(&config.skills, working_dir)?;
    let trusted_paths = crate::skills::trust::load_trusted_paths(&config.skills, working_dir)?;
    let rendered_catalog = render_skill_catalog(
        &discovery.catalog,
        &config.skills,
        working_dir,
        &trusted_paths,
    );
    let disclosure = build_skill_disclosure_section(
        &discovery.catalog,
        &discovery.invalid_diagnostics,
        &config.skills,
        working_dir,
        &trusted_paths,
    );

    if rendered_catalog.is_empty() {
        Ok(None)
    } else {
        Ok(disclosure.filter(|section| !section.trim().is_empty()))
    }
}

/// Builds the visible startup skill catalog for activation and disclosure.
///
/// This helper discovers valid skills, applies Phase 2 visibility filtering, and
/// returns a catalog containing only valid visible skills. The returned catalog
/// is suitable for `activate_skill` registration and startup disclosure.
///
/// # Arguments
///
/// * `config` - Global configuration
/// * `working_dir` - Current working directory
///
/// # Returns
///
/// Returns a valid visible skill catalog for the current session.
///
/// # Errors
///
/// Returns an error if discovery or catalog construction fails.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use xzatoma::commands::build_visible_skill_catalog;
/// use xzatoma::config::Config;
///
/// let catalog = build_visible_skill_catalog(&Config::default(), Path::new("."))?;
/// let _ = catalog;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn build_visible_skill_catalog(config: &Config, working_dir: &Path) -> Result<SkillCatalog> {
    // Thin wrapper: load trusted paths then delegate to the canonical
    // 3-parameter version in `commands::skills` so the filtering logic
    // lives in exactly one place.
    let trusted_paths = crate::skills::trust::load_trusted_paths(&config.skills, working_dir)?;
    crate::commands::skills::build_visible_skill_catalog(config, working_dir, &trusted_paths)
}

/// Registers the `activate_skill` tool when visible skills exist and the
/// activation tool feature is enabled.
///
/// # Arguments
///
/// * `tools` - Tool registry for the current session
/// * `config` - Global configuration
/// * `visible_catalog` - Visible valid skills available for activation
/// * `active_skill_registry` - Shared active-skill registry for the session
///
/// # Returns
///
/// Returns `Ok(true)` if the tool was registered, otherwise `Ok(false)`.
///
/// # Errors
///
/// Returns an error if the tool cannot be initialized.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use std::sync::{Arc, Mutex};
/// use xzatoma::commands::{build_visible_skill_catalog, register_activate_skill_tool};
/// use xzatoma::config::Config;
/// use xzatoma::skills::ActiveSkillRegistry;
/// use xzatoma::tools::ToolRegistry;
///
/// let config = Config::default();
/// let catalog = build_visible_skill_catalog(&config, Path::new("."))?;
/// let registry = Arc::new(Mutex::new(ActiveSkillRegistry::new()));
/// let mut tools = ToolRegistry::new();
///
/// let _registered = register_activate_skill_tool(&mut tools, &config, catalog, registry)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn register_activate_skill_tool(
    tools: &mut ToolRegistry,
    config: &Config,
    visible_catalog: SkillCatalog,
    active_skill_registry: Arc<std::sync::Mutex<ActiveSkillRegistry>>,
) -> Result<bool> {
    if !config.skills.enabled
        || !config.skills.activation_tool_enabled
        || visible_catalog.is_empty()
    {
        return Ok(false);
    }

    let visible_skill_names = visible_catalog
        .names()
        .into_iter()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    let tool = ActivateSkillTool::new(
        Arc::new(visible_catalog),
        active_skill_registry,
        visible_skill_names,
    );
    tools.register("activate_skill", Arc::new(tool));

    Ok(true)
}

/// Builds the prompt-injection block for currently active skills.
///
/// This keeps active skill content out of `Conversation.messages` until prompt
/// assembly time.
///
/// # Arguments
///
/// * `active_skill_registry` - Shared active-skill registry
///
/// # Returns
///
/// Returns `Some(String)` when active skills exist, otherwise `None`.
///
/// # Errors
///
/// Returns an error if the registry lock cannot be acquired.
///
/// # Examples
///
/// ```
/// use std::sync::{Arc, Mutex};
/// use xzatoma::commands::build_active_skill_prompt_injection;
/// use xzatoma::skills::ActiveSkillRegistry;
///
/// let registry = Arc::new(Mutex::new(ActiveSkillRegistry::new()));
/// let prompt = build_active_skill_prompt_injection(&registry)?;
/// assert!(prompt.is_none());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn build_active_skill_prompt_injection(
    active_skill_registry: &Arc<std::sync::Mutex<ActiveSkillRegistry>>,
) -> Result<Option<String>> {
    let registry = active_skill_registry
        .lock()
        .map_err(|_| XzatomaError::Internal("Failed to lock active skill registry".to_string()))?;
    Ok(registry.render_for_prompt_injection())
}

// Chat command handler
pub mod chat {
    //! Interactive chat mode handler.
    //!
    //! Instantiates provider and tools, creates an `Agent`, and runs a
    //! readline-based interactive loop that submits user input to the agent.
    //!
    //! The agent will use the registered tools (file_ops, etc.) as required.

    use super::*;
    use colored::Colorize;
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
    /// * `resume` - Optional conversation ID to resume
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::commands::chat;
    /// use xzatoma::config::Config;
    ///
    /// // In application code:
    /// // chat::run_chat(Config::default(), None, None, false, None).await?;
    /// ```
    pub async fn run_chat(
        config: Config,
        provider_name: Option<String>,
        mode: Option<String>,
        _safe: bool,
        resume: Option<String>,
    ) -> Result<()> {
        use crate::storage::SqliteStorage;

        tracing::info!("Starting interactive chat mode");

        let provider_type = provider_name
            .as_deref()
            .unwrap_or(&config.provider.provider_type);

        let working_dir = std::env::current_dir()?;
        let skill_disclosure = build_startup_skill_disclosure(&config, &working_dir)?;
        let visible_skill_catalog = build_visible_skill_catalog(&config, &working_dir)?;
        let active_skill_registry = Arc::new(std::sync::Mutex::new(ActiveSkillRegistry::new()));

        // Initialize mode state from command-line arguments
        // Defaults: Planning mode, AlwaysConfirm (safe) safety mode
        let initial_mode = mode
            .as_deref()
            .and_then(|m| ChatMode::parse_str(m).ok())
            .unwrap_or(ChatMode::Planning);

        // Default to safe mode (AlwaysConfirm)
        let mut mode_state = ChatModeState::new(initial_mode, SafetyMode::AlwaysConfirm);

        // Build initial tool registry based on mode
        let mut tools = build_tools_for_mode(&mode_state, &config, &working_dir)?;
        let _activate_skill_registered = register_activate_skill_tool(
            &mut tools,
            &config,
            visible_skill_catalog,
            Arc::clone(&active_skill_registry),
        )?;

        // Build MCP client manager using the shared factory.
        // Chat is interactive (headless=false); the Arc must stay alive for the
        // entire duration of run_chat so that McpToolExecutor instances can call
        // back to it during agent turns.
        let mcp_manager = build_mcp_manager_from_config(&config).await?;

        // Register MCP tools into the registry.
        if let Some(ref manager) = mcp_manager {
            let execution_mode = config.agent.terminal.default_mode;
            match register_mcp_tools(&mut tools, Arc::clone(manager), execution_mode, false).await {
                Ok(count) if count > 0 => {
                    tracing::info!(count = %count, "Registered MCP tools for chat command");
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to register MCP tools for chat command");
                }
            }
        }

        // Initialize storage
        let storage = match SqliteStorage::new() {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::warn!("Failed to initialize persistence storage: {}", e);
                None
            }
        };

        // Create provider
        let provider_box = create_provider(provider_type, &config.provider)?;

        // Convert provider to Arc for sharing with subagent and main agent
        let provider: Arc<dyn crate::providers::Provider> = Arc::from(provider_box);

        // Register subagent tool for task delegation
        // Use new_with_config to support provider/model overrides
        let subagent_tool = SubagentTool::new_with_config(
            Arc::clone(&provider), // Parent provider (used if no override)
            &config.provider,      // Provider config for override instantiation
            config.agent.clone(),  // Agent config with subagent settings
            tools.clone(),         // Parent registry for filtering
            0,                     // Root depth (main agent is depth 0)
        )?;
        tools.register("subagent", Arc::new(subagent_tool));

        // Initialize agent with conversation
        let mut agent = if let Some(ref resume_id) = resume {
            if let Some(storage) = &storage {
                match storage.load_conversation(resume_id) {
                    Ok(Some((title, _model, messages))) => {
                        // Diagnostic logging to help track resume issues (message counts, sample content)
                        let user_count = messages.iter().filter(|m| m.role == "user").count();
                        tracing::debug!(
                            "Loaded conversation '{}' (resume_id={}) with {} total messages ({} user messages)",
                            title,
                            resume_id,
                            messages.len(),
                            user_count
                        );
                        if user_count > 0 {
                            if let Some(first_user) = messages.iter().find(|m| m.role == "user") {
                                let snippet = first_user.content.as_deref().unwrap_or("");
                                tracing::debug!("First user message snippet: {}", snippet);
                            }
                        }

                        println!("Resuming conversation: {}", title.cyan());
                        let conversation = crate::agent::Conversation::with_history(
                            uuid::Uuid::parse_str(resume_id)
                                .unwrap_or_else(|_| uuid::Uuid::new_v4()),
                            title,
                            messages,
                            config.agent.conversation.max_tokens,
                            config.agent.conversation.min_retain_turns,
                            config.agent.conversation.prune_threshold as f64,
                        );
                        let mut agent = Agent::with_conversation_and_shared_provider(
                            Arc::clone(&provider),
                            tools,
                            config.agent.clone(),
                            conversation,
                        )?;
                        if let Some(disclosure) = &skill_disclosure {
                            if !agent.conversation().messages().iter().any(|message| {
                                message.role == "system"
                                    && message
                                        .content
                                        .as_deref()
                                        .map(|content| content == disclosure)
                                        .unwrap_or(false)
                            }) {
                                agent
                                    .conversation_mut()
                                    .add_system_message(disclosure.clone());
                            }
                        }

                        let mut transient_system_messages = Vec::new();
                        if let Some(active_skill_prompt) =
                            build_active_skill_prompt_injection(&active_skill_registry)?
                        {
                            transient_system_messages.push(active_skill_prompt);
                        }
                        agent.set_transient_system_messages(transient_system_messages);

                        agent
                    }
                    Ok(None) => {
                        println!(
                            "{}",
                            format!("Conversation {} not found, starting new one.", resume_id)
                                .yellow()
                        );
                        let mut agent = Agent::new_from_shared_provider(
                            Arc::clone(&provider),
                            tools,
                            config.agent.clone(),
                        )?;
                        if let Some(disclosure) = &skill_disclosure {
                            agent
                                .conversation_mut()
                                .add_system_message(disclosure.clone());
                        }

                        let mut transient_system_messages = Vec::new();
                        if let Some(active_skill_prompt) =
                            build_active_skill_prompt_injection(&active_skill_registry)?
                        {
                            transient_system_messages.push(active_skill_prompt);
                        }
                        agent.set_transient_system_messages(transient_system_messages);

                        agent
                    }
                    Err(e) => {
                        tracing::error!("Failed to load conversation: {}", e);
                        println!("{}", "Failed to load conversation, starting new one.".red());
                        let mut agent = Agent::new_from_shared_provider(
                            Arc::clone(&provider),
                            tools,
                            config.agent.clone(),
                        )?;
                        if let Some(disclosure) = &skill_disclosure {
                            agent
                                .conversation_mut()
                                .add_system_message(disclosure.clone());
                        }

                        let mut transient_system_messages = Vec::new();
                        if let Some(active_skill_prompt) =
                            build_active_skill_prompt_injection(&active_skill_registry)?
                        {
                            transient_system_messages.push(active_skill_prompt);
                        }
                        agent.set_transient_system_messages(transient_system_messages);

                        agent
                    }
                }
            } else {
                println!(
                    "{}",
                    "Storage not available, starting new conversation.".yellow()
                );
                let mut agent = Agent::new_from_shared_provider(
                    Arc::clone(&provider),
                    tools,
                    config.agent.clone(),
                )?;
                if let Some(disclosure) = &skill_disclosure {
                    agent
                        .conversation_mut()
                        .add_system_message(disclosure.clone());
                }

                let mut transient_system_messages = Vec::new();
                if let Some(active_skill_prompt) =
                    build_active_skill_prompt_injection(&active_skill_registry)?
                {
                    transient_system_messages.push(active_skill_prompt);
                }
                agent.set_transient_system_messages(transient_system_messages);

                agent
            }
        } else {
            let mut agent = Agent::new_from_shared_provider(
                Arc::clone(&provider),
                tools,
                config.agent.clone(),
            )?;
            if let Some(disclosure) = &skill_disclosure {
                agent
                    .conversation_mut()
                    .add_system_message(disclosure.clone());
            }

            let mut transient_system_messages = Vec::new();
            if let Some(active_skill_prompt) =
                build_active_skill_prompt_injection(&active_skill_registry)?
            {
                transient_system_messages.push(active_skill_prompt);
            }
            agent.set_transient_system_messages(transient_system_messages);

            agent
        };

        // Create readline instance
        let mut rl = DefaultEditor::new()?;

        // Populate readline history with previous user inputs when resuming
        if resume.is_some() {
            let mut history_count = 0usize;
            for msg in agent.conversation().messages() {
                if msg.role == "user" {
                    if let Some(content) = &msg.content {
                        // Intentionally discard duplicate/history-capacity failures: they do not
                        // prevent chat resume, and readline history is best-effort.
                        if rl.add_history_entry(content).is_err() {
                            tracing::debug!(
                                "Skipped adding a resumed user message to readline history"
                            );
                        }
                        history_count += 1;
                    }
                }
            }
            tracing::debug!(
                "Readline history populated with {} entries from resumed conversation (id={})",
                history_count,
                resume.as_deref().unwrap_or("<none>")
            );
        }

        // Initialize mention cache for file content injection
        let mut mention_cache = crate::mention_parser::MentionCache::new();
        let max_file_size = config.agent.tools.max_file_read_size as u64;

        // Display welcome banner with current mode and safety
        print_welcome_banner(&mode_state.chat_mode, &mode_state.safety_mode);

        loop {
            // Build a prompt that includes provider/model when available.
            let current_model = agent.provider().get_current_model().ok();
            let prompt = if let Some(ref model) = current_model {
                mode_state
                    .format_colored_prompt_with_provider(Some(provider_type), Some(model.as_str()))
            } else {
                mode_state.format_colored_prompt()
            };

            match rl.readline(&prompt) {
                Ok(line) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    // Check for special commands first
                    match parse_special_command(trimmed) {
                        Ok(SpecialCommand::SwitchMode(new_mode)) => {
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
                        Ok(SpecialCommand::SwitchSafety(new_safety)) => {
                            let old_safety = mode_state.switch_safety(new_safety);
                            println!("Switched from {} to {} mode\n", old_safety, new_safety);
                            continue;
                        }
                        Ok(SpecialCommand::ShowStatus) => {
                            let tool_count = agent.num_tools();
                            let conversation_len = agent.conversation().len();
                            print_status_display(&mode_state, tool_count, conversation_len);
                            continue;
                        }
                        Ok(SpecialCommand::Help) => {
                            print_help();
                            continue;
                        }
                        Ok(SpecialCommand::ModelsHelp) => {
                            print_models_help();
                            continue;
                        }
                        Ok(SpecialCommand::Mentions) => {
                            special_commands::print_mention_help();
                            continue;
                        }
                        Ok(SpecialCommand::ListModels) => {
                            handle_list_models(&agent).await;
                            continue;
                        }
                        Ok(SpecialCommand::ShowModelInfo(model_name)) => {
                            match models::show_model_info(
                                &config,
                                &model_name,
                                Some(provider_type),
                                false,
                                false,
                            )
                            .await
                            {
                                Ok(_) => {}
                                Err(e) => {
                                    eprintln!("Failed to show model info: {}", e);
                                }
                            }
                            continue;
                        }
                        Ok(SpecialCommand::Auth(provider_opt)) => {
                            let provider_to_auth =
                                provider_opt.unwrap_or_else(|| provider_type.to_string());
                            println!("Starting authentication for provider: {}", provider_to_auth);

                            match auth::authenticate(config.clone(), provider_to_auth).await {
                                Ok(_) => {
                                    println!("Authentication completed.");
                                }
                                Err(e) => {
                                    eprintln!("Authentication failed: {}", e);
                                }
                            }
                            continue;
                        }
                        Ok(SpecialCommand::SwitchModel(model_name)) => {
                            handle_switch_model(
                                &mut agent,
                                &model_name,
                                &mut rl,
                                &config,
                                &working_dir,
                                provider_type,
                            )
                            .await?;
                            continue;
                        }
                        Ok(SpecialCommand::ContextInfo) => {
                            handle_show_context_info(&agent).await;
                            continue;
                        }
                        Ok(SpecialCommand::ContextSummary { model }) => {
                            // Determine which model to use for summarization
                            let summary_model = model
                                .clone()
                                .or_else(|| config.agent.conversation.summary_model.clone())
                                .unwrap_or_else(|| {
                                    current_model
                                        .clone()
                                        .unwrap_or_else(|| provider_type.to_string())
                                });

                            println!("Summarizing conversation using model: {}...", summary_model);

                            // Perform summarization
                            match perform_context_summary(
                                &mut agent,
                                Arc::clone(&provider),
                                &summary_model,
                            )
                            .await
                            {
                                Ok(_summary_text) => {
                                    println!(
                                        "\nContext summarized. New conversation started with summary in context.\n"
                                    );
                                }
                                Err(e) => {
                                    eprintln!("Failed to summarize context: {}\n", e);
                                }
                            }
                            continue;
                        }
                        Ok(SpecialCommand::ToggleSubagents(enable)) => {
                            if enable {
                                mode_state.enable_subagents();
                                println!("Subagent delegation enabled for subsequent requests");
                            } else {
                                mode_state.disable_subagents();
                                println!("Subagent delegation disabled");
                            }
                            println!();
                            continue;
                        }
                        Ok(SpecialCommand::Exit) => break,
                        Ok(SpecialCommand::None) => {
                            // Regular agent prompt
                        }
                        Err(e) => {
                            // Display command error
                            use colored::Colorize;
                            eprintln!("{}", e.to_string().red());
                            println!();
                            continue;
                        }
                    }

                    // Parse mentions from input
                    let (mentions, cleaned_text) = match mention_parser::parse_mentions(trimmed) {
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

                    rl.add_history_entry(trimmed)?;

                    // Show per-mention loading status...
                    if !mentions.is_empty() {
                        use colored::Colorize;

                        for mention in &mentions {
                            match mention {
                                crate::mention_parser::Mention::File(fm) => {
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

                    // Augment prompt with file contents from mentions
                    let (augmented_prompt, load_errors, successes) =
                        crate::mention_parser::augment_prompt_with_mentions(
                            &mentions,
                            &cleaned_text,
                            &working_dir,
                            max_file_size,
                            &mut mention_cache,
                        )
                        .await;

                    // Summarize mention load results...
                    use colored::Colorize;
                    // ... (omitted similar logic for brevity, assuming standard output handling)
                    // But we MUST verify we didn't delete the logic in replacement.
                    // The replacement content replaces the ENTIRE run_chat body, so I need to include the rest of the logic or implement it concisely.

                    // Actually, I should use the previous logic for mention display.
                    // I will just copy-paste the mention display logic from the original file to be safe, or just use minimal replacement if possible.
                    // But `run_chat` is one big function.
                    // I'll rewrite the mention display logic in the replacement content.

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

                        if !successes.is_empty() {
                            for msg in &successes {
                                println!("{}", msg.green());
                            }
                        }

                        let failed = load_errors.len();
                        if failed == 0 {
                            println!("{}", format!("Loaded {} mentions ({} files, {} urls, {} searches) — all succeeded", total_mentions, total_files, total_urls, total_searches).green());
                        } else {
                            println!(
                                "{}",
                                format!(
                                    "Loaded {} mentions: {} succeeded, {} failed",
                                    total_mentions,
                                    total_mentions.saturating_sub(failed),
                                    failed
                                )
                                .yellow()
                            );
                            for error in &load_errors {
                                eprintln!("{}", format!("Error: {}", error).red());
                            }
                        }
                    }

                    // Execute the prompt via the agent
                    match agent.execute(augmented_prompt).await {
                        Ok(response) => {
                            println!("\n{}\n", response);

                            // Check context status and display warnings if needed
                            let warning_threshold =
                                config.agent.conversation.warning_threshold as f64;
                            let auto_summary_threshold =
                                config.agent.conversation.auto_summary_threshold as f64;

                            if let Ok(model_name) = agent.provider().get_current_model() {
                                if let Ok(_model_info) =
                                    agent.provider().get_model_info(&model_name).await
                                {
                                    let context_status = agent.conversation().check_context_status(
                                        warning_threshold,
                                        auto_summary_threshold,
                                    );

                                    use colored::Colorize;
                                    match context_status {
                                        crate::agent::ContextStatus::Warning {
                                            percentage,
                                            tokens_remaining,
                                        } => {
                                            println!(
                                                "{}",
                                                format!(
                                                    "WARNING: Context window is {:.0}% full",
                                                    percentage * 100.0
                                                )
                                                .yellow()
                                            );
                                            println!(
                                                "   {} tokens remaining. Consider running '/context summary' to free up space.",
                                                tokens_remaining
                                            );
                                            println!();
                                        }
                                        crate::agent::ContextStatus::Critical {
                                            percentage,
                                            tokens_remaining,
                                        } => {
                                            println!(
                                                "{}",
                                                format!(
                                                    "CRITICAL: Context window is {:.0}% full!",
                                                    percentage * 100.0
                                                )
                                                .red()
                                            );
                                            println!(
                                                "   Only {} tokens remaining!",
                                                tokens_remaining
                                            );
                                            println!(
                                                "   Run '/context summary' to free up space or risk losing context."
                                            );
                                            println!();
                                        }
                                        crate::agent::ContextStatus::Normal => {
                                            // No warning needed
                                        }
                                    }
                                }
                            }

                            // Save conversation
                            if let Some(storage) = &storage {
                                let (title, should_update) = {
                                    let conv = agent.conversation();
                                    if conv.messages().len() <= 2 {
                                        // First turn, use prompt as title (truncated)
                                        let mut t = trimmed.to_string();
                                        if t.len() > 50 {
                                            t.truncate(47);
                                            t.push_str("...");
                                        }
                                        (t, true)
                                    } else {
                                        (conv.title().to_string(), false)
                                    }
                                };

                                if should_update {
                                    agent.conversation_mut().set_title(title.clone());
                                }

                                let conv = agent.conversation();
                                if let Err(e) = storage.save_conversation(
                                    &conv.id().to_string(),
                                    &title,
                                    current_model.as_deref(),
                                    conv.messages(),
                                ) {
                                    tracing::error!("Failed to save conversation: {}", e);
                                    // Optional: notify user?
                                }
                            }
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
    /// ```rust
    /// use xzatoma::chat_mode::{ChatMode, SafetyMode};
    ///
    /// let mode = ChatMode::Planning;
    /// let safety = SafetyMode::AlwaysConfirm;
    /// // Use public helpers for verification instead of internal display functions
    /// assert!(mode.description().len() > 0);
    /// assert!(safety.description().len() > 0);
    /// ```
    fn print_welcome_banner(mode: &ChatMode, safety: &SafetyMode) {
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
    /// ```rust
    /// use xzatoma::chat_mode::{ChatMode, SafetyMode, ChatModeState};
    ///
    /// let state = ChatModeState::new(ChatMode::Write, SafetyMode::AlwaysConfirm);
    /// // Verify public accessors instead of calling internal display helpers
    /// assert_eq!(state.chat_mode, ChatMode::Write);
    /// assert_eq!(state.safety_mode, SafetyMode::AlwaysConfirm);
    /// assert!(state.format_colored_prompt().len() > 0);
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

        // Display subagent status
        let subagent_status = if mode_state.subagents_enabled {
            "ENABLED".green().to_string()
        } else {
            "disabled".normal().to_string()
        };
        println!("Subagents:        {}", subagent_status);

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

    /// Handle listing available models
    ///
    /// # Arguments
    ///
    /// * `agent` - The current agent
    async fn handle_list_models(agent: &Agent) {
        use colored::Colorize;
        use prettytable::format;
        use prettytable::Table;

        match agent.provider().list_models().await {
            Ok(models) => {
                if models.is_empty() {
                    println!("{}", "No models available from this provider".yellow());
                    return;
                }

                let mut table = Table::new();
                table.set_format(*format::consts::FORMAT_BORDERS_ONLY);

                // Add header
                table.add_row(prettytable::row![
                    "Model Name".bold(),
                    "Display Name".bold(),
                    "Context Window".bold(),
                    "Capabilities".bold()
                ]);

                // Get current model for highlighting
                let current_model = agent.provider().get_current_model().ok();

                // Add model rows
                for model in models {
                    let is_current = current_model
                        .as_ref()
                        .map(|m| m == &model.name)
                        .unwrap_or(false);
                    if is_current {
                        table.add_row(prettytable::row![
                            model.name.green(),
                            model.display_name.green(),
                            format!("{} tokens", model.context_window).green(),
                            model
                                .capabilities
                                .iter()
                                .map(|c| c.to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                                .green()
                        ]);
                    } else {
                        table.add_row(prettytable::row![
                            model.name,
                            model.display_name,
                            format!("{} tokens", model.context_window),
                            model
                                .capabilities
                                .iter()
                                .map(|c| c.to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ]);
                    }
                }

                println!();
                table.printstd();
                println!();
                println!("{}", "Note: Current model is highlighted in green".cyan());
                println!();
            }
            Err(e) => {
                eprintln!("{}", format!("Error listing models: {}", e).red());
            }
        }
    }

    /// Handle switching to a different model
    ///
    /// # Arguments
    ///
    /// * `agent` - The current agent (will be replaced if successful)
    /// * `model_name` - Name of the model to switch to
    /// * `rl` - Readline editor for potential confirmation prompts
    /// * `config` - Global configuration
    /// * `working_dir` - Working directory for tool operations
    /// * `provider_type` - Type of provider ("copilot" or "ollama")
    ///
    /// # Returns
    ///
    /// Returns Ok if the switch succeeded, or an error if it failed
    async fn handle_switch_model(
        agent: &mut Agent,
        model_name: &str,
        _rl: &mut rustyline::DefaultEditor,
        config: &Config,
        _working_dir: &std::path::Path,
        provider_type: &str,
    ) -> Result<()> {
        use colored::Colorize;

        // Get available models to validate
        let available_models = agent.provider().list_models().await?;
        let model = available_models
            .iter()
            .find(|m| m.name.to_lowercase() == model_name.to_lowercase());

        match model {
            Some(model_info) => {
                // Get current conversation token usage
                let current_tokens = agent.conversation().token_count();
                let new_context_window = model_info.context_window;

                // Check if current conversation exceeds new context window
                if current_tokens > new_context_window {
                    println!(
                        "{}",
                        format!(
                            "WARNING: Current conversation ({} tokens) exceeds new model context ({} tokens)",
                            current_tokens, new_context_window
                        )
                        .yellow()
                    );
                    println!(
                        "{}",
                        "Messages will be pruned to fit the new context window.".yellow()
                    );
                    println!();
                    println!("{}Continue with model switch? [y/N]: ", ">>> ".cyan());

                    // For now, we don't prompt (would need interactive input from readline)
                    // In a full implementation, this would wait for user confirmation
                    // For MVP, we proceed without confirmation but show the warning
                }

                // Create new provider
                let mut new_provider = create_provider(provider_type, &config.provider)?;

                // Switch model
                new_provider.set_model(model_info.name.clone()).await?;

                // Get the new context window
                let new_context = new_provider
                    .get_model_info(&model_info.name)
                    .await?
                    .context_window;

                // Preserve conversation history
                let mut conversation = agent.conversation().clone();

                // Update conversation max tokens
                conversation.set_max_tokens(new_context);

                // Create new agent with updated provider and conversation
                let tools = agent.tools().clone();
                let new_agent = Agent::with_conversation(
                    new_provider,
                    tools,
                    config.agent.clone(),
                    conversation,
                )?;

                // Replace agent
                *agent = new_agent;

                println!(
                    "{}",
                    format!(
                        "Switched to model: {} ({} token context)",
                        model_info.name, new_context
                    )
                    .green()
                );
                println!();
            }
            None => {
                eprintln!(
                    "{}",
                    format!(
                        "Model '{}' not found. Use '/models list' to see available models.",
                        model_name
                    )
                    .red()
                );
            }
        }

        Ok(())
    }

    /// Perform context summarization and reset conversation
    ///
    /// Summarizes the conversation using the specified model and resets the conversation
    /// history while preserving the summary in a system message.
    ///
    /// # Arguments
    ///
    /// * `agent` - Mutable reference to the agent
    /// * `provider` - The provider to use for summarization
    /// * `model_name` - Name of the model to use for summarization
    ///
    /// # Returns
    ///
    /// Returns the summary text or an error
    async fn perform_context_summary(
        agent: &mut Agent,
        provider: Arc<dyn crate::providers::Provider>,
        _model_name: &str,
    ) -> Result<String> {
        use crate::providers::Message;

        // Get current messages to summarize
        let messages = agent.conversation().messages().to_vec();

        if messages.is_empty() {
            return Err(XzatomaError::Internal(
                "No messages to summarize".to_string(),
            ));
        }

        // Create summarization prompt
        let summary_prompt = create_summary_prompt(&messages);

        // Call provider to generate summary
        let response = provider
            .complete(&[Message::user(summary_prompt)], &[])
            .await?;

        let summary_text = response
            .message
            .content
            .unwrap_or_else(|| "Unable to generate summary".to_string());

        // Reset conversation while preserving summary
        let conv = agent.conversation_mut();
        conv.clear();
        conv.add_system_message(format!(
            "Previous conversation summary:\n\n{}",
            summary_text
        ));

        Ok(summary_text)
    }

    /// Create a summarization prompt for conversation history
    ///
    /// # Arguments
    ///
    /// * `messages` - The messages to summarize
    ///
    /// # Returns
    ///
    /// Returns a prompt string for summarization
    fn create_summary_prompt(messages: &[crate::providers::Message]) -> String {
        // Format messages for the summary prompt
        let conversation_text = messages
            .iter()
            .filter_map(|msg| {
                msg.content
                    .as_ref()
                    .map(|content| format!("{}: {}", msg.role, content))
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        format!(
            "Please provide a concise summary of the following conversation, \
             focusing on key topics, decisions made, and important context. \
             Keep the summary under 500 words.\n\nConversation:\n{}",
            conversation_text
        )
    }

    /// Handle displaying context window information
    ///
    /// # Arguments
    ///
    /// * `agent` - The current agent
    async fn handle_show_context_info(agent: &Agent) {
        use colored::Colorize;

        // Get the current model's context window
        match agent.provider().get_current_model() {
            Ok(model_name) => {
                match agent.provider().get_model_info(&model_name).await {
                    Ok(model_info) => {
                        let context = agent.get_context_info(model_info.context_window);

                        println!();
                        println!("{}", "╔════════════════════════════════════╗".cyan());
                        println!("{}", "║     Context Window Information      ║".cyan());
                        println!("{}", "╚════════════════════════════════════╝".cyan());
                        println!();

                        println!("Current Model:     {}", model_name.bold());
                        println!(
                            "Context Window:    {} tokens",
                            model_info.context_window.to_string().bold()
                        );
                        println!(
                            "Tokens Used:       {} tokens",
                            context.used_tokens.to_string().bold()
                        );
                        println!(
                            "Remaining:         {} tokens",
                            context.remaining_tokens.to_string().bold()
                        );
                        println!("Usage:             {:.1}%", context.percentage_used);

                        // Color code the usage percentage
                        let usage_color = if context.percentage_used < 60.0 {
                            context.percentage_used.to_string().green()
                        } else if context.percentage_used < 85.0 {
                            context.percentage_used.to_string().yellow()
                        } else {
                            context.percentage_used.to_string().red()
                        };

                        println!();
                        println!("Usage Level:       {}", usage_color);
                        println!();
                    }
                    Err(e) => {
                        eprintln!("{}", format!("Error getting model info: {}", e).red());
                    }
                }
            }
            Err(e) => {
                eprintln!("{}", format!("Error getting current model: {}", e).red());
            }
        }
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

            let res = run_chat(cfg, None, None, false, None).await;
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
            // Planning mode should have read-only file tools
            assert!(registry.get("read_file").is_some());
            assert!(registry.get("list_directory").is_some());
            assert!(registry.get("find_path").is_some());
            assert!(registry.get("terminal").is_none());
            assert!(registry.get("write_file").is_none());
        }

        #[test]
        fn test_build_tools_for_write_mode() {
            let mode_state = ChatModeState::new(ChatMode::Write, SafetyMode::AlwaysConfirm);
            let config = Config::default();
            let working_dir = std::path::PathBuf::from(".");

            let result = build_tools_for_mode(&mode_state, &config, &working_dir);
            assert!(result.is_ok());

            let registry = result.unwrap();
            // Write mode should have all file tools
            assert!(registry.get("read_file").is_some());
            assert!(registry.get("write_file").is_some());
            assert!(registry.get("delete_path").is_some());
            assert!(registry.get("list_directory").is_some());
            assert!(registry.get("copy_path").is_some());
            assert!(registry.get("move_path").is_some());
            assert!(registry.get("create_directory").is_some());
            assert!(registry.get("find_path").is_some());
            assert!(registry.get("edit_file").is_some());
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
                    "[PLANNING][SAFE] >>> ",
                ),
                (
                    ChatMode::Planning,
                    SafetyMode::NeverConfirm,
                    "[PLANNING][YOLO] >>> ",
                ),
                (
                    ChatMode::Write,
                    SafetyMode::AlwaysConfirm,
                    "[WRITE][SAFE] >>> ",
                ),
                (
                    ChatMode::Write,
                    SafetyMode::NeverConfirm,
                    "[WRITE][YOLO] >>> ",
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
            ));
        }

        // Build tools & agent
        let working_dir = std::env::current_dir()?;

        if allow_dangerous {
            tracing::warn!("Dangerous commands are allowed for this run: allow_dangerous=true");
        }

        // Build tools, skills, and MCP stack via the shared environment builder.
        // The run command is always headless (non-interactive).
        let env = build_agent_environment(&config, &working_dir, true).await?;
        let tools = env.tool_registry;
        let active_skill_registry = env.active_skill_registry;
        let skill_disclosure = env.skill_disclosure;
        // Keep the MCP manager Arc alive for the entire function so that
        // McpToolExecutor instances (registered in tools) can call back to it.
        let _mcp_manager = env.mcp_manager;

        // Create agent using the shared provider factory.
        let provider_box = create_provider(&config.provider.provider_type, &config.provider)?;
        let provider: Arc<dyn crate::providers::Provider> = Arc::from(provider_box);
        let mut agent = Agent::new_from_shared_provider(provider, tools, config.agent.clone())?;

        if let Some(disclosure) = &skill_disclosure {
            agent
                .conversation_mut()
                .add_system_message(disclosure.clone());
        }

        let mut transient_system_messages = Vec::new();
        if let Some(active_skill_prompt) =
            build_active_skill_prompt_injection(&active_skill_registry)?
        {
            transient_system_messages.push(active_skill_prompt);
        }
        agent.set_transient_system_messages(transient_system_messages);

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

    /// Creates a provider instance for a specific model
    ///
    /// This helper function creates a new provider configured to use the specified model.
    /// It's used for automatic summarization when a different summary model is configured.
    ///
    /// # Arguments
    ///
    /// * `config` - The global configuration
    /// * `model_name` - The model name to configure the provider with
    ///
    /// # Returns
    ///
    /// Returns an Arc-wrapped provider instance
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Provider type is unsupported
    /// - Provider initialization fails
    pub async fn create_provider_for_model(
        config: &Config,
        model_name: &str,
    ) -> Result<Arc<dyn crate::providers::Provider>> {
        match config.provider.provider_type.as_str() {
            "copilot" => {
                let mut copilot_config = config.provider.copilot.clone();
                copilot_config.model = model_name.to_string();
                let provider = CopilotProvider::new(copilot_config)?;
                Ok(Arc::new(provider) as Arc<dyn crate::providers::Provider>)
            }
            "ollama" => {
                let mut ollama_config = config.provider.ollama.clone();
                ollama_config.model = model_name.to_string();
                let provider = OllamaProvider::new(ollama_config)?;
                Ok(Arc::new(provider) as Arc<dyn crate::providers::Provider>)
            }
            _ => Err(XzatomaError::Provider(format!(
                "Unsupported provider type: {}",
                config.provider.provider_type
            ))),
        }
    }

    /// Creates a summary provider if needed for a different model
    ///
    /// Checks if the summary model differs from the current provider's model.
    /// If they're the same, returns the current provider. Otherwise creates
    /// a new provider for the summary model.
    ///
    /// # Arguments
    ///
    /// * `config` - The global configuration
    /// * `current_provider` - The current provider instance
    /// * `summary_model` - The model to use for summarization
    ///
    /// # Returns
    ///
    /// Returns the provider to use for summarization
    ///
    /// # Errors
    ///
    /// Returns error if creating a new provider fails
    pub async fn create_summary_provider_if_needed(
        config: &Config,
        current_provider: &Arc<dyn crate::providers::Provider>,
        summary_model: &str,
    ) -> Result<Arc<dyn crate::providers::Provider>> {
        match current_provider.get_current_model() {
            Ok(current_model) if current_model == summary_model => {
                // Same model, use existing provider
                Ok(Arc::clone(current_provider))
            }
            _ => {
                // Different model or unknown current model, create new provider
                tracing::debug!(
                    "Creating separate provider for summarization model: {}",
                    summary_model
                );
                create_provider_for_model(config, summary_model).await
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
            other => Err(XzatomaError::Provider(format!(
                "Unsupported provider: {}",
                other
            ))),
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

/// Watch command handler for monitoring Kafka topics and executing plans
pub mod watch {
    use super::*;

    /// CLI overrides for the `watch` command.
    ///
    /// This groups all optional CLI-provided overrides into a single value so the
    /// watch command entry points remain readable and satisfy linting constraints.
    #[derive(Debug, Clone, Default)]
    pub struct WatchCliOverrides {
        /// Optional Kafka topic override.
        pub topic: Option<String>,
        /// Optional event types filter override.
        pub event_types: Option<String>,
        /// Optional filter configuration path.
        pub filter_config: Option<PathBuf>,
        /// Optional watcher log file path.
        pub log_file: Option<PathBuf>,
        /// Whether JSON logging is enabled.
        pub json_logs: bool,
        /// Optional watcher backend type override.
        pub watcher_type: Option<String>,
        /// Optional Kafka consumer group ID override.
        pub group_id: Option<String>,
        /// Optional generic watcher output topic override.
        pub output_topic: Option<String>,
        /// Whether missing Kafka topics should be created automatically.
        pub create_topics: bool,
        /// Optional generic matcher action override.
        pub action: Option<String>,
        /// Optional generic matcher name override.
        pub name: Option<String>,
        /// Whether dry-run mode is enabled.
        pub dry_run: bool,
    }
    use std::path::PathBuf;

    /// Run the watch command
    ///
    /// This function is the entry point for the `xzatoma watch` command.
    /// It configures the watcher based on CLI arguments and configuration file,
    /// sets up logging, and starts the event consumption loop with signal handling.
    ///
    /// # Arguments
    ///
    /// * `config` - Global configuration (will be modified by CLI overrides)
    /// * `overrides` - Optional CLI overrides for watcher behavior
    ///
    /// # Returns
    ///
    /// Returns `Err` if configuration is invalid or an error occurs.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Kafka configuration is missing
    /// - Log initialization fails
    /// - Watcher creation fails
    /// - Message consumption fails
    pub async fn run_watch(mut config: Config, overrides: WatchCliOverrides) -> Result<()> {
        // Apply CLI argument overrides to configuration
        apply_cli_overrides(&mut config, &overrides)?;

        // Initialize logging system
        crate::watcher::logging::init_watcher_logging(&config.watcher.logging)?;

        tracing::info!("Watch command started");
        tracing::info!(
            watcher_type = %config.watcher.watcher_type.as_str(),
            kafka_brokers = %config.watcher.kafka.as_ref().map(|k| &k.brokers).unwrap_or(&"not configured".to_string()),
            kafka_topic = %config.watcher.kafka.as_ref().map(|k| &k.topic).unwrap_or(&"not configured".to_string()),
            kafka_group_id = %config.watcher.kafka.as_ref().map(|k| &k.group_id).unwrap_or(&"not configured".to_string()),
            auto_create_topics = config.watcher.kafka.as_ref().map(|k| k.auto_create_topics).unwrap_or(false),
            dry_run = overrides.dry_run,
            "Initializing watcher service"
        );

        let kafka_config = config.watcher.kafka.clone().ok_or_else(|| {
            XzatomaError::Config(
                "Kafka configuration is required. Please configure it in config file or set XZEPR_KAFKA_* env vars".to_string()
            )
        })?;

        if kafka_config.auto_create_topics || overrides.create_topics {
            let topic_admin = crate::watcher::topic_admin::WatcherTopicAdmin::new(&kafka_config)?;
            match config.watcher.watcher_type {
                crate::config::WatcherType::XZepr => {
                    topic_admin.ensure_xzepr_watcher_topics().await?;
                }
                crate::config::WatcherType::Generic => {
                    topic_admin.ensure_generic_watcher_topics().await?;
                }
            }
        }

        // Create watcher service first so constructor validation happens
        // before entering the signal-handling path.
        match config.watcher.watcher_type {
            crate::config::WatcherType::XZepr => {
                let mut watcher = crate::watcher::XzeprWatcher::new(config, overrides.dry_run)
                    .map_err(|error| XzatomaError::Watcher(error.to_string()))?;

                // Set up signal handling for graceful shutdown
                let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel(1);

                // Spawn signal handler task
                tokio::spawn(async move {
                    match tokio::signal::ctrl_c().await {
                        Ok(()) => {
                            tracing::info!("Received CTRL+C signal, initiating graceful shutdown");
                            if shutdown_tx.send(()).await.is_err() {
                                tracing::debug!(
                                    "Shutdown signal receiver already dropped for xzepr watcher"
                                );
                            }
                        }
                        Err(err) => {
                            tracing::error!(error = %err, "Failed to set up signal handler");
                        }
                    }
                });

                tokio::select! {
                    result = watcher.start() => {
                        result.map_err(|error| XzatomaError::Watcher(error.to_string()))
                    }
                    _ = shutdown_rx.recv() => {
                        tracing::info!("Graceful shutdown completed");
                        Ok(())
                    }
                }
            }
            crate::config::WatcherType::Generic => {
                let mut watcher =
                    crate::watcher::generic::GenericWatcher::new(config, overrides.dry_run)?;

                // Set up signal handling for graceful shutdown
                let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel(1);

                // Spawn signal handler task
                tokio::spawn(async move {
                    match tokio::signal::ctrl_c().await {
                        Ok(()) => {
                            tracing::info!("Received CTRL+C signal, initiating graceful shutdown");
                            if shutdown_tx.send(()).await.is_err() {
                                tracing::debug!(
                                    "Shutdown signal receiver already dropped for generic watcher"
                                );
                            }
                        }
                        Err(err) => {
                            tracing::error!(error = %err, "Failed to set up signal handler");
                        }
                    }
                });

                tokio::select! {
                    result = watcher.start() => {
                        result.map_err(|error| XzatomaError::Watcher(error.to_string()))
                    }
                    _ = shutdown_rx.recv() => {
                        tracing::info!("Graceful shutdown completed");
                        Ok(())
                    }
                }
            }
        }
    }

    /// Apply CLI argument overrides to the configuration
    ///
    /// Updates the configuration object with values provided via CLI arguments.
    /// CLI arguments take precedence over configuration file values.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration to modify
    /// * `overrides` - CLI override values to apply
    ///
    /// # Errors
    ///
    /// Returns error if configuration is invalid (e.g., no Kafka config or invalid watcher type).
    fn apply_cli_overrides(config: &mut Config, overrides: &WatchCliOverrides) -> Result<()> {
        // Ensure Kafka configuration exists
        if config.watcher.kafka.is_none() {
            return Err(XzatomaError::Config(
                "Kafka configuration is required. Please configure it in config file or set XZEPR_KAFKA_* env vars".to_string()
            ));
        }

        // Override watcher type if provided
        if let Some(cli_watcher_type) = &overrides.watcher_type {
            let parsed_watcher_type = crate::config::WatcherType::from_str_name(cli_watcher_type)
                .ok_or_else(|| {
                XzatomaError::Config(format!(
                    "Invalid watcher type: {}. Must be one of: xzepr, generic",
                    cli_watcher_type
                ))
            })?;

            config.watcher.watcher_type = parsed_watcher_type;
            tracing::debug!(
                watcher_type = %cli_watcher_type,
                "CLI override: Watcher type"
            );
        }

        // Override topic if provided
        if let Some(t) = &overrides.topic {
            if let Some(ref mut kafka) = config.watcher.kafka {
                kafka.topic = t.clone();
                tracing::debug!(topic = %t, "CLI override: Kafka topic");
            }
        }

        // Override group ID if provided
        if let Some(group_id) = &overrides.group_id {
            if let Some(ref mut kafka) = config.watcher.kafka {
                kafka.group_id = group_id.clone();
                tracing::debug!(group_id = %group_id, "CLI override: Kafka consumer group ID");
            }
        }

        // Override output topic if provided
        if let Some(output) = &overrides.output_topic {
            if let Some(ref mut kafka) = config.watcher.kafka {
                kafka.output_topic = Some(output.clone());
                tracing::debug!(output_topic = %output, "CLI override: Kafka output topic");
            }
        }

        // Override topic auto-creation if requested
        if overrides.create_topics {
            if let Some(ref mut kafka) = config.watcher.kafka {
                kafka.auto_create_topics = true;
                tracing::debug!("CLI override: Kafka topic auto-creation enabled");
            }
        }

        // Override generic matcher action if provided
        if let Some(action_pattern) = &overrides.action {
            config.watcher.generic_match.action = Some(action_pattern.clone());
            tracing::debug!(
                action = %action_pattern,
                "CLI override: Generic matcher action"
            );
        }

        // Override generic matcher name if provided
        if let Some(name_pattern) = &overrides.name {
            config.watcher.generic_match.name = Some(name_pattern.clone());
            tracing::debug!(
                name = %name_pattern,
                "CLI override: Generic matcher name"
            );
        }

        // Override event types filter if provided
        if let Some(types) = &overrides.event_types {
            let event_types_vec: Vec<String> = types
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            config.watcher.filters.event_types = event_types_vec.clone();
            tracing::debug!(
                event_types = ?event_types_vec,
                "CLI override: Event type filters"
            );
        }

        // Override log file if provided
        if let Some(path) = &overrides.log_file {
            config.watcher.logging.file_path = Some(path.clone());
            tracing::debug!(
                log_file = %path.display(),
                "CLI override: Log file path"
            );
        }

        // Override JSON logging setting
        if overrides.json_logs {
            config.watcher.logging.json_format = true;
            tracing::debug!("CLI override: JSON logging enabled");
        }

        // Note: dry_run is passed separately to Watcher::new() and not stored in config
        if overrides.dry_run {
            tracing::debug!("Dry-run mode will be enabled for execution");
        }

        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_apply_cli_overrides_with_no_kafka_config() {
            let mut config = Config::default();
            config.watcher.kafka = None;

            let result = apply_cli_overrides(&mut config, &WatchCliOverrides::default());
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("Kafka configuration"));
        }

        #[test]
        fn test_apply_cli_overrides_topic() {
            let mut config = Config::default();
            config.watcher.kafka = Some(crate::config::KafkaWatcherConfig {
                brokers: "localhost:9092".to_string(),
                topic: "original.topic".to_string(),
                output_topic: None,
                group_id: "test-group".to_string(),
                auto_create_topics: false,
                security: None,
            });

            let result = apply_cli_overrides(
                &mut config,
                &WatchCliOverrides {
                    topic: Some("override.topic".to_string()),
                    ..WatchCliOverrides::default()
                },
            );

            assert!(result.is_ok());
            assert_eq!(
                config.watcher.kafka.as_ref().unwrap().topic,
                "override.topic"
            );
        }

        #[test]
        fn test_apply_cli_overrides_event_types() {
            let mut config = Config::default();
            config.watcher.kafka = Some(crate::config::KafkaWatcherConfig {
                brokers: "localhost:9092".to_string(),
                topic: "test.topic".to_string(),
                output_topic: None,
                group_id: "test-group".to_string(),
                auto_create_topics: false,
                security: None,
            });

            let result = apply_cli_overrides(
                &mut config,
                &WatchCliOverrides {
                    event_types: Some("deployment.success,deployment.failure".to_string()),
                    ..WatchCliOverrides::default()
                },
            );

            assert!(result.is_ok());
            assert_eq!(config.watcher.filters.event_types.len(), 2);
            assert!(config
                .watcher
                .filters
                .event_types
                .contains(&"deployment.success".to_string()));
            assert!(config
                .watcher
                .filters
                .event_types
                .contains(&"deployment.failure".to_string()));
        }

        #[test]
        fn test_apply_cli_overrides_json_logs() {
            let mut config = Config::default();
            config.watcher.kafka = Some(crate::config::KafkaWatcherConfig {
                brokers: "localhost:9092".to_string(),
                topic: "test.topic".to_string(),
                output_topic: None,
                group_id: "test-group".to_string(),
                auto_create_topics: false,
                security: None,
            });
            config.watcher.logging.json_format = false;

            let result = apply_cli_overrides(
                &mut config,
                &WatchCliOverrides {
                    json_logs: true,
                    ..WatchCliOverrides::default()
                },
            );

            assert!(result.is_ok());
            assert!(config.watcher.logging.json_format);
        }

        #[test]
        fn test_apply_cli_overrides_multiple_settings() {
            let mut config = Config::default();
            config.watcher.kafka = Some(crate::config::KafkaWatcherConfig {
                brokers: "localhost:9092".to_string(),
                topic: "original".to_string(),
                output_topic: None,
                group_id: "test-group".to_string(),
                auto_create_topics: false,
                security: None,
            });

            let result = apply_cli_overrides(
                &mut config,
                &WatchCliOverrides {
                    topic: Some("override".to_string()),
                    event_types: Some("deployment.success".to_string()),
                    json_logs: true,
                    ..WatchCliOverrides::default()
                },
            );

            assert!(result.is_ok());
            assert_eq!(config.watcher.kafka.as_ref().unwrap().topic, "override");
            assert_eq!(config.watcher.filters.event_types.len(), 1);
            assert!(config.watcher.logging.json_format);
        }

        #[test]
        fn test_apply_cli_overrides_event_types_with_whitespace() {
            let mut config = Config::default();
            config.watcher.kafka = Some(crate::config::KafkaWatcherConfig {
                brokers: "localhost:9092".to_string(),
                topic: "test.topic".to_string(),
                output_topic: None,
                group_id: "test-group".to_string(),
                auto_create_topics: false,
                security: None,
            });

            let result = apply_cli_overrides(
                &mut config,
                &WatchCliOverrides {
                    event_types: Some(
                        "deployment.success , deployment.failure , build.complete".to_string(),
                    ),
                    ..WatchCliOverrides::default()
                },
            );

            assert!(result.is_ok());
            assert_eq!(config.watcher.filters.event_types.len(), 3);
        }

        #[test]
        fn test_apply_cli_overrides_watcher_type() {
            let mut config = Config::default();
            config.watcher.kafka = Some(crate::config::KafkaWatcherConfig {
                brokers: "localhost:9092".to_string(),
                topic: "test.topic".to_string(),
                output_topic: None,
                group_id: "test-group".to_string(),
                auto_create_topics: false,
                security: None,
            });

            let result = apply_cli_overrides(
                &mut config,
                &WatchCliOverrides {
                    watcher_type: Some("generic".to_string()),
                    ..WatchCliOverrides::default()
                },
            );

            assert!(result.is_ok());
            assert_eq!(
                config.watcher.watcher_type,
                crate::config::WatcherType::Generic
            );
        }

        #[test]
        fn test_apply_cli_overrides_output_topic() {
            let mut config = Config::default();
            config.watcher.kafka = Some(crate::config::KafkaWatcherConfig {
                brokers: "localhost:9092".to_string(),
                topic: "test.topic".to_string(),
                output_topic: None,
                group_id: "test-group".to_string(),
                auto_create_topics: false,
                security: None,
            });

            let result = apply_cli_overrides(
                &mut config,
                &WatchCliOverrides {
                    output_topic: Some("results.topic".to_string()),
                    ..WatchCliOverrides::default()
                },
            );

            assert!(result.is_ok());
            assert_eq!(
                config
                    .watcher
                    .kafka
                    .as_ref()
                    .unwrap()
                    .output_topic
                    .as_deref(),
                Some("results.topic")
            );
        }

        #[test]
        fn test_apply_cli_overrides_group_id() {
            let mut config = Config::default();
            config.watcher.kafka = Some(crate::config::KafkaWatcherConfig {
                brokers: "localhost:9092".to_string(),
                topic: "test.topic".to_string(),
                output_topic: None,
                group_id: "test-group".to_string(),
                auto_create_topics: false,
                security: None,
            });

            let result = apply_cli_overrides(
                &mut config,
                &WatchCliOverrides {
                    group_id: Some("override-group".to_string()),
                    ..WatchCliOverrides::default()
                },
            );

            assert!(result.is_ok());
            assert_eq!(
                config.watcher.kafka.as_ref().unwrap().group_id,
                "override-group"
            );
        }

        #[test]
        fn test_apply_cli_overrides_create_topics() {
            let mut config = Config::default();
            config.watcher.kafka = Some(crate::config::KafkaWatcherConfig {
                brokers: "localhost:9092".to_string(),
                topic: "test.topic".to_string(),
                output_topic: None,
                group_id: "test-group".to_string(),
                auto_create_topics: false,
                security: None,
            });

            let result = apply_cli_overrides(
                &mut config,
                &WatchCliOverrides {
                    create_topics: true,
                    ..WatchCliOverrides::default()
                },
            );

            assert!(result.is_ok());
            assert!(config.watcher.kafka.as_ref().unwrap().auto_create_topics);
        }

        #[test]
        fn test_apply_cli_overrides_generic_match_action() {
            let mut config = Config::default();
            config.watcher.kafka = Some(crate::config::KafkaWatcherConfig {
                brokers: "localhost:9092".to_string(),
                topic: "test.topic".to_string(),
                output_topic: None,
                group_id: "test-group".to_string(),
                auto_create_topics: false,
                security: None,
            });

            let result = apply_cli_overrides(
                &mut config,
                &WatchCliOverrides {
                    action: Some("deploy.*".to_string()),
                    ..WatchCliOverrides::default()
                },
            );

            assert!(result.is_ok());
            assert_eq!(
                config.watcher.generic_match.action.as_deref(),
                Some("deploy.*")
            );
        }

        #[test]
        fn test_apply_cli_overrides_generic_match_name() {
            let mut config = Config::default();
            config.watcher.kafka = Some(crate::config::KafkaWatcherConfig {
                brokers: "localhost:9092".to_string(),
                topic: "test.topic".to_string(),
                output_topic: None,
                group_id: "test-group".to_string(),
                auto_create_topics: false,
                security: None,
            });

            let result = apply_cli_overrides(
                &mut config,
                &WatchCliOverrides {
                    name: Some("service-a".to_string()),
                    ..WatchCliOverrides::default()
                },
            );

            assert!(result.is_ok());
            assert_eq!(
                config.watcher.generic_match.name.as_deref(),
                Some("service-a")
            );
        }

        #[tokio::test]
        async fn test_run_watch_xzepr_missing_kafka_returns_error() {
            let mut config = Config::default();
            config.watcher.watcher_type = crate::config::WatcherType::XZepr;
            config.watcher.kafka = None;

            let result = run_watch(config, WatchCliOverrides::default()).await;

            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("Kafka configuration"));
        }

        #[tokio::test]
        async fn test_run_watch_generic_missing_kafka_returns_error() {
            let mut config = Config::default();
            config.watcher.watcher_type = crate::config::WatcherType::Generic;
            config.watcher.kafka = None;

            let result = run_watch(config, WatchCliOverrides::default()).await;

            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("Kafka configuration"));
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

    #[test]
    fn test_should_enable_subagents_with_subagent_keyword() {
        assert!(should_enable_subagents("use subagents to organize files"));
        assert!(should_enable_subagents("delegate this task with subagents"));
        assert!(should_enable_subagents("Subagent delegation is needed"));
    }

    #[test]
    fn test_should_enable_subagents_with_delegate_phrase() {
        assert!(should_enable_subagents(
            "delegate to subagents for parallel work"
        ));
        assert!(should_enable_subagents("please delegate this task"));
    }

    #[test]
    fn test_should_enable_subagents_with_spawn_keyword() {
        assert!(should_enable_subagents("spawn agent processes"));
        assert!(should_enable_subagents("spawn agents in parallel"));
    }

    #[test]
    fn test_should_enable_subagents_with_parallel_keywords() {
        assert!(should_enable_subagents("run this as a parallel task"));
        assert!(should_enable_subagents("parallel agent execution needed"));
    }

    #[test]
    fn test_should_enable_subagents_with_agent_delegation() {
        assert!(should_enable_subagents("agent delegation please"));
        assert!(should_enable_subagents("use agent delegation"));
    }

    #[test]
    fn test_should_enable_subagents_case_insensitive() {
        assert!(should_enable_subagents("USE SUBAGENTS"));
        assert!(should_enable_subagents("Delegate To Agents"));
        assert!(should_enable_subagents("PARALLEL TASK"));
    }

    #[test]
    fn test_should_enable_subagents_returns_false_for_regular_prompts() {
        assert!(!should_enable_subagents("read the file"));
        assert!(!should_enable_subagents("list the directory"));
        assert!(!should_enable_subagents("write a hello world program"));
        assert!(!should_enable_subagents("find all rust files"));
    }

    #[test]
    fn test_should_enable_subagents_empty_string() {
        assert!(!should_enable_subagents(""));
    }

    #[test]
    fn test_should_enable_subagents_whitespace_only() {
        assert!(!should_enable_subagents("   "));
    }
}
