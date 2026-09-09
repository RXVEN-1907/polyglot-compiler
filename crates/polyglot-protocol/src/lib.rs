//! Polyglot Protocol - JSON-RPC 2.0 based communication between driver and plugins

pub mod handler;
pub mod transport;
pub mod types;

pub use handler::{Plugin, PluginServer};
pub use transport::{ServerTransport, StdioTransport};
pub use types::*;