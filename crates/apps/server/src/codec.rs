//! The two encodings a Connect call's messages travel in (D254): binary protobuf and the
//! protobuf JSON mapping (the contract's pbjson impls), chosen by the request's content type
//! and answered in the same one.

use prost::Message;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{Code, ConnectError};

/// A message encoding, with the content types it goes by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    /// Binary protobuf: `application/proto`, `application/connect+proto`.
    Proto,
    /// Protobuf JSON: `application/json`, `application/connect+json`.
    Json,
}

/// Each codec with its unary and its streaming content type.
const CONTENT_TYPES: [(Codec, &str, &str); 2] = [
    (Codec::Proto, "application/proto", "application/connect+proto"),
    (Codec::Json, "application/json", "application/connect+json"),
];

/// Whether a call is unary or a server stream, which picks the content type a codec goes by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// One request message, one response message.
    Unary,
    /// One request message, then enveloped response messages.
    Stream,
}

impl Codec {
    /// The codec whose content type for a call of `shape` is `essence` (compared without case).
    pub fn of(essence: &str, shape: Shape) -> Option<Self> {
        CONTENT_TYPES.into_iter().find(|entry| content_type(*entry, shape).eq_ignore_ascii_case(essence)).map(|e| e.0)
    }

    /// The content type of a call of `shape` in this codec.
    pub fn content_type(self, shape: Shape) -> &'static str {
        CONTENT_TYPES.into_iter().find(|entry| entry.0 == self).map_or("", |entry| content_type(entry, shape))
    }

    /// The request message in `body`; `invalid_argument` when it is not one, without echoing it.
    pub fn decode<T: Message + Default + DeserializeOwned>(self, body: &[u8]) -> Result<T, ConnectError> {
        let decoded = match self {
            Self::Proto => T::decode(body).ok(),
            Self::Json => serde_json::from_slice(body).ok(),
        };
        decoded.ok_or_else(|| {
            ConnectError::new(Code::InvalidArgument, "the request is not a valid message for this method")
        })
    }

    /// The response message's bytes; `internal` when the JSON mapping refuses it (an enum field
    /// holding a number the enum does not name).
    pub fn encode<T: Message + Serialize>(self, message: &T) -> Result<Vec<u8>, ConnectError> {
        match self {
            Self::Proto => Ok(message.encode_to_vec()),
            Self::Json => serde_json::to_vec(message)
                .map_err(|_| ConnectError::new(Code::Internal, "the response could not be written as JSON")),
        }
    }
}

const fn content_type((_, unary, stream): (Codec, &'static str, &'static str), shape: Shape) -> &'static str {
    match shape {
        Shape::Unary => unary,
        Shape::Stream => stream,
    }
}

#[cfg(test)]
#[path = "codec_tests.rs"]
mod tests;
