//! Integration tests for Phase 5: Sandbox guard, per-tool opt-out, SSRF, and terminal hardening.
//!
//! These tests exercise the full sandbox stack at the integration boundary:
//! - `SandboxGuard::check_write` / `check_read` path enforcement
//! - Per-tool opt-out via `sandboxed_tools["file_ops"] = false`
//! - `SsrfValidator` host-allowlist enforcement for the fetch tool
//! - `CommandValidator` / `TerminalTool` integration: safe writes inside the
//!   project root and `.git` write protection
//! - Kernel sandbox (Landlock/Seatbelt) end-to-end via an `#[ignore]` test
//!   that is only run in environments with the kernel sandbox active

use std::path::{Path, PathBuf};
use tempfile::TempDir;
use xzatoma::config::{ExecutionMode, SandboxConfig, TerminalConfig};
use xzatoma::error::{SandboxError, XzatomaError};
use xzatoma::sandbox::SandboxGuard;
use xzatoma::tools::ToolExecutor;
use xzatoma::tools::fetch::SsrfValidator;
use xzatoma::tools::terminal::{CommandValidator, TerminalTool};

// ---------------------------------------------------------------------------
// Guard-level tests (pure logic, no I/O, no network)
// ---------------------------------------------------------------------------

/// When `sandbox.enabled = false`, `SandboxGuard` short-circuits both
/// `check_write` and `check_read` unconditionally, regardless of path.
#[test]
fn test_sandbox_disabled_bypasses_guard() {
    let tmp = TempDir::new().unwrap();
    let cfg = SandboxConfig {
        enabled: false,
        ..SandboxConfig::default()
    };
    let guard = SandboxGuard::new(cfg, tmp.path().to_path_buf());

    // Paths that would normally be denied are permitted when disabled.
    assert!(
        guard.check_write(Path::new("/etc/passwd")).is_ok(),
        "disabled guard must allow any write"
    );
    assert!(
        guard.check_read(Path::new("/etc/shadow")).is_ok(),
        "disabled guard must allow any read"
    );
}

/// When `sandboxed_tools["file_ops"] = false`, the guard short-circuits
/// for both read and write checks, implementing the per-tool opt-out
/// pattern that mirrors `TerminalTool`'s handling of `sandboxed_tools["terminal"]`.
#[test]
fn test_sandbox_per_tool_opt_out_file_ops() {
    let tmp = TempDir::new().unwrap();
    let outside = PathBuf::from("/tmp/xzatoma_per_tool_opt_out_test");

    let mut cfg = SandboxConfig::default();
    // sandbox is still enabled globally; only file_ops is opted out.
    assert!(cfg.enabled, "default config must have sandbox enabled");
    cfg.sandboxed_tools.insert("file_ops".to_string(), false);

    let guard = SandboxGuard::new(cfg, tmp.path().to_path_buf());

    assert!(
        guard.check_write(&outside).is_ok(),
        "file_ops=false must bypass check_write even for paths outside root"
    );
    assert!(
        guard.check_read(&outside).is_ok(),
        "file_ops=false must bypass check_read even for paths outside root"
    );
}

/// `SandboxGuard::check_write` returns `SandboxError::PathDenied` for a path
/// that is outside the sandbox root when sandboxing is enabled.
///
/// This test represents the application-level layer that `WriteFileTool`
/// exercises when a `SandboxGuard` is attached via `with_sandbox_guard`.
#[test]
fn test_write_file_tool_blocked_outside_root() {
    let tmp = TempDir::new().unwrap();
    let guard = SandboxGuard::new(SandboxConfig::default(), tmp.path().to_path_buf());

    // A stable absolute path that cannot be a descendant of tmp.
    let outside = PathBuf::from("/tmp/xzatoma_write_blocked_outside_root_test");

    let result = guard.check_write(&outside);
    assert!(result.is_err(), "path outside root must be denied");
    assert!(
        matches!(result, Err(SandboxError::PathDenied { .. })),
        "expected PathDenied variant, got: {result:?}"
    );
}

/// `SandboxGuard::check_write` returns `SandboxError::PathDenied` for any
/// path that contains a `.git` component when `protect_git = true` (the
/// default). This applies even when the path is otherwise inside the root.
///
/// This test represents the guard layer that `WriteFileTool` will exercise
/// when a `SandboxGuard` is attached via `with_sandbox_guard`.
#[test]
fn test_write_file_tool_blocked_git_dir() {
    let tmp = TempDir::new().unwrap();
    // protect_git = true is already the default; use SandboxConfig::default().
    let guard = SandboxGuard::new(SandboxConfig::default(), tmp.path().to_path_buf());

    // The .git directory does not need to exist on disk; the guard performs
    // lexical component inspection only.
    let git_path = tmp.path().join(".git/ORIG_HEAD");

    let result = guard.check_write(&git_path);
    assert!(result.is_err(), ".git write must be denied");
    assert!(
        matches!(result, Err(SandboxError::PathDenied { .. })),
        "expected PathDenied variant for .git path, got: {result:?}"
    );
}

/// `SandboxGuard::check_read` returns `SandboxError::PathDenied` for a path
/// that is outside the sandbox root when sandboxing is enabled.
///
/// This test represents the guard layer that `ReadFileTool` will exercise
/// when a `SandboxGuard` is attached via `with_sandbox_guard`.
#[test]
fn test_read_file_tool_blocked_outside_root() {
    let tmp = TempDir::new().unwrap();
    let guard = SandboxGuard::new(SandboxConfig::default(), tmp.path().to_path_buf());

    let outside = PathBuf::from("/tmp/xzatoma_read_blocked_outside_root_test");

    let result = guard.check_read(&outside);
    assert!(
        result.is_err(),
        "path outside root must be denied for reads"
    );
    assert!(
        matches!(result, Err(SandboxError::PathDenied { .. })),
        "expected PathDenied variant, got: {result:?}"
    );
}

/// `SsrfValidator` with a non-empty `allowed_hosts` list blocks any host that
/// is not explicitly listed. This is the application-level guard applied by
/// `FetchTool` when `sandbox.network_allowed_hosts` is configured.
///
/// No network connection is made: `SsrfValidator::validate` performs only
/// string-level and allowlist checks before any I/O occurs.
#[test]
fn test_fetch_tool_blocked_by_allowed_hosts() {
    let validator = SsrfValidator::new().with_allowed_hosts(vec!["example.com".to_string()]);

    let result = validator.validate("https://evil.internal/api");
    assert!(
        result.is_err(),
        "host not in allowed_hosts must be blocked; got Ok"
    );

    // The listed host must still be reachable through the allowlist.
    let allowed = validator.validate("https://example.com/page");
    assert!(
        allowed.is_ok(),
        "explicitly allowed host must pass validation; got: {allowed:?}"
    );
}

// ---------------------------------------------------------------------------
// Terminal tool tests (Unix only, run actual child processes)
// ---------------------------------------------------------------------------

/// `TerminalTool` with `FullAutonomous` mode allows `touch` to create a file
/// inside the working directory (the sandbox root). This is the baseline
/// write-is-allowed case that the more restrictive tests depend on.
#[cfg(unix)]
#[tokio::test]
async fn test_terminal_can_write_inside_project() {
    let tmp = TempDir::new().unwrap();
    let validator = CommandValidator::new(ExecutionMode::FullAutonomous, tmp.path().to_path_buf());
    let tool = TerminalTool::new(validator, TerminalConfig::default());

    let result = tool
        .execute(serde_json::json!({"command": "touch sandbox_write_test.txt"}))
        .await
        .unwrap();

    assert!(
        result.success,
        "touch inside project dir must succeed; error: {:?}",
        result.error
    );
    assert!(
        tmp.path().join("sandbox_write_test.txt").exists(),
        "created file must exist on disk"
    );
}

/// `CommandValidator::validate` rejects `touch .git/ORIG_HEAD` with
/// `PathOutsideWorkingDirectory` via the Phase 3 `.git` path guard. This
/// is the application-level belt that blocks git-directory writes before
/// any child process is spawned.
#[test]
fn test_terminal_git_write_blocked_by_guard() {
    let tmp = TempDir::new().unwrap();
    // The .git directory itself does not need to exist: the check is purely
    // a lexical component inspection that fires on any path containing ".git".
    std::fs::create_dir_all(tmp.path().join(".git")).unwrap();

    let validator = CommandValidator::new(ExecutionMode::FullAutonomous, tmp.path().to_path_buf());

    let err = validator
        .validate("touch .git/ORIG_HEAD")
        .expect_err("touch .git/ORIG_HEAD must be rejected by CommandValidator");

    assert!(
        matches!(err, XzatomaError::PathOutsideWorkingDirectory(_)),
        "expected PathOutsideWorkingDirectory, got: {err}"
    );
}

/// Verifies that the kernel sandbox (Landlock on Linux, Seatbelt on macOS)
/// blocks writes outside the project root.
///
/// This test is `#[ignore]` because it requires the platform kernel sandbox to
/// be active. Run with:
///
/// ```bash
/// cargo test -p xzatoma --test sandbox_integration -- --include-ignored
/// ```
///
/// Note: `CommandValidator` already blocks absolute paths at the application
/// level, so this test also validates the defence-in-depth approach: at least
/// one layer must deny the escape attempt even before the kernel sandbox fires.
#[cfg(unix)]
#[tokio::test]
#[ignore = "requires kernel sandbox"]
async fn test_terminal_cannot_write_outside_project() {
    use xzatoma::config::Config;

    let tmp = TempDir::new().unwrap();
    let mut cfg = Config::default();
    cfg.agent.terminal.default_mode = ExecutionMode::FullAutonomous;

    let tool = TerminalTool::from_config(&cfg, tmp.path().to_path_buf())
        .expect("TerminalTool::from_config should succeed");

    // touch /tmp/... escapes the project root. With the kernel sandbox active
    // the child process receives a permission-denied signal before execution
    // completes. CommandValidator also rejects absolute paths at the
    // application level, so the result is a failure regardless.
    let result = tool
        .execute(serde_json::json!({"command": "touch /tmp/xzatoma_escape_test_kernel"}))
        .await;

    assert!(
        !result.map(|r| r.success).unwrap_or(false),
        "write outside sandbox root must be denied"
    );
}
