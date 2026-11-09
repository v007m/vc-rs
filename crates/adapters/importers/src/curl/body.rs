//! A curl command's data as an api body. JSON keeps its shape, with each top-level field a var
//! whose default is the curl's value; form data and `-F` fields are maps; anything else is text.
//! A field named like a credential is written empty, with a note.

use indexmap::IndexMap;
use serde_json::{Map, Value, json};

use super::options::Request;
use crate::naming::camel;
use crate::secrets::credential;
use crate::url::pairs;

/// What a body field holding a credential is told by.
const LEFT_OUT: &str = "a credential is never written, so it is empty";

/// An api's body, its content type and the vars its fields became.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Body {
    pub value: Option<Value>,
    pub content_type: Option<String>,
    pub vars: IndexMap<String, Value>,
}

/// Reads data of one content type: the data, the vars already taken, the notes.
type Reader = fn(&str, &IndexMap<String, Value>, &mut Vec<String>) -> Body;

/// The readers by a word their content type holds; any other type is text.
const READERS: [(&str, Reader); 2] = [("json", json), ("x-www-form-urlencoded", form)];

/// The body `request` sends: none with `-G` or no data; `taken` holds the path's vars.
pub fn body(request: &Request, taken: &IndexMap<String, Value>, found: &mut Vec<String>) -> Body {
    if !request.form.is_empty() {
        return multipart(&request.form, found);
    }
    if request.get || request.data.is_empty() {
        return Body::default();
    }
    let content_type = content_type(request);
    let lower = content_type.to_ascii_lowercase();
    let read = READERS.iter().find(|(word, _)| lower.contains(word)).map_or(text as Reader, |(_, read)| *read);
    Body { content_type: Some(content_type), ..read(&request.data.join("&"), taken, found) }
}

/// The `content-type` header, else JSON for `--json`, else the form type curl sends `-d` as.
fn content_type(request: &Request) -> String {
    let header = request.headers.iter().find(|(name, _)| name.eq_ignore_ascii_case("content-type"));
    let fallback = if request.json { "application/json" } else { "application/x-www-form-urlencoded" };
    header.map_or(fallback, |(_, value)| value.as_str()).to_owned()
}

fn text(data: &str, _: &IndexMap<String, Value>, _: &mut Vec<String>) -> Body {
    Body { value: Some(json!(data)), ..Body::default() }
}

/// A JSON body, each top-level field a var unless its name is taken; not JSON, it is text.
fn json(data: &str, taken: &IndexMap<String, Value>, found: &mut Vec<String>) -> Body {
    let Ok(value) = serde_json::from_str::<Value>(data) else {
        found.push("the body is not valid JSON, so it is kept as text".to_owned());
        return text(data, taken, found);
    };
    let fields = match scrubbed(value, found) {
        Value::Object(fields) => fields,
        value => return Body { value: Some(value), ..Body::default() },
    };
    let mut body = Body::default();
    let mut written = Map::new();
    for (field, value) in fields {
        let var = camel(&field);
        if var.is_empty() || taken.contains_key(&var) || body.vars.contains_key(&var) {
            written.insert(field, value);
        } else {
            written.insert(field, json!(format!("${{var.{var}}}")));
            body.vars.insert(var, value);
        }
    }
    Body { value: Some(Value::Object(written)), ..body }
}

/// `value` with each field named like a credential, at any depth, emptied.
fn scrubbed(value: Value, found: &mut Vec<String>) -> Value {
    match value {
        Value::Object(fields) => {
            let mut kept = Map::new();
            for (name, value) in fields {
                let value = if credential(&name) { left_out(&name, found) } else { scrubbed(value, found) };
                kept.insert(name, value);
            }
            Value::Object(kept)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(|item| scrubbed(item, found)).collect()),
        value => value,
    }
}

fn left_out(name: &str, found: &mut Vec<String>) -> Value {
    found.push(format!("body field {name}: {LEFT_OUT}"));
    json!("")
}

/// A form body as a map of its decoded fields.
fn form(data: &str, _: &IndexMap<String, Value>, found: &mut Vec<String>) -> Body {
    let fields = pairs(data).map(|(name, value)| field(name, json!(value), found));
    Body { value: Some(Value::Object(fields.collect())), ..Body::default() }
}

/// `-F` fields as a multipart map; a file one (`@file`, `<file`) is written empty with a note.
fn multipart(fields: &[String], found: &mut Vec<String>) -> Body {
    let mut written = Map::new();
    for text in fields {
        let (name, value) = text.split_once('=').unwrap_or((text, ""));
        let value = if value.starts_with(['@', '<']) {
            found.push(format!("form field {name}: the file it sends is not read, so it is empty"));
            String::new()
        } else {
            value.to_owned()
        };
        let (name, value) = field(name.to_owned(), json!(value), found);
        written.insert(name, value);
    }
    Body {
        value: Some(Value::Object(written)),
        content_type: Some("multipart/form-data".to_owned()),
        ..Body::default()
    }
}

/// A form field, emptied when it holds a credential.
fn field(name: String, value: Value, found: &mut Vec<String>) -> (String, Value) {
    let value = if credential(&name) { left_out(&name, found) } else { value };
    (name, value)
}

#[cfg(test)]
#[path = "body_tests.rs"]
mod tests;
