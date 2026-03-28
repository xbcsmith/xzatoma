/// ACP HTTP handler wrappers.
///
/// This module provides a small transport-facing wrapper layer for Phase 2 ACP
/// discovery endpoints. The concrete handler implementations live in
/// `crate::acp::server`, and this module re-exports them through a dedicated
/// handlers surface so later phases can evolve route wiring without coupling
/// callers directly to server bootstrap internals.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::handlers::handle_ping;
///
/// let _ = handle_ping;
/// ```
pub use crate::acp::server::{
    handle_agent_by_name, handle_agents, handle_create_run, handle_get_run, handle_get_run_events,
    handle_ping, AcpHttpError, AcpHttpErrorBody, AgentsListResponse, AgentsQuery,
    CreateRunRequestBody, CreateRunResponseBody, PingResponse, RunEventsResponseBody,
};
