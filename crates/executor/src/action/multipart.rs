//! `multipart/form-data` bodies (D274, RFC 7578): each field of a map is a part, a list
//! repeats it, and a null is left out.

use bytes::Bytes;
use http::HeaderValue;
use serde_json::{Map, Value};
use vc_model::Failure;

use crate::Unread;
use crate::action::encode::{Encoded, Encoding, items};
use crate::action::media;
use crate::action::request::{JSON, rendering};

/// A part's file name, `content-type` and bytes.
type Part = (Option<String>, Option<&'static str>, Bytes);

/// RFC 2046 section 5.1.1: a boundary is at most this long.
const MAX_BOUNDARY: usize = 70;

/// `fields` as parts between a boundary: the one the written `content-type` names, else `vc-`
/// and 24 hex characters from the run's random source, added to the header (D274). A named
/// boundary must be valid and appear in no part; a drawn one cannot.
pub fn multipart(fields: &Map<String, Value>, encoding: &Encoding<'_>) -> Result<Encoded, Failure> {
    let (boundary, header) = match named_boundary(encoding.content_type) {
        Some(named) => (named?, None),
        None => drawn(encoding)?,
    };
    let delimiter = format!("--{boundary}");
    let named = header.is_none().then_some(delimiter.as_bytes());
    let mut body = Vec::new();
    for (name, value) in fields {
        for (at, item) in items(value) {
            let (head, bytes) = framed(name, part(item, (name, at), encoding)?);
            if named.is_some_and(|delimiter| holds(head.as_bytes(), delimiter) || holds(&bytes, delimiter)) {
                return Err(rendering(&format!(
                    "the multipart boundary `{boundary}` appears inside the part `{name}`; name another, or \
                     leave it out and one is drawn (D274)"
                )));
            }
            let pieces: [&[u8]; 5] = [delimiter.as_bytes(), b"\r\n", head.as_bytes(), &bytes, b"\r\n"];
            for piece in pieces {
                body.extend_from_slice(piece);
            }
        }
    }
    body.extend_from_slice(format!("{delimiter}--\r\n").as_bytes());
    Ok((Bytes::from(body), header))
}

/// The `boundary` parameter of `content_type`, its quotes removed; V5111 when it is not a valid
/// RFC 2046 boundary.
fn named_boundary(content_type: &HeaderValue) -> Option<Result<String, Failure>> {
    let params = content_type.to_str().ok()?.split(';').skip(1);
    let named = params.filter_map(|param| param.split_once('=')).find_map(|(name, value)| {
        name.trim().eq_ignore_ascii_case("boundary").then(|| value.trim().trim_matches('"').to_owned())
    })?;
    Some(valid_boundary(named))
}

/// `boundary` when it is 1 to 70 RFC 2046 `bchars` that do not end in a space.
fn valid_boundary(boundary: String) -> Result<String, Failure> {
    let wrong = if boundary.is_empty() {
        "it is empty"
    } else if boundary.len() > MAX_BOUNDARY {
        "it is longer than 70 characters"
    } else if boundary.ends_with(' ') {
        "it ends in a space"
    } else if !boundary.bytes().all(bchar) {
        "it holds a character RFC 2046 does not allow"
    } else {
        return Ok(boundary);
    };
    Err(rendering(&format!("the multipart boundary `{boundary}` is not valid: {wrong} (RFC 2046)")))
}

/// RFC 2046's `bchars`: letters, digits, a space and `'()+_,-./:=?`.
fn bchar(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"'()+_,-./:=? ".contains(&byte)
}

/// Whether `bytes` hold `delimiter`.
fn holds(bytes: &[u8], delimiter: &[u8]) -> bool {
    bytes.windows(delimiter.len()).any(|window| window == delimiter)
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

/// One part after its delimiter line: its headers and the blank line after them, and its bytes.
fn framed(name: &str, (file_name, kind, bytes): Part) -> (String, Bytes) {
    let mut head = format!("Content-Disposition: form-data; name=\"{}\"", quoted(name));
    head.extend(file_name.map(|file_name| format!("; filename=\"{}\"", quoted(&file_name))));
    head.extend(kind.map(|kind| format!("\r\nContent-Type: {kind}")));
    head.push_str("\r\n\r\n");
    (head, bytes)
}

/// What one value of field `name` (item `at` of its list) sends: a `$file` map is the bytes the
/// engine read for it, with its name and type; another map or a list is JSON, and anything else
/// its text, with no type. A `$file` written with a templated path is V5111 whatever it renders to.
fn part(value: &Value, (name, at): (&str, Option<usize>), encoding: &Encoding<'_>) -> Result<Part, Failure> {
    let pointer = vc_model::part_pointer(name, at);
    let Some(path) = vc_model::file_path(value) else {
        if matches!(encoding.files.get(encoding.action, &pointer), Some(Err(Unread::Templated))) {
            return Err(media::templated(&pointer));
        }
        return Ok(match value {
            Value::Object(_) | Value::Array(_) => (None, Some(JSON), Bytes::from(value.to_string())),
            scalar => (None, None, Bytes::from(vc_template::to_text(scalar))),
        });
    };
    let read = media::file(encoding.files, (encoding.action, &pointer), path)?;
    let (bytes, kind) = read.ok_or_else(|| {
        rendering(&format!(
            "the `$file` in part `{name}` arrives in a `${{…}}` value; its path must be written in the body (D272)"
        ))
    })?;
    Ok((Some(vc_model::basename(path).to_owned()), Some(kind), bytes))
}

/// `text` for a quoted name, with `"`, CR and LF percent-encoded as browsers send them.
fn quoted(text: &str) -> String {
    text.replace('"', "%22").replace('\r', "%0D").replace('\n', "%0A")
}

#[cfg(test)]
#[path = "multipart_tests.rs"]
mod tests;
