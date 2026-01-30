//! Command-line interface definition for XZatoma
//!
//! This module defines the CLI structure using clap's derive API,
//! providing commands for chat, plan execution, and authentication.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// XZatoma - Autonomous AI agent CLI
///
/// Execute tasks through conversation with AI providers using
/// basic file system and terminal tools.
#[derive(Parser, Debug, Clone)]
#[command(name = "xzatoma")]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Path to configuration file
    #[arg(short, long, default_value = "config/config.yaml")]
    pub config: Option<String>,

    /// Enable verbose logging
    #[arg(short, long)]
    pub verbose: bool,

    /// Override the path to the history database (or set env XZATOMA_HISTORY_DB)
    #[arg(long, env = "XZATOMA_HISTORY_DB")]
    pub storage_path: Option<String>,

    /// Command to execute
    #[command(subcommand)]
    pub command: Commands,
}

/// Available commands for XZatoma
#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Start interactive chat mode with the agent
    Chat {
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
    },

    /// Execute a plan or prompt
    Run {
        /// Path to plan file (YAML format)
        #[arg(short, long)]
        plan: Option<PathBuf>,

        /// Direct prompt to execute (alternative to plan file)
        #[arg(long)]
        prompt: Option<String>,

        /// Allow dangerous commands without confirmation
        #[arg(long)]
        allow_dangerous: bool,
    },

    /// Watch Kafka topic for events and execute plans
    Watch {
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

        /// Dry run mode (parse but don't execute plans)
        #[arg(long)]
        dry_run: bool,
    },

    /// Authenticate with a provider
    Auth {
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
        /// Model management subcommand
        #[command(subcommand)]
        command: ModelCommand,
    },

    /// Manage conversation history
    History {
        /// History management subcommand
        #[command(subcommand)]
        command: HistoryCommand,
    },
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

/// History management subcommands
#[derive(Subcommand, Debug, Clone)]
pub enum HistoryCommand {
    /// List saved conversations
    List,

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
            config: Some("config/config.yaml".to_string()),
            verbose: false,
            storage_path: None,
            command: Commands::Auth {
                provider: Some("copilot".to_string()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_default() {
        let cli = Cli::default();
        assert_eq!(cli.config, Some("config/config.yaml".to_string()));
        assert!(!cli.verbose);

        // default command should be `auth` with provider defaulting to "copilot"
        if let Commands::Auth { provider } = cli.command {
            assert_eq!(provider, Some("copilot".to_string()));
        } else {
            panic!("Expected default command to be Auth");
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
        if let Commands::Chat {
            provider,
            mode: _,
            safe: _,
            resume: _,
        } = cli.command
        {
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
        if let Commands::History { command } = cli.command {
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
        if let Commands::History { command } = cli.command {
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
    fn test_cli_parse_storage_path() {
        // Include a subcommand (auth) so clap parsing succeeds (avoids MissingSubcommand).
        let cli = Cli::try_parse_from(["xzatoma", "--storage-path", "/tmp/my.db", "auth"])
            .expect("failed to parse CLI args (storage-path)");
        assert_eq!(cli.storage_path, Some("/tmp/my.db".to_string()));
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
    fn test_cli_parse_auth() {
        let cli = Cli::try_parse_from(["xzatoma", "auth", "--provider", "copilot"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Auth { provider } = cli.command {
            assert_eq!(provider, Some("copilot".to_string()));
        } else {
            panic!("Expected Auth command");
        }
    }

    #[test]
    fn test_cli_parse_auth_without_provider() {
        // `auth` subcommand without `--provider` should parse (provider left as None)
        let cli = Cli::try_parse_from(["xzatoma", "auth"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Auth { provider } = cli.command {
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
            resume: _,
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
            provider: _,
            mode,
            safe: _,
            resume: _,
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
            resume: _,
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
            resume: _,
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
    fn test_cli_parse_with_config() {
        let cli = Cli::try_parse_from([
            "xzatoma",
            "--config",
            "custom.yaml",
            "auth",
            "--provider",
            "copilot",
        ]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert_eq!(cli.config, Some("custom.yaml".to_string()));
    }

    #[test]
    fn test_cli_parse_with_verbose() {
        let cli = Cli::try_parse_from(["xzatoma", "-v", "auth", "--provider", "copilot"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert!(cli.verbose);
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
        if let Commands::Models { command } = cli.command {
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
        if let Commands::Models { command } = cli.command {
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
        if let Commands::Models { command } = cli.command {
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
        if let Commands::Models { command } = cli.command {
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
        if let Commands::Models { command } = cli.command {
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
        if let Commands::Models { command } = cli.command {
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
        if let Commands::Models { command } = cli.command {
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
        if let Commands::Models { command } = cli.command {
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
        if let Commands::Models { command } = cli.command {
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
        if let Commands::Models { command } = cli.command {
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
        if let Commands::Models { command } = cli.command {
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
        if let Commands::Models { command } = cli.command {
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
        if let Commands::Models { command } = cli.command {
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
        if let Commands::Models { command } = cli.command {
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

        // Verify flags exist
        assert!(help.contains("--json"), "list help missing --json flag");
        assert!(
            help.contains("--summary"),
            "list help missing --summary flag"
        );

        // Verify descriptive text mentions JSON and summary
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

        // Verify flags exist
        assert!(help.contains("--json"), "info help missing --json flag");
        assert!(
            help.contains("--summary"),
            "info help missing --summary flag"
        );

        // Verify descriptive text mentions JSON and summary
        assert!(
            help.to_lowercase().contains("json"),
            "info help missing json description"
        );
        assert!(
            help.to_lowercase().contains("summary"),
            "info help missing summary description"
        );
    }
}
