//! Sandbox engine: applies a [`SandboxPolicy`] to the current process.
//!
//! This module uses the `nono` crate to apply OS-level isolation.
//! On Linux it uses Landlock; on macOS it uses Seatbelt (`sandbox_init`).
//! The `apply_sandbox` call is irreversible for the lifetime of the process.

use crate::error::SandboxError;
use crate::sandbox::policy::{SandboxContext, SandboxPolicy};
use nono::{AccessMode, CapabilitySet, Sandbox};
use std::path::Path;

/// Apply the kernel-enforced sandbox policy to the current process.
///
/// This function is intended to be called inside a `pre_exec` closure after
/// `fork` and before `exec`. Once called, the restrictions cannot be removed.
///
/// On Linux, `nono` uses Landlock (kernel 5.13+). On macOS, `nono` uses
/// Seatbelt (`sandbox_init`).
///
/// # Errors
///
/// - `SandboxError::KernelVersionTooOld` on Linux when Landlock ABI v1 is
///   not available (kernel too old or Landlock not compiled in).
/// - `SandboxError::SeatbeltApplyFailed` on macOS when `sandbox_init` fails.
/// - `SandboxError::Engine` for other internal failures.
pub fn apply_sandbox(policy: &SandboxPolicy) -> Result<SandboxContext, SandboxError> {
    // On Linux, verify that Landlock ABI v1 or higher is available.
    #[cfg(target_os = "linux")]
    nono::detect_abi().map_err(|_| SandboxError::KernelVersionTooOld { found: 0 })?;

    let caps = build_capability_set(policy)?;

    // Apply the sandbox -- this call is irreversible for the current process.
    Sandbox::apply_auto(&caps).map_err(map_nono_error)?;

    Ok(SandboxContext::applied())
}

/// Convert a [`SandboxPolicy`] into a `nono::CapabilitySet`.
///
/// Paths that do not exist at call time are silently skipped with a trace
/// log entry. Critical paths should be verified before calling
/// [`apply_sandbox`].
fn build_capability_set(policy: &SandboxPolicy) -> Result<CapabilitySet, SandboxError> {
    let mut caps = CapabilitySet::new();

    for path in &policy.allow_read_paths {
        caps = try_allow_any(caps, path, AccessMode::Read);
    }

    for path in &policy.allow_write_paths {
        caps = try_allow_any(caps, path, AccessMode::ReadWrite);
    }

    // nono has no Execute AccessMode; Read is sufficient for binary execution
    // since the OS enforces the execute bit independently of the sandbox grant.
    for path in &policy.allow_execute_paths {
        caps = try_allow_any(caps, path, AccessMode::Read);
    }

    // On macOS, inject Seatbelt S-expression deny rules for write-protected paths.
    // On Linux, Landlock is purely allowlist-based; these paths are enforced
    // at the application layer by SandboxGuard (Phase 5).
    #[cfg(target_os = "macos")]
    {
        for path in &policy.deny_write_paths {
            let abs = absolute_path(path);
            let rule = format!("(deny file-write* (subpath \"{}\"))", abs.to_string_lossy());
            caps = caps.platform_rule(rule).map_err(|e| {
                SandboxError::Engine(format!(
                    "Seatbelt deny rule failed for {}: {}",
                    path.display(),
                    e
                ))
            })?;
        }
    }

    if !policy.allow_network {
        caps = caps.block_network();
    }

    Ok(caps)
}

/// Try to add a path grant to the capability set; skip gracefully on failure.
///
/// Uses `allow_path` for directories and `allow_file` for regular files.
/// Paths that do not exist, or that are special files (device nodes, sockets,
/// named pipes), are skipped with a trace log and the capability set is
/// returned unchanged.
fn try_allow_any(caps: CapabilitySet, path: &Path, mode: AccessMode) -> CapabilitySet {
    if !path.exists() {
        tracing::trace!("sandbox: skipping non-existent path: {}", path.display());
        return caps;
    }

    if !path.is_dir() && !path.is_file() {
        // Device files, sockets, named pipes -- not handled by nono's allow_path/allow_file.
        tracing::trace!(
            "sandbox: skipping special file (not a directory or regular file): {}",
            path.display()
        );
        return caps;
    }

    // Clone before moving `caps` so we can recover on failure.
    let backup = caps.clone();
    let result = if path.is_dir() {
        caps.allow_path(path, mode)
    } else {
        caps.allow_file(path, mode)
    };

    match result {
        Ok(new_caps) => new_caps,
        Err(e) => {
            tracing::warn!(
                "sandbox: could not grant {:?} access to {}: {}",
                mode,
                path.display(),
                e
            );
            backup
        }
    }
}

/// Return an absolute version of `path`, lexically normalizing relative inputs.
fn absolute_path(path: &Path) -> std::path::PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        // SAFETY: current_dir() only fails if the CWD was deleted after process
        // start; falling back to "/" produces an absolute path in all cases.
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));
        cwd.join(path)
    }
}

/// Map a `nono::NonoError` to the appropriate `SandboxError` variant.
fn map_nono_error(e: nono::NonoError) -> SandboxError {
    let msg = e.to_string();
    match &e {
        #[cfg(target_os = "macos")]
        nono::NonoError::SandboxInit(_) => SandboxError::SeatbeltApplyFailed { message: msg },
        nono::NonoError::UnsupportedPlatform(_) => SandboxError::KernelVersionTooOld { found: 0 },
        _ => SandboxError::Engine(msg),
    }
}
