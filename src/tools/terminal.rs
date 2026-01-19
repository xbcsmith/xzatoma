//! Terminal execution tool for XZatoma
//!
//! This module implements:
//! - `CommandValidator` — allowlist/denylist & path validation for terminal commands
//! - `TerminalTool` — a `ToolExecutor` that runs validated shell commands with timeout,
//!   output truncation, and metadata
//! - `execute_command` convenience wrapper for quick usage
//!
//! Design notes:
//! - Denylist items are blocked in all modes
//! - In `RestrictedAutonomous`, only allowlist commands are permitted
//! - `Interactive` always returns `CommandRequiresConfirmation` to require user approval
//! - `FullAutonomous` allows all non-dangerous commands
//! - Path validation is symlink-aware (canonicalizes existing paths) to prevent escapes
//! - SafetyMode affects confirmation requirements:
//!   - `AlwaysConfirm`: Requires explicit confirmation for terminal operations
//!   - `NeverConfirm`: Allows operations without confirmation (YOLO mode)
//!
//! # Examples
//!
//! ```no_run
//! use xzatoma::tools::terminal::{execute_command, TerminalTool, CommandValidator};
//! use xzatoma::config::{ExecutionMode, TerminalConfig};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let result = execute_command("echo hello", std::path::PathBuf::from("."), ExecutionMode::RestrictedAutonomous).await?;
//!     println!("{}", result);
//!     Ok(())
//! }
//! ```

use std::path::{Component, PathBuf};
use std::process::{Command as StdCommand, Stdio};
use std::time::Duration;

use anyhow::Context;
use async_trait::async_trait;
use regex::Regex;
use serde_json::{json, Value};
use tokio::process::Command;
use tokio::time;

use crate::chat_mode::SafetyMode;
use crate::config::{ExecutionMode, TerminalConfig};
use crate::error::{Result, XzatomaError};
use crate::tools::{ToolExecutor, ToolResult};

/// Convenience wrapper to execute a single command using TerminalTool with a default config
pub async fn execute_command(
    command: &str,
    working_dir: PathBuf,
    mode: ExecutionMode,
) -> Result<String> {
    let validator = CommandValidator::new(mode, working_dir.clone());
    let config = TerminalConfig {
        default_mode: mode,
        ..Default::default()
    };

    let tool = TerminalTool::new(validator, config);

    let params = json!({
        "command": command,
        "timeout_seconds": tool.config.timeout_seconds,
        "confirm": true,
    });

    let tool_result = tool.execute(params).await?;
    if tool_result.success {
        Ok(tool_result.output)
    } else {
        Err(anyhow::Error::from(XzatomaError::Tool(
            tool_result
                .error
                .unwrap_or_else(|| "command failed".to_string()),
        )))
    }
}

/// Command validator for terminal safety checks
#[derive(Debug, Clone)]
pub struct CommandValidator {
    /// Execution mode to govern validation policy
    pub mode: ExecutionMode,
    /// Working directory to constrain path validations
    pub working_dir: PathBuf,
    /// Allowlist of safe commands for restricted mode
    pub allowlist: Vec<String>,
    /// Denylist of dangerous patterns (regex)
    pub denylist: Vec<Regex>,
}

impl CommandValidator {
    /// Construct a new `CommandValidator` instance with allowlist and denylist defaults
    pub fn new(mode: ExecutionMode, working_dir: PathBuf) -> Self {
        let allowlist = vec![
            "ls", "cat", "grep", "find", "echo", "pwd", "whoami", "head", "tail", "wc", "sort",
            "uniq", "diff", "git", "cargo", "rustc", "npm", "node", "python", "python3", "go",
            "make", "cmake", "which", "basename", "dirname", "realpath",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let denylist_patterns = vec![
            r"rm\s+-rf\s+/\s*$",
            r"rm\s+-rf\s+/\*",
            r"rm\s+-rf\s+~",
            r"rm\s+-rf\s+\$HOME",
            r"dd\s+if=/dev/zero",
            r"dd\s+if=/dev/random",
            r"dd\s+of=/dev/sd[a-z]",
            r"mkfs\.",
            r":\(\)\{:\|:&\};:",
            r"while\s+true.*do.*done",
            r"for\s*\(\(;;",
            r"curl\s+.*\|\s*sh",
            r"wget\s+.*\|\s*sh",
            r"curl\s+.*\|\s*bash",
            r"wget\s+.*\|\s*bash",
            r"\bsudo\s+",
            r"\bsu\s+",
            r"\bchmod\s+[0-7]*7[0-7]*",
            r"\beval\s*\(",
            r"\bexec\s*\(",
            r"import\s+os.*system",
            r">\s*/dev/sd[a-z]",
            r">\s*/dev/hd[a-z]",
            r"/etc/passwd",
            r"/etc/shadow",
            r"~/.ssh/",
            r"\$HOME/\.ssh/",
        ];

        let denylist = denylist_patterns
            .into_iter()
            .map(|p| Regex::new(p).expect("invalid denylist regex"))
            .collect();

        Self {
            mode,
            working_dir,
            allowlist,
            denylist,
        }
    }

    /// Validate the passed command string for the configured execution mode
    ///
    /// Returns:
    /// - Ok(()) if valid
    /// - Err(XzatomaError::CommandRequiresConfirmation(_)) if confirmation is required (interactive or restricted)
    /// - Err(XzatomaError::DangerousCommand(_)) if command matches denylist
    /// - Err(XzatomaError::PathOutsideWorkingDirectory(_)) if path escapes the working directory
    pub fn validate(&self, command: &str) -> std::result::Result<(), XzatomaError> {
        // Denylist first - block always
        for r in &self.denylist {
            if r.is_match(command) {
                return Err(XzatomaError::DangerousCommand(format!(
                    "Command matches dangerous pattern: {}",
                    command
                )));
            }
        }

        // Empty command: treat as a tool error
        if command.trim().is_empty() {
            return Err(XzatomaError::Tool("Empty command".to_string()));
        }

        match self.mode {
            ExecutionMode::Interactive => Err(XzatomaError::CommandRequiresConfirmation(
                command.to_string(),
            )),
            ExecutionMode::RestrictedAutonomous => {
                // Get first token i.e. command name
                let name = command
                    .split_whitespace()
                    .next()
                    .ok_or_else(|| XzatomaError::Tool("Empty command".to_string()))?;
                if !self.allowlist.contains(&name.to_string()) {
                    return Err(XzatomaError::CommandRequiresConfirmation(format!(
                        "Command '{}' not in allowlist",
                        name
                    )));
                }

                self.validate_paths(command)?;
                Ok(())
            }
            ExecutionMode::FullAutonomous => {
                // Full autonomous still must validate paths
                self.validate_paths(command)?;
                Ok(())
            }
        }
    }

    /// Lexically normalize an absolute path (resolve '.' and '..') without following symlinks
    fn lexical_absolute_normalize(mut path: PathBuf) -> PathBuf {
        use std::path::Component;

        if !path.is_absolute() {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
            path = cwd.join(path);
        }
        let mut normalized = PathBuf::new();
        for comp in path.components() {
            match comp {
                Component::Prefix(p) => normalized.push(p.as_os_str()),
                Component::RootDir => normalized.push("/"),
                Component::CurDir => {}
                Component::ParentDir => {
                    normalized.pop();
                }
                Component::Normal(p) => normalized.push(p),
            }
        }
        normalized
    }

    /// Resolve a canonical working directory for robust comparisons
    fn resolve_canonical_working(&self) -> PathBuf {
        if self.working_dir.is_absolute() {
            if let Ok(canon) = self.working_dir.canonicalize() {
                return canon;
            }
            return Self::lexical_absolute_normalize(self.working_dir.clone());
        }
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let joined = cwd.join(&self.working_dir);
        if let Ok(canon) = joined.canonicalize() {
            return canon;
        }
        Self::lexical_absolute_normalize(joined)
    }

    /// Validate all path-like tokens in the command do not escape the working directory.
    ///
    /// Performs a conservative lexical check for non-existent files and canonicalized
    /// verification for existing ones (following symlinks).
    fn validate_paths(&self, command: &str) -> std::result::Result<(), XzatomaError> {
        let tokens = command.split_whitespace().collect::<Vec<&str>>();
        let canonical_working = self.resolve_canonical_working();

        for token in tokens {
            let t = token.trim().trim_matches(|c| c == '"' || c == '\'');
            if t.is_empty() {
                continue;
            }
            // Skip common shell tokens
            if matches!(t, "|" | "||" | "&&" | ">" | ">>" | "<" | ";" | "&") {
                continue;
            }

            let candidate = if t.starts_with("--") && t.contains('=') {
                t.split_once('=').map(|(_, v)| v).unwrap_or(t)
            } else {
                t
            };

            if candidate.starts_with('-') || candidate.is_empty() {
                continue;
            }

            let candidate_path = self.working_dir.join(candidate);

            let looks_like_path = candidate.contains('/')
                || candidate == "."
                || candidate == ".."
                || candidate.starts_with("./")
                || candidate.starts_with("../")
                || candidate_path.exists();

            if !looks_like_path {
                continue;
            }

            // Reject absolute paths
            if candidate.starts_with('/') {
                return Err(XzatomaError::PathOutsideWorkingDirectory(format!(
                    "Absolute path not permitted: {}",
                    candidate
                )));
            }
            // Reject home usage
            if candidate.starts_with('~') {
                return Err(XzatomaError::PathOutsideWorkingDirectory(format!(
                    "Home directory path not allowed: {}",
                    candidate
                )));
            }
            // Reject lexical traversal
            if candidate.contains("..") {
                return Err(XzatomaError::PathOutsideWorkingDirectory(format!(
                    "Directory traversal not allowed: {}",
                    candidate
                )));
            }

            // If the candidate exists use canonicalization to prevent symlink escape
            if let Ok(canonical_target) = candidate_path.canonicalize() {
                if !canonical_target.starts_with(&canonical_working) {
                    return Err(XzatomaError::PathOutsideWorkingDirectory(format!(
                        "Path escapes working directory: {} -> {:?}",
                        candidate, canonical_target
                    )));
                }
            }
        }

        Ok(())
    }
}

/// Terminal tool implementing `ToolExecutor`
///
/// Accepts:
/// - `command` (string): required
/// - `confirm` (boolean): confirm for restricted commands
/// - `timeout_seconds` (integer): override default timeout
/// - `max_stdout_bytes` (integer): override stdout truncation
/// - `max_stderr_bytes` (integer): override stderr truncation
pub struct TerminalTool {
    pub validator: CommandValidator,
    pub config: TerminalConfig,
    pub safety_mode: SafetyMode,
}

impl TerminalTool {
    /// Create a new TerminalTool with default SafetyMode::AlwaysConfirm
    pub fn new(validator: CommandValidator, config: TerminalConfig) -> Self {
        Self {
            validator,
            config,
            safety_mode: SafetyMode::AlwaysConfirm,
        }
    }

    /// Set the safety mode for this tool
    ///
    /// # Arguments
    ///
    /// * `mode` - The safety mode to use
    ///
    /// # Returns
    ///
    /// Returns self for method chaining
    pub fn with_safety_mode(mut self, mode: SafetyMode) -> Self {
        self.safety_mode = mode;
        self
    }

    /// Set the safety mode for this tool (mutating)
    ///
    /// # Arguments
    ///
    /// * `mode` - The safety mode to use
    pub fn set_safety_mode(&mut self, mode: SafetyMode) {
        self.safety_mode = mode;
    }
}

#[async_trait]
impl ToolExecutor for TerminalTool {
    fn tool_definition(&self) -> Value {
        json!({
            "name": "terminal",
            "description": "Execute validated shell commands in the working directory",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to execute" },
                    "confirm": { "type": "boolean" },
                    "timeout_seconds": { "type": "integer" },
                    "max_stdout_bytes": { "type": "integer" },
                    "max_stderr_bytes": { "type": "integer" }
                },
                "required": ["command"]
            }
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let command = params["command"]
            .as_str()
            .ok_or_else(|| {
                anyhow::anyhow!(XzatomaError::Tool(
                    "Missing 'command' parameter".to_string()
                ))
            })?
            .to_string();

        let confirm = params["confirm"].as_bool().unwrap_or(false);
        let timeout_seconds = params["timeout_seconds"]
            .as_u64()
            .unwrap_or(self.config.timeout_seconds);
        let max_stdout_bytes = params["max_stdout_bytes"]
            .as_u64()
            .unwrap_or(self.config.max_stdout_bytes as u64) as usize;
        let max_stderr_bytes = params["max_stderr_bytes"]
            .as_u64()
            .unwrap_or(self.config.max_stderr_bytes as u64) as usize;

        // Validate permission and paths
        match self.validator.validate(&command) {
            Ok(()) => {}
            Err(XzatomaError::CommandRequiresConfirmation(_)) => {
                // Check SafetyMode
                match self.safety_mode {
                    SafetyMode::AlwaysConfirm => {
                        // Require explicit confirmation
                        if !confirm {
                            return Ok(ToolResult::error(format!(
                                "Command requires confirmation in SAFE mode: {}",
                                command
                            )));
                        }
                    }
                    SafetyMode::NeverConfirm => {
                        // Proceed without confirmation (YOLO mode)
                    }
                }
            }
            Err(err) => {
                // For dangerous/path errors, return a ToolResult error
                return Ok(ToolResult::error(err.to_string()));
            }
        }

        // Build the shell invocation
        let mut cmd = if cfg!(unix) {
            let mut c = Command::new("sh");
            c.arg("-c").arg(&command);
            c
        } else {
            let mut c = Command::new("cmd");
            c.arg("/C").arg(&command);
            c
        };

        cmd.current_dir(self.validator.working_dir.clone());
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        // Spawn and obtain a child
        let child = cmd.spawn().map_err(|e| {
            anyhow::anyhow!(XzatomaError::Tool(format!(
                "Failed to spawn command: {}",
                e
            )))
        })?;

        let start = std::time::Instant::now();

        // Preserve the pid for a best-effort kill when we need to kill from outside.
        let pid = child.id();

        // Spawn wait_with_output in a background task so we can poll with select and kill by PID if needed.
        let wait_handle = tokio::spawn(async move { child.wait_with_output().await });

        // Pin join handle so we can await via select without moving it
        let mut join_fut = Box::pin(wait_handle);
        let sleep = time::sleep(Duration::from_secs(timeout_seconds));
        tokio::pin!(sleep);

        // Await either join_fut or timeout; when timeout triggers we attempt a best-effort kill using OS commands.
        let output = tokio::select! {
            join_res = &mut join_fut => {
                match join_res {
                    Ok(Ok(out)) => out,
                    Ok(Err(e)) => return Err(anyhow::anyhow!(XzatomaError::Tool(format!("Failed waiting for command output: {}", e)))),
                    Err(e) => return Err(anyhow::anyhow!(XzatomaError::Tool(format!("Join error waiting for command: {}", e)))),
                }
            }
            _ = &mut sleep => {
                // Timeout -> attempt kill by pid (OS command; best-effort).
                if let Some(pid) = pid {
                    #[cfg(unix)]
                    {
                        let _ = StdCommand::new("kill").arg("-9").arg(pid.to_string()).status();
                    }
                    #[cfg(windows)]
                    {
                        let _ = StdCommand::new("taskkill").args(&["/PID", &pid.to_string(), "/F"]).status();
                    }
                }
                // Await join handle after kill to collect any output
                match join_fut.await {
                    Ok(Ok(out)) => out,
                    Ok(Err(e)) => return Err(anyhow::anyhow!(XzatomaError::Tool(format!("Failed waiting for command output after kill: {}", e)))),
                    Err(e) => return Err(anyhow::anyhow!(XzatomaError::Tool(format!("Join error after kill: {}", e)))),
                }
            }
        };

        let elapsed_ms = start.elapsed().as_millis();
        let mut stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let mut stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if stdout.len() > max_stdout_bytes {
            stdout.truncate(max_stdout_bytes);
            stdout.push_str("\n... (stdout truncated)");
        }

        if stderr.len() > max_stderr_bytes {
            stderr.truncate(max_stderr_bytes);
            stderr.push_str("\n... (stderr truncated)");
        }

        let combined = if stderr.is_empty() {
            stdout.clone()
        } else {
            format!("STDOUT:\n{}\n\nSTDERR:\n{}", stdout, stderr)
        };

        let status = output.status;
        let mut res = if status.success() {
            ToolResult::success(combined)
        } else {
            let code_str = status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "-".to_string());
            ToolResult::error(format!("Exit code {}: {}", code_str, combined))
        };

        res = res
            .with_metadata(
                "exit_code".to_string(),
                status
                    .code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            )
            .with_metadata("duration_ms".to_string(), elapsed_ms.to_string());

        Ok(res)
    }
}

/// Validate a command for safety (convenience)
pub fn validate_command(command: &str, mode: ExecutionMode, working_dir: PathBuf) -> Result<()> {
    let validator = CommandValidator::new(mode, working_dir);
    validator.validate(command).map_err(Into::into)
}

/// Return true when the command is considered dangerous or not allowed
pub fn is_dangerous_command(command: &str, mode: ExecutionMode, working_dir: PathBuf) -> bool {
    validate_command(command, mode, working_dir).is_err()
}

/// Simple parser that splits a command line on whitespace
pub fn parse_command(command: &str) -> Vec<String> {
    command.split_whitespace().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TerminalConfig;
    use std::fs as stdfs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_execute_command_echo() {
        let dir = tempdir().unwrap();
        let validator = CommandValidator::new(
            ExecutionMode::RestrictedAutonomous,
            dir.path().to_path_buf(),
        );
        let config = TerminalConfig::default();
        let tool = TerminalTool::new(validator, config);

        let params = json!({ "command": "echo hello" });
        let res = tool.execute(params).await.unwrap();
        assert!(res.success);
        assert!(res.output.contains("hello"));
    }

    #[tokio::test]
    async fn test_terminal_tool_block_dangerous() {
        let dir = tempdir().unwrap();
        let validator =
            CommandValidator::new(ExecutionMode::FullAutonomous, dir.path().to_path_buf());
        let config = TerminalConfig::default();
        let tool = TerminalTool::new(validator, config);

        let params = json!({ "command": "rm -rf /" });
        let res = tool.execute(params).await.unwrap();
        assert!(!res.success);
        assert!(
            res.error
                .unwrap_or_default()
                .to_lowercase()
                .contains("dangerous")
                || res.output.is_empty()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_terminal_tool_timeout() {
        let dir = tempdir().unwrap();
        let validator =
            CommandValidator::new(ExecutionMode::FullAutonomous, dir.path().to_path_buf());
        let config = TerminalConfig {
            timeout_seconds: 1,
            ..Default::default()
        };
        let tool = TerminalTool::new(validator, config);

        let params = json!({ "command": "sleep 2", "timeout_seconds": 1 });
        let res = tool.execute(params).await.unwrap();
        // Expect a failure or empty result (killed)
        assert!(!res.success);
    }

    #[test]
    fn test_command_validator_allowlist_and_denylist() {
        let tmp = PathBuf::from("/tmp");
        let v_restricted = CommandValidator::new(ExecutionMode::RestrictedAutonomous, tmp.clone());
        assert!(v_restricted.validate("ls -la").is_ok());
        assert!(v_restricted.validate("git status").is_ok());
        assert!(matches!(
            v_restricted.validate("vim file.txt").unwrap_err(),
            XzatomaError::CommandRequiresConfirmation(_)
        ));

        let v_full = CommandValidator::new(ExecutionMode::FullAutonomous, tmp.clone());
        assert!(matches!(
            v_full.validate("rm -rf /").unwrap_err(),
            XzatomaError::DangerousCommand(_)
        ));
    }

    #[tokio::test]
    async fn test_validate_paths_relative_safe_and_outside() {
        let dir = tempdir().unwrap();
        let work_dir = dir.path().to_path_buf();

        // inside path allowed
        let v = CommandValidator::new(ExecutionMode::FullAutonomous, work_dir.clone());
        // write a test file
        stdfs::write(work_dir.join("file.txt"), "x").unwrap();
        assert!(v.validate("cat file.txt").is_ok());
        assert!(v.validate("cat ./file.txt").is_ok());

        // outside path is rejected
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("o.txt");
        stdfs::write(&outside_file, "o").unwrap();
        // Using absolute path explicitly should be rejected
        assert!(v
            .validate(&format!("cat {}", outside_file.display()))
            .is_err());

        // Option value referencing outside file should be rejected
        let res = v.validate(&format!("ls --path={}", outside_file.display()));
        assert!(res.is_err());
    }
}
