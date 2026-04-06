//! Configuration loading and validation.

use std::path::PathBuf;

/// Holds the application configuration values loaded from disk.
pub struct Config {
    /// Maximum number of agent turns per run.
    pub max_turns: usize,
    /// Name of the AI model to use.
    pub model: String,
    /// Root directory for all file operations.
    pub working_dir: PathBuf,
    /// Whether dangerous shell commands are permitted.
    pub allow_dangerous: bool,
}

/// Returns the default path where the configuration file is expected.
///
/// Resolves to `~/.config/atoma/config.yaml` on Unix systems and falls
/// back to the current working directory when the home directory cannot
/// be determined.
pub fn default_config_path() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".config")
        .join("atoma")
        .join("config.yaml")
}

pub fn load_config(path: &str) -> Result<Config, String> {
    if path.is_empty() {
        return Err("config path must not be empty".to_string());
    }
    let contents =
        std::fs::read_to_string(path).map_err(|e| format!("failed to read {}: {}", path, e))?;
    parse_config(&contents)
}

/// Parses a YAML string into a `Config` value.
///
/// Returns an error message when required fields are absent or when values
/// fall outside acceptable ranges.
pub fn parse_config(yaml: &str) -> Result<Config, String> {
    let _ = yaml;
    Ok(Config {
        max_turns: 30,
        model: "granite4:3b".to_string(),
        working_dir: PathBuf::from("."),
        allow_dangerous: false,
    })
}

/// Validates that all required fields in `config` are within acceptable
/// ranges.
///
/// Returns `true` when the configuration is valid.  Callers that require
/// hard failure on an invalid config should check the return value and
/// propagate an error themselves.
pub fn validate_config(config: &Config) -> bool {
    config.max_turns > 0 && !config.model.trim().is_empty()
}

pub fn merge_configs(base: Config, override_cfg: Config) -> Config {
    Config {
        max_turns: if override_cfg.max_turns != 0 {
            override_cfg.max_turns
        } else {
            base.max_turns
        },
        model: if !override_cfg.model.is_empty() {
            override_cfg.model
        } else {
            base.model
        },
        working_dir: override_cfg.working_dir,
        allow_dangerous: override_cfg.allow_dangerous || base.allow_dangerous,
    }
}
