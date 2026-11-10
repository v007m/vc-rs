//! The web server `vc ui` runs (D101): the engine API over the Connect protocol, for the web UI.
//!
//! [`ConnectService`] serves one protobuf service's methods at `POST /api/<service>/<method>`:
//! unary calls as `application/proto` and server streams as `application/connect+proto`
//! envelopes, ending with an end-stream message. A failed call is a [`ConnectError`], sent as
//! Connect error JSON. Bodies are binary protobuf only; the JSON codec waits on pbjson.
//!
//! [`ServerBuilder`] puts the services, the mounts (MCP over HTTP) and the web UI's [`Files`]
//! behind one guard and listens on `127.0.0.1` only. Every request needs the process's random
//! token, which only the [`LaunchUrl`] shows: its `GET /?token=…` sets an `HttpOnly`,
//! `SameSite=Strict` cookie and redirects to `/`; other clients send `Authorization: Bearer`.
//! `Host` must name the server, a write from another site's page is refused, there is no CORS,
//! and the CSP is built here from the page's script hashes alone.
//!
//! [`EngineApi`] is `vibranium.engine.v1.EngineService` over one engine: each RPC a thin
//! adapter over one engine facade method, and `Run` the run's events as they happen.

mod authoring_rpcs;
mod builder;
mod code;
mod csp;
#[cfg(feature = "ui")]
mod embedded;
mod engine_api;
#[cfg(test)]
mod engine_fixture;
mod envelope;
mod error;
#[cfg(test)]
mod fixture;
mod guard;
mod history_rpcs;
mod project_rpcs;
mod run_stream;
mod service;
mod site;
mod token;
mod wire_diagnostics;
mod wire_events;
mod wire_history;
mod wire_results;

pub use builder::{LaunchUrl, Server, ServerBuilder};
pub use code::Code;
#[cfg(feature = "ui")]
pub use embedded::EmbeddedUi;
pub use engine_api::EngineApi;
pub use error::{ConnectError, ServerError};
pub use service::{ConnectService, Sink};
pub use site::Files;
