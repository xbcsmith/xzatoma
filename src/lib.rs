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
//! - `providers`: AI provider abstraction and implementations (Copilot, Ollama, OpenAI)
//! - `tools`: File operations, terminal execution, and tool registry
//! - `config`: Configuration management and validation
//! - `error`: Error types and result aliases
//! - `cli`: Command-line interface definition
//!
//! # Example
//!
//! ```
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

// Enforce justified fallibility in production code. Any bare `unwrap`/`expect`
// on a production path must be removed or annotated with an explicit
// `#[allow(...)]` plus a `// SAFETY:` justification. Test and doc-test code is
// exempt via `clippy.toml` (`allow-unwrap-in-tests`/`allow-expect-in-tests`).
#![warn(clippy::unwrap_used, clippy::expect_used)]

pub mod acp;
pub mod agent;
pub mod chat_mode;
pub mod cli;
pub mod commands;
pub mod config;
pub mod error;
pub mod mcp;
pub mod mention_parser;
pub mod prompts;
pub mod providers;
pub mod security;
pub mod skills;
pub mod storage;
pub mod tools;
pub mod watcher;

// Re-export commonly used types
pub use agent::Agent;
pub use chat_mode::{ChatMode, ChatModeParseError, SafetyMode, SafetyModeParseError};
pub use config::Config;
pub use error::{Result, XzatomaError};
pub use mention_parser::{
    FileMention, LoadError, LoadErrorKind, Mention, MentionCache, MentionContent, SearchMention,
    UrlMention, augment_prompt_with_mentions, load_file_content, parse_mentions,
};
pub use tools::{GrepTool, SearchMatch};

#[cfg(test)]
pub mod test_utils;
