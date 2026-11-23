//! Mock HTTP server for end-to-end tests and benchmarks.
//!
//! Tests start one server per test with [`MockServer::start`] (port 0) and read its
//! request counters through [`MockHandle::stats`] or `GET /__stats`. [`MockServer::start_tls`]
//! serves the same routes over https, with client certificate checks, plus `GET /whoami`.
//! `ANY /aws/…` checks an AWS Signature Version 4 made with [`AWS_ACCESS_KEY_ID`] and
//! [`AWS_SECRET_ACCESS_KEY`].

mod error;
mod routes;
mod server;
mod state;
mod stats;
mod store;
mod tls;

pub use error::MockError;
pub use routes::{AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY};
pub use server::{MockHandle, MockServer};
pub use stats::Stats;
pub use tls::{ClientAuth, MockTls};
