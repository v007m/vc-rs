//! Encoders for a map body, chosen by the media type of the request's `content-type`: a
//! URL-encoded form (D273) and multipart parts (D274). A media type with no encoder sends the
//! map as JSON.

use bytes::Bytes;
use http::HeaderValue;
use serde_json::{Map, Value};
use vc_interfaces::Random;
use vc_model::{ActionId, Failure, MULTIPART, form_text};

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
    /// The action sending the body, whose `$file` parts the engine read.
    pub action: ActionId,
}

/// A map body's bytes, and the `content-type` to send in place of the one written, if any.
pub type Encoded = (Bytes, Option<HeaderValue>);

/// Encodes a map body's fields.
pub type Encoder = fn(&Map<String, Value>, &Encoding<'_>) -> Result<Encoded, Failure>;

/// The encoder of each media type, lower case.
const ENCODERS: [(&str, Encoder); 2] = [("application/x-www-form-urlencoded", form), (MULTIPART, multipart)];

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
        for (_, item) in items(value) {
            pairs.push(format!("{}={}", form_text(key), form_text(&form_value(key, item)?)));
        }
    }
    Ok((Bytes::from(pairs.join("&")), None))
}

/// One form value as text; V5111 naming what field `key` holds when it is an object or a list.
fn form_value(key: &str, item: &Value) -> Result<String, Failure> {
    let nested = match item {
        Value::Object(_) => "an object",
        Value::Array(_) => "a list",
        scalar => return Ok(vc_template::to_text(scalar)),
    };
    Err(rendering(&format!("the form field `{key}` holds {nested}; a form field takes text or a list")))
}

/// A field's values with their index in its list: each item of a list, else the value itself
/// with none; nulls left out.
pub fn items(value: &Value) -> impl Iterator<Item = (Option<usize>, &Value)> {
    let (items, listed) = match value {
        Value::Array(items) => (items.as_slice(), true),
        value => (std::slice::from_ref(value), false),
    };
    let indexed = items.iter().enumerate().map(move |(at, item)| (listed.then_some(at), item));
    indexed.filter(|(_, item)| !item.is_null())
}

#[cfg(test)]
#[path = "encode_tests.rs"]
mod tests;
