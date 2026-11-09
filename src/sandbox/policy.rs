// TODO: Phase 2 - implement SandboxPolicy and SandboxContext

/// Sandbox access-control policy (stub; implementation in Phase 2).
///
/// In Phase 2 this will hold allow/deny lists for filesystem paths and
/// the network enable flag derived from `SandboxConfig`.
pub struct SandboxPolicy;

/// Token proving the sandbox has been applied to the current process (stub; implementation in Phase 2).
///
/// In Phase 2 this will be a zero-sized token returned by `apply_sandbox`
/// that prevents callers from bypassing the sandbox.
pub struct SandboxContext {
    _private: (),
}
