// TODO: Phase 2 - implement apply_sandbox for Linux (Landlock) and macOS (Seatbelt)

use crate::error::SandboxError;

/// Apply the kernel-enforced sandbox policy to the current process (stub; implementation in Phase 2).
///
/// In Phase 2 this will apply Landlock (Linux) or Seatbelt/sandbox_init
/// (macOS) based on the provided `SandboxPolicy`. The call is irreversible
/// for the lifetime of the process.
///
/// # Errors
///
/// Returns `SandboxError` if the kernel sandbox cannot be applied.
pub fn apply_sandbox() -> Result<(), SandboxError> {
    Ok(())
}
