use std::io;
use std::net::SocketAddr;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Errors raised while starting, querying or stopping the mock server.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MockError {
    /// The listener could not bind to the requested address.
    #[error("cannot bind the mock server to {addr}: {source}")]
    Bind {
        /// The requested address.
        addr: SocketAddr,
        /// The underlying I/O error.
        source: io::Error,
    },
    /// Serving or waiting for a signal failed.
    #[error("mock server I/O error: {0}")]
    Io(#[from] io::Error),
    /// The state actor or the server task has stopped.
    #[error("the mock server has stopped")]
    Stopped,
}

impl IntoResponse for MockError {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
    }
}
