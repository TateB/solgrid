//! solgrid LSP server — Language Server Protocol integration.
//!
//! Provides real-time linting, code actions, formatting, hover documentation,
//! and suppression comment completion for Solidity files via the LSP protocol.

pub mod actions;
pub mod builtins;
pub mod completion;
pub mod convert;
pub mod definition;
pub mod diagnostics;
pub mod document;
pub mod format;
pub mod hover;
pub mod resolve;
pub mod server;
pub mod symbols;

pub use server::{run_server, SolgridServer};
