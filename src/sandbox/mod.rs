//! Kernel-enforced sandboxing for XZatoma tool execution.
//!
//! This module provides OS-level process isolation using Landlock (Linux) and
//! Seatbelt (macOS) via the `nono` crate.
//!
//! - Phase 1: error types, config schema, and module skeleton.
//! - Phase 2: `SandboxPolicy` builder, `SandboxContext`, and `apply_sandbox` engine.
//! - Phase 3: `TerminalTool` and `CommandValidator` integration.
//! - Phase 4: host-filtering HTTP proxy and `FetchTool` hardening.
//! - Phase 5: `SandboxGuard` for file-operation tools.

pub mod engine;
pub mod guard;
pub mod policy;
pub mod proxy;

pub use engine::apply_sandbox;
pub use guard::SandboxGuard;
pub use policy::{SandboxContext, SandboxPolicy};

pub use crate::error::SandboxError;
