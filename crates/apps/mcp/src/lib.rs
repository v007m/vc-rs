//! The MCP server for AI agents, as `vc mcp` serves it over stdio and, with the `http` feature,
//! `vc ui` at `/mcp` (D197).
//!
//! [`VcServer`] answers `tools/list` and `tools/call` from the [`Tools`] registry, each tool one
//! engine method: `list_definitions`, `search`, `get_definition`, `check`, `query_history`
//! and `explain_failure` only read; `run_tests`, `call_api` and `write_definition` act and are
//! gated by the profiles' `agents` rules in the engine (D195). An error or refusal comes back as
//! a tool result with `isError` set and `{kind, reason, help}` in its structured content.

mod act;
mod args;
mod error;
#[cfg(test)]
mod fixture;
mod handshake;
mod history;
#[cfg(feature = "http")]
mod http;
mod read;
mod schema;
mod server;
mod tools;

pub use args::Args;
pub use error::McpError;
#[cfg(feature = "http")]
pub use http::HttpService;
pub use server::VcServer;
pub use tools::{Cx, Tool, Tools};
