//! Plugins: on disk, and running.
//!
//! `vc plugin add` fetches one from a `path:` folder or an `npm:` package into a staging folder,
//! and once its permissions are approved it moves to `.vibranium/plugins/<id>/`.
//! [`FsPluginStore`] is the `vc_interfaces::PluginStore` the engine uses. A folder's integrity is
//! a BLAKE3 hash over its files; an npm package's is the one npm reports. Programs (`npm`, `tar`)
//! run with their arguments as a list, never through a shell.
//!
//! A run talks to each plugin it uses through a [`StdioPluginHost`], the
//! `vc_interfaces::PluginHost` that starts the plugin's process on first use and speaks JSON-RPC
//! 2.0 to it over stdin and stdout, each message framed by a `Content-Length` header as in LSP
//! (`docs/design/plugin-protocol.md`).

mod folder;
mod framing;
mod host;
mod npm;
mod rpc;
mod session;
mod store;

pub use host::{DATA_ENV, PROTOCOL_VERSION, StdioPluginHost};
pub use store::FsPluginStore;
