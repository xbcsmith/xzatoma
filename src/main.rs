//! XZatoma - Autonomous AI agent CLI
//!
//! Main entry point for the XZatoma agent application.

use anyhow::Result;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod agent;
mod chat_mode;
mod cli;
mod commands;
mod config;
mod error;
mod mention_parser;
mod prompts;
mod providers;
mod tools;

use crate::cli::{Cli, Commands, ModelCommand};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    init_tracing();

    // Parse command line arguments
    let cli = Cli::parse_args();

    // Load configuration
    let config_path = cli.config.as_deref().unwrap_or("config/config.yaml");
    let config = config::Config::load(config_path, &cli)?;

    // Validate configuration
    config.validate()?;

    // Execute command
    match cli.command {
        Commands::Chat {
            provider,
            mode,
            safe,
        } => {
            tracing::info!("Starting interactive chat mode");
            if let Some(p) = &provider {
                tracing::debug!("Using provider override: {}", p);
            }
            if let Some(m) = &mode {
                tracing::debug!("Using mode override: {}", m);
            }
            if safe {
                tracing::debug!("Safety mode enabled");
            }

            // Delegate to the chat command handler
            // Moves `config` into the handler (match arms are exclusive)
            commands::chat::run_chat(config, provider, mode, safe).await?;
            Ok(())
        }
        Commands::Run {
            plan,
            prompt,
            allow_dangerous,
        } => {
            tracing::info!("Starting plan execution mode");
            if let Some(plan_path) = &plan {
                tracing::debug!("Loading plan from: {}", plan_path.display());
            }
            if let Some(prompt_text) = &prompt {
                tracing::debug!("Using prompt: {}", prompt_text);
            }
            if allow_dangerous {
                tracing::warn!("Dangerous commands are allowed!");
            }

            // Convert plan PathBuf to String before passing it to the command handler.
            let plan_str = plan.map(|p| p.to_string_lossy().to_string());
            commands::run::run_plan_with_options(config, plan_str, prompt, allow_dangerous).await?;
            Ok(())
        }
        Commands::Watch {
            topic,
            event_types,
            filter_config,
            log_file,
            json_logs: _,
            dry_run,
        } => {
            tracing::info!("Starting watcher mode");
            if let Some(t) = &topic {
                tracing::debug!("Overriding topic: {}", t);
            }
            if let Some(et) = &event_types {
                tracing::debug!("Filtering event types: {}", et);
            }
            if let Some(fc) = &filter_config {
                tracing::debug!("Using filter config: {}", fc.display());
            }
            if let Some(lf) = &log_file {
                tracing::debug!("Writing logs to: {}", lf.display());
            }
            if dry_run {
                tracing::warn!("Dry run mode enabled - plans will not be executed");
            }
            tracing::warn!("Watch command is not yet implemented");
            Ok(())
        }
        Commands::Auth { provider } => {
            // Use CLI `--provider` override when supplied; otherwise fall back to the
            // configured/default provider from `config`.
            let provider = provider.unwrap_or_else(|| config.provider.provider_type.clone());
            tracing::info!("Starting authentication for provider: {}", provider);
            commands::auth::authenticate(config, provider).await?;
            Ok(())
        }
        Commands::Models { command } => {
            tracing::info!("Starting model management command");
            match command {
                ModelCommand::List { provider } => {
                    commands::models::list_models(&config, provider.as_deref()).await?;
                    Ok(())
                }
                ModelCommand::Info { model, provider } => {
                    commands::models::show_model_info(&config, &model, provider.as_deref()).await?;
                    Ok(())
                }
                ModelCommand::Current { provider } => {
                    commands::models::show_current_model(&config, provider.as_deref()).await?;
                    Ok(())
                }
            }
        }
    }
}

/// Initialize tracing subscriber with environment filter
fn init_tracing() {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("xzatoma=info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}
