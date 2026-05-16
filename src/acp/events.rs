/// ACP event models.
///
/// This module re-exports the canonical ACP event domain types from
/// `crate::acp::types` so the ACP surface stays coherent across the crate.
/// `types.rs` is the single source of truth for protocol-facing ACP events,
/// event kinds, and timestamp helpers.
///
/// Keeping this module as a thin compatibility layer allows the rest of the
/// ACP module tree to retain a responsibility-based layout without maintaining
/// duplicate event implementations.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::events::{AcpEvent, AcpEventKind};
///
/// let event = AcpEvent::new(
///     AcpEventKind::SessionCreated,
///     None,
///     serde_json::json!({ "session": "session_1" }),
/// )
/// .unwrap();
///
/// assert_eq!(event.kind, AcpEventKind::SessionCreated);
/// ```
pub use crate::acp::types::{now_rfc3339, AcpEvent, AcpEventKind};
