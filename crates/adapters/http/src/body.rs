use std::io::Read;

use bytes::{Bytes, BytesMut};
use http::HeaderMap;
use http::header::CONTENT_ENCODING;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use vc_interfaces::TransportError;
use vc_model::Size;

/// Undoes one content encoding, reading at most `cap` bytes of output.
type Decoder = fn(&[u8], u64) -> std::io::Result<Vec<u8>>;

/// The content encodings this transport decodes, by their `content-encoding` name.
const DECODERS: &[(&str, Decoder)] = &[
    ("gzip", |input, cap| take(flate2::read::MultiGzDecoder::new(input), cap)),
    ("x-gzip", |input, cap| take(flate2::read::MultiGzDecoder::new(input), cap)),
    ("deflate", |input, cap| take(flate2::read::ZlibDecoder::new(input), cap)),
    ("br", |input, cap| take(brotli_decompressor::Decompressor::new(input, 8 * 1024), cap)),
    ("zstd", |input, cap| take(zstd::stream::read::Decoder::new(input)?, cap)),
];

/// The `accept-encoding` value sent when a request names none.
pub const ACCEPT_ENCODING: &str = "gzip, br, zstd, deflate";

fn take(reader: impl Read, cap: u64) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    reader.take(cap).read_to_end(&mut out)?;
    Ok(out)
}

/// Reads the body off the wire, refusing more than `limit` bytes.
pub async fn read(mut body: Incoming, limit: Size) -> Result<Bytes, TransportError> {
    let mut wire = BytesMut::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|error| TransportError::Protocol { reason: error.to_string() })?;
        if let Ok(data) = frame.into_data() {
            if (wire.len() + data.len()) as u64 > limit.bytes() {
                return Err(TransportError::BodyTooLarge { limit });
            }
            wire.extend_from_slice(&data);
        }
    }
    Ok(wire.freeze())
}

/// Undoes the response's content encodings, last applied first. The output is capped at
/// `limit`, so a small compressed body cannot expand without bound (a zip bomb).
pub fn decode(headers: &HeaderMap, wire: Bytes, limit: Size) -> Result<Bytes, TransportError> {
    let encodings: Vec<String> = headers
        .get_all(CONTENT_ENCODING)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(|name| name.trim().to_ascii_lowercase())
        .filter(|name| !name.is_empty() && name != "identity")
        .collect();
    encodings.iter().rev().try_fold(wire, |body, encoding| decode_one(encoding, &body, limit))
}

fn decode_one(encoding: &str, body: &[u8], limit: Size) -> Result<Bytes, TransportError> {
    let fail = |reason: String| TransportError::Decode { encoding: encoding.to_owned(), reason };
    let decoder = DECODERS.iter().find(|(name, _)| *name == encoding).map(|(_, decoder)| *decoder);
    let decoder = decoder.ok_or_else(|| fail("unsupported encoding".to_owned()))?;
    let out = decoder(body, limit.bytes().saturating_add(1)).map_err(|error| fail(error.to_string()))?;
    if out.len() as u64 > limit.bytes() {
        return Err(TransportError::BodyTooLarge { limit });
    }
    Ok(Bytes::from(out))
}

#[cfg(test)]
#[path = "body_tests.rs"]
mod tests;
