//! Application-level path guard for file-operation tools.
//!
//! [`SandboxGuard`] enforces `SandboxConfig` filesystem boundaries in Rust
//! before any syscall is attempted. It is a belt-and-suspenders layer that
//! complements the kernel sandbox (Landlock on Linux, Seatbelt on macOS):
//! the kernel provides the hard OS barrier; `SandboxGuard` surfaces a clear,
//! actionable error message earlier in the call chain.
//!
//! When `sandbox.enabled = false`, both [`SandboxGuard::check_read`] and
//! [`SandboxGuard::check_write`] return `Ok(())` unconditionally so that
//! the guard imposes zero overhead in unsandboxed configurations.

use std::path::{Component, Path, PathBuf};

use crate::config::SandboxConfig;
use crate::error::SandboxError;

/// Application-level path guard enforcing the sandbox policy for file tools.
///
/// Constructed once per session from the runtime `SandboxConfig` and the
/// project root directory. The guard checks every file-tool path before the
/// tool performs any I/O, returning `SandboxError::PathDenied` if the path
/// falls outside the allowed locations.
///
/// # Examples
///
/// ```
/// use xzatoma::sandbox::SandboxGuard;
/// use xzatoma::config::SandboxConfig;
/// use std::path::PathBuf;
///
/// let mut cfg = SandboxConfig::default();
/// cfg.enabled = false;
/// let guard = SandboxGuard::new(cfg, PathBuf::from("/project"));
/// // When disabled, all checks pass unconditionally.
/// assert!(guard.check_read(std::path::Path::new("/etc/passwd")).is_ok());
/// assert!(guard.check_write(std::path::Path::new("/etc/passwd")).is_ok());
/// ```
#[derive(Debug, Clone)]
pub struct SandboxGuard {
    cfg: SandboxConfig,
    /// Resolved absolute filesystem root (`cfg.filesystem_root` or the
    /// project root supplied at construction time).
    root: PathBuf,
}

impl SandboxGuard {
    /// Construct a guard from config and the runtime project root.
    ///
    /// When `cfg.filesystem_root` is set it is used as the allowed root;
    /// otherwise `project_root` is used.
    ///
    /// # Arguments
    ///
    /// * `cfg` - Sandbox configuration, including `enabled`, path lists, and
    ///   the `protect_git` flag.
    /// * `project_root` - Fallback root when `cfg.filesystem_root` is `None`.
    ///
    /// # Returns
    ///
    /// A new `SandboxGuard` ready to validate paths.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::sandbox::SandboxGuard;
    /// use xzatoma::config::SandboxConfig;
    /// use std::path::PathBuf;
    ///
    /// let guard = SandboxGuard::new(SandboxConfig::default(), PathBuf::from("."));
    /// ```
    pub fn new(cfg: SandboxConfig, project_root: PathBuf) -> Self {
        let root = cfg
            .filesystem_root
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or(project_root);
        Self { cfg, root }
    }

    /// Verify that `path` is within an allowed read location.
    ///
    /// A path is allowed when it is a descendant of `self.root` or of any
    /// entry in `cfg.extra_read_paths`. When `cfg.enabled = false` this
    /// method always returns `Ok(())`. When the `"file_ops"` key in
    /// `cfg.sandboxed_tools` is set to `false` the guard also short-circuits
    /// unconditionally, matching the per-tool opt-out pattern used by
    /// `TerminalTool` for the `"terminal"` key.
    ///
    /// # Arguments
    ///
    /// * `path` - The filesystem path to validate (may be relative or
    ///   absolute; will be canonicalized or lexically normalized).
    ///
    /// # Returns
    ///
    /// `Ok(())` if the path is within an allowed read location.
    ///
    /// # Errors
    ///
    /// Returns [`SandboxError::PathDenied`] if `path` is outside all allowed
    /// read paths and sandboxing is enabled.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::sandbox::SandboxGuard;
    /// use xzatoma::config::SandboxConfig;
    /// use std::path::PathBuf;
    ///
    /// let mut cfg = SandboxConfig::default();
    /// cfg.enabled = false;
    /// let guard = SandboxGuard::new(cfg, PathBuf::from("/project"));
    /// assert!(guard.check_read(std::path::Path::new("/etc/shadow")).is_ok());
    /// ```
    pub fn check_read(&self, path: &Path) -> Result<(), SandboxError> {
        if !self.cfg.enabled {
            return Ok(());
        }

        // Per-tool opt-out: when the "file_ops" key is set to `false` in
        // sandboxed_tools the guard bypasses enforcement for file-operation
        // tools. This mirrors how TerminalTool respects the "terminal" key.
        if self.cfg.sandboxed_tools.get("file_ops") == Some(&false) {
            return Ok(());
        }

        let resolved = Self::resolve_path(path);
        let root = Self::resolve_path(&self.root);

        if resolved.starts_with(&root) {
            return Ok(());
        }

        for extra in &self.cfg.extra_read_paths {
            let extra_resolved = Self::resolve_path(Path::new(extra));
            if resolved.starts_with(&extra_resolved) {
                return Ok(());
            }
        }

        Err(SandboxError::PathDenied {
            path: path.display().to_string(),
            reason: "path is outside allowed read locations".to_string(),
        })
    }

    /// Verify that `path` is within an allowed write location and not inside
    /// a protected `.git` directory.
    ///
    /// A path is allowed when it is a descendant of `self.root` or of any
    /// entry in `cfg.extra_write_paths`, **and** does not contain a `.git`
    /// component when `cfg.protect_git = true`. The `.git` check is performed
    /// first so that an attempt to write inside `.git` is rejected even when
    /// the path would otherwise fall within `self.root`.
    ///
    /// When `cfg.enabled = false` this method always returns `Ok(())`. When
    /// the `"file_ops"` key in `cfg.sandboxed_tools` is set to `false` the
    /// guard also short-circuits unconditionally, matching the per-tool
    /// opt-out pattern used by `TerminalTool` for the `"terminal"` key.
    ///
    /// # Arguments
    ///
    /// * `path` - The filesystem path to validate.
    ///
    /// # Returns
    ///
    /// `Ok(())` if the path is within an allowed write location.
    ///
    /// # Errors
    ///
    /// Returns [`SandboxError::PathDenied`] if:
    /// - the path contains a `.git` component and `protect_git = true`, or
    /// - the path is outside all allowed write paths.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::sandbox::SandboxGuard;
    /// use xzatoma::config::SandboxConfig;
    /// use std::path::PathBuf;
    ///
    /// let mut cfg = SandboxConfig::default();
    /// cfg.enabled = false;
    /// let guard = SandboxGuard::new(cfg, PathBuf::from("/project"));
    /// assert!(guard.check_write(std::path::Path::new("/etc/passwd")).is_ok());
    /// ```
    pub fn check_write(&self, path: &Path) -> Result<(), SandboxError> {
        if !self.cfg.enabled {
            return Ok(());
        }

        // Per-tool opt-out: when the "file_ops" key is set to `false` in
        // sandboxed_tools the guard bypasses enforcement for file-operation
        // tools. This mirrors how TerminalTool respects the "terminal" key.
        if self.cfg.sandboxed_tools.get("file_ops") == Some(&false) {
            return Ok(());
        }

        let resolved = Self::resolve_path(path);

        // Protect .git: reject any path containing a .git component, even
        // when it is otherwise inside the allowed root.
        if self.cfg.protect_git {
            for component in resolved.components() {
                if let Component::Normal(name) = component
                    && name == ".git"
                {
                    return Err(SandboxError::PathDenied {
                        path: path.display().to_string(),
                        reason: "writes to .git directories are protected \
                                 (set sandbox.protect_git = false to allow)"
                            .to_string(),
                    });
                }
            }
        }

        let root = Self::resolve_path(&self.root);

        if resolved.starts_with(&root) {
            return Ok(());
        }

        for extra in &self.cfg.extra_write_paths {
            let extra_resolved = Self::resolve_path(Path::new(extra));
            if resolved.starts_with(&extra_resolved) {
                return Ok(());
            }
        }

        Err(SandboxError::PathDenied {
            path: path.display().to_string(),
            reason: "path is outside allowed write locations".to_string(),
        })
    }

    /// Resolve a path to its canonical absolute form, falling back to lexical
    /// normalization when the path does not yet exist on disk.
    fn resolve_path(path: &Path) -> PathBuf {
        // Prefer canonical resolution (follows symlinks, requires the path to
        // exist). Fall back to lexical normalization for not-yet-created paths
        // (e.g. a file about to be written for the first time).
        if let Ok(canonical) = path.canonicalize() {
            return canonical;
        }
        Self::lexical_absolute_normalize(path.to_path_buf())
    }

    /// Lexically normalize an absolute path (resolve `.` and `..`) without
    /// following symlinks.
    ///
    /// If `path` is relative it is first joined to the process working
    /// directory to make it absolute.
    fn lexical_absolute_normalize(mut path: PathBuf) -> PathBuf {
        if !path.is_absolute() {
            // SAFETY: current_dir() can only fail on Unix if the process cwd
            // has been deleted; fall back to "/" to avoid panicking.
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
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn guard_with_root(tmp: &TempDir) -> SandboxGuard {
        SandboxGuard::new(SandboxConfig::default(), tmp.path().to_path_buf())
    }

    // -----------------------------------------------------------------------
    // check_write
    // -----------------------------------------------------------------------

    #[test]
    fn test_sandbox_guard_check_write_allows_path_inside_root() {
        let tmp = TempDir::new().unwrap();
        // Create the target so canonicalize succeeds.
        let target = tmp.path().join("src/main.rs");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, "fn main() {}").unwrap();

        let guard = guard_with_root(&tmp);
        assert!(guard.check_write(&target).is_ok());
    }

    #[test]
    fn test_sandbox_guard_check_write_blocks_path_outside_root() {
        let tmp = TempDir::new().unwrap();
        let guard = guard_with_root(&tmp);

        // /tmp itself is a sibling of tmp's contents; it is NOT a descendant
        // of tmp. Use a stable path that cannot accidentally be inside `tmp`.
        let outside = PathBuf::from("/tmp/xzatoma_escape_test_check_write");
        let result = guard.check_write(&outside);
        assert!(result.is_err(), "expected PathDenied for path outside root");
        assert!(matches!(result, Err(SandboxError::PathDenied { .. })));
    }

    #[test]
    fn test_sandbox_guard_check_write_blocks_git_dir() {
        let tmp = TempDir::new().unwrap();
        // The target path does not need to exist; lexical normalization handles it.
        let git_path = tmp.path().join(".git/ORIG_HEAD");

        let cfg = SandboxConfig {
            protect_git: true,
            ..SandboxConfig::default()
        };
        let guard = SandboxGuard::new(cfg, tmp.path().to_path_buf());

        let result = guard.check_write(&git_path);
        assert!(result.is_err());
        assert!(matches!(result, Err(SandboxError::PathDenied { .. })));
    }

    #[test]
    fn test_sandbox_guard_check_write_allows_git_when_protect_false() {
        let tmp = TempDir::new().unwrap();
        // Create the .git dir so canonicalize succeeds (required for starts_with check).
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let git_path = git_dir.join("ORIG_HEAD");
        std::fs::write(&git_path, "").unwrap();

        let cfg = SandboxConfig {
            protect_git: false,
            ..SandboxConfig::default()
        };
        let guard = SandboxGuard::new(cfg, tmp.path().to_path_buf());

        assert!(guard.check_write(&git_path).is_ok());
    }

    // -----------------------------------------------------------------------
    // check_read
    // -----------------------------------------------------------------------

    #[test]
    fn test_sandbox_guard_check_read_allows_extra_read_path() {
        let tmp = TempDir::new().unwrap();
        let extra_dir = TempDir::new().unwrap();
        let extra_file = extra_dir.path().join("data.txt");
        std::fs::write(&extra_file, "data").unwrap();

        let cfg = SandboxConfig {
            extra_read_paths: vec![extra_dir.path().to_string_lossy().into_owned()],
            ..SandboxConfig::default()
        };
        let guard = SandboxGuard::new(cfg, tmp.path().to_path_buf());

        assert!(guard.check_read(&extra_file).is_ok());
    }

    #[test]
    fn test_sandbox_guard_check_read_blocks_outside_all_paths() {
        let tmp = TempDir::new().unwrap();
        let guard = guard_with_root(&tmp);

        let outside = PathBuf::from("/tmp/xzatoma_escape_test_check_read");
        let result = guard.check_read(&outside);
        assert!(result.is_err());
        assert!(matches!(result, Err(SandboxError::PathDenied { .. })));
    }

    // -----------------------------------------------------------------------
    // sandbox disabled
    // -----------------------------------------------------------------------

    #[test]
    fn test_sandbox_guard_file_ops_opt_out_bypasses_write_check() {
        let tmp = TempDir::new().unwrap();
        let outside = PathBuf::from("/tmp/xzatoma_guard_file_ops_opt_out_write");

        let mut cfg = SandboxConfig::default();
        cfg.sandboxed_tools.insert("file_ops".to_string(), false);
        let guard = SandboxGuard::new(cfg, tmp.path().to_path_buf());

        // Even though outside is not under tmp, file_ops opt-out short-circuits.
        assert!(
            guard.check_write(&outside).is_ok(),
            "file_ops=false must bypass check_write"
        );
    }

    #[test]
    fn test_sandbox_guard_file_ops_opt_out_bypasses_read_check() {
        let tmp = TempDir::new().unwrap();
        let outside = PathBuf::from("/tmp/xzatoma_guard_file_ops_opt_out_read");

        let mut cfg = SandboxConfig::default();
        cfg.sandboxed_tools.insert("file_ops".to_string(), false);
        let guard = SandboxGuard::new(cfg, tmp.path().to_path_buf());

        assert!(
            guard.check_read(&outside).is_ok(),
            "file_ops=false must bypass check_read"
        );
    }

    #[test]
    fn test_sandbox_guard_disabled_allows_any_write() {
        let tmp = TempDir::new().unwrap();
        let cfg = SandboxConfig {
            enabled: false,
            ..SandboxConfig::default()
        };
        let guard = SandboxGuard::new(cfg, tmp.path().to_path_buf());

        assert!(guard.check_write(Path::new("/etc/passwd")).is_ok());
    }

    #[test]
    fn test_sandbox_guard_disabled_allows_any_read() {
        let tmp = TempDir::new().unwrap();
        let cfg = SandboxConfig {
            enabled: false,
            ..SandboxConfig::default()
        };
        let guard = SandboxGuard::new(cfg, tmp.path().to_path_buf());

        assert!(guard.check_read(Path::new("/etc/shadow")).is_ok());
    }
}
