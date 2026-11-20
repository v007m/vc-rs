//! The canonical request (D435) and the S3 path rules (D436).

use std::borrow::Cow;
use std::collections::BTreeMap;

use http::Uri;
use http::uri::PathAndQuery;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, percent_encode};
use vc_model::{RenderedRequest, wire_host};

/// Every byte but `A-Za-z0-9-._~` is encoded: a query name or value, or an S3 path segment.
const UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');
/// As [`UNRESERVED`], keeping `/`: a path for every service but S3.
const PATH: &AsciiSet = &UNRESERVED.remove(b'/');
/// Headers the transport or a proxy may add, drop or change, so they are never signed.
const UNSIGNED: [&str; 11] = [
    "authorization",
    "connection",
    "content-length",
    "expect",
    "keep-alive",
    "proxy-authorization",
    "te",
    "transfer-encoding",
    "upgrade",
    "user-agent",
    "x-amzn-trace-id",
];

/// A canonical request and the names it signs. Its bytes never leave the signer (D439).
pub struct Canonical {
    text: Vec<u8>,
    signed: String,
}

impl Canonical {
    /// The canonical request, as bytes: a header value need not be UTF-8.
    pub fn text(&self) -> &[u8] {
        &self.text
    }

    /// `SignedHeaders`: the signed names, sorted, joined with `;`.
    pub fn signed(&self) -> &str {
        &self.signed
    }
}

/// The canonical request for `request`, whose body hashes to `payload`. With `s3`, the path is
/// signed as sent: the signer has already rewritten it with [`sent_uri`].
pub fn request(request: &RenderedRequest, payload: &str, s3: bool) -> Canonical {
    let mut text = Vec::with_capacity(512);
    let path = if s3 { sent(request.uri.path()) } else { path(request.uri.path()) };
    for line in [request.method.as_str(), &path, &query(request.uri.query())] {
        text.extend_from_slice(line.as_bytes());
        text.push(b'\n');
    }
    let signed = headers(request, &mut text);
    text.push(b'\n');
    text.extend_from_slice(signed.as_bytes());
    text.push(b'\n');
    text.extend_from_slice(payload.as_bytes());
    Canonical { text, signed }
}

/// `uri` as the signer sends it, so what is signed is what is sent: each `+` in the query as
/// `%20` (D453), and with `s3`, the path in AWS's S3 encoding (D436).
pub fn sent_uri(uri: Uri, s3: bool) -> Uri {
    let path = if s3 { s3_path(uri.path()) } else { uri.path().to_owned() };
    let query = uri.query().map(spaced);
    if path == uri.path() && query.as_deref() == uri.query() {
        return uri;
    }
    let target = query.map_or_else(|| path.clone(), |query| format!("{path}?{query}"));
    let Ok(target) = PathAndQuery::try_from(target) else { return uri };
    let mut parts = uri.clone().into_parts();
    parts.path_and_query = Some(target);
    Uri::from_parts(parts).unwrap_or(uri)
}

/// An S3 path: not normalised, each segment decoded and then encoded once. A `%` without two
/// hex digits after it is a literal `%`.
fn s3_path(path: &str) -> String {
    sent(&path.split('/').map(encoded).collect::<Vec<_>>().join("/"))
}

/// `path` as sent, `/` when it is empty.
fn sent(path: &str) -> String {
    if path.is_empty() { "/".to_owned() } else { path.to_owned() }
}

/// Any other service's path: normalised, then encoded again (AWS's double encoding).
fn path(path: &str) -> String {
    percent_encode(normalized(path).as_bytes(), PATH).to_string()
}

/// `path` without dot segments or runs of `/` (RFC 3986 section 5.2.4, as AWS normalises).
fn normalized(path: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    let mut trailing = false;
    for segment in path.split('/') {
        trailing = matches!(segment, "" | "." | "..");
        if segment == ".." {
            kept.pop();
        } else if !matches!(segment, "" | ".") {
            kept.push(segment);
        }
    }
    let mut out = kept.iter().fold(String::with_capacity(path.len() + 1), |mut out, segment| {
        out.push('/');
        out.push_str(segment);
        out
    });
    if trailing || out.is_empty() {
        out.push('/');
    }
    out
}

/// The query, each name and value decoded and encoded again, sorted by name then value. A `+`
/// is a space, as AWS reads a query; a literal plus is written `%2B` (D453).
fn query(query: Option<&str>) -> String {
    let mut pairs: Vec<(String, String)> = spaced(query.unwrap_or_default())
        .split('&')
        .filter(|item| !item.is_empty())
        .map(|item| item.split_once('=').unwrap_or((item, "")))
        .map(|(name, value)| (encoded(name), encoded(value)))
        .collect();
    pairs.sort();
    pairs.iter().map(|(name, value)| format!("{name}={value}")).collect::<Vec<_>>().join("&")
}

/// `query` with each `+` as `%20`.
fn spaced(query: &str) -> String {
    query.replace('+', "%20")
}

/// `text` percent-decoded (`+` stays `+`), then encoded outside `A-Za-z0-9-._~`.
fn encoded(text: &str) -> String {
    let bytes: Cow<'_, [u8]> = percent_decode_str(text).into();
    percent_encode(&bytes, UNRESERVED).to_string()
}

/// Writes the canonical headers to `text`, each `name:value\n`, and returns `SignedHeaders`.
/// `host` is the request's own when it sets one, else the one the transport sends (D438).
fn headers(request: &RenderedRequest, text: &mut Vec<u8>) -> String {
    let host = wire_host(&request.uri);
    let mut named: BTreeMap<&str, Vec<&[u8]>> = BTreeMap::new();
    for (name, value) in &request.headers {
        if !UNSIGNED.contains(&name.as_str()) {
            named.entry(name.as_str()).or_default().push(value.as_bytes());
        }
    }
    if let Some(host) = &host {
        named.entry("host").or_insert_with(|| vec![host.as_bytes()]);
    }
    for (name, values) in &named {
        text.extend_from_slice(name.as_bytes());
        text.push(b':');
        for (index, value) in values.iter().enumerate() {
            if index > 0 {
                text.push(b',');
            }
            push_value(text, value);
        }
        text.push(b'\n');
    }
    named.keys().copied().collect::<Vec<_>>().join(";")
}

/// Writes `value` trimmed, with each run of spaces or tabs as one space.
fn push_value(text: &mut Vec<u8>, value: &[u8]) {
    let mut gap = false;
    for &byte in value.trim_ascii() {
        if byte == b' ' || byte == b'\t' {
            gap = true;
            continue;
        }
        if gap {
            text.push(b' ');
            gap = false;
        }
        text.push(byte);
    }
}
