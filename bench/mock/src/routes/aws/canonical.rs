//! The canonical request AWS computes from a request as it arrives.

use std::borrow::Cow;

use axum::body::Bytes;
use axum::http::request::Parts;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, percent_encode};
use sha2::{Digest, Sha256};

use super::{Refusal, hex, refused};

/// Every byte but `A-Za-z0-9-._~` is encoded.
const UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');
/// As [`UNRESERVED`], keeping `/`: a path for every service but S3, which AWS encodes again.
const PATH: &AsciiSet = &UNRESERVED.remove(b'/');
const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";

/// The canonical request of `parts` and `body`, signing the headers `names`. S3 signs the path
/// as it arrives; any other service signs it encoded again. Paths are not normalised: a client
/// sends what it signed, and the tests send no dot segments.
pub fn request(parts: &Parts, body: &Bytes, names: &[&str], s3: bool) -> Result<String, Refusal> {
    let path = parts.uri.path();
    let path = if s3 { path.to_owned() } else { percent_encode(path.as_bytes(), PATH).to_string() };
    let headers = headers(parts, names)?;
    let payload = payload(parts, body)?;
    let query = query(parts.uri.query().unwrap_or_default());
    Ok(format!("{}\n{path}\n{query}\n{headers}\n{}\n{payload}", parts.method, names.join(";")))
}

/// Each name and value decoded and encoded again, sorted; a `+` is a space.
fn query(query: &str) -> String {
    let spaced = query.replace('+', "%20");
    let mut pairs: Vec<(String, String)> = spaced
        .split('&')
        .filter(|item| !item.is_empty())
        .map(|item| item.split_once('=').unwrap_or((item, "")))
        .map(|(name, value)| (encoded(name), encoded(value)))
        .collect();
    pairs.sort();
    pairs.iter().map(|(name, value)| format!("{name}={value}")).collect::<Vec<_>>().join("&")
}

fn encoded(text: &str) -> String {
    let bytes: Cow<'_, [u8]> = percent_decode_str(text).into();
    percent_encode(&bytes, UNRESERVED).to_string()
}

/// `name:value\n` for each signed name, its values trimmed and joined with `,`. The `host` of
/// an HTTP/2 request is its `:authority`.
fn headers(parts: &Parts, names: &[&str]) -> Result<String, Refusal> {
    if !names.contains(&"host") {
        return Err(refused("IncompleteSignature", "the host header is not signed"));
    }
    let mut text = String::new();
    for name in names {
        let mut values: Vec<String> = parts.headers.get_all(*name).iter().map(|value| tidy(value.as_bytes())).collect();
        if values.is_empty() && *name == "host" {
            values.extend(parts.uri.authority().map(|authority| authority.as_str().to_owned()));
        }
        if values.is_empty() {
            return Err(refused("SignatureDoesNotMatch", &format!("the signed header `{name}` is not sent")));
        }
        text.push_str(&format!("{name}:{}\n", values.join(",")));
    }
    Ok(text)
}

/// `value` trimmed, with each run of spaces as one.
fn tidy(value: &[u8]) -> String {
    String::from_utf8_lossy(value).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `x-amz-content-sha256` when it is sent, which must match the body unless it is
/// `UNSIGNED-PAYLOAD`; else the body's hash.
fn payload(parts: &Parts, body: &Bytes) -> Result<String, Refusal> {
    let hashed = hex(&Sha256::digest(body));
    let Some(claimed) = parts.headers.get("x-amz-content-sha256").and_then(|value| value.to_str().ok()) else {
        return Ok(hashed);
    };
    if claimed != UNSIGNED_PAYLOAD && claimed != hashed {
        return Err(refused("XAmzContentSHA256Mismatch", "x-amz-content-sha256 is not the body's hash"));
    }
    Ok(claimed.to_owned())
}
