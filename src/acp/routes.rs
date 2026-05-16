/// ACP HTTP route construction wrappers.
///
/// This module provides a small transport-facing routing surface for ACP
/// discovery endpoints. The concrete router implementation lives in
/// `crate::acp::server`, and this module re-exports the route builder and
/// related state types so route composition can evolve without coupling
/// callers directly to server bootstrap internals.
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
