//! `multipart/form-data` bodies (D274, RFC 7578): each field of a map is a part, a list
//! repeats it, and a null is left out.

use std::collections::BTreeSet;

use bytes::Bytes;
use http::HeaderValue;
use serde_json::{Map, Value};
use vc_model::{BodySpec, FILE_KEY, Failure, TemplateValue};

use crate::action::encode::{Encoded, Encoding};
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
    let mut body = Vec::new();
    for (name, value) in fields {
        for (at, item) in indexed(value) {
            let part = framed(name, part(item, (name, at), encoding)?);
            if header.is_none() && holds(&part, &boundary) {
                return Err(rendering(&format!(
                    "the multipart boundary `{boundary}` appears inside the part `{name}`; name another, or \
                     leave it out and one is drawn (D274)"
                )));
            }
            body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
            body.extend_from_slice(&part);
            body.extend_from_slice(b"\r\n");
        }
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Ok((Bytes::from(body), header))
}

/// The pointers of the `$file` parts `spec` writes with a literal path, a field's or an item of
/// its list's (D274); V5111 for one whose path holds `${…}`, which the engine cannot read.
pub fn written_parts(spec: &BodySpec) -> Result<BTreeSet<String>, Failure> {
    let (BodySpec::Json(TemplateValue::Map(fields)) | BodySpec::Extend { changes: TemplateValue::Map(fields), .. }) =
        spec
    else {
        return Ok(BTreeSet::new());
    };
    let mut written = BTreeSet::new();
    for (name, value) in fields {
        for (at, item) in written_items(value) {
            match literal_file(item) {
                Some(true) => drop(written.insert(vc_model::part_pointer(name, at))),
                Some(false) => {
                    return Err(rendering(&format!(
                        "the `$file` part `{name}` cannot hold `${{…}}` references in its path (D272)"
                    )));
                }
                None => {}
            }
        }
    }
    Ok(written)
}

/// A written field's values with their list index: each item of a list, else the value.
fn written_items(value: &TemplateValue) -> Vec<(Option<usize>, &TemplateValue)> {
    match value {
        TemplateValue::List(items) => items.iter().enumerate().map(|(at, item)| (Some(at), item)).collect(),
        value => vec![(None, value)],
    }
}

/// Whether a written `$file` map's path is literal text; `None` when `value` is no `$file` map
/// with a text path.
fn literal_file(value: &TemplateValue) -> Option<bool> {
    let TemplateValue::Map(map) = value else { return None };
    match map.get(FILE_KEY).filter(|_| map.len() == 1)? {
        TemplateValue::String(path) => Some(path.as_literal().is_some()),
        _ => None,
    }
}

/// A field's values with their list index, nulls left out.
fn indexed(value: &Value) -> Vec<(Option<usize>, &Value)> {
    let items: Vec<(Option<usize>, &Value)> = match value {
        Value::Array(items) => items.iter().enumerate().map(|(at, item)| (Some(at), item)).collect(),
        value => vec![(None, value)],
    };
    items.into_iter().filter(|(_, item)| !item.is_null()).collect()
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

/// Whether `part` holds the delimiter `--boundary`.
fn holds(part: &[u8], boundary: &str) -> bool {
    let delimiter = format!("--{boundary}");
    part.windows(delimiter.len()).any(|window| window == delimiter.as_bytes())
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

/// One part after its delimiter line: its headers, a blank line and its bytes.
fn framed(name: &str, (file_name, kind, bytes): Part) -> Vec<u8> {
    let mut head = format!("Content-Disposition: form-data; name=\"{}\"", quoted(name));
    head.extend(file_name.map(|file_name| format!("; filename=\"{}\"", quoted(&file_name))));
    head.extend(kind.map(|kind| format!("\r\nContent-Type: {kind}")));
    let mut part = format!("{head}\r\n\r\n").into_bytes();
    part.extend_from_slice(&bytes);
    part
}

/// What one value of field `name` (item `at` of its list) sends: a `$file` map is the bytes the
/// engine read for it, with its name and type; another map or a list is JSON, and anything else
/// its text, with no type.
fn part(value: &Value, (name, at): (&str, Option<usize>), encoding: &Encoding<'_>) -> Result<Part, Failure> {
    let Some(path) = vc_model::file_path(value) else {
        return Ok(match value {
            Value::Object(_) | Value::Array(_) => (None, Some(JSON), Bytes::from(value.to_string())),
            scalar => (None, None, Bytes::from(vc_template::to_text(scalar))),
        });
    };
    let pointer = vc_model::part_pointer(name, at);
    match media::file(encoding.files, (encoding.action, &pointer), path)? {
        Some((bytes, kind)) => Ok((Some(vc_model::basename(path).to_owned()), Some(kind), bytes)),
        None if encoding.written.contains(&pointer) => Err(rendering(&format!(
            "the `$file` `{path}` in part `{name}` was not read before the run: the body's `content-type` names \
             multipart only through `${{…}}`, or this is an internal error"
        ))),
        None => Err(rendering(&format!(
            "the `$file` in part `{name}` arrives in a `${{…}}` value; its path must be written in the body (D272)"
        ))),
    }
}

/// `text` for a quoted name, with `"`, CR and LF percent-encoded as browsers send them.
fn quoted(text: &str) -> String {
    text.replace('"', "%22").replace('\r', "%0D").replace('\n', "%0A")
}

#[cfg(test)]
#[path = "multipart_tests.rs"]
mod tests;
