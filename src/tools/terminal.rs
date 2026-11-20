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
//! # Security posture
//!
//! The regex denylist is BEST-EFFORT defense-in-depth only. It MUST NOT be
//! relied upon as a security boundary: regex patterns are trivially bypassed
//! (encoding tricks, alternate binaries, whitespace, environment indirection),
//! and treating them as a boundary invites a false sense of safety. The real
//! containment comes from the shell-free tokenizer ([`parse_command_line`],
//! which rejects shell operators so no `|`, `;`, `&&`, `$()`, or redirection
//! is ever interpreted) combined with path canonicalization
//! ([`CommandValidator::validate_paths`], which resolves symlinks and rejects
//! any argument that escapes the working directory). The interpreter check
//! ([`is_interpreter_invocation`]) is likewise best-effort hardening that
//! forces confirmation for programs that can execute arbitrary code.
//!
//! # Examples
//!
//! ```
//! use xzatoma::tools::terminal::{execute_command, TerminalTool, CommandValidator};
//! use xzatoma::config::{ExecutionMode, TerminalConfig};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let result = execute_command("echo hello", std::path::PathBuf::from("."), ExecutionMode::RestrictedAutonomous, None).await?;
//!     println!("{}", result);
//!     Ok(())
//! }
//! ```

use std::path::PathBuf;
use std::process::{Command as StdCommand, Stdio};
use std::time::Duration;

use async_trait::async_trait;
use regex::Regex;
use serde_json::{Value, json};
use tokio::process::Command;
use tokio::time;

use crate::chat_mode::SafetyMode;
use crate::config::{ExecutionMode, TerminalConfig};
use crate::error::{Result, XzatomaError};
use crate::tools::{ToolExecutor, ToolResult};

/// Parsed command line with program and arguments
///
/// # Examples
///
/// ```
/// use xzatoma::tools::terminal::parse_command_line;
///
/// let parsed = parse_command_line("echo hello").unwrap();
/// assert_eq!(parsed.program, "echo");
/// assert_eq!(parsed.args, vec!["hello".to_string()]);
/// ```
#[derive(Debug, Clone)]
pub struct ParsedCommand {
    /// Program name or path
    pub program: String,
    /// Arguments for the program
    pub args: Vec<String>,
}

impl ParsedCommand {
    fn tokens(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.program.as_str()).chain(self.args.iter().map(String::as_str))
    }
}

/// Convenience wrapper to execute a single command using `TerminalTool` with a default config.
///
/// # Arguments
///
/// * `command` - The command string to execute
/// * `working_dir` - Directory to run the command in
/// * `mode` - Execution mode governing validation policy
/// * `sandbox_config` - Optional sandbox configuration; when `Some`, a
///   `SandboxPolicy` is built and applied in the child process
///
/// # Errors
///
/// Returns `XzatomaError` if validation fails, the command fails, or the
/// sandbox policy cannot be built.
pub async fn execute_command(
    command: &str,
    working_dir: PathBuf,
    mode: ExecutionMode,
    sandbox_config: Option<&crate::config::SandboxConfig>,
) -> Result<String> {
    let validator = CommandValidator::new(mode, working_dir.clone());
    let config = TerminalConfig {
        default_mode: mode,
        ..Default::default()
    };

    let mut tool = TerminalTool::new(validator, config);
    if let Some(cfg) = sandbox_config {
        let policy = crate::sandbox::SandboxPolicy::from_config(cfg, &working_dir)?;
        tool = tool.with_sandbox_policy(policy);
    }

    let params = json!({
        "command": command,
        "timeout_seconds": tool.config.timeout_seconds,
        "confirm": true,
    });

    let tool_result = tool.execute(params).await?;
    if tool_result.success {
        Ok(tool_result.output)
    } else {
        Err(XzatomaError::Tool(
            tool_result
                .error
                .unwrap_or_else(|| "command failed".to_string()),
        ))
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
    /// Programs blocked when `sandbox.network_enabled = false`.
    ///
    /// Populated by `CommandValidator::from_config`; empty when sandbox is
    /// disabled or network is permitted.
    pub network_blocked_programs: Vec<String>,
}

impl CommandValidator {
    /// Construct a new `CommandValidator` instance with allowlist and denylist defaults
    pub fn new(mode: ExecutionMode, working_dir: PathBuf) -> Self {
        let allowlist = vec![
            "ls", "cat", "grep", "find", "echo", "pwd", "whoami", "head", "tail", "wc", "sort",
            "uniq", "diff", "which", "basename", "dirname", "realpath",
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
            r"\bnc\s+-[el]",
            r"\bsocat\s+.*EXEC",
        ];

        // SAFETY: The denylist patterns are compile-time string constants whose
        // validity is verified by existing tests, so `Regex::new` cannot fail at
        // runtime.
        #[allow(clippy::expect_used)]
        let denylist = denylist_patterns
            .into_iter()
            .map(|p| Regex::new(p).expect("invalid denylist regex"))
            .collect();

        Self {
            mode,
            working_dir,
            allowlist,
            denylist,
            network_blocked_programs: Vec::new(),
        }
    }

    /// Construct a `CommandValidator` from the full application configuration.
    ///
    /// Sets `network_blocked_programs` to the fixed list of programs that open
    /// raw TCP/UDP sockets and do not honour proxy environment variables when
    /// `cfg.sandbox.enabled = true` and `cfg.sandbox.network_enabled = false`.
    ///
    /// # Arguments
    ///
    /// * `cfg` - Application configuration
    /// * `working_dir` - Directory commands are constrained to
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::tools::terminal::CommandValidator;
    /// use xzatoma::config::Config;
    ///
    /// let cfg = Config::default();
    /// let validator = CommandValidator::from_config(&cfg, std::path::PathBuf::from("/tmp"));
    /// ```
    pub fn from_config(cfg: &crate::config::Config, working_dir: PathBuf) -> Self {
        let mut validator = Self::new(cfg.agent.terminal.default_mode, working_dir);
        if cfg.sandbox.enabled
            && !cfg.sandbox.network_enabled
            && cfg.sandbox.sandboxed_tools.get("terminal") != Some(&false)
        {
            validator.network_blocked_programs = vec![
                "ssh".to_string(),
                "scp".to_string(),
                "sftp".to_string(),
                "ftp".to_string(),
                "nc".to_string(),
                "ncat".to_string(),
                "netcat".to_string(),
                "telnet".to_string(),
                "socat".to_string(),
                "nmap".to_string(),
            ];
        }
        validator
    }
    ///
    /// Returns:
    /// - Ok(()) if valid
    /// - Err(XzatomaError::CommandRequiresConfirmation(_)) if confirmation is required (interactive or restricted)
    /// - Err(XzatomaError::DangerousCommand(_)) if command matches denylist
    /// - Err(XzatomaError::PathOutsideWorkingDirectory(_)) if path escapes the working directory
    pub fn validate(&self, command: &str) -> std::result::Result<(), XzatomaError> {
        let parsed = parse_command_line(command)?;
        // Network-blocked programs check: must come before the denylist loop
        // so the error message is actionable (tells the user which config to change).
        let program_basename = std::path::Path::new(&parsed.program)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&parsed.program);
        if self
            .network_blocked_programs
            .contains(&program_basename.to_string())
        {
            return Err(XzatomaError::DangerousCommand(format!(
                "program '{}' is blocked: sandbox network is disabled; \
                 set sandbox.network_enabled = true to permit network tools",
                program_basename
            )));
        }
        // Denylist first - block always
        for r in &self.denylist {
            if r.is_match(command) {
                return Err(XzatomaError::DangerousCommand(format!(
                    "Command matches dangerous pattern: {}",
                    command
                )));
            }
        }

        match self.mode {
            ExecutionMode::Interactive => Err(XzatomaError::CommandRequiresConfirmation(
                command.to_string(),
            )),
            ExecutionMode::RestrictedAutonomous => {
                // Get first token i.e. command name
                let name = parsed.program.as_str();
                if !self.allowlist.contains(&name.to_string()) {
                    return Err(XzatomaError::CommandRequiresConfirmation(format!(
                        "Command '{}' not in allowlist",
                        name
                    )));
                }

                self.validate_restricted_arguments(&parsed)?;
                self.validate_paths(&parsed)?;
                Ok(())
            }
            ExecutionMode::FullAutonomous => {
                // Full autonomous still must validate paths
                self.validate_paths(&parsed)?;
                // Best-effort hardening: interpreters can execute arbitrary
                // code, so require confirmation even in FullAutonomous mode.
                if is_interpreter_invocation(parsed.program.as_str()) {
                    return Err(XzatomaError::CommandRequiresConfirmation(format!(
                        "Interpreter '{}' can execute arbitrary code; confirm before running",
                        parsed.program
                    )));
                }
                Ok(())
            }
        }
    }

    fn validate_restricted_arguments(
        &self,
        parsed: &ParsedCommand,
    ) -> std::result::Result<(), XzatomaError> {
        if parsed.program == "find"
            && parsed
                .args
                .iter()
                .any(|arg| matches!(arg.as_str(), "-exec" | "-execdir" | "-delete"))
        {
            return Err(XzatomaError::CommandRequiresConfirmation(
                "find actions that execute commands or delete files require confirmation"
                    .to_string(),
            ));
        }

        Ok(())
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
    fn validate_paths(&self, parsed: &ParsedCommand) -> std::result::Result<(), XzatomaError> {
        let canonical_working = self.resolve_canonical_working();

        for token in parsed.tokens() {
            let t = token.trim();
            if t.is_empty() {
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

            // Belt-and-suspenders: reject paths that target .git directories in
            // any argument position, regardless of kernel sandbox state, so the
            // error message is always actionable.
            if candidate_path.components().any(|c| c.as_os_str() == ".git") {
                return Err(XzatomaError::PathOutsideWorkingDirectory(format!(
                    "writes to .git directories are forbidden: {}",
                    candidate_path.display()
                )));
            }

            // If the candidate exists use canonicalization to prevent symlink escape
            if let Ok(canonical_target) = candidate_path.canonicalize()
                && !canonical_target.starts_with(&canonical_working)
            {
                return Err(XzatomaError::PathOutsideWorkingDirectory(format!(
                    "Path escapes working directory: {} -> {:?}",
                    candidate, canonical_target
                )));
            }
        }

        Ok(())
    }
}

/// Parse a command string into program and arguments without shell features
///
/// # Examples
///
/// ```
/// use xzatoma::tools::terminal::parse_command_line;
///
/// let parsed = parse_command_line("echo \"hello world\"").unwrap();
/// assert_eq!(parsed.program, "echo");
/// assert_eq!(parsed.args, vec!["hello world".to_string()]);
/// ```
pub fn parse_command_line(command: &str) -> std::result::Result<ParsedCommand, XzatomaError> {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut chars = command.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    let mut saw_non_ws = false;

    while let Some(ch) = chars.next() {
        match ch {
            '\'' if !in_double => {
                in_single = !in_single;
                saw_non_ws = true;
            }
            '"' if !in_single => {
                in_double = !in_double;
                saw_non_ws = true;
            }
            '\\' if !in_single => {
                if let Some(next) = chars.next() {
                    current.push(next);
                    saw_non_ws = true;
                } else {
                    return Err(XzatomaError::Tool(
                        "Invalid escape at end of command".to_string(),
                    ));
                }
            }
            c if c.is_whitespace() && !in_single && !in_double => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            '|' | '&' | ';' | '<' | '>' if !in_single && !in_double => {
                return Err(XzatomaError::Tool(
                    "Shell operators are not allowed in terminal commands".to_string(),
                ));
            }
            '`' if !in_single && !in_double => {
                return Err(XzatomaError::Tool(
                    "Command substitution is not allowed".to_string(),
                ));
            }
            '$' if !in_single && !in_double => {
                if matches!(chars.peek(), Some('(')) {
                    return Err(XzatomaError::Tool(
                        "Command substitution is not allowed".to_string(),
                    ));
                }
                current.push(ch);
                saw_non_ws = true;
            }
            _ => {
                current.push(ch);
                if !ch.is_whitespace() {
                    saw_non_ws = true;
                }
            }
        }
    }

    if in_single || in_double {
        return Err(XzatomaError::Tool(
            "Unterminated quoted string in command".to_string(),
        ));
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    if tokens.is_empty() && !saw_non_ws {
        return Err(XzatomaError::Tool("Empty command".to_string()));
    }

    if tokens.is_empty() {
        return Err(XzatomaError::Tool(
            "Command parsing produced no tokens".to_string(),
        ));
    }

    let program = tokens.remove(0);
    if program.contains('=') && !program.contains('/') && !program.starts_with(".") {
        return Err(XzatomaError::Tool(
            "Environment assignments are not supported in terminal commands".to_string(),
        ));
    }

    Ok(ParsedCommand {
        program,
        args: tokens,
    })
}

/// Return `true` when `program` names a language interpreter or shell.
///
/// The basename of `program` is compared against a fixed set of interpreters:
/// `python`, `python3`, `sh`, `bash`, `zsh`, `fish`, `node`, `deno`, `ruby`,
/// `perl`, and `php`. Any leading directory prefix is stripped first, so
/// `/usr/bin/python3` matches just like `python3`.
///
/// This is best-effort hardening used by [`CommandValidator::validate`] to
/// force confirmation before running programs that can execute arbitrary
/// code, even in [`ExecutionMode::FullAutonomous`]. It is NOT a security
/// boundary.
///
/// # Examples
///
/// ```
/// use xzatoma::tools::terminal::is_interpreter_invocation;
///
/// assert!(is_interpreter_invocation("bash"));
/// assert!(is_interpreter_invocation("/usr/bin/python3"));
/// assert!(!is_interpreter_invocation("echo"));
/// ```
pub fn is_interpreter_invocation(program: &str) -> bool {
    // Strip any directory prefix (handles both `/` and, defensively, `\`).
    let basename = program.rsplit(['/', '\\']).next().unwrap_or(program);
    matches!(
        basename,
        "python"
            | "python3"
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
            | "node"
            | "deno"
            | "ruby"
            | "perl"
            | "php"
    )
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
    /// Kernel sandbox policy applied to each child process before execution.
    pub sandbox_policy: Option<std::sync::Arc<crate::sandbox::SandboxPolicy>>,
    /// Address of the running host-filtering HTTP proxy, if any.
    pub proxy_addr: Option<std::net::SocketAddr>,
}

impl TerminalTool {
    /// Create a new TerminalTool with default SafetyMode::AlwaysConfirm
    pub fn new(validator: CommandValidator, config: TerminalConfig) -> Self {
        Self {
            validator,
            config,
            safety_mode: SafetyMode::AlwaysConfirm,
            sandbox_policy: None,
            proxy_addr: None,
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

    /// Attach a sandbox policy to this tool (builder pattern).
    ///
    /// The policy is applied to each child process in the `fork`-before-exec hook.
    ///
    /// # Arguments
    ///
    /// * `policy` - The sandbox policy to apply
    pub fn with_sandbox_policy(mut self, policy: crate::sandbox::SandboxPolicy) -> Self {
        self.sandbox_policy = Some(std::sync::Arc::new(policy));
        self
    }

    /// Set the address of the host-filtering HTTP proxy (builder pattern).
    ///
    /// When set, `HTTP_PROXY`, `HTTPS_PROXY`, `http_proxy`, `https_proxy`, and
    /// `NO_PROXY` environment variables are injected into each child process.
    ///
    /// # Arguments
    ///
    /// * `addr` - Socket address of the running proxy
    pub fn with_proxy_addr(mut self, addr: std::net::SocketAddr) -> Self {
        self.proxy_addr = Some(addr);
        self
    }

    /// Construct a `TerminalTool` from the full application configuration.
    ///
    /// Applies a sandbox policy when `cfg.sandbox.enabled = true` and the
    /// `"terminal"` key is not `false` in `cfg.sandbox.sandboxed_tools`.
    ///
    /// # Arguments
    ///
    /// * `cfg` - Application configuration
    /// * `working_dir` - Directory commands are constrained to
    ///
    /// # Errors
    ///
    /// Returns `XzatomaError` if `SandboxPolicy::from_config` fails to resolve
    /// configured paths.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::tools::terminal::TerminalTool;
    /// use xzatoma::config::Config;
    ///
    /// let cfg = Config::default();
    /// let tool = TerminalTool::from_config(&cfg, std::path::PathBuf::from("/tmp")).unwrap();
    /// assert!(tool.sandbox_policy.is_some());
    /// ```
    pub fn from_config(
        cfg: &crate::config::Config,
        working_dir: PathBuf,
    ) -> crate::error::Result<Self> {
        let validator = CommandValidator::from_config(cfg, working_dir.clone());
        let mut tool = Self {
            validator,
            config: cfg.agent.terminal.clone(),
            safety_mode: SafetyMode::AlwaysConfirm,
            sandbox_policy: None,
            proxy_addr: None,
        };
        let terminal_sandboxed = cfg.sandbox.sandboxed_tools.get("terminal") != Some(&false);
        if cfg.sandbox.enabled && terminal_sandboxed {
            let policy = crate::sandbox::SandboxPolicy::from_config(&cfg.sandbox, &working_dir)?;
            tool.sandbox_policy = Some(std::sync::Arc::new(policy));
        }
        Ok(tool)
    }
}

#[async_trait]
impl ToolExecutor for TerminalTool {
    fn tool_definition(&self) -> Value {
        json!({
            "name": "terminal",
            "description": "Execute validated commands in the working directory (no shell operators)",
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
            .ok_or_else(|| XzatomaError::Tool("Missing 'command' parameter".to_string()))?
            .to_string();

        let parsed = parse_command_line(&command)?;

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

        // Build the program invocation without shell parsing
        let mut cmd = Command::new(&parsed.program);
        cmd.args(&parsed.args);

        cmd.current_dir(self.validator.working_dir.clone());
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        // SAFETY: This closure runs in the child process after `fork` and before
        // `exec`. It must be async-signal-safe (no heap allocation, no Rust
        // runtime, no locking). `setpgid` is async-signal-safe. The Landlock
        // ruleset application via `nono` must perform no heap allocation beyond
        // what the kernel requires directly. The Seatbelt `sandbox_init` wrapper
        // in `nono` is documented as signal-safe; verify against the nono
        // release notes when updating the dependency version.
        #[cfg(unix)]
        {
            let policy_for_exec = self.sandbox_policy.clone();
            unsafe {
                cmd.pre_exec(move || {
                    // 1. Place child in its own process group.
                    if libc::setpgid(0, 0) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    // 2. Apply kernel sandbox, if configured.
                    if let Some(policy) = &policy_for_exec {
                        crate::sandbox::apply_sandbox(policy).map_err(|e| {
                            std::io::Error::new(std::io::ErrorKind::PermissionDenied, e.to_string())
                        })?;
                    }
                    Ok(())
                });
            }
        }
        // Inject proxy environment variables when a host-filtering proxy is active.
        if let Some(proxy_addr) = self.proxy_addr {
            let proxy_url = format!("http://{}", proxy_addr);
            cmd.env("HTTP_PROXY", &proxy_url)
                .env("HTTPS_PROXY", &proxy_url)
                .env("http_proxy", &proxy_url)
                .env("https_proxy", &proxy_url)
                .env("NO_PROXY", "127.0.0.1,localhost");
        }

        // Spawn and obtain a child
        let child = cmd
            .spawn()
            .map_err(|e| XzatomaError::Tool(format!("Failed to spawn command: {}", e)))?;

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
                    Ok(Err(e)) => {
                        return Err(XzatomaError::Tool(format!(
                            "Failed waiting for command output: {}",
                            e
                        )))
                    }
                    Err(e) => {
                        return Err(XzatomaError::Tool(format!(
                            "Join error waiting for command: {}",
                            e
                        )))
                    }
                }
            }
            _ = &mut sleep => {
                // Timeout -> terminate the process group where supported so
                // child processes do not survive the tool timeout.
                if let Some(pid) = pid {
                    terminate_process_tree(pid);
                }
                // Await join handle after kill to collect any output
                match join_fut.await {
                    Ok(Ok(out)) => out,
                    Ok(Err(e)) => {
                        return Err(XzatomaError::Tool(format!(
                            "Failed waiting for command output after kill: {}",
                            e
                        )))
                    }
                    Err(e) => {
                        return Err(XzatomaError::Tool(format!(
                            "Join error after kill: {}",
                            e
                        )))
                    }
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

fn terminate_process_tree(pid: u32) {
    #[cfg(unix)]
    {
        let group = format!("-{}", pid);
        match StdCommand::new("kill").arg("-TERM").arg(&group).status() {
            Ok(status) if status.success() => {}
            Ok(status) => tracing::warn!(
                pid = pid,
                status = %status,
                "Failed to terminate terminal process group with SIGTERM"
            ),
            Err(error) => tracing::warn!(
                pid = pid,
                error = %error,
                "Failed to run kill for terminal process group"
            ),
        }

        std::thread::sleep(Duration::from_millis(100));

        match StdCommand::new("kill").arg("-KILL").arg(&group).status() {
            Ok(status) if status.success() => {}
            Ok(status) => tracing::warn!(
                pid = pid,
                status = %status,
                "Failed to terminate terminal process group with SIGKILL"
            ),
            Err(error) => tracing::warn!(
                pid = pid,
                error = %error,
                "Failed to run kill -KILL for terminal process group"
            ),
        }
    }

    #[cfg(windows)]
    {
        match StdCommand::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()
        {
            Ok(status) if status.success() => {}
            Ok(status) => tracing::warn!(
                pid = pid,
                status = %status,
                "Failed to terminate terminal process tree"
            ),
            Err(error) => tracing::warn!(
                pid = pid,
                error = %error,
                "Failed to run taskkill for terminal process tree"
            ),
        }
    }
}

/// Validate a command for safety (convenience)
pub fn validate_command(command: &str, mode: ExecutionMode, working_dir: PathBuf) -> Result<()> {
    let validator = CommandValidator::new(mode, working_dir);
    validator.validate(command)
}

/// Return true when the command is considered dangerous or not allowed
pub fn is_dangerous_command(command: &str, mode: ExecutionMode, working_dir: PathBuf) -> bool {
    validate_command(command, mode, working_dir).is_err()
}

/// Simple parser that splits a command line on whitespace
pub fn parse_command(command: &str) -> Vec<String> {
    parse_command_line(command)
        .map(|parsed| {
            std::iter::once(parsed.program)
                .chain(parsed.args)
                .collect::<Vec<String>>()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, TerminalConfig};
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
        assert!(matches!(
            v_restricted.validate("git status").unwrap_err(),
            XzatomaError::CommandRequiresConfirmation(_)
        ));
        assert!(matches!(
            v_restricted.validate("python -c 'print(1)'").unwrap_err(),
            XzatomaError::CommandRequiresConfirmation(_)
        ));
        assert!(matches!(
            v_restricted.validate("find . -delete").unwrap_err(),
            XzatomaError::CommandRequiresConfirmation(_)
        ));
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

    #[test]
    fn test_parse_command_line_with_quotes() {
        let parsed = parse_command_line("echo \"hello world\"").unwrap();
        assert_eq!(parsed.program, "echo");
        assert_eq!(parsed.args, vec!["hello world".to_string()]);
    }

    #[test]
    fn test_parse_command_line_rejects_shell_operators() {
        let err = parse_command_line("echo hi | grep h").unwrap_err();
        assert!(err.to_string().contains("Shell operators"));
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
        assert!(
            v.validate(&format!("cat {}", outside_file.display()))
                .is_err()
        );

        // Option value referencing outside file should be rejected
        let res = v.validate(&format!("ls --path={}", outside_file.display()));
        assert!(res.is_err());
    }

    #[test]
    fn test_is_interpreter_invocation_bare_bash_is_true() {
        assert!(is_interpreter_invocation("bash"));
    }

    #[test]
    fn test_is_interpreter_invocation_absolute_python3_is_true() {
        assert!(is_interpreter_invocation("/usr/bin/python3"));
    }

    #[test]
    fn test_is_interpreter_invocation_echo_is_false() {
        assert!(!is_interpreter_invocation("echo"));
    }

    #[test]
    fn test_is_interpreter_invocation_ls_is_false() {
        assert!(!is_interpreter_invocation("ls"));
    }

    #[test]
    fn test_validate_full_autonomous_interpreter_requires_confirmation() {
        let tmp = PathBuf::from("/tmp");
        let v = CommandValidator::new(ExecutionMode::FullAutonomous, tmp);
        // bash is an interpreter: FullAutonomous must now demand confirmation.
        assert!(matches!(
            v.validate("bash -c 'echo hi'").unwrap_err(),
            XzatomaError::CommandRequiresConfirmation(_)
        ));
    }

    #[test]
    fn test_validate_full_autonomous_non_interpreter_is_ok() {
        let tmp = PathBuf::from("/tmp");
        let v = CommandValidator::new(ExecutionMode::FullAutonomous, tmp);
        // echo is not an interpreter: FullAutonomous still allows it.
        assert!(v.validate("echo hi").is_ok());
    }

    // --- Phase 3: sandbox integration tests ---

    #[test]
    fn test_command_validator_from_config_blocks_ssh_when_network_disabled() {
        // Default config: sandbox.enabled=true, network_enabled=false.
        let cfg = Config::default();
        assert!(cfg.sandbox.enabled);
        assert!(!cfg.sandbox.network_enabled);
        let validator = CommandValidator::from_config(&cfg, PathBuf::from("/tmp"));
        let err = validator.validate("ssh user@host").unwrap_err();
        assert!(
            matches!(err, XzatomaError::DangerousCommand(ref msg) if msg.contains("network")),
            "expected network-blocked DangerousCommand; got: {err}"
        );
    }

    #[test]
    fn test_command_validator_from_config_allows_ssh_when_network_enabled() {
        let mut cfg = Config::default();
        cfg.sandbox.network_enabled = true;
        let validator = CommandValidator::from_config(&cfg, PathBuf::from("/tmp"));
        let result = validator.validate("ssh user@host");
        // Must not be blocked specifically for network reasons.
        if let Err(XzatomaError::DangerousCommand(ref msg)) = result {
            assert!(
                !msg.contains("network"),
                "ssh should not be blocked for network when network_enabled=true; got: {msg}"
            );
        }
        // Other error variants (e.g. CommandRequiresConfirmation) are acceptable.
    }

    #[test]
    fn test_command_validator_from_config_empty_blocked_list_when_sandbox_disabled() {
        let mut cfg = Config::default();
        cfg.sandbox.enabled = false;
        let validator = CommandValidator::from_config(&cfg, PathBuf::from("/tmp"));
        assert!(
            validator.network_blocked_programs.is_empty(),
            "network_blocked_programs must be empty when sandbox is disabled"
        );
    }

    #[test]
    fn test_command_validator_from_config_empty_blocked_list_when_terminal_opt_out() {
        let mut cfg = Config::default();
        cfg.sandbox
            .sandboxed_tools
            .insert("terminal".to_string(), false);
        let validator = CommandValidator::from_config(&cfg, PathBuf::from("/tmp"));
        assert!(
            validator.network_blocked_programs.is_empty(),
            "network_blocked_programs must be empty when terminal is opted out"
        );
    }

    #[test]
    fn test_command_validator_blocks_nc_listen() {
        let v = CommandValidator::new(ExecutionMode::FullAutonomous, PathBuf::from("/tmp"));
        let err = v.validate("nc -l 4444").unwrap_err();
        assert!(
            matches!(err, XzatomaError::DangerousCommand(_)),
            "nc -l should match denylist; got: {err}"
        );
    }

    #[test]
    fn test_command_validator_blocks_socat_exec() {
        let v = CommandValidator::new(ExecutionMode::FullAutonomous, PathBuf::from("/tmp"));
        let err = v.validate("socat - EXEC:/bin/sh").unwrap_err();
        assert!(
            matches!(err, XzatomaError::DangerousCommand(_)),
            "socat EXEC should match denylist; got: {err}"
        );
    }

    #[test]
    fn test_command_validator_git_path_blocked_in_write_position() {
        let dir = tempdir().unwrap();
        let v = CommandValidator::new(ExecutionMode::FullAutonomous, dir.path().to_path_buf());
        let err = v.validate("touch .git/config").unwrap_err();
        assert!(
            matches!(err, XzatomaError::PathOutsideWorkingDirectory(_)),
            "path containing .git should be rejected; got: {err}"
        );
    }

    #[test]
    fn test_terminal_tool_from_config_sandbox_policy_present_when_enabled() {
        let dir = tempdir().unwrap();
        let cfg = Config::default();
        assert!(
            cfg.sandbox.enabled,
            "default config must have sandbox enabled"
        );
        let tool = TerminalTool::from_config(&cfg, dir.path().to_path_buf()).unwrap();
        assert!(
            tool.sandbox_policy.is_some(),
            "sandbox_policy must be Some when sandbox.enabled=true"
        );
    }

    #[test]
    fn test_terminal_tool_from_config_sandbox_policy_absent_when_disabled() {
        let dir = tempdir().unwrap();
        let mut cfg = Config::default();
        cfg.sandbox.enabled = false;
        let tool = TerminalTool::from_config(&cfg, dir.path().to_path_buf()).unwrap();
        assert!(
            tool.sandbox_policy.is_none(),
            "sandbox_policy must be None when sandbox.enabled=false"
        );
    }

    #[test]
    fn test_terminal_tool_from_config_sandbox_policy_absent_when_tool_opt_out() {
        let dir = tempdir().unwrap();
        let mut cfg = Config::default();
        cfg.sandbox
            .sandboxed_tools
            .insert("terminal".to_string(), false);
        let tool = TerminalTool::from_config(&cfg, dir.path().to_path_buf()).unwrap();
        assert!(
            tool.sandbox_policy.is_none(),
            "sandbox_policy must be None when terminal is opted out"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_terminal_sandbox_allows_write_inside_root() {
        let dir = tempdir().unwrap();
        let mut cfg = Config::default();
        // Use FullAutonomous so 'touch' does not require allowlist confirmation.
        cfg.agent.terminal.default_mode = ExecutionMode::FullAutonomous;
        let tool = TerminalTool::from_config(&cfg, dir.path().to_path_buf()).unwrap();
        let params = json!({ "command": "touch sandbox_write_test.txt" });
        let res = tool.execute(params).await.unwrap();
        assert!(
            res.success,
            "touch inside sandbox root should succeed; error: {:?}",
            res.error
        );
    }

    #[ignore = "requires kernel sandbox"]
    #[cfg(unix)]
    #[tokio::test]
    async fn test_terminal_sandbox_blocks_write_outside_root() {
        let dir = tempdir().unwrap();
        let mut cfg = Config::default();
        cfg.agent.terminal.default_mode = ExecutionMode::FullAutonomous;
        let tool = TerminalTool::from_config(&cfg, dir.path().to_path_buf()).unwrap();
        // touch /tmp/xzatoma_escape is outside the sandbox root; expect failure
        // (either application-level path rejection or kernel sandbox denial).
        let params = json!({ "command": "touch /tmp/xzatoma_sandbox_escape_test" });
        let res = tool.execute(params).await.unwrap();
        assert!(!res.success, "write outside sandbox root must be denied");
    }

    #[ignore = "requires kernel sandbox"]
    #[cfg(unix)]
    #[tokio::test]
    async fn test_terminal_sandbox_blocks_git_write() {
        let dir = tempdir().unwrap();
        // Create a real .git directory so the path exists and reaches the kernel sandbox.
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        let mut cfg = Config::default();
        cfg.agent.terminal.default_mode = ExecutionMode::FullAutonomous;
        let tool = TerminalTool::from_config(&cfg, dir.path().to_path_buf()).unwrap();
        let params = json!({ "command": "touch .git/ORIG_HEAD" });
        let res = tool.execute(params).await.unwrap();
        assert!(!res.success, "write to .git must be denied");
    }
}
