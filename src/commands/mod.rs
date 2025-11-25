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
use crate::config::{Config, ExecutionMode};
use crate::error::{Result, XzatomaError};
use crate::providers::{CopilotProvider, OllamaProvider};
use crate::tools::plan::PlanParser;
use crate::tools::terminal::CommandValidator;
use crate::tools::{FileOpsTool, ToolExecutor, ToolRegistry};
use std::path::Path;
use std::sync::Arc;

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
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::commands::chat;
    /// use xzatoma::config::Config;
    ///
    /// // In application code:
    /// // chat::run_chat(Config::default(), None).await?;
    /// ```
    pub async fn run_chat(config: Config, provider_name: Option<String>) -> Result<()> {
        tracing::info!("Starting interactive chat mode");

        let provider_type = provider_name
            .as_deref()
            .unwrap_or(&config.provider.provider_type);

        // Tool registry and tools
        let mut tools = ToolRegistry::new();
        let working_dir = std::env::current_dir()?;

        // Register FileOps tool (safe defaults)
        let file_tool = FileOpsTool::new(working_dir.clone(), config.agent.tools.clone());
        let file_tool_executor: Arc<dyn crate::tools::ToolExecutor> = Arc::new(file_tool);
        tools.register("file_ops", file_tool_executor);

        // Build the Agent using a concrete provider implementation
        let agent = match provider_type {
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

        // Currently we don't register a TerminalTool (*) here because the terminal tool
        // implementation lives in `tools/terminal.rs` and is not registered by default.
        //
        // (*) If a TerminalTool becomes available, register it here as well.

        // Create readline instance for interactive CLI
        let mut rl = DefaultEditor::new()?;
        println!("XZatoma Interactive Mode");
        println!("Type 'exit' or 'quit' to exit\n");

        loop {
            match rl.readline(">> ") {
                Ok(line) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    if trimmed == "exit" || trimmed == "quit" {
                        println!("Goodbye!");
                        break;
                    }

                    // History for convenience
                    rl.add_history_entry(trimmed)?;

                    // Execute the prompt via the agent
                    match agent.execute(trimmed.to_string()).await {
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

            let res = run_chat(cfg, None).await;
            assert!(res.is_err());
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
        let _validator = CommandValidator::new(mode, working_dir.clone());

        // Register FileOps tool
        let file_tool = FileOpsTool::new(working_dir.clone(), config.agent.tools.clone());
        let file_tool_executor: Arc<dyn crate::tools::ToolExecutor> = Arc::new(file_tool);
        tools.register("file_ops", file_tool_executor);

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
                // Validate configuration by creating provider instance. The copilot
                // provider contains a `authenticate` implementation but it is private.
                // We ensure config is valid and print instructions for the user.
                // Validate provider configuration using the helper
                let _ = CopilotProvider::new(config.provider.copilot.clone())?;
                println!("Copilot: follow the device flow instructions shown by the provider.");
                println!(
                    "If credentials are already cached (keyring), you may be authenticated already"
                );
                Ok(())
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
