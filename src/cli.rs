//! Command-line interface definition for XZatoma
//!
//! This module defines the CLI structure using clap's derive API,
//! providing commands for chat, plan execution, and authentication.
//!
//! Shared options (`--config`, `--verbose`, `--storage-path`) are carried by
//! [`CommonArgs`], which is flattened into every [`Commands`] variant.  Flags
//! must appear after the subcommand token:
//! `xzatoma subcommand --config path.yaml`.

use crate::config::LogFormat;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// Shared options that apply to every subcommand.
///
/// Each [`Commands`] variant flattens `CommonArgs` so that shared flags appear
/// after the subcommand token: `xzatoma subcommand --config x.yaml`.
///
/// # Examples
///
/// ```
/// use xzatoma::cli::{Cli, CommonArgs};
/// use clap::Parser;
///
/// let cli = Cli::parse_from(["xzatoma", "auth", "--verbose"]);
/// assert!(cli.command.common_args().verbose);
/// ```
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct CommonArgs {
    /// Path to the configuration file.
    ///
    /// When omitted, xzatoma searches for a config file in this order:
    /// 1. `~/.config/xzatoma/config.yaml` (XDG standard user config)
    /// 2. `config/config.yaml` (project-relative development fallback)
    ///
    /// If neither file exists, built-in defaults are used. Set the
    /// `XZATOMA_CONFIG` environment variable as an alternative to this flag.
    #[arg(short = 'c', long, env = "XZATOMA_CONFIG")]
    pub config: Option<String>,

    /// Enable verbose logging.
    ///
    /// Deprecated: use `--debug` or `--trace` instead.
    #[arg(short = 'v', long)]
    pub verbose: bool,

    /// Enable debug-level logging.
    ///
    /// Equivalent to `RUST_LOG=debug`. Takes precedence over `--verbose`.
    /// Precedence: `--trace` > `--debug` > `--verbose` > `RUST_LOG` env > info default.
    #[arg(long, env = "XZATOMA_DEBUG")]
    pub debug: bool,

    /// Enable trace-level logging.
    ///
    /// Equivalent to `RUST_LOG=trace`. Implies `--debug`. Use for LLM
    /// transcript capture and deep protocol inspection.
    /// Precedence: `--trace` > `--debug` > `--verbose` > `RUST_LOG` env > info default.
    #[arg(long, env = "XZATOMA_TRACE")]
    pub trace: bool,

    /// Override the path to the history database (or set env XZATOMA_HISTORY_DB)
    #[arg(long, env = "XZATOMA_HISTORY_DB")]
    pub storage_path: Option<String>,

    /// Override log format for stderr output.
    ///
    /// Accepts `plain`, `compact`, or `json`. Overrides `config.log.stderr_format`
    /// when set. Env: `XZATOMA_LOG_FORMAT`.
    #[arg(long, env = "XZATOMA_LOG_FORMAT", value_enum)]
    pub log_format: Option<LogFormat>,

    /// Write a second log stream to the given file path.
    ///
    /// The file is created or appended to. Defaults to JSON format unless
    /// overridden by `XZATOMA_LOG_FILE_FORMAT`.
    /// Distinct from `--log-file` in the `watch` subcommand, which controls
    /// per-event watcher output. Env: `XZATOMA_LOG_FILE`.
    #[arg(id = "global-logfile", long = "logfile", env = "XZATOMA_LOG_FILE")]
    pub log_file: Option<PathBuf>,
}

/// XZatoma - Autonomous AI agent CLI.
///
/// Execute tasks through conversation with AI providers using basic file system
/// and terminal tools.
///
/// Shared options (`--config`, `--verbose`, `--storage-path`) must appear
/// after the subcommand token: `xzatoma subcommand --flag`.
#[derive(Parser, Debug, Clone)]
#[command(name = "xzatoma")]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Command to execute
    #[command(subcommand)]
    pub command: Commands,
}

/// Available commands for XZatoma
#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Start interactive chat mode with the agent
    Chat {
        /// Shared flags (config, verbose, storage-path)
        #[command(flatten)]
        common: CommonArgs,

        /// Override the provider from config (copilot, ollama)
        #[arg(short, long)]
        provider: Option<String>,

        /// Chat mode: planning (read-only) or write (read/write)
        #[arg(short, long, default_value = "planning")]
        mode: Option<String>,

        /// Enable safety mode (always confirm dangerous operations)
        #[arg(short = 's', long)]
        safe: bool,

        /// Resume a specific conversation by ID
        #[arg(long)]
        resume: Option<String>,

        /// Thinking effort level for models that support extended reasoning.
        ///
        /// Accepted values: none, low, medium, high, extra_high.
        /// When omitted, the value from the configuration file is used.
        /// When set to "none", reasoning parameters are cleared even if the
        /// configuration file specifies a level.
        #[arg(long)]
        thinking_effort: Option<String>,

        /// Override the system prompt for this session.
        ///
        /// On resumed chat sessions this flag always replaces any stored system
        /// message. When running a plan the plan's own system_prompt field takes
        /// precedence over this flag.
        #[arg(long)]
        system_prompt: Option<String>,

        /// Stream model output tokens to the terminal as they are generated.
        ///
        /// When set, reasoning tokens (thinking) are printed with a visual
        /// indicator before the response tokens. Requires the configured
        /// provider to support streaming.
        #[arg(long)]
        streaming: bool,
    },

    /// Execute a plan or prompt
    Run {
        /// Shared flags (config, verbose, storage-path)
        #[command(flatten)]
        common: CommonArgs,

        /// Path to plan file (YAML format)
        #[arg(short, long)]
        plan: Option<PathBuf>,

        /// Direct prompt to execute (alternative to plan file)
        #[arg(long)]
        prompt: Option<String>,

        /// Allow dangerous commands without confirmation
        #[arg(long)]
        allow_dangerous: bool,

        /// Thinking effort level for models that support extended reasoning.
        ///
        /// Accepted values: none, low, medium, high, extra_high.
        /// When omitted, the value from the configuration file is used.
        #[arg(long)]
        thinking_effort: Option<String>,

        /// Override the system prompt for this session.
        ///
        /// When running a plan file, the plan's own system_prompt field takes
        /// precedence over this flag.
        #[arg(long)]
        system_prompt: Option<String>,

        /// Stream model output tokens to the terminal as they are generated.
        ///
        /// When set, response and reasoning tokens are printed progressively.
        /// Requires the configured provider to support streaming.
        #[arg(long)]
        streaming: bool,
    },

    /// Run as an ACP stdio agent subprocess for Zed or another ACP-compatible client
    Agent {
        /// Shared flags (config, verbose, storage-path)
        #[command(flatten)]
        common: CommonArgs,

        /// Override the provider from config (copilot, ollama, openai)
        #[arg(long)]
        provider: Option<String>,

        /// Override the model within the selected provider
        #[arg(long)]
        model: Option<String>,

        /// Allow dangerous terminal commands without confirmation
        #[arg(long)]
        allow_dangerous: bool,

        /// Fallback workspace root when the ACP client does not provide one
        #[arg(long)]
        working_dir: Option<PathBuf>,

        /// Override the system prompt for this agent session.
        #[arg(long)]
        system_prompt: Option<String>,

        /// Stream model output tokens to the terminal as they are generated.
        ///
        /// Note: the agent command uses ACP stdio mode; this flag is accepted
        /// for API consistency but has no effect on the ACP protocol stream.
        #[arg(long)]
        streaming: bool,
    },

    /// Watch Kafka topic for events and execute plans
    Watch {
        /// Shared flags (config, verbose, storage-path)
        #[command(flatten)]
        common: CommonArgs,

        /// Kafka topic to watch (overrides config)
        #[arg(short, long)]
        topic: Option<String>,

        /// Event types to process (comma-separated)
        #[arg(short = 'e', long)]
        event_types: Option<String>,

        /// Filter configuration file (YAML)
        #[arg(short = 'f', long)]
        filter_config: Option<PathBuf>,

        /// Log output file (defaults to STDOUT only)
        #[arg(short = 'l', long)]
        log_file: Option<PathBuf>,

        /// Enable JSON-formatted logging
        #[arg(long, default_value = "true")]
        json_logs: bool,

        /// Watcher backend type: "xzepr" or "generic" (default: from config file)
        #[arg(long)]
        watcher_type: Option<String>,

        /// Kafka consumer group ID (overrides config)
        #[arg(long)]
        group_id: Option<String>,

        /// Output topic for publishing results (generic watcher only; defaults to input topic)
        #[arg(long)]
        output_topic: Option<String>,

        /// Create missing Kafka topics automatically at watcher startup
        #[arg(long)]
        create_topics: bool,

        /// Generic matcher: regex pattern for the action field (case-insensitive)
        #[arg(long)]
        action: Option<String>,

        /// Generic matcher: regex pattern for the name field (case-insensitive)
        #[arg(long)]
        name: Option<String>,

        /// Generic matcher: regex pattern for the version field (case-insensitive)
        #[arg(long = "match-version")]
        match_version: Option<String>,

        /// Kafka broker addresses (comma-separated, overrides config)
        #[arg(long)]
        brokers: Option<String>,

        /// Dry run mode (parse but don't execute plans)
        #[arg(long)]
        dry_run: bool,

        /// Override the system prompt for agent sessions triggered by this watcher.
        #[arg(long)]
        system_prompt: Option<String>,
    },

    /// Authenticate with a provider
    Auth {
        /// Shared flags (config, verbose, storage-path)
        #[command(flatten)]
        common: CommonArgs,

        /// Provider to authenticate with (copilot, ollama)
        ///
        /// Use `--provider <name>` to override; if omitted the configured/default
        /// provider will be used.
        #[arg(short, long)]
        provider: Option<String>,
    },

    /// Manage AI models (discover, inspect, and query models)
    ///
    /// Subcommands:
    /// - `list` — List available models
    /// - `info` — Show detailed info for a model
    /// - `current` — Show the currently active model
    ///
    /// Examples:
    ///   xzatoma models list --summary
    ///   xzatoma models info --model gpt-4 --json
    Models {
        /// Shared flags (config, verbose, storage-path)
        #[command(flatten)]
        common: CommonArgs,

        /// Model management subcommand
        #[command(subcommand)]
        command: ModelCommand,
    },

    /// Manage conversation history
    History {
        /// Shared flags (config, verbose, storage-path)
        #[command(flatten)]
        common: CommonArgs,

        /// History management subcommand
        #[command(subcommand)]
        command: HistoryCommand,
    },

    /// Replay subagent conversations for debugging
    Replay {
        /// Shared flags (config, verbose, storage-path)
        #[command(flatten)]
        common: CommonArgs,

        /// Conversation ID to replay
        #[arg(long, short = 'i')]
        id: Option<String>,

        /// List all conversations
        #[arg(long, short = 'l')]
        list: bool,

        /// Path to conversation database
        #[arg(long, default_value = "~/.xzatoma/conversations.db")]
        db_path: std::path::PathBuf,

        /// Limit for list results
        #[arg(long, default_value = "10")]
        limit: usize,

        /// Offset for pagination
        #[arg(long, default_value = "0")]
        offset: usize,

        /// Show conversation tree (with nested subagents)
        #[arg(long, short = 't')]
        tree: bool,
    },

    /// MCP server management commands
    Mcp {
        /// Shared flags (config, verbose, storage-path)
        #[command(flatten)]
        common: CommonArgs,

        /// MCP subcommand to execute
        #[command(subcommand)]
        command: crate::commands::mcp::McpCommands,
    },

    /// ACP server management commands
    Acp {
        /// Shared flags (config, verbose, storage-path)
        #[command(flatten)]
        common: CommonArgs,

        /// ACP subcommand to execute
        #[command(subcommand)]
        command: AcpCommand,
    },

    /// Manage agent skills
    Skills {
        /// Shared flags (config, verbose, storage-path)
        #[command(flatten)]
        common: CommonArgs,

        /// Skills subcommand to execute
        #[command(subcommand)]
        command: SkillsCommand,
    },
}

impl Commands {
    /// Return the [`CommonArgs`] embedded in the active variant.
    ///
    /// This is the sole extraction point used by `main()` and any call site
    /// that needs shared flags before dispatching to a command handler.
    ///
    /// # Returns
    ///
    /// A reference to the [`CommonArgs`] for the active subcommand.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::cli::{Cli, Commands};
    /// use clap::Parser;
    ///
    /// let cli = Cli::parse_from(["xzatoma", "auth", "--verbose"]);
    /// assert!(cli.command.common_args().verbose);
    /// ```
    pub fn common_args(&self) -> &CommonArgs {
        match self {
            Commands::Chat { common, .. } => common,
            Commands::Run { common, .. } => common,
            Commands::Agent { common, .. } => common,
            Commands::Watch { common, .. } => common,
            Commands::Auth { common, .. } => common,
            Commands::Models { common, .. } => common,
            Commands::History { common, .. } => common,
            Commands::Replay { common, .. } => common,
            Commands::Mcp { common, .. } => common,
            Commands::Acp { common, .. } => common,
            Commands::Skills { common, .. } => common,
        }
    }
}

/// Skills management subcommands
#[derive(Subcommand, Debug, Clone)]
pub enum SkillsCommand {
    /// List valid loaded skills only
    List,

    /// Validate configured skill roots and show diagnostics
    Validate,

    /// Show metadata for one valid loaded skill
    Show {
        /// Skill name to inspect
        name: String,
    },

    /// Show effective discovery paths and trust state
    Paths,

    /// Manage skill trust state
    Trust {
        /// Trust subcommand to execute
        #[command(subcommand)]
        command: SkillsTrustCommand,
    },
}

/// Skills trust subcommands
#[derive(Subcommand, Debug, Clone)]
pub enum SkillsTrustCommand {
    /// Show trust configuration and trusted paths
    Show,

    /// Mark a project path trusted
    Add(SkillsTrustPathArgs),

    /// Remove trust for a project path
    Remove(SkillsTrustPathArgs),
}

/// Shared path arguments for skills trust operations
#[derive(Args, Debug, Clone)]
pub struct SkillsTrustPathArgs {
    /// Path to add or remove from the trust store
    pub path: PathBuf,
}

/// Model management subcommands
#[derive(Subcommand, Debug, Clone)]
pub enum ModelCommand {
    /// List available models.
    ///
    /// Output formats:
    /// - Use `--json` to produce pretty-printed JSON suitable for scripting or exporting.
    /// - Use `--summary` to produce a human-friendly compact summary table. Combine both to
    ///   include summary data in JSON output (`--json --summary`).
    ///
    /// Examples:
    ///   xzatoma models list --summary
    ///   xzatoma models list --json > all_models.json
    ///   xzatoma models list --json --summary > all_models_with_summary.json
    List {
        /// Filter by provider (copilot, ollama)
        #[arg(short, long)]
        provider: Option<String>,

        #[arg(
            short,
            long,
            help = "Output in pretty JSON format (useful for scripting/export)."
        )]
        json: bool,

        #[arg(
            short = 's',
            long,
            help = "Output a compact, human-readable summary table."
        )]
        summary: bool,
    },

    /// Show detailed information about a model.
    ///
    /// Output formats:
    /// - `--json` returns pretty-printed JSON with full model details (good for programmatic use).
    /// - `--summary` returns a compact, human-readable summary combining basic info and provider metadata.
    ///
    /// Examples:
    ///   xzatoma models info --model gpt-4 --summary
    ///   xzatoma models info --model gpt-4 --json > gpt4_info.json
    Info {
        /// Model name/identifier
        #[arg(short, long)]
        model: String,

        /// Filter by provider (copilot, ollama)
        #[arg(short, long)]
        provider: Option<String>,

        #[arg(
            short,
            long,
            help = "Output in pretty JSON format (useful for scripting/export)."
        )]
        json: bool,

        #[arg(
            short = 's',
            long,
            help = "Output a compact, human-readable summary for the specified model."
        )]
        summary: bool,
    },

    /// Show the currently active model
    Current {
        /// Filter by provider (copilot, ollama)
        #[arg(short, long)]
        provider: Option<String>,
    },
}

/// ACP server management subcommands
#[derive(Subcommand, Debug, Clone)]
pub enum AcpCommand {
    /// Start the ACP HTTP discovery server
    Serve {
        /// Override bind host for the ACP server
        #[arg(long)]
        host: Option<String>,

        /// Override bind port for the ACP server
        #[arg(long)]
        port: Option<u16>,

        /// Override ACP route base path in versioned mode
        #[arg(long)]
        base_path: Option<String>,

        /// Enable ACP root-compatible routing
        #[arg(long)]
        root_compatible: bool,

        /// Override the system prompt for all agent runs handled by this server.
        #[arg(long)]
        system_prompt: Option<String>,
    },

    /// Print the effective ACP configuration after file and environment overrides
    Config,

    /// List active or recent ACP runs for inspection
    Runs {
        /// Optional session identifier to filter runs
        #[arg(long)]
        session_id: Option<String>,

        /// Maximum number of runs to display
        #[arg(long, default_value = "20")]
        limit: usize,
    },

    /// Validate ACP manifest and configuration compatibility
    Validate {
        /// Optional manifest path to validate
        #[arg(long)]
        manifest: Option<PathBuf>,
    },
}

/// History management subcommands
#[derive(Subcommand, Debug, Clone)]
pub enum HistoryCommand {
    /// List saved conversations
    List,

    /// Show detailed message-level history for a conversation
    Show {
        /// Conversation ID to display
        #[arg(short, long)]
        id: String,

        /// Output raw JSON format instead of formatted display
        #[arg(short, long)]
        raw: bool,

        /// Show only the last N messages (default: all)
        #[arg(short = 'n', long)]
        limit: Option<usize>,
    },

    /// Delete a saved conversation
    Delete {
        /// ID of the conversation to delete
        #[arg(short, long)]
        id: String,
    },
}

impl Cli {
    /// Parse command line arguments
    ///
    /// # Returns
    ///
    /// Returns the parsed CLI structure
    pub fn parse_args() -> Self {
        Self::parse()
    }
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            command: Commands::Auth {
                common: CommonArgs::default(),
                provider: Some("copilot".to_string()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_cli_default() {
        let cli = Cli::default();
        assert_eq!(cli.command.common_args().config, None);
        assert!(!cli.command.common_args().verbose);

        if let Commands::Auth { provider, .. } = cli.command {
            assert_eq!(provider, Some("copilot".to_string()));
        } else {
            panic!("Expected default command to be Auth");
        }
    }

    #[test]
    fn test_cli_parses_agent_defaults() {
        let cli = Cli::try_parse_from(["xzatoma", "agent"]);
        assert!(cli.is_ok());

        let cli = cli.unwrap();
        match cli.command {
            Commands::Agent {
                provider,
                model,
                allow_dangerous,
                working_dir,
                ..
            } => {
                assert!(provider.is_none());
                assert!(model.is_none());
                assert!(!allow_dangerous);
                assert!(working_dir.is_none());
            }
            other => panic!("expected agent command, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_parses_agent_with_provider() {
        let cli = Cli::try_parse_from(["xzatoma", "agent", "--provider", "ollama"]);
        assert!(cli.is_ok());

        let cli = cli.unwrap();
        match cli.command {
            Commands::Agent { provider, .. } => {
                assert_eq!(provider.as_deref(), Some("ollama"));
            }
            other => panic!("expected agent command, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_parses_agent_with_copilot_provider_and_model() {
        let cli = Cli::try_parse_from([
            "xzatoma",
            "agent",
            "--provider",
            "copilot",
            "--model",
            "gpt-4o",
        ]);
        assert!(cli.is_ok());

        let cli = cli.unwrap();
        match cli.command {
            Commands::Agent {
                provider,
                model,
                allow_dangerous,
                ..
            } => {
                assert_eq!(provider.as_deref(), Some("copilot"));
                assert_eq!(model.as_deref(), Some("gpt-4o"));
                assert!(!allow_dangerous);
            }
            other => panic!("expected agent command, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_parses_agent_with_provider_model_and_allow_dangerous() {
        let cli = Cli::try_parse_from([
            "xzatoma",
            "agent",
            "--provider",
            "openai",
            "--model",
            "gpt-4o",
            "--allow-dangerous",
        ]);
        assert!(cli.is_ok());

        let cli = cli.unwrap();
        match cli.command {
            Commands::Agent {
                provider,
                model,
                allow_dangerous,
                ..
            } => {
                assert_eq!(provider.as_deref(), Some("openai"));
                assert_eq!(model.as_deref(), Some("gpt-4o"));
                assert!(allow_dangerous);
            }
            other => panic!("expected agent command, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_parses_agent_with_working_dir() {
        let cli = Cli::try_parse_from([
            "xzatoma",
            "agent",
            "--working-dir",
            "/tmp/xzatoma-zed-workspace",
        ]);
        assert!(cli.is_ok());

        let cli = cli.unwrap();
        match cli.command {
            Commands::Agent { working_dir, .. } => {
                assert_eq!(
                    working_dir,
                    Some(PathBuf::from("/tmp/xzatoma-zed-workspace"))
                );
            }
            other => panic!("expected agent command, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_parses_acp_serve_defaults() {
        let cli = Cli::parse_from(["xzatoma", "acp", "serve"]);

        match cli.command {
            Commands::Acp { command, .. } => match command {
                AcpCommand::Serve {
                    host,
                    port,
                    base_path,
                    root_compatible,
                    ..
                } => {
                    assert!(host.is_none());
                    assert!(port.is_none());
                    assert!(base_path.is_none());
                    assert!(!root_compatible);
                }
                other => panic!("expected ACP serve command, got {:?}", other),
            },
            other => panic!("expected ACP command, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_parses_acp_serve_overrides() {
        let cli = Cli::parse_from([
            "xzatoma",
            "acp",
            "serve",
            "--host",
            "0.0.0.0",
            "--port",
            "9000",
            "--base-path",
            "/acp",
            "--root-compatible",
        ]);

        match cli.command {
            Commands::Acp { command, .. } => match command {
                AcpCommand::Serve {
                    host,
                    port,
                    base_path,
                    root_compatible,
                    ..
                } => {
                    assert_eq!(host.as_deref(), Some("0.0.0.0"));
                    assert_eq!(port, Some(9000));
                    assert_eq!(base_path.as_deref(), Some("/acp"));
                    assert!(root_compatible);
                }
                other => panic!("expected ACP serve command, got {:?}", other),
            },
            other => panic!("expected ACP command, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_parses_acp_config_subcommand() {
        let cli = Cli::parse_from(["xzatoma", "acp", "config"]);

        match cli.command {
            Commands::Acp { command, .. } => match command {
                AcpCommand::Config => {}
                other => panic!("expected ACP config command, got {:?}", other),
            },
            other => panic!("expected ACP command, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_parses_acp_runs_subcommand_with_filters() {
        let cli = Cli::parse_from([
            "xzatoma",
            "acp",
            "runs",
            "--session-id",
            "session_123",
            "--limit",
            "5",
        ]);

        match cli.command {
            Commands::Acp { command, .. } => match command {
                AcpCommand::Runs { session_id, limit } => {
                    assert_eq!(session_id.as_deref(), Some("session_123"));
                    assert_eq!(limit, 5);
                }
                other => panic!("expected ACP runs command, got {:?}", other),
            },
            other => panic!("expected ACP command, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_parses_acp_validate_subcommand() {
        let cli = Cli::parse_from([
            "xzatoma",
            "acp",
            "validate",
            "--manifest",
            "docs/reference/acp_manifest.json",
        ]);

        match cli.command {
            Commands::Acp { command, .. } => match command {
                AcpCommand::Validate { manifest } => {
                    assert_eq!(
                        manifest,
                        Some(PathBuf::from("docs/reference/acp_manifest.json"))
                    );
                }
                other => panic!("expected ACP validate command, got {:?}", other),
            },
            other => panic!("expected ACP command, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_parse_chat_command() {
        let cli = Cli::try_parse_from(["xzatoma", "chat"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert!(matches!(cli.command, Commands::Chat { .. }));
    }

    #[test]
    fn test_cli_parse_chat_with_provider() {
        let cli = Cli::try_parse_from(["xzatoma", "chat", "--provider", "ollama"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Chat { provider, .. } = cli.command {
            assert_eq!(provider, Some("ollama".to_string()));
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_cli_parse_chat_with_resume() {
        let cli = Cli::try_parse_from(["xzatoma", "chat", "--resume", "abc123"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Chat { resume, .. } = cli.command {
            assert_eq!(resume, Some("abc123".to_string()));
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_cli_parse_history_list() {
        let cli = Cli::try_parse_from(["xzatoma", "history", "list"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::History { command, .. } = cli.command {
            assert!(matches!(command, HistoryCommand::List));
        } else {
            panic!("Expected History command");
        }
    }

    #[test]
    fn test_cli_parse_history_delete() {
        let cli = Cli::try_parse_from(["xzatoma", "history", "delete", "--id", "session123"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::History { command, .. } = cli.command {
            if let HistoryCommand::Delete { id } = command {
                assert_eq!(id, "session123".to_string());
            } else {
                panic!("Expected Delete command");
            }
        } else {
            panic!("Expected History command");
        }
    }

    #[test]
    fn test_cli_parse_history_show_parses_id() {
        let cli = Cli::try_parse_from(["xzatoma", "history", "show", "--id", "abc123"]).unwrap();

        match cli.command {
            Commands::History {
                command: HistoryCommand::Show { id, raw, limit },
                ..
            } => {
                assert_eq!(id, "abc123");
                assert!(!raw);
                assert_eq!(limit, None);
            }
            _ => panic!("Expected History::Show command"),
        }
    }

    #[test]
    fn test_cli_parse_history_show_parses_raw_flag() {
        let cli =
            Cli::try_parse_from(["xzatoma", "history", "show", "--id", "abc123", "--raw"]).unwrap();

        match cli.command {
            Commands::History {
                command: HistoryCommand::Show { id, raw, limit },
                ..
            } => {
                assert_eq!(id, "abc123");
                assert!(raw);
                assert_eq!(limit, None);
            }
            _ => panic!("Expected History::Show command"),
        }
    }

    #[test]
    fn test_cli_parse_history_show_parses_limit() {
        let cli = Cli::try_parse_from([
            "xzatoma", "history", "show", "--id", "abc123", "--limit", "10",
        ])
        .unwrap();

        match cli.command {
            Commands::History {
                command: HistoryCommand::Show { id, raw, limit },
                ..
            } => {
                assert_eq!(id, "abc123");
                assert!(!raw);
                assert_eq!(limit, Some(10));
            }
            _ => panic!("Expected History::Show command"),
        }
    }

    #[test]
    fn test_cli_parse_storage_path_after_subcommand() {
        let cli = Cli::try_parse_from(["xzatoma", "auth", "--storage-path", "/tmp/my.db"])
            .expect("failed to parse CLI args (storage-path)");
        assert_eq!(
            cli.command.common_args().storage_path,
            Some("/tmp/my.db".to_string())
        );
    }

    #[test]
    fn test_cli_parse_run_with_plan() {
        let cli = Cli::try_parse_from(["xzatoma", "run", "--plan", "test.yaml"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Run {
            plan,
            prompt,
            allow_dangerous,
            ..
        } = cli.command
        {
            assert_eq!(plan, Some(PathBuf::from("test.yaml")));
            assert_eq!(prompt, None);
            assert!(!allow_dangerous);
        } else {
            panic!("Expected Run command");
        }
    }

    #[test]
    fn test_cli_parse_run_with_prompt() {
        let cli = Cli::try_parse_from(["xzatoma", "run", "--prompt", "Hello world"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Run {
            plan,
            prompt,
            allow_dangerous,
            ..
        } = cli.command
        {
            assert_eq!(plan, None);
            assert_eq!(prompt, Some("Hello world".to_string()));
            assert!(!allow_dangerous);
        } else {
            panic!("Expected Run command");
        }
    }

    #[test]
    fn test_cli_parse_run_with_allow_dangerous() {
        let cli = Cli::try_parse_from(["xzatoma", "run", "--prompt", "test", "--allow-dangerous"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Run {
            plan,
            prompt,
            allow_dangerous,
            ..
        } = cli.command
        {
            assert_eq!(plan, None);
            assert_eq!(prompt, Some("test".to_string()));
            assert!(allow_dangerous);
        } else {
            panic!("Expected Run command");
        }
    }

    #[test]
    fn test_cli_parse_watch_defaults() {
        let cli = Cli::try_parse_from(["xzatoma", "watch"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();

        if let Commands::Watch {
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
            match_version,
            brokers,
            dry_run,
            ..
        } = cli.command
        {
            assert_eq!(topic, None);
            assert_eq!(event_types, None);
            assert_eq!(filter_config, None);
            assert_eq!(log_file, None);
            assert!(json_logs);
            assert_eq!(watcher_type, None);
            assert_eq!(group_id, None);
            assert_eq!(output_topic, None);
            assert!(!create_topics);
            assert_eq!(action, None);
            assert_eq!(name, None);
            assert_eq!(match_version, None);
            assert_eq!(brokers, None);
            assert!(!dry_run);
        } else {
            panic!("Expected Watch command");
        }
    }

    #[test]
    fn test_cli_parse_watch_with_extended_flags() {
        let cli = Cli::try_parse_from([
            "xzatoma",
            "watch",
            "--watcher-type",
            "generic",
            "--output-topic",
            "plans.output",
            "--action",
            "deploy.*",
            "--name",
            "service-a",
            "--dry-run",
        ]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();

        if let Commands::Watch {
            watcher_type,
            group_id,
            output_topic,
            create_topics,
            action,
            name,
            dry_run,
            ..
        } = cli.command
        {
            assert_eq!(watcher_type, Some("generic".to_string()));
            assert_eq!(group_id, None);
            assert_eq!(output_topic, Some("plans.output".to_string()));
            assert!(!create_topics);
            assert_eq!(action, Some("deploy.*".to_string()));
            assert_eq!(name, Some("service-a".to_string()));
            assert!(dry_run);
        } else {
            panic!("Expected Watch command");
        }
    }

    #[test]
    fn test_cli_parse_watch_with_brokers_flag() {
        let cli =
            Cli::try_parse_from(["xzatoma", "watch", "--brokers", "broker1:9092,broker2:9092"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();

        if let Commands::Watch { brokers, .. } = cli.command {
            assert_eq!(brokers, Some("broker1:9092,broker2:9092".to_string()));
        } else {
            panic!("Expected Watch command");
        }
    }

    #[test]
    fn test_cli_parse_watch_with_match_version_flag() {
        let cli = Cli::try_parse_from(["xzatoma", "watch", "--match-version", "^1\\.2\\..*"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();

        if let Commands::Watch { match_version, .. } = cli.command {
            assert_eq!(match_version, Some("^1\\.2\\..*".to_string()));
        } else {
            panic!("Expected Watch command");
        }
    }

    #[test]
    fn test_cli_parse_watch_with_group_id_and_create_topics() {
        let cli = Cli::try_parse_from([
            "xzatoma",
            "watch",
            "--group-id",
            "watchers-prod",
            "--create-topics",
        ]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();

        if let Commands::Watch {
            group_id,
            create_topics,
            ..
        } = cli.command
        {
            assert_eq!(group_id, Some("watchers-prod".to_string()));
            assert!(create_topics);
        } else {
            panic!("Expected Watch command");
        }
    }

    #[test]
    fn test_cli_parse_auth() {
        let cli = Cli::try_parse_from(["xzatoma", "auth", "--provider", "copilot"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Auth { provider, .. } = cli.command {
            assert_eq!(provider, Some("copilot".to_string()));
        } else {
            panic!("Expected Auth command");
        }
    }

    #[test]
    fn test_cli_parse_auth_without_provider() {
        let cli = Cli::try_parse_from(["xzatoma", "auth"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Auth { provider, .. } = cli.command {
            assert_eq!(provider, None);
        } else {
            panic!("Expected Auth command");
        }
    }

    #[test]
    fn test_cli_parse_chat_with_mode_planning() {
        let cli = Cli::try_parse_from(["xzatoma", "chat", "--mode", "planning"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Chat {
            provider,
            mode,
            safe,
            ..
        } = cli.command
        {
            assert_eq!(provider, None);
            assert_eq!(mode, Some("planning".to_string()));
            assert!(!safe);
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_cli_parse_chat_with_mode_write() {
        let cli = Cli::try_parse_from(["xzatoma", "chat", "--mode", "write"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Chat {
            provider: _, mode, ..
        } = cli.command
        {
            assert_eq!(mode, Some("write".to_string()));
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_cli_parse_chat_with_safe_flag() {
        let cli = Cli::try_parse_from(["xzatoma", "chat", "--safe"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Chat {
            provider: _,
            mode,
            safe,
            ..
        } = cli.command
        {
            assert!(safe);
            assert_eq!(mode, Some("planning".to_string())); // default mode
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_cli_parse_chat_safe_short_flag() {
        let cli = Cli::try_parse_from(["xzatoma", "chat", "-s"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Chat { safe, .. } = cli.command {
            assert!(safe);
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_cli_parse_chat_mode_default() {
        let cli = Cli::try_parse_from(["xzatoma", "chat"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Chat { mode, safe, .. } = cli.command {
            assert_eq!(mode, Some("planning".to_string())); // default is planning
            assert!(!safe); // default is no safety flag
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_cli_parse_chat_with_all_flags() {
        let cli = Cli::try_parse_from([
            "xzatoma",
            "chat",
            "--provider",
            "ollama",
            "--mode",
            "write",
            "--safe",
        ]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Chat {
            provider,
            mode,
            safe,
            ..
        } = cli.command
        {
            assert_eq!(provider, Some("ollama".to_string()));
            assert_eq!(mode, Some("write".to_string()));
            assert!(safe);
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_cli_parse_watch_with_config_after_subcommand() {
        let cli = Cli::try_parse_from(["xzatoma", "watch", "--config", "custom.yaml"]);
        assert!(cli.is_ok());

        let cli = cli.unwrap();
        assert_eq!(
            cli.command.common_args().config,
            Some("custom.yaml".to_string())
        );
    }

    #[test]
    fn test_cli_parse_with_verbose_after_subcommand() {
        let cli = Cli::try_parse_from(["xzatoma", "auth", "--provider", "copilot", "--verbose"]);
        assert!(cli.is_ok());

        let cli = cli.unwrap();
        assert!(cli.command.common_args().verbose);
    }

    #[test]
    fn test_cli_parse_missing_command() {
        let cli = Cli::try_parse_from(["xzatoma"]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_cli_parse_invalid_command() {
        let cli = Cli::try_parse_from(["xzatoma", "invalid"]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_cli_parse_models_list() {
        let cli = Cli::try_parse_from(["xzatoma", "models", "list"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Models { command, .. } = cli.command {
            assert!(matches!(command, ModelCommand::List { .. }));
        } else {
            panic!("Expected Models command");
        }
    }

    #[test]
    fn test_cli_parse_models_list_with_provider() {
        let cli = Cli::try_parse_from(["xzatoma", "models", "list", "--provider", "ollama"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Models { command, .. } = cli.command {
            if let ModelCommand::List { provider, .. } = command {
                assert_eq!(provider, Some("ollama".to_string()));
            } else {
                panic!("Expected List command");
            }
        } else {
            panic!("Expected Models command");
        }
    }

    #[test]
    fn test_cli_parse_models_list_with_json() {
        let cli = Cli::try_parse_from(["xzatoma", "models", "list", "--json"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Models { command, .. } = cli.command {
            if let ModelCommand::List {
                provider,
                json,
                summary,
            } = command
            {
                assert_eq!(provider, None);
                assert!(json);
                assert!(!summary);
            } else {
                panic!("Expected List command");
            }
        } else {
            panic!("Expected Models command");
        }
    }

    #[test]
    fn test_cli_parse_models_list_with_summary() {
        let cli = Cli::try_parse_from(["xzatoma", "models", "list", "--summary"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Models { command, .. } = cli.command {
            if let ModelCommand::List {
                provider,
                json,
                summary,
            } = command
            {
                assert_eq!(provider, None);
                assert!(!json);
                assert!(summary);
            } else {
                panic!("Expected List command");
            }
        } else {
            panic!("Expected Models command");
        }
    }

    #[test]
    fn test_cli_parse_models_list_with_json_and_summary() {
        let cli = Cli::try_parse_from(["xzatoma", "models", "list", "--json", "--summary"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Models { command, .. } = cli.command {
            if let ModelCommand::List {
                provider,
                json,
                summary,
            } = command
            {
                assert_eq!(provider, None);
                assert!(json);
                assert!(summary);
            } else {
                panic!("Expected List command");
            }
        } else {
            panic!("Expected Models command");
        }
    }

    #[test]
    fn test_cli_parse_models_list_short_flags() {
        let cli = Cli::try_parse_from(["xzatoma", "models", "list", "-j", "-s"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Models { command, .. } = cli.command {
            if let ModelCommand::List {
                provider,
                json,
                summary,
            } = command
            {
                assert_eq!(provider, None);
                assert!(json);
                assert!(summary);
            } else {
                panic!("Expected List command");
            }
        } else {
            panic!("Expected Models command");
        }
    }

    #[test]
    fn test_cli_parse_models_info() {
        let cli = Cli::try_parse_from(["xzatoma", "models", "info", "--model", "gpt-4"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Models { command, .. } = cli.command {
            if let ModelCommand::Info {
                model, provider, ..
            } = command
            {
                assert_eq!(model, "gpt-4");
                assert_eq!(provider, None);
            } else {
                panic!("Expected Info command");
            }
        } else {
            panic!("Expected Models command");
        }
    }

    #[test]
    fn test_cli_parse_models_info_with_provider() {
        let cli = Cli::try_parse_from([
            "xzatoma",
            "models",
            "info",
            "--model",
            "llama3.2:latest",
            "--provider",
            "ollama",
        ]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Models { command, .. } = cli.command {
            if let ModelCommand::Info {
                model, provider, ..
            } = command
            {
                assert_eq!(model, "llama3.2:latest");
                assert_eq!(provider, Some("ollama".to_string()));
            } else {
                panic!("Expected Info command");
            }
        } else {
            panic!("Expected Models command");
        }
    }

    #[test]
    fn test_cli_parse_models_info_with_json() {
        let cli = Cli::try_parse_from(["xzatoma", "models", "info", "--model", "gpt-4", "--json"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Models { command, .. } = cli.command {
            if let ModelCommand::Info {
                model,
                provider,
                json,
                summary,
            } = command
            {
                assert_eq!(model, "gpt-4");
                assert_eq!(provider, None);
                assert!(json);
                assert!(!summary);
            } else {
                panic!("Expected Info command");
            }
        } else {
            panic!("Expected Models command");
        }
    }

    #[test]
    fn test_cli_parse_models_info_with_summary() {
        let cli =
            Cli::try_parse_from(["xzatoma", "models", "info", "--model", "gpt-4", "--summary"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Models { command, .. } = cli.command {
            if let ModelCommand::Info {
                model,
                provider,
                json,
                summary,
            } = command
            {
                assert_eq!(model, "gpt-4");
                assert_eq!(provider, None);
                assert!(!json);
                assert!(summary);
            } else {
                panic!("Expected Info command");
            }
        } else {
            panic!("Expected Models command");
        }
    }

    #[test]
    fn test_cli_parse_models_info_with_json_and_summary() {
        let cli = Cli::try_parse_from([
            "xzatoma",
            "models",
            "info",
            "--model",
            "gpt-4",
            "--json",
            "--summary",
        ]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Models { command, .. } = cli.command {
            if let ModelCommand::Info {
                model,
                provider,
                json,
                summary,
            } = command
            {
                assert_eq!(model, "gpt-4");
                assert_eq!(provider, None);
                assert!(json);
                assert!(summary);
            } else {
                panic!("Expected Info command");
            }
        } else {
            panic!("Expected Models command");
        }
    }

    #[test]
    fn test_cli_parse_models_info_short_flags() {
        let cli =
            Cli::try_parse_from(["xzatoma", "models", "info", "--model", "gpt-4", "-j", "-s"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Models { command, .. } = cli.command {
            if let ModelCommand::Info {
                model,
                provider,
                json,
                summary,
            } = command
            {
                assert_eq!(model, "gpt-4");
                assert_eq!(provider, None);
                assert!(json);
                assert!(summary);
            } else {
                panic!("Expected Info command");
            }
        } else {
            panic!("Expected Models command");
        }
    }

    #[test]
    fn test_cli_parse_models_current() {
        let cli = Cli::try_parse_from(["xzatoma", "models", "current"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Models { command, .. } = cli.command {
            assert!(matches!(command, ModelCommand::Current { .. }));
        } else {
            panic!("Expected Models command");
        }
    }

    #[test]
    fn test_cli_parse_models_current_with_provider() {
        let cli = Cli::try_parse_from(["xzatoma", "models", "current", "--provider", "copilot"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Models { command, .. } = cli.command {
            if let ModelCommand::Current { provider } = command {
                assert_eq!(provider, Some("copilot".to_string()));
            } else {
                panic!("Expected Current command");
            }
        } else {
            panic!("Expected Models command");
        }
    }

    #[test]
    fn test_models_list_help_contains_json_and_summary_help() {
        use clap::CommandFactory;
        let mut cmd = Cli::command();
        let models_cmd = cmd
            .find_subcommand_mut("models")
            .expect("models subcommand missing");
        let list_cmd = models_cmd
            .find_subcommand_mut("list")
            .expect("list subcommand missing");

        let help = list_cmd.render_long_help().to_string();

        assert!(help.contains("--json"), "list help missing --json flag");
        assert!(
            help.contains("--summary"),
            "list help missing --summary flag"
        );

        assert!(
            help.to_lowercase().contains("json"),
            "list help missing json description"
        );
        assert!(
            help.to_lowercase().contains("summary"),
            "list help missing summary description"
        );
    }

    #[test]
    fn test_models_info_help_contains_json_and_summary_help() {
        use clap::CommandFactory;
        let mut cmd = Cli::command();
        let models_cmd = cmd
            .find_subcommand_mut("models")
            .expect("models subcommand missing");
        let info_cmd = models_cmd
            .find_subcommand_mut("info")
            .expect("info subcommand missing");

        let help = info_cmd.render_long_help().to_string();

        assert!(help.contains("--json"), "info help missing --json flag");
        assert!(
            help.contains("--summary"),
            "info help missing --summary flag"
        );

        assert!(
            help.to_lowercase().contains("json"),
            "info help missing json description"
        );
        assert!(
            help.to_lowercase().contains("summary"),
            "info help missing summary description"
        );
    }

    #[test]
    fn test_cli_parse_chat_with_thinking_effort_high() {
        let cli = Cli::try_parse_from(["xzatoma", "chat", "--thinking-effort", "high"]).unwrap();
        if let Commands::Chat {
            thinking_effort, ..
        } = cli.command
        {
            assert_eq!(thinking_effort, Some("high".to_string()));
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_cli_parse_chat_with_thinking_effort_none() {
        let cli = Cli::try_parse_from(["xzatoma", "chat", "--thinking-effort", "none"]).unwrap();
        if let Commands::Chat {
            thinking_effort, ..
        } = cli.command
        {
            assert_eq!(thinking_effort, Some("none".to_string()));
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_cli_parse_chat_thinking_effort_defaults_none() {
        let cli = Cli::try_parse_from(["xzatoma", "chat"]).unwrap();
        if let Commands::Chat {
            thinking_effort, ..
        } = cli.command
        {
            assert_eq!(thinking_effort, None);
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_cli_parse_run_with_thinking_effort_medium() {
        let cli = Cli::try_parse_from([
            "xzatoma",
            "run",
            "--prompt",
            "hello",
            "--thinking-effort",
            "medium",
        ])
        .unwrap();
        if let Commands::Run {
            thinking_effort, ..
        } = cli.command
        {
            assert_eq!(thinking_effort, Some("medium".to_string()));
        } else {
            panic!("Expected Run command");
        }
    }

    #[test]
    fn test_cli_parse_run_thinking_effort_defaults_none() {
        let cli = Cli::try_parse_from(["xzatoma", "run", "--prompt", "hello"]).unwrap();
        if let Commands::Run {
            thinking_effort, ..
        } = cli.command
        {
            assert_eq!(thinking_effort, None);
        } else {
            panic!("Expected Run command");
        }
    }

    // --- Common args and flag parsing tests ---

    #[test]
    #[serial_test::serial]
    fn test_common_args_config_default() {
        // Guard: ensure XZATOMA_CONFIG is absent for the duration of this test.
        let _guard = std::env::var("XZATOMA_CONFIG").ok();
        unsafe {
            std::env::remove_var("XZATOMA_CONFIG");
        }
        let cli = Cli::try_parse_from(["xzatoma", "chat"]).unwrap();
        let result = cli.command.common_args().config.clone();
        // Restore if it was set before.
        if let Some(prev) = _guard {
            unsafe {
                std::env::set_var("XZATOMA_CONFIG", prev);
            }
        }
        assert_eq!(result, None);
    }

    #[test]
    fn test_common_args_config_explicit_flag() {
        let cli = Cli::try_parse_from(["xzatoma", "chat", "--config", "/tmp/my.yaml"]).unwrap();
        assert_eq!(
            cli.command.common_args().config,
            Some("/tmp/my.yaml".to_string())
        );
    }

    #[test]
    #[serial_test::serial]
    fn test_common_args_config_env_var() {
        unsafe {
            std::env::set_var("XZATOMA_CONFIG", "/tmp/env.yaml");
        }
        let cli = Cli::try_parse_from(["xzatoma", "chat"]).unwrap();
        let cfg = cli.command.common_args().config.clone();
        unsafe {
            std::env::remove_var("XZATOMA_CONFIG");
        }
        assert_eq!(cfg, Some("/tmp/env.yaml".to_string()));
    }

    #[test]
    fn test_common_args_storage_path_env() {
        unsafe {
            std::env::set_var("XZATOMA_HISTORY_DB", "/tmp/x");
        }
        let cli = Cli::try_parse_from(["xzatoma", "chat"]).unwrap();
        let storage = cli.command.common_args().storage_path.clone();
        unsafe {
            std::env::remove_var("XZATOMA_HISTORY_DB");
        }
        assert_eq!(storage, Some("/tmp/x".to_string()));
    }

    #[test]
    fn test_flag_before_subcommand_is_rejected() {
        let result = Cli::try_parse_from(["xzatoma", "--config", "x.yaml", "chat"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_nested_subcommand_with_common_flags() {
        let cli = Cli::try_parse_from(["xzatoma", "models", "--config", "x.yaml", "list"]).unwrap();
        assert_eq!(cli.command.common_args().config, Some("x.yaml".to_string()));
    }

    // --- Debug and trace flag tests ---

    #[test]
    fn test_debug_flag_after_subcommand() {
        let cli = Cli::try_parse_from(["xzatoma", "chat", "--debug"]).unwrap();
        assert!(cli.command.common_args().debug);
    }

    #[test]
    fn test_trace_flag_after_subcommand() {
        let cli = Cli::try_parse_from(["xzatoma", "run", "--trace", "--prompt", "x"]).unwrap();
        assert!(cli.command.common_args().trace);
    }

    #[test]
    fn test_debug_and_trace_independent() {
        let cli = Cli::try_parse_from(["xzatoma", "chat", "--debug"]).unwrap();
        assert!(cli.command.common_args().debug);
        assert!(!cli.command.common_args().trace);
    }

    #[test]
    fn test_xzatoma_debug_env_sets_flag() {
        unsafe {
            std::env::set_var("XZATOMA_DEBUG", "true");
        }
        let cli = Cli::try_parse_from(["xzatoma", "chat"]).unwrap();
        let debug = cli.command.common_args().debug;
        unsafe {
            std::env::remove_var("XZATOMA_DEBUG");
        }
        assert!(debug);
    }

    // --- Log format flag tests ---

    #[test]
    fn test_log_format_json_after_chat_subcommand() {
        let cli = Cli::try_parse_from(["xzatoma", "chat", "--log-format", "json"]).unwrap();
        assert_eq!(cli.command.common_args().log_format, Some(LogFormat::Json));
    }

    #[test]
    fn test_log_format_plain_after_chat_subcommand() {
        let cli = Cli::try_parse_from(["xzatoma", "chat", "--log-format", "plain"]).unwrap();
        assert_eq!(cli.command.common_args().log_format, Some(LogFormat::Plain));
    }

    #[test]
    fn test_log_format_compact_after_run_subcommand() {
        let cli = Cli::try_parse_from([
            "xzatoma",
            "run",
            "--log-format",
            "compact",
            "--prompt",
            "hi",
        ])
        .unwrap();
        assert_eq!(
            cli.command.common_args().log_format,
            Some(LogFormat::Compact)
        );
    }

    #[test]
    fn test_logfile_after_run_subcommand() {
        let cli = Cli::try_parse_from([
            "xzatoma",
            "run",
            "--logfile",
            "/tmp/x.log",
            "--prompt",
            "hi",
        ])
        .unwrap();
        assert_eq!(
            cli.command.common_args().log_file,
            Some(PathBuf::from("/tmp/x.log"))
        );
    }

    #[test]
    fn test_log_format_defaults_to_none() {
        let cli = Cli::try_parse_from(["xzatoma", "chat"]).unwrap();
        assert_eq!(cli.command.common_args().log_format, None);
    }

    #[test]
    fn test_logfile_defaults_to_none() {
        let cli = Cli::try_parse_from(["xzatoma", "chat"]).unwrap();
        assert_eq!(cli.command.common_args().log_file, None);
    }

    #[test]
    fn test_log_format_invalid_value_is_rejected() {
        let result = Cli::try_parse_from(["xzatoma", "chat", "--log-format", "ndjson"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_cli_parse_chat_with_system_prompt_flag() {
        let cli = Cli::try_parse_from(["xzatoma", "chat", "--system-prompt", "you are a pirate"])
            .unwrap();
        match cli.command {
            Commands::Chat { system_prompt, .. } => {
                assert_eq!(system_prompt.as_deref(), Some("you are a pirate"));
            }
            _ => panic!("expected Chat command"),
        }
    }

    #[test]
    fn test_cli_parse_chat_system_prompt_defaults_none() {
        let cli = Cli::try_parse_from(["xzatoma", "chat"]).unwrap();
        match cli.command {
            Commands::Chat { system_prompt, .. } => {
                assert!(system_prompt.is_none());
            }
            _ => panic!("expected Chat command"),
        }
    }

    #[test]
    fn test_cli_parse_run_with_system_prompt_flag() {
        let cli = Cli::try_parse_from([
            "xzatoma",
            "run",
            "--prompt",
            "hello",
            "--system-prompt",
            "act as a pirate",
        ])
        .unwrap();
        match cli.command {
            Commands::Run { system_prompt, .. } => {
                assert_eq!(system_prompt.as_deref(), Some("act as a pirate"));
            }
            _ => panic!("expected Run command"),
        }
    }

    #[test]
    fn test_cli_parse_run_system_prompt_defaults_none() {
        let cli = Cli::try_parse_from(["xzatoma", "run", "--prompt", "hi"]).unwrap();
        match cli.command {
            Commands::Run { system_prompt, .. } => {
                assert!(system_prompt.is_none());
            }
            _ => panic!("expected Run command"),
        }
    }

    #[test]
    fn test_cli_parse_agent_with_system_prompt_flag() {
        let cli = Cli::try_parse_from([
            "xzatoma",
            "agent",
            "--system-prompt",
            "act as senior engineer",
        ])
        .unwrap();
        match cli.command {
            Commands::Agent { system_prompt, .. } => {
                assert_eq!(system_prompt.as_deref(), Some("act as senior engineer"));
            }
            _ => panic!("expected Agent command"),
        }
    }

    #[test]
    fn test_cli_parse_agent_system_prompt_defaults_none() {
        let cli = Cli::try_parse_from(["xzatoma", "agent"]).unwrap();
        match cli.command {
            Commands::Agent { system_prompt, .. } => {
                assert!(system_prompt.is_none());
            }
            _ => panic!("expected Agent command"),
        }
    }

    #[test]
    fn test_cli_parse_watch_with_system_prompt_flag() {
        let cli = Cli::try_parse_from(["xzatoma", "watch", "--system-prompt", "you are a watcher"])
            .unwrap();
        match cli.command {
            Commands::Watch { system_prompt, .. } => {
                assert_eq!(system_prompt.as_deref(), Some("you are a watcher"));
            }
            _ => panic!("expected Watch command"),
        }
    }

    #[test]
    fn test_cli_parse_watch_system_prompt_defaults_none() {
        let cli = Cli::try_parse_from(["xzatoma", "watch"]).unwrap();
        match cli.command {
            Commands::Watch { system_prompt, .. } => {
                assert!(system_prompt.is_none());
            }
            _ => panic!("expected Watch command"),
        }
    }

    #[test]
    fn test_cli_parse_acp_serve_with_system_prompt_flag() {
        let cli = Cli::try_parse_from([
            "xzatoma",
            "acp",
            "serve",
            "--system-prompt",
            "act as acp agent",
        ])
        .unwrap();
        match cli.command {
            Commands::Acp { command, .. } => match command {
                AcpCommand::Serve { system_prompt, .. } => {
                    assert_eq!(system_prompt.as_deref(), Some("act as acp agent"));
                }
                _ => panic!("expected AcpCommand::Serve"),
            },
            _ => panic!("expected Acp command"),
        }
    }

    #[test]
    fn test_cli_parse_acp_serve_system_prompt_defaults_none() {
        let cli = Cli::try_parse_from(["xzatoma", "acp", "serve"]).unwrap();
        match cli.command {
            Commands::Acp { command, .. } => match command {
                AcpCommand::Serve { system_prompt, .. } => {
                    assert!(system_prompt.is_none());
                }
                _ => panic!("expected AcpCommand::Serve"),
            },
            _ => panic!("expected Acp command"),
        }
    }

    #[test]
    fn test_cli_parse_chat_with_streaming_flag() {
        let cli = Cli::try_parse_from(["xzatoma", "chat", "--streaming"]).unwrap();
        if let Commands::Chat { streaming, .. } = cli.command {
            assert!(streaming);
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_cli_parse_chat_streaming_defaults_false() {
        let cli = Cli::try_parse_from(["xzatoma", "chat"]).unwrap();
        if let Commands::Chat { streaming, .. } = cli.command {
            assert!(!streaming);
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_cli_parse_run_with_streaming_flag() {
        let cli =
            Cli::try_parse_from(["xzatoma", "run", "--prompt", "hello", "--streaming"]).unwrap();
        if let Commands::Run { streaming, .. } = cli.command {
            assert!(streaming);
        } else {
            panic!("Expected Run command");
        }
    }

    #[test]
    fn test_cli_parse_agent_with_streaming_flag() {
        let cli = Cli::try_parse_from(["xzatoma", "agent", "--streaming"]).unwrap();
        if let Commands::Agent { streaming, .. } = cli.command {
            assert!(streaming);
        } else {
            panic!("Expected Agent command");
        }
    }
}
