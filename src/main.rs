//! XZatoma - Autonomous AI agent CLI
//!
#![doc = "XZatoma - Autonomous AI agent CLI"]
#![doc = "Main entry point for the XZatoma agent application."]

use anyhow::Result;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

// Removed unused grouped imports to satisfy clippy

use xzatoma::cli::{Cli, Commands, ModelCommand};
use xzatoma::commands;
use xzatoma::config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    init_tracing();

    // Parse command line arguments
    let cli = Cli::parse_args();

    // If the user supplied a storage path on the CLI (or via env),
    // mirror it into XZATOMA_HISTORY_DB so the storage initializer can pick it up.
    // This keeps callers unchanged while allowing `SqliteStorage::new()` to
    // honor an override.
    if let Some(db_path) = &cli.storage_path {
        std::env::set_var("XZATOMA_HISTORY_DB", db_path);
        tracing::info!("Using storage DB override from CLI: {}", db_path);
    }

    // Load configuration
    let config_path = cli.config.as_deref().unwrap_or("config/config.yaml");
    let config = Config::load(config_path, &cli)?;

    // Validate configuration
    config.validate()?;

    // Execute command
    match cli.command {
        Commands::Chat {
            provider,
            mode,
            safe,
            resume,
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
            if let Some(r) = &resume {
                tracing::debug!("Resuming conversation: {}", r);
            }

            // Delegate to the chat command handler
            // Moves `config` into the handler (match arms are exclusive)
            commands::chat::run_chat(config, provider, mode, safe, resume).await?;
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
            json_logs,
            dry_run,
        } => {
            tracing::info!("Starting watcher mode");
            commands::watch::run_watch(
                config,
                topic,
                event_types,
                filter_config,
                log_file,
                json_logs,
                dry_run,
            )
            .await?;
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
                ModelCommand::List {
                    provider,
                    json,
                    summary,
                } => {
                    commands::models::list_models(&config, provider.as_deref(), json, summary)
                        .await?;
                    Ok(())
                }
                ModelCommand::Info {
                    model,
                    provider,
                    json,
                    summary,
                } => {
                    commands::models::show_model_info(
                        &config,
                        &model,
                        provider.as_deref(),
                        json,
                        summary,
                    )
                    .await?;
                    Ok(())
                }
                ModelCommand::Current { provider } => {
                    commands::models::show_current_model(&config, provider.as_deref()).await?;
                    Ok(())
                }
            }
        }
        Commands::History { command } => {
            tracing::info!("Starting history command");
            commands::history::handle_history(command)?;
            Ok(())
        }
        Commands::Replay {
            id,
            list,
            db_path,
            limit,
            offset,
            tree,
        } => {
            tracing::info!("Starting replay command for conversation debugging");
            let args = commands::replay::ReplayArgs {
                id,
                list,
                db_path,
                limit,
                offset,
                tree,
            };
            commands::replay::run_replay(args).await?;
            Ok(())
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
