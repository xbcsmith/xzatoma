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
        provider: String,
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
                provider: "copilot".to_string(),
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
        if let Commands::Chat { provider } = cli.command {
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
        let cli = Cli::try_parse_from(["xzatoma", "auth", "copilot"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        if let Commands::Auth { provider } = cli.command {
            assert_eq!(provider, "copilot");
        } else {
            panic!("Expected Auth command");
        }
    }

    #[test]
    fn test_cli_parse_with_config() {
        let cli = Cli::try_parse_from(["xzatoma", "--config", "custom.yaml", "auth", "copilot"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert_eq!(cli.config, Some("custom.yaml".to_string()));
    }

    #[test]
    fn test_cli_parse_with_verbose() {
        let cli = Cli::try_parse_from(["xzatoma", "-v", "auth", "copilot"]);
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
}
