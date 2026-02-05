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
pub mod persistence;

pub use conversation::{ContextInfo, Conversation};
pub use core::Agent;
pub use persistence::{
    new_conversation_id, now_rfc3339, ConversationMetadata, ConversationRecord, ConversationStore,
};
