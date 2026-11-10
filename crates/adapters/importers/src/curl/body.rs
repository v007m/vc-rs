//! A curl command's data as an api body, read by the shared [`crate::body`] rules; `-F` fields
//! are a multipart map. A field named like a credential is written empty, with a note.

use indexmap::IndexMap;
use serde_json::{Map, Value, json};

use super::options::Request;
use crate::body::{Body, Scrub, Sent, field, read};

/// What a body field holding a credential is told by.
const LEFT_OUT: &str = "a credential is never written, so it is empty";

/// The notes of one command: a credential is written empty and named in a note.
struct Emptied<'f>(&'f mut Vec<String>);

impl Scrub for Emptied<'_> {
    fn credential(&mut self, name: &str) -> Value {
        self.0.push(format!("body field {name}: {LEFT_OUT}"));
        json!("")
    }

    fn note(&mut self, message: String) {
        self.0.push(message);
    }
}

/// The body `request` sends: none with `-G` or no data; `taken` holds the path's vars.
pub fn body(request: &Request, taken: &IndexMap<String, Value>, found: &mut Vec<String>) -> Body {
    let scrub = &mut Emptied(found);
    if !request.form.is_empty() {
        return multipart(&request.form, scrub);
    }
    if request.get || request.data.is_empty() {
        return Body::default();
    }
    read(&Sent { data: &request.data.join("&"), content_type: &content_type(request) }, taken, scrub)
}

/// The `content-type` header, else JSON for `--json`, else the form type curl sends `-d` as.
fn content_type(request: &Request) -> String {
    let header = request.headers.iter().find(|(name, _)| name.eq_ignore_ascii_case("content-type"));
    let fallback = if request.json { "application/json" } else { "application/x-www-form-urlencoded" };
    header.map_or(fallback, |(_, value)| value.as_str()).to_owned()
}

/// `-F` fields as a multipart map; a file one (`@file`, `<file`) is written empty with a note.
fn multipart(fields: &[String], scrub: &mut Emptied<'_>) -> Body {
    let mut written = Map::new();
    for text in fields {
        let (name, value) = text.split_once('=').unwrap_or((text, ""));
        let value = if value.starts_with(['@', '<']) {
            scrub.note(format!("form field {name}: the file it sends is not read, so it is empty"));
            String::new()
        } else {
            value.to_owned()
        };
        let (name, value) = field(name.to_owned(), json!(value), scrub);
        written.insert(name, value);
    }
    Body {
        value: Some(Value::Object(written)),
        content_type: Some("multipart/form-data".to_owned()),
        ..Body::default()
    }
}

#[cfg(test)]
#[path = "body_tests.rs"]
mod tests;
