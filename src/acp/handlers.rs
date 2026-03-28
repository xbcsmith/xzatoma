/// ACP HTTP handler wrappers.
///
/// This module provides a small transport-facing wrapper layer for the ACP HTTP
/// surface. The concrete handler implementations live in `crate::acp::server`,
/// and this module re-exports them through a dedicated handlers surface so
/// later phases can evolve route wiring without coupling callers directly to
/// server bootstrap internals.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::handlers::{handle_get_session, handle_ping};
///
/// let _ = handle_ping;
/// let _ = handle_get_session;
/// ```
pub use crate::acp::server::{
    handle_agent_by_name, handle_agents, handle_cancel_run, handle_create_run, handle_get_run,
    handle_get_run_events, handle_get_session, handle_ping, handle_resume_run, AcpHttpError,
    AcpHttpErrorBody, AgentsListResponse, AgentsQuery, CancelRunRequestBody, CreateRunRequestBody,
    CreateRunResponseBody, PingResponse, ResumeRunRequestBody, RunEventsResponseBody,
    SessionResponseBody,
};
