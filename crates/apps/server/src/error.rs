//! A failed call, as Connect error JSON: the body of a unary error, or the `error` of the
//! end-stream message that closes a server stream.

use std::io;
use std::net::SocketAddr;

use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::Code;

/// A failed call. Its message is written by the server and never copies the request, so a
/// token or secret the request carried cannot come back in it.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{}: {message}", code.wire().0)]
#[non_exhaustive]
pub struct ConnectError {
    /// Why the call failed.
    pub code: Code,
    /// What failed, for a person to read.
    pub message: String,
}

impl ConnectError {
    /// An error with `code` and `message`.
    #[must_use]
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }

    /// The request or its message is over the size limit.
    pub(crate) fn too_large() -> Self {
        Self::new(Code::ResourceExhausted, "the request is over the 4 MiB limit")
    }

    fn wire(&self) -> Wire<'_> {
        Wire { code: self.code.wire().0, message: &self.message }
    }
}

impl IntoResponse for ConnectError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.code.wire().1).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body = serde_json::to_vec(&self.wire()).unwrap_or_default();
        (status, [(header::CONTENT_TYPE, "application/json")], body).into_response()
    }
}

/// The JSON of the end-stream message: `{}` after a stream that succeeded, else its error.
pub fn end_stream(error: Option<&ConnectError>) -> Vec<u8> {
    serde_json::to_vec(&EndStream { error: error.map(ConnectError::wire) }).unwrap_or_default()
}

#[derive(Serialize)]
struct Wire<'a> {
    code: &'static str,
    message: &'a str,
}

#[derive(Serialize)]
struct EndStream<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Wire<'a>>,
}

/// Why the server could not start or stopped.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServerError {
    /// The system's random source failed, so there is no token.
    #[error("the system random source failed, so no access token could be made")]
    Random,
    /// The address could not be listened on, such as a port in use.
    #[error("could not listen on {address}: {source}")]
    Bind {
        /// The loopback address.
        address: SocketAddr,
        /// Why.
        source: io::Error,
    },
    /// Serving failed.
    #[error("the server stopped: {0}")]
    Serve(#[source] io::Error),
}
