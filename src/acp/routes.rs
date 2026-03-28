/// ACP HTTP route construction wrappers.
///
/// This module provides a small transport-facing routing surface for Phase 2
/// ACP discovery endpoints. The concrete router implementation lives in
/// `crate::acp::server`, and this module re-exports the route builder and
/// related state types through a dedicated routes module so later phases can
/// evolve route composition without coupling callers directly to server
/// bootstrap internals.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::routes::{build_router, AcpPathStrategy, AcpServerState};
/// use xzatoma::Config;
///
/// let config = Config::default();
/// let state = AcpServerState::from_config(&config).unwrap();
/// let _router = build_router(state, &config.acp);
/// assert!(matches!(
///     AcpPathStrategy::from_config(&config.acp),
///     AcpPathStrategy::Versioned { .. }
/// ));
/// ```
pub use crate::acp::server::{build_router, AcpPathStrategy, AcpServerState};
