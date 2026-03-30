//! XZepr watcher backend
//!
//! This module contains all XZepr-specific watcher code, consolidated from the
//! former top-level `src/xzepr/` and the XZepr-specific portions of the former
//! `src/watcher/` module, as part of the generic watcher architecture.
//!
//! XZepr is a fully supported, permanent watcher backend and an equal configuration
//! peer alongside the generic watcher. No deprecation notices apply.
//!
//! # Modules
//!
//! - [`consumer`]: XZepr Kafka consumer, HTTP API client, and CloudEvents message types
//! - [`filter`]: Event filtering for XZepr CloudEvents messages
//! - [`plan_extractor`]: Plan extraction strategies for XZepr CloudEvent payloads
//! - [`watcher`]: Core XZepr watcher service that wires consumer, filter, and extractor
//!
//! # Access
//!
//! XZepr-specific types are intentionally NOT re-exported at the top-level
//! `crate::watcher` module. They are exclusively accessible via
//! `crate::watcher::xzepr::*` (or through the backward-compatible
//! `crate::xzepr::*` re-exports in `src/xzepr/mod.rs`).
//!
//! The only top-level watcher re-export is:
//! `pub use xzepr::watcher::Watcher as XzeprWatcher`

pub mod consumer;
pub mod filter;
pub mod plan_extractor;
// The module is intentionally named `watcher` inside the `xzepr` package;
// the name mirrors the public type (`Watcher`) it exposes and matches the
// file layout convention used throughout this crate.
#[allow(clippy::module_inception)]
pub mod watcher;
