//! Mock HTTP server for end-to-end tests and benchmarks.
//!
//! Tests start one server per test with [`MockServer::start`] (port 0) and read its
//! request counters through [`MockHandle::stats`] or `GET /__stats`.

mod error;
mod routes;
mod server;
mod state;
mod stats;
mod store;

pub use error::MockError;
pub use server::{MockHandle, MockServer};
pub use stats::Stats;
