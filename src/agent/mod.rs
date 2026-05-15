//! Agent module for XZatoma
//!
//! This module contains the core agent logic, including conversation management,
//! tool execution, and the main agent execution loop.

pub mod conversation;
pub mod core;
pub mod events;
pub mod metrics;
pub mod persistence;
pub mod quota;
pub(crate) mod thinking;
pub use thinking::extract_thinking;

pub use conversation::{ContextInfo, ContextStatus, Conversation};
pub use core::Agent;
pub use events::{AgentExecutionEvent, AgentObserver, NoOpObserver};
pub use metrics::{init_metrics_exporter, SubagentMetrics};
pub use persistence::{
    new_conversation_id, now_rfc3339, ConversationMetadata, ConversationRecord, ConversationStore,
};
pub use quota::{QuotaLimits, QuotaTracker, QuotaUsage};
