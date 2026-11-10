//! Encoders for a map body, chosen by the media type of the request's `content-type`: a
//! URL-encoded form (D273) and multipart parts (D274). A media type with no encoder sends the
//! map as JSON.

use bytes::Bytes;
use http::HeaderValue;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC};
use serde_json::{Map, Value};
use vc_interfaces::Random;
use vc_model::Failure;

use crate::BodyFiles;
use crate::action::multipart::multipart;
use crate::action::request::rendering;

/// What an encoder is given besides the fields.
pub struct Encoding<'e> {
    /// The `$file` bodies the engine read before the run, for file parts.
    pub files: &'e BodyFiles,
    /// The run's seeded random source, for a multipart boundary.
    pub random: &'e dyn Random,
    /// The `content-type` as written.
    pub content_type: &'e HeaderValue,
}

/// A map body's bytes, and the `content-type` to send in place of the one written, if any.
pub type Encoded = (Bytes, Option<HeaderValue>);

/// Encodes a map body's fields.
pub type Encoder = fn(&Map<String, Value>, &Encoding<'_>) -> Result<Encoded, Failure>;

/// The encoder of each media type, lower case.
const ENCODERS: [(&str, Encoder); 2] =
    [("application/x-www-form-urlencoded", form), ("multipart/form-data", multipart)];

/// The WHATWG form serializer keeps alphanumerics and `*-._`; a space becomes `+`.
const FORM_KEPT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'*').remove(b'-').remove(b'.').remove(b'_').remove(b' ');

/// The encoder for `content_type`'s media type, its parameters and case ignored.
pub fn encoder(content_type: &HeaderValue) -> Option<Encoder> {
    let media = content_type.to_str().ok()?.split(';').next()?.trim().to_ascii_lowercase();
    ENCODERS.iter().find(|(name, _)| *name == media).map(|(_, encode)| *encode)
}

/// `fields` as `application/x-www-form-urlencoded` (D273): a scalar is sent as text, a list
/// repeats its key, a null is left out, and an object fails. The header is sent as written.
pub fn form(fields: &Map<String, Value>, _: &Encoding<'_>) -> Result<Encoded, Failure> {
    let mut pairs = Vec::new();
    for (key, value) in fields {
        for item in items(value) {
            if item.is_object() || item.is_array() {
                return Err(rendering(&format!(
                    "the form field `{key}` holds an object; a form field takes text or a list"
                )));
            }
            pairs.push(format!("{}={}", form_text(key), form_text(&vc_template::to_text(item))));
        }
    }
    Ok((Bytes::from(pairs.join("&")), None))
}

/// A field's values: each item of a list, else the value itself, nulls left out.
pub fn items(value: &Value) -> impl Iterator<Item = &Value> {
    let items = match value {
        Value::Array(items) => items.as_slice(),
        value => std::slice::from_ref(value),
    };
    items.iter().filter(|item| !item.is_null())
}

/// `text` encoded as a form name or value.
fn form_text(text: &str) -> String {
    percent_encoding::utf8_percent_encode(text, FORM_KEPT).to_string().replace(' ', "+")
}

#[cfg(test)]
#[path = "encode_tests.rs"]
mod tests;
