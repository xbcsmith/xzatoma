//! XZatoma - Autonomous AI agent CLI
//!
#![doc = "XZatoma - Autonomous AI agent CLI"]
#![doc = "Main entry point for the XZatoma agent application."]

use std::{fs::OpenOptions, path::Path, sync::Arc};

use xzatoma::error::Result;

use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};
use xzatoma::config::LogFormat;

// Removed unused grouped imports to satisfy clippy

use xzatoma::cli::{AcpCommand, Cli, Commands, ModelCommand, SkillsCommand};
use xzatoma::commands;

use xzatoma::config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments first so logging flags are available before the
    // subscriber is initialised.
    let cli = Cli::parse_args();

    // Clone CommonArgs so cli.command can be moved in the match below.
    let common = cli.command.common_args().clone();

    // --verbose maps to debug level for backward compatibility.
    let debug = common.debug || common.verbose;
    let trace = common.trace;

    // Derive stderr format from the global --log-format flag, defaulting to plain.
    let stderr_format = common.log_format.unwrap_or(LogFormat::Plain);

    // Initialise the global tracing subscriber exactly once.
    init_tracing(
        debug,
        trace,
        stderr_format,
        LogFormat::Json,
        common.log_file.as_deref(),
    );

    // If the user supplied a storage path on the CLI (or via env),
    // mirror it into XZATOMA_HISTORY_DB so the storage initializer can pick it up.
    // This keeps callers unchanged while allowing `SqliteStorage::new()` to
    // honor an override.
    if let Some(db_path) = &common.storage_path {
        std::env::set_var("XZATOMA_HISTORY_DB", db_path);
        tracing::info!("Using storage DB override from CLI: {}", db_path);
    }

    // Load configuration
    let config_path = common.config.as_deref().unwrap_or("config/config.yaml");
    let config = Config::load(config_path, &common)?;

    // Validate configuration
    config.validate()?;

    // Execute command
    match cli.command {
        Commands::Chat {
            provider,
            mode,
            safe,
            resume,
            thinking_effort,
            system_prompt,
            ..
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
            if let Some(ref e) = thinking_effort {
                tracing::debug!("Using thinking effort: {}", e);
            }
            if let Some(ref sp) = system_prompt {
                tracing::debug!("Using system prompt override (length={})", sp.len());
            }

            // Delegate to the chat command handler
            // Moves `config` into the handler (match arms are exclusive)
            commands::chat::run_chat(
                config,
                provider,
                mode,
                safe,
                resume,
                thinking_effort,
                system_prompt,
            )
            .await?;
            Ok(())
        }
        Commands::Run {
            plan,
            prompt,
            allow_dangerous,
            thinking_effort,
            system_prompt,
            ..
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
            if let Some(ref e) = thinking_effort {
                tracing::debug!("Using thinking effort: {}", e);
            }
            if let Some(ref sp) = system_prompt {
                tracing::debug!("Using system prompt override (length={})", sp.len());
            }

            // Convert plan PathBuf to String before passing it to the command handler.
            let plan_str = plan.map(|p| p.to_string_lossy().to_string());
            commands::run::run_plan_with_options(
                config,
                plan_str,
                prompt,
                allow_dangerous,
                thinking_effort,
                system_prompt,
            )
            .await?;
            Ok(())
        }
        Commands::Watch {
            topic,
            event_types,
            filter_config,
            log_file,
            json_logs,
            watcher_type,
            group_id,
            output_topic,
            create_topics,
            action,
            name,
            dry_run,
            brokers,
            match_version,
            system_prompt,
            ..
        } => {
            tracing::info!("Starting watcher mode");
            commands::watch::run_watch(
                config,
                commands::watch::WatchCliOverrides {
                    topic,
                    event_types,
                    filter_config,
                    log_file,
                    json_logs,
                    watcher_type,
                    group_id,
                    output_topic,
                    create_topics,
                    action,
                    name,
                    dry_run,
                    brokers,
                    match_version,
                    system_prompt,
                },
            )
            .await?;
            Ok(())
        }
        Commands::Auth { provider, .. } => {
            // Use CLI `--provider` override when supplied; otherwise fall back to the
            // configured/default provider from `config`.
            let provider = provider.unwrap_or_else(|| config.provider.provider_type.clone());
            tracing::info!("Starting authentication for provider: {}", provider);
            commands::auth::authenticate(config, provider).await?;
            Ok(())
        }
        Commands::Models { command, .. } => {
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
        Commands::History { command, .. } => {
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
            ..
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
        Commands::Mcp { command, .. } => {
            tracing::info!("Starting MCP command");
            commands::mcp::handle_mcp(command, config).await?;
            Ok(())
        }
        Commands::Agent {
            provider,
            model,
            allow_dangerous,
            working_dir,
            system_prompt,
            ..
        } => {
            commands::agent::handle_agent(
                provider,
                model,
                allow_dangerous,
                working_dir,
                system_prompt,
                config,
            )
            .await?;
            Ok(())
        }
        Commands::Acp { command, .. } => {
            tracing::info!("Starting ACP command");
            match &command {
                AcpCommand::Serve { .. }
                | AcpCommand::Config
                | AcpCommand::Runs { .. }
                | AcpCommand::Validate { .. } => {
                    commands::acp::handle_acp(command, config).await?;
                    Ok(())
                }
            }
        }
        Commands::Skills { command, .. } => {
            tracing::info!("Starting skills command");
            match command {
                SkillsCommand::List => {
                    commands::skills::list_skills(config)?;
                    Ok(())
                }
                SkillsCommand::Validate => {
                    commands::skills::validate_skills(config)?;
                    Ok(())
                }
                SkillsCommand::Show { name } => {
                    commands::skills::show_skill(config, &name)?;
                    Ok(())
                }
                SkillsCommand::Paths => {
                    commands::skills::show_paths(config)?;
                    Ok(())
                }
                SkillsCommand::Trust { command } => {
                    commands::skills::handle_trust(command, config)?;
                    Ok(())
                }
            }
        }
    }
}

/// Return the default log-level directive string for the given level flags.
///
/// This is extracted so that the level-selection logic can be unit-tested
/// without initialising a tracing subscriber (which may only be done once
/// per process).
///
/// Precedence: `trace` > `debug` > info default.
///
/// # Arguments
///
/// * `debug` - When `true` and `trace` is `false`, returns `"debug"`.
/// * `trace` - When `true`, returns `"trace"` regardless of `debug`.
///
/// # Examples
///
/// ```no_run
/// // log_level_str is pub(crate) in the binary — use the unit tests in this
/// // module to verify its behaviour rather than a doc example.
/// ```
pub(crate) fn log_level_str(debug: bool, trace: bool) -> &'static str {
    if trace {
        "trace"
    } else if debug {
        "debug"
    } else {
        "xzatoma=info"
    }
}

/// Initialize the global tracing subscriber.
///
/// Must be called exactly once per process. Calling it a second time will
/// panic; use `RUST_LOG` to override the log level at runtime instead.
///
/// # Arguments
///
/// * `debug` - When `true` and `trace` is `false`, sets the default level to `DEBUG`.
/// * `trace` - When `true`, sets the default level to `TRACE`.
/// * `stderr_format` - Output format for the stderr sink.
/// * `file_format` - Output format for the optional file sink.
/// * `log_file` - Optional path to an additional log-file sink. The file
///   is created (or appended to) in the format specified by `file_format`.
pub(crate) fn init_tracing(
    debug: bool,
    trace: bool,
    stderr_format: LogFormat,
    file_format: LogFormat,
    log_file: Option<&Path>,
) {
    let level = log_level_str(debug, trace);
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    // Open the optional file sink. Failures are non-fatal: we print a
    // warning to stderr and continue without the file layer.
    let file_sink: Option<Arc<std::fs::File>> = log_file.and_then(|path| {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map(Arc::new)
            .map_err(|e| eprintln!("Warning: failed to open log file '{}': {e}", path.display()))
            .ok()
    });

    let stderr_layer = match stderr_format {
        LogFormat::Plain => fmt::layer().with_writer(std::io::stderr).boxed(),
        LogFormat::Compact => fmt::layer().compact().with_writer(std::io::stderr).boxed(),
        LogFormat::Json => fmt::layer().json().with_writer(std::io::stderr).boxed(),
    };

    let file_layer = file_sink.map(|f| match file_format {
        LogFormat::Plain => fmt::layer().with_writer(f).boxed(),
        LogFormat::Compact => fmt::layer().compact().with_writer(f).boxed(),
        LogFormat::Json => fmt::layer().json().with_writer(f).boxed(),
    });

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stderr_layer)
        .with(file_layer)
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_tracing_verbose_false_defaults_to_info() {
        assert_eq!(log_level_str(false, false), "xzatoma=info");
    }

    #[test]
    fn test_init_tracing_verbose_true_uses_debug() {
        assert_eq!(log_level_str(true, false), "debug");
    }

    #[test]
    fn test_init_tracing_debug_flag_uses_debug() {
        assert_eq!(log_level_str(true, false), "debug");
    }

    #[test]
    fn test_init_tracing_trace_flag_uses_trace() {
        assert_eq!(log_level_str(false, true), "trace");
    }

    #[test]
    fn test_init_tracing_trace_overrides_debug() {
        assert_eq!(log_level_str(true, true), "trace");
    }
}
