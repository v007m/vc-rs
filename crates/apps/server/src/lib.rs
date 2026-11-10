//! The web server `vc ui` runs (D101): the engine API over the Connect protocol, for the web UI.
//!
//! [`ConnectService`] serves one protobuf service's methods at `POST /api/<service>/<method>`:
//! unary calls as `application/proto` and server streams as `application/connect+proto`
//! envelopes, ending with an end-stream message. A failed call is a [`ConnectError`], sent as
//! Connect error JSON. Bodies are binary protobuf only; the JSON codec waits on pbjson.

mod code;
mod envelope;
mod error;
mod service;

pub use code::Code;
pub use error::ConnectError;
pub use service::{ConnectService, Sink};
