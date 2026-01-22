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

    /// Authenticate with a provider
    Auth {
        /// Provider to authenticate with (copilot, ollama)
        ///
        /// Use `--provider <name>` to override; if omitted the configured/default
        /// provider will be used.
        #[arg(short, long)]
        provider: Option<String>,
    },

    /// Manage AI models
    Models {
        /// Model management subcommand
        #[command(subcommand)]
        command: ModelCommand,
    },
}

/// Model management subcommands
#[derive(Subcommand, Debug, Clone)]
pub enum ModelCommand {
    /// List available models
    List {
        /// Filter by provider (copilot, ollama)
        #[arg(short, long)]
        provider: Option<String>,
    },

    /// Show detailed information about a model
    Info {
        /// Model name/identifier
        #[arg(short, long)]
        model: String,

        /// Filter by provider (copilot, ollama)
        #[arg(short, long)]
        provider: Option<String>,
    },

    /// Show the currently active model
    Current {
        /// Filter by provider (copilot, ollama)
        #[arg(short, long)]
        provider: Option<String>,
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
        } = cli.command
        {
            assert_eq!(provider, Some("ollama".to_string()));
        } else {
            panic!("Expected Chat command");
        }
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
            if let ModelCommand::List { provider } = command {
                assert_eq!(provider, Some("ollama".to_string()));
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
            if let ModelCommand::Info { model, provider } = command {
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
            if let ModelCommand::Info { model, provider } = command {
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
}
