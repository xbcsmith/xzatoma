//! XZatoma - Autonomous AI agent CLI
//!
//! Main entry point for the XZatoma agent application.

use anyhow::Result;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod agent;
mod cli;
mod config;
mod error;
mod providers;
mod tools;

use cli::{Cli, Commands};

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
        Commands::Chat { provider } => {
            tracing::info!("Starting interactive chat mode");
            if let Some(p) = provider {
                tracing::debug!("Using provider override: {}", p);
            }
            println!("Chat mode not yet implemented");
            Ok(())
        }
        Commands::Run {
            plan,
            prompt,
            allow_dangerous,
        } => {
            tracing::info!("Starting plan execution mode");
            if let Some(plan_path) = plan {
                tracing::debug!("Loading plan from: {}", plan_path.display());
            }
            if let Some(prompt_text) = prompt {
                tracing::debug!("Using prompt: {}", prompt_text);
            }
            if allow_dangerous {
                tracing::warn!("Dangerous commands are allowed!");
            }
            println!("Run mode not yet implemented");
            Ok(())
        }
        Commands::Auth { provider } => {
            tracing::info!("Starting authentication for provider: {}", provider);
            println!("Auth mode not yet implemented");
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
