//! The docs playground's composition root (D293): `vc check` and `vc run` over a project held in
//! memory, built for `wasm32-wasip1` and run in a Web Worker.
//!
//! It wires the same core as `vc-engine` (the compiler, the planner, the executor, the auth types
//! and the spec reporter) to ports that need no host: the files the page sends
//! (`vc_interfaces::MemorySources`), the demo service called in process ([`InProcessTransport`]), the system clock
//! (`vc_adapter_system::SystemClock`, which WASI answers) and an empty environment. [`execute`] prints what the CLI prints for the
//! same command, through a [`Sink`]. What needs a host is refused with a message, never a panic:
//! hooks (V5115), programs other than `vc demo` and `echo`, plugins, secrets, the history, the cache, alerts, report
//! files, the browser sign-in and every host but the demo's.

mod bodies;
mod check;
mod commands;
mod compile;
mod console;
mod environment;
mod error;
mod playground;
mod prepare;
mod print;
mod request;
mod run;
mod sink;
#[cfg(test)]
mod testing;
mod transport;
mod unsupported;

pub use commands::PageCommands;
pub use environment::PageEnvironment;
pub use error::{BrowserError, Checked};
pub use playground::execute;
pub use request::{Command, Request};
pub use sink::{Sink, Stream};
pub use transport::{DEMO_HOSTS, InProcessTransport};
