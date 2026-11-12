//! A recorded request body as an api body, shared by the curl and HAR importers. JSON keeps its
//! shape, with each top-level field a var whose default is the recorded value; a form is a map;
//! anything else is text. A field named like a credential, at any depth, is what the importer's
//! [`Scrub`] puts in its place.

use indexmap::IndexMap;
use serde_json::{Map, Value, json};
use vc_model::credential;

use crate::naming::camel;
use crate::url::pairs;

/// An api's body, its content type and the vars its fields became.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Body {
    pub value: Option<Value>,
    pub content_type: Option<String>,
    pub vars: IndexMap<String, Value>,
}

/// What an importer does with what it leaves out of a body.
pub trait Scrub {
    /// The value written in place of the credential in the field `name`.
    fn credential(&mut self, name: &str) -> Value;
    /// Tells the user `message`.
    fn note(&mut self, message: String);
}

/// Reads data of one content type: the data, the vars already taken, the scrub.
type Reader = fn(&str, &IndexMap<String, Value>, &mut dyn Scrub) -> Body;

/// The readers by a word their content type holds; any other type is text.
const READERS: [(&str, Reader); 2] = [("json", json), ("x-www-form-urlencoded", form)];

/// A body as a request sent it: its data and content type.
pub struct Sent<'a> {
    pub data: &'a str,
    pub content_type: &'a str,
}

/// The body `sent` gives; `taken` holds the path's vars.
pub fn read(sent: &Sent<'_>, taken: &IndexMap<String, Value>, scrub: &mut dyn Scrub) -> Body {
    let lower = sent.content_type.to_ascii_lowercase();
    let read = READERS.iter().find(|(word, _)| lower.contains(word)).map_or(text as Reader, |(_, read)| *read);
    Body { content_type: Some(sent.content_type.to_owned()), ..read(sent.data, taken, scrub) }
}

fn text(data: &str, _: &IndexMap<String, Value>, _: &mut dyn Scrub) -> Body {
    Body { value: Some(json!(data)), ..Body::default() }
}

/// A JSON body, each top-level field a var unless its name is taken; not JSON, it is text.
fn json(data: &str, taken: &IndexMap<String, Value>, scrub: &mut dyn Scrub) -> Body {
    let Ok(value) = serde_json::from_str::<Value>(data) else {
        scrub.note("the body is not valid JSON, so it is kept as text".to_owned());
        return text(data, taken, scrub);
    };
    let fields = match scrubbed(value, scrub) {
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

/// `value` with each field named like a credential, at any depth, scrubbed.
fn scrubbed(value: Value, scrub: &mut dyn Scrub) -> Value {
    match value {
        Value::Object(fields) => {
            let mut kept = Map::new();
            for (name, value) in fields {
                let value = if credential(&name) { scrub.credential(&name) } else { scrubbed(value, scrub) };
                kept.insert(name, value);
            }
            Value::Object(kept)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(|item| scrubbed(item, scrub)).collect()),
        value => value,
    }
}

/// A form body as a map of its decoded fields.
fn form(data: &str, _: &IndexMap<String, Value>, scrub: &mut dyn Scrub) -> Body {
    let fields = pairs(data).map(|(name, value)| field(name, json!(value), scrub));
    Body { value: Some(Value::Object(fields.collect())), ..Body::default() }
}

/// A form field, scrubbed when it holds a credential.
pub fn field(name: String, value: Value, scrub: &mut dyn Scrub) -> (String, Value) {
    let value = if credential(&name) { scrub.credential(&name) } else { value };
    (name, value)
}
