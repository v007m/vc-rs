//! Reading request bodies under the size limit, and Connect's streaming envelope: a flags byte,
//! the message's length as a big-endian `u32`, then the message.

use axum::body::Body;
use bytes::{BufMut as _, Bytes, BytesMut};
use http_body_util::{BodyExt as _, LengthLimitError, Limited};

use crate::{Code, ConnectError};

/// The largest request message the server reads (4 MiB).
pub const MAX_MESSAGE: usize = 4 * 1024 * 1024;

/// The envelope's flags byte and length prefix.
const HEAD: usize = 5;

/// The flag of the message that ends a stream.
const END_STREAM: u8 = 0b10;

/// The body of a request, read until it passes `limit` bytes and no further.
pub async fn read(body: Body, limit: usize) -> Result<Bytes, ConnectError> {
    match Limited::new(body, limit).collect().await {
        Ok(collected) => Ok(collected.to_bytes()),
        Err(error) if error.is::<LengthLimitError>() => Err(ConnectError::too_large()),
        Err(_) => Err(ConnectError::new(Code::InvalidArgument, "the request body could not be read")),
    }
}

/// The body of a server-streaming request: one message, at most [`MAX_MESSAGE`] long.
pub async fn read_single(body: Body) -> Result<Bytes, ConnectError> {
    single(&read(body, MAX_MESSAGE + HEAD).await?)
}

/// The one message an enveloped request body carries. The length prefix is checked against
/// the limit before the message is looked at.
pub fn single(body: &Bytes) -> Result<Bytes, ConnectError> {
    let Some(&[flags, a, b, c, d]) = body.first_chunk::<HEAD>() else {
        return Err(malformed());
    };
    let length = usize::try_from(u32::from_be_bytes([a, b, c, d])).unwrap_or(usize::MAX);
    if length > MAX_MESSAGE {
        return Err(ConnectError::too_large());
    }
    if flags != 0 || body.len() != HEAD + length {
        return Err(malformed());
    }
    Ok(body.slice(HEAD..))
}

/// One message of a server stream.
pub fn message(payload: &[u8]) -> Bytes {
    frame(0, payload)
}

/// The message that ends a server stream, with its end-stream JSON.
pub fn end_stream(json: &[u8]) -> Bytes {
    frame(END_STREAM, json)
}

fn frame(flags: u8, payload: &[u8]) -> Bytes {
    let mut out = BytesMut::with_capacity(HEAD + payload.len());
    out.put_u8(flags);
    out.put_u32(u32::try_from(payload.len()).unwrap_or(u32::MAX));
    out.put_slice(payload);
    out.freeze()
}

fn malformed() -> ConnectError {
    ConnectError::new(Code::InvalidArgument, "a streaming request must be one uncompressed enveloped message")
}

#[cfg(test)]
#[path = "envelope_tests.rs"]
mod tests;
