//! XZatoma - Autonomous AI agent CLI library
//!
//! This library provides the core functionality for the XZatoma autonomous agent,
//! including agent execution, provider abstractions, tool management, and configuration.
//!
//! # Architecture
//!
//! The library is organized into the following modules:
//!
//! - `agent`: Core agent logic, conversation management, and execution loop
//! - `providers`: AI provider abstraction and implementations (Copilot, Ollama)
//! - `tools`: File operations, terminal execution, and tool registry
//! - `config`: Configuration management and validation
//! - `error`: Error types and result aliases
//! - `cli`: Command-line interface definition
//!
//! # Example
//!
//! ```no_run
//! use xzatoma::{Config, Agent};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = Config::load("config.yaml", &Default::default())?;
//!     config.validate()?;
//!
//!     // Agent usage would go here
//!     Ok(())
//! }
//! ```

pub mod agent;
pub mod chat_mode;
pub mod cli;
pub mod commands;
pub mod config;
pub mod error;
pub mod mention_parser;
pub mod prompts;
pub mod providers;
pub mod tools;

// Re-export commonly used types
pub use agent::Agent;
pub use chat_mode::{ChatMode, SafetyMode};
pub use config::Config;
pub use error::{Result, XzatomaError};
pub use mention_parser::{parse_mentions, FileMention, Mention, SearchMention, UrlMention};

#[cfg(test)]
pub mod test_utils;
