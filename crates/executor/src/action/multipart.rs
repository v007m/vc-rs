//! `multipart/form-data` bodies (D274, RFC 7578): each field of a map is a part, a list
//! repeats it, and a null is left out.

use bytes::Bytes;
use http::HeaderValue;
use serde_json::{Map, Value};
use vc_model::Failure;

use crate::BodyFiles;
use crate::action::encode::{Encoded, Encoding, items};
use crate::action::media;
use crate::action::request::{JSON, rendering};

/// The key of a single-key map that names a file part.
const FILE: &str = "$file";

/// A part's file name, `content-type` and bytes.
type Part = (Option<String>, Option<&'static str>, Bytes);

/// `fields` as parts between a boundary: the one the written `content-type` names, else `vc-`
/// and 24 hex characters from the run's random source, added to the header (D274).
pub fn multipart(fields: &Map<String, Value>, encoding: &Encoding<'_>) -> Result<Encoded, Failure> {
    let (boundary, header) = match named_boundary(encoding.content_type) {
        Some(named) => (named, None),
        None => drawn(encoding)?,
    };
    let mut body = Vec::new();
    for (name, value) in fields {
        for item in items(value) {
            let (file_name, kind, bytes) = part(item, encoding.files)?;
            let mut head = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{}\"", quoted(name));
            head.extend(file_name.map(|file_name| format!("; filename=\"{}\"", quoted(&file_name))));
            head.extend(kind.map(|kind| format!("\r\nContent-Type: {kind}")));
            body.extend_from_slice(format!("{head}\r\n\r\n").as_bytes());
            body.extend_from_slice(&bytes);
            body.extend_from_slice(b"\r\n");
        }
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Ok((Bytes::from(body), header))
}

/// The `boundary` parameter of `content_type`, its quotes removed.
fn named_boundary(content_type: &HeaderValue) -> Option<String> {
    let params = content_type.to_str().ok()?.split(';').skip(1);
    params.filter_map(|param| param.split_once('=')).find_map(|(name, value)| {
        name.trim().eq_ignore_ascii_case("boundary").then(|| value.trim().trim_matches('"').to_owned())
    })
}

/// A boundary drawn from the run's random source, and the written header carrying it.
fn drawn(encoding: &Encoding<'_>) -> Result<(String, Option<HeaderValue>), Failure> {
    let (high, low) = (encoding.random.next_u64(), encoding.random.next_u64());
    let boundary = format!("vc-{high:016x}{:08x}", low >> 32);
    let written = encoding.content_type.to_str().unwrap_or_default().trim_end_matches([';', ' ']);
    let header = HeaderValue::try_from(format!("{written}; boundary={boundary}"));
    let header =
        header.map_err(|error| rendering(&format!("the multipart `content-type` cannot carry a boundary: {error}")))?;
    Ok((boundary, Some(header)))
}

/// What one value sends: a `$file` map is the file's bytes with its name and type, another
/// map or a list is JSON, and anything else its text, with no type.
fn part(value: &Value, files: &BodyFiles) -> Result<Part, Failure> {
    if let Some(path) = file_path(value) {
        let (bytes, kind) = media::file(files, path)?;
        let file_name = path.rsplit('/').next().unwrap_or(path).to_owned();
        return Ok((Some(file_name), Some(kind), bytes));
    }
    Ok(match value {
        Value::Object(_) | Value::Array(_) => (None, Some(JSON), Bytes::from(value.to_string())),
        scalar => (None, None, Bytes::from(vc_template::to_text(scalar))),
    })
}

/// The path of a map whose only key is `$file`.
fn file_path(value: &Value) -> Option<&str> {
    let Value::Object(map) = value else { return None };
    map.get(FILE).filter(|_| map.len() == 1).and_then(Value::as_str)
}

/// `text` for a quoted name, with `"`, CR and LF percent-encoded as browsers send them.
fn quoted(text: &str) -> String {
    text.replace('"', "%22").replace('\r', "%0D").replace('\n', "%0A")
}

#[cfg(test)]
#[path = "multipart_tests.rs"]
mod tests;
