//! MCP OAuth 2.1 / OIDC authorization
//!
//! This module implements the full authorization flow required by the MCP
//! `2025-11-25` specification for HTTP transport connections.
//!
//! Authorization applies only to HTTP transport; stdio servers obtain
//! credentials from environment variables per the MCP specification.
//!
//! # Module Layout
//!
//! - [`discovery`]   -- RFC 9728 protected resource metadata and OIDC
//!   discovery
//! - [`flow`]        -- OAuth 2.1 authorization code flow with PKCE
//! - [`manager`]     -- High-level auth manager coordinating all sub-modules
//! - [`pkce`]        -- PKCE `S256` challenge generation and verification
//! - [`token_store`] -- Secure token persistence via OS keyring

pub mod discovery;
pub mod flow;
pub mod manager;
pub mod pkce;
pub mod token_store;
