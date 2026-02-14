//! Agent module for XZatoma
//!
//! This module contains the core agent logic, including conversation management,
//! tool execution, and the main agent execution loop.

// Phase 1: Allow unused code for placeholder implementations
#![allow(dead_code)]
#![allow(unused_imports)]

pub mod conversation;
pub mod core;
pub mod executor;
pub mod metrics;
pub mod persistence;
pub mod quota;

pub use conversation::{ContextInfo, ContextStatus, Conversation};
pub use core::Agent;
pub use metrics::{init_metrics_exporter, SubagentMetrics};
pub use persistence::{
    new_conversation_id, now_rfc3339, ConversationMetadata, ConversationRecord, ConversationStore,
};
pub use quota::{QuotaLimits, QuotaTracker, QuotaUsage};
