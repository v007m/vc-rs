use std::io::Write;

use bytes::Bytes;
use flate2::Compression;
use flate2::write::{GzEncoder, ZlibEncoder};
use http::header::CONTENT_ENCODING;
use http::{HeaderMap, HeaderValue};
use vc_interfaces::TransportError;
use vc_model::Size;

use super::decode;

const BIG: Size = Size::from_bytes(1 << 20);

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn deflate(data: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn zstd(data: &[u8]) -> Vec<u8> {
    zstd::stream::encode_all(data, 0).unwrap()
}

fn encoded(value: &'static str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_ENCODING, HeaderValue::from_static(value));
    headers
}

#[test]
fn a_body_without_an_encoding_passes_through() {
    let body = Bytes::from_static(b"plain");
    assert_eq!(decode(&HeaderMap::new(), body.clone(), BIG), Ok(body.clone()));
    assert_eq!(decode(&encoded("identity"), body.clone(), BIG), Ok(body));
}

#[test]
fn decodes_gzip_deflate_and_zstd_in_any_case() {
    assert_eq!(decode(&encoded("gzip"), gzip(b"hello").into(), BIG).unwrap(), "hello");
    assert_eq!(decode(&encoded("X-GZIP"), gzip(b"hello").into(), BIG).unwrap(), "hello");
    assert_eq!(decode(&encoded("deflate"), deflate(b"hello").into(), BIG).unwrap(), "hello");
    assert_eq!(decode(&encoded("zstd"), zstd(b"hello").into(), BIG).unwrap(), "hello");
}

#[test]
fn stacked_encodings_are_undone_last_first() {
    let body = zstd(&gzip(b"layered"));
    assert_eq!(decode(&encoded("gzip, identity, zstd"), body.into(), BIG).unwrap(), "layered");
}

#[test]
fn caps_the_decoded_size_so_a_zip_bomb_stops_early() {
    let bomb = gzip(&vec![0_u8; 4 << 20]);
    assert!(bomb.len() < 16 * 1024, "the bomb is small on the wire");
    let limit = Size::from_bytes(1 << 20);
    assert_eq!(decode(&encoded("gzip"), bomb.into(), limit), Err(TransportError::BodyTooLarge { limit }));
}

#[test]
fn a_body_exactly_at_the_limit_is_kept() {
    let limit = Size::from_bytes(5);
    assert_eq!(decode(&encoded("gzip"), gzip(b"12345").into(), limit).unwrap(), "12345");
    assert_eq!(decode(&encoded("gzip"), gzip(b"123456").into(), limit), Err(TransportError::BodyTooLarge { limit }));
}

#[test]
fn an_unknown_or_broken_encoding_is_a_decode_error() {
    let unknown = decode(&encoded("compress"), Bytes::from_static(b"x"), BIG).err();
    let expected = TransportError::Decode { encoding: "compress".into(), reason: "unsupported encoding".into() };
    assert_eq!(unknown, Some(expected));
    let broken = decode(&encoded("gzip"), Bytes::from_static(b"not gzip"), BIG).err();
    assert!(matches!(broken, Some(TransportError::Decode { ref encoding, .. }) if encoding == "gzip"), "{broken:?}");
}
