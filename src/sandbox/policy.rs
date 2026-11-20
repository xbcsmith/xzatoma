//! Sandbox access-control policy for XZatoma tool execution.
//!
//! This module defines [`SandboxPolicy`], a builder that accumulates
//! filesystem and network access rules, and [`SandboxContext`], the
//! zero-sized proof type returned when the policy has been applied.

use std::path::{Path, PathBuf};

use crate::config::SandboxConfig;
use crate::error::SandboxError;

/// Builder that accumulates filesystem and network access rules before
/// applying them to a child process via [`crate::sandbox::apply_sandbox`].
///
/// `SandboxPolicy` is `Send + Sync` so it can be wrapped in `Arc` and
/// referenced from a `pre_exec` closure.
///
/// # Examples
///
/// ```
/// use xzatoma::sandbox::policy::SandboxPolicy;
///
/// let policy = SandboxPolicy::new()
///     .allow_read("/tmp")
///     .allow_write("/tmp/output")
///     .with_network(false);
/// ```
#[derive(Debug, Clone)]
pub struct SandboxPolicy {
    /// Paths for which read access is granted.
    pub(crate) allow_read_paths: Vec<PathBuf>,
    /// Paths for which write (and read) access is granted.
    pub(crate) allow_write_paths: Vec<PathBuf>,
    /// Paths for which write access is explicitly denied even if a parent grants it.
    pub(crate) deny_write_paths: Vec<PathBuf>,
    /// Paths for which execute access is granted.
    pub(crate) allow_execute_paths: Vec<PathBuf>,
    /// When `true`, outbound network access is permitted.
    pub(crate) allow_network: bool,
}

impl SandboxPolicy {
    /// Create a new empty policy with no path grants and network disabled.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::sandbox::policy::SandboxPolicy;
    ///
    /// let policy = SandboxPolicy::new();
    /// // All path lists are empty and network access is denied.
    /// ```
    pub fn new() -> Self {
        Self {
            allow_read_paths: Vec::new(),
            allow_write_paths: Vec::new(),
            deny_write_paths: Vec::new(),
            allow_execute_paths: Vec::new(),
            allow_network: false,
        }
    }

    /// Grant read access to a path and its subtree (builder pattern).
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::sandbox::policy::SandboxPolicy;
    ///
    /// let policy = SandboxPolicy::new().allow_read("/usr/share");
    /// ```
    pub fn allow_read(mut self, path: impl Into<PathBuf>) -> Self {
        self.allow_read_paths.push(path.into());
        self
    }

    /// Grant write (and read) access to a path and its subtree (builder pattern).
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::sandbox::policy::SandboxPolicy;
    ///
    /// let policy = SandboxPolicy::new().allow_write("/tmp/agent");
    /// ```
    pub fn allow_write(mut self, path: impl Into<PathBuf>) -> Self {
        self.allow_write_paths.push(path.into());
        self
    }

    /// Narrow-deny write access to a path, overriding a broader `allow_write` (builder pattern).
    ///
    /// On macOS, this injects a Seatbelt `(deny file-write* (subpath "..."))` rule.
    /// On Linux, the deny is enforced at the application layer by `SandboxGuard` (Phase 5).
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::sandbox::policy::SandboxPolicy;
    ///
    /// let policy = SandboxPolicy::new()
    ///     .allow_write("/project")
    ///     .deny_write("/project/.git");
    /// ```
    pub fn deny_write(mut self, path: impl Into<PathBuf>) -> Self {
        self.deny_write_paths.push(path.into());
        self
    }

    /// Grant execute rights to a path and its subtree (builder pattern).
    ///
    /// Maps to `AccessMode::Read` in the `nono` crate; the OS enforces the
    /// execute bit independently of the sandbox read grant.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::sandbox::policy::SandboxPolicy;
    ///
    /// let policy = SandboxPolicy::new().allow_execute("/usr/bin");
    /// ```
    pub fn allow_execute(mut self, path: impl Into<PathBuf>) -> Self {
        self.allow_execute_paths.push(path.into());
        self
    }

    /// Set network access for this policy (builder pattern).
    ///
    /// `true` = allow outbound network, `false` = deny (default).
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::sandbox::policy::SandboxPolicy;
    ///
    /// let policy = SandboxPolicy::new().with_network(false);
    /// ```
    pub fn with_network(mut self, allow: bool) -> Self {
        self.allow_network = allow;
        self
    }

    /// Build a `SandboxPolicy` from a [`SandboxConfig`] and a project root.
    ///
    /// Paths are resolved in this order:
    ///
    /// 1. `filesystem_root` (or `project_root` when `None`) -- read + write.
    /// 2. Each entry in `extra_read_paths` -- read only.
    /// 3. Each entry in `extra_write_paths` -- write (implies read).
    /// 4. `.git` subdirectories under `filesystem_root` when `protect_git = true` -- deny write.
    /// 5. `std::env::temp_dir()` -- read + write (always added).
    /// 6. Platform system paths required for dynamic linking -- read + execute.
    ///
    /// ## System paths added unconditionally
    ///
    /// Linux: `/bin`, `/usr/bin`, `/usr/local/bin`, `/lib`, `/lib64`,
    /// `/usr/lib`, `/usr/local/lib`, `/etc/ld.so.cache`, `/etc/ld.so.conf`,
    /// `/etc/ld.so.conf.d`, `/proc/self`, `/dev/null`, `/dev/urandom`
    ///
    /// macOS: `/bin`, `/usr/bin`, `/usr/local/bin`, `/usr/lib`,
    /// `/System/Library`, `/Library/Frameworks`, `/private/var/db/dyld`,
    /// `/usr/share`
    ///
    /// # Errors
    ///
    /// Returns `SandboxError::PolicyPathError` if an `extra_read_paths` or
    /// `extra_write_paths` entry is an empty string.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::SandboxConfig;
    /// use xzatoma::sandbox::policy::SandboxPolicy;
    ///
    /// let cfg = SandboxConfig::default();
    /// let root = std::env::temp_dir();
    /// let policy = SandboxPolicy::from_config(&cfg, &root);
    /// assert!(policy.is_ok());
    /// ```
    pub fn from_config(
        cfg: &SandboxConfig,
        project_root: &Path,
    ) -> Result<SandboxPolicy, SandboxError> {
        let root: PathBuf = cfg
            .filesystem_root
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| project_root.to_path_buf());

        let mut policy = SandboxPolicy::new()
            .allow_read(root.clone())
            .allow_write(root.clone());

        for entry in &cfg.extra_read_paths {
            if entry.is_empty() {
                return Err(SandboxError::PolicyPathError {
                    path: entry.clone(),
                    reason: "extra_read_paths entry is empty".to_string(),
                });
            }
            policy = policy.allow_read(PathBuf::from(entry));
        }

        for entry in &cfg.extra_write_paths {
            if entry.is_empty() {
                return Err(SandboxError::PolicyPathError {
                    path: entry.clone(),
                    reason: "extra_write_paths entry is empty".to_string(),
                });
            }
            policy = policy
                .allow_read(PathBuf::from(entry))
                .allow_write(PathBuf::from(entry));
        }

        if cfg.protect_git {
            for git_dir in find_git_dirs(&root, 4) {
                policy = policy.deny_write(git_dir);
            }
        }

        let tmpdir = std::env::temp_dir();
        policy = policy.allow_read(tmpdir.clone()).allow_write(tmpdir);

        for sys_path in system_paths() {
            policy = policy.allow_read(sys_path.clone()).allow_execute(sys_path);
        }

        if cfg.network_enabled {
            policy = policy.with_network(true);
        }

        Ok(policy)
    }
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self::new()
    }
}

/// Zero-sized type returned by a successful [`crate::sandbox::apply_sandbox`] call.
///
/// Holding a `SandboxContext` proves that the sandbox was applied in the
/// current process; it carries no runtime data.
#[derive(Debug)]
pub struct SandboxContext {
    _private: (),
}

impl SandboxContext {
    /// Construct the proof token.
    ///
    /// This is only callable from [`crate::sandbox::engine::apply_sandbox`].
    pub(crate) fn applied() -> Self {
        Self { _private: () }
    }
}

/// Find all `.git` directories under `root` up to `max_depth` levels deep.
fn find_git_dirs(root: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if !root.exists() {
        return dirs;
    }
    let walker = walkdir::WalkDir::new(root)
        .max_depth(max_depth)
        .follow_links(false)
        .into_iter();
    for entry in walker.filter_map(|e| e.ok()) {
        if entry.file_type().is_dir() && entry.file_name() == ".git" {
            dirs.push(entry.into_path());
        }
    }
    dirs
}

/// Returns the platform-specific system paths needed for executable and
/// library access.
///
/// These paths are added unconditionally by [`SandboxPolicy::from_config`]
/// to ensure that dynamically linked binaries and system tools can execute
/// inside the sandbox.
fn system_paths() -> Vec<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        [
            "/bin",
            "/usr/bin",
            "/usr/local/bin",
            "/lib",
            "/lib64",
            "/usr/lib",
            "/usr/local/lib",
            "/etc/ld.so.cache",
            "/etc/ld.so.conf",
            "/etc/ld.so.conf.d",
            "/proc/self",
            "/dev/null",
            "/dev/urandom",
        ]
        .iter()
        .map(PathBuf::from)
        .collect()
    }
    #[cfg(target_os = "macos")]
    {
        [
            "/bin",
            "/usr/bin",
            "/usr/local/bin",
            "/usr/lib",
            "/System/Library",
            "/Library/Frameworks",
            "/private/var/db/dyld",
            "/usr/share",
        ]
        .iter()
        .map(PathBuf::from)
        .collect()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_new_has_no_paths_and_no_network() {
        let policy = SandboxPolicy::new();
        assert!(policy.allow_read_paths.is_empty());
        assert!(policy.allow_write_paths.is_empty());
        assert!(policy.deny_write_paths.is_empty());
        assert!(policy.allow_execute_paths.is_empty());
        assert!(!policy.allow_network);
    }

    #[test]
    fn test_policy_from_config_adds_root_read_and_write() {
        let tmp = tempfile::TempDir::new()
            // SAFETY: TempDir::new() only fails on OS-level errors.
            .expect("failed to create temp dir");
        let cfg = crate::config::SandboxConfig::default();
        let policy =
            SandboxPolicy::from_config(&cfg, tmp.path()).expect("from_config should succeed");
        assert!(
            policy.allow_read_paths.contains(&tmp.path().to_path_buf()),
            "root must be in allow_read_paths"
        );
        assert!(
            policy.allow_write_paths.contains(&tmp.path().to_path_buf()),
            "root must be in allow_write_paths"
        );
    }

    #[test]
    fn test_policy_from_config_adds_tmpdir() {
        let tmp = tempfile::TempDir::new().expect("failed to create temp dir");
        let cfg = crate::config::SandboxConfig::default();
        let policy =
            SandboxPolicy::from_config(&cfg, tmp.path()).expect("from_config should succeed");
        let tmpdir = std::env::temp_dir();
        assert!(
            policy.allow_read_paths.contains(&tmpdir),
            "temp_dir must be in allow_read_paths"
        );
        assert!(
            policy.allow_write_paths.contains(&tmpdir),
            "temp_dir must be in allow_write_paths"
        );
    }

    #[test]
    fn test_policy_from_config_protect_git_adds_deny_write() {
        let tmp = tempfile::TempDir::new().expect("failed to create temp dir");
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir(&git_dir).expect("failed to create .git dir");

        let cfg = crate::config::SandboxConfig {
            protect_git: true,
            ..crate::config::SandboxConfig::default()
        };

        let policy =
            SandboxPolicy::from_config(&cfg, tmp.path()).expect("from_config should succeed");
        assert!(
            policy.deny_write_paths.contains(&git_dir),
            ".git must be in deny_write_paths"
        );
    }

    #[test]
    fn test_policy_from_config_extra_read_paths_added() {
        let tmp = tempfile::TempDir::new().expect("failed to create temp dir");
        let extra = tmp.path().join("extra_read");
        std::fs::create_dir(&extra).expect("failed to create extra dir");

        let cfg = crate::config::SandboxConfig {
            extra_read_paths: vec![extra.to_string_lossy().into_owned()],
            ..crate::config::SandboxConfig::default()
        };

        let policy =
            SandboxPolicy::from_config(&cfg, tmp.path()).expect("from_config should succeed");
        assert!(
            policy.allow_read_paths.contains(&extra),
            "extra read path must be in allow_read_paths"
        );
    }

    #[test]
    fn test_policy_from_config_extra_write_paths_added() {
        let tmp = tempfile::TempDir::new().expect("failed to create temp dir");
        let extra = tmp.path().join("extra_write");
        std::fs::create_dir(&extra).expect("failed to create extra dir");

        let cfg = crate::config::SandboxConfig {
            extra_write_paths: vec![extra.to_string_lossy().into_owned()],
            ..crate::config::SandboxConfig::default()
        };

        let policy =
            SandboxPolicy::from_config(&cfg, tmp.path()).expect("from_config should succeed");
        assert!(
            policy.allow_read_paths.contains(&extra),
            "extra write path must also appear in allow_read_paths"
        );
        assert!(
            policy.allow_write_paths.contains(&extra),
            "extra write path must be in allow_write_paths"
        );
    }

    #[test]
    fn test_policy_from_config_network_enabled_sets_flag() {
        let tmp = tempfile::TempDir::new().expect("failed to create temp dir");
        let cfg = crate::config::SandboxConfig {
            network_enabled: true,
            ..crate::config::SandboxConfig::default()
        };

        let policy =
            SandboxPolicy::from_config(&cfg, tmp.path()).expect("from_config should succeed");
        assert!(policy.allow_network, "allow_network must be true");
    }

    #[test]
    fn test_policy_from_config_network_disabled_clears_flag() {
        let tmp = tempfile::TempDir::new().expect("failed to create temp dir");
        let cfg = crate::config::SandboxConfig {
            network_enabled: false,
            ..crate::config::SandboxConfig::default()
        };

        let policy =
            SandboxPolicy::from_config(&cfg, tmp.path()).expect("from_config should succeed");
        assert!(!policy.allow_network, "allow_network must be false");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_policy_from_config_adds_linux_system_paths() {
        let tmp = tempfile::TempDir::new().expect("failed to create temp dir");
        let cfg = crate::config::SandboxConfig::default();
        let policy =
            SandboxPolicy::from_config(&cfg, tmp.path()).expect("from_config should succeed");
        assert!(
            policy.allow_read_paths.contains(&PathBuf::from("/bin")),
            "/bin must be in allow_read_paths on Linux"
        );
        assert!(
            policy.allow_read_paths.contains(&PathBuf::from("/usr/lib")),
            "/usr/lib must be in allow_read_paths on Linux"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_policy_from_config_adds_macos_system_paths() {
        let tmp = tempfile::TempDir::new().expect("failed to create temp dir");
        let cfg = crate::config::SandboxConfig::default();
        let policy =
            SandboxPolicy::from_config(&cfg, tmp.path()).expect("from_config should succeed");
        assert!(
            policy
                .allow_read_paths
                .contains(&PathBuf::from("/System/Library")),
            "/System/Library must be in allow_read_paths on macOS"
        );
    }

    #[test]
    fn test_sandbox_context_applied_is_debug() {
        let ctx = SandboxContext::applied();
        let debug_str = format!("{ctx:?}");
        assert!(debug_str.contains("SandboxContext"));
    }

    #[test]
    fn test_policy_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SandboxPolicy>();
    }
}
