//! A request's body by its mode, read by the shared body rules once its variables are template
//! text: `raw` by its content type (the header's, else its language's), `urlencoded` as a
//! form, `graphql` as JSON with its query and variables. A multipart or file body is left out
//! with a note.

use serde_json::{Map, Value, json};

use super::collection::Body;
use super::vars::Filling;
use crate::body::{self as shared, Scrub, Sent, field, read};

/// Reads a body of one mode, given the content type its header names.
type Mode = fn(&Body, Option<&str>, &mut Filling<'_, '_>) -> shared::Body;

/// The readers by mode; any other mode has no body.
const MODES: [(&str, Mode); 5] =
    [("raw", raw), ("urlencoded", urlencoded), ("graphql", graphql), ("formdata", formdata), ("file", file)];

/// The content type a raw body's language gives when no header names one.
const LANGUAGES: [(&str, &str); 5] = [
    ("json", "application/json"),
    ("xml", "application/xml"),
    ("html", "text/html"),
    ("javascript", "application/javascript"),
    ("text", "text/plain"),
];

/// The body `body` sends, under the content type its header names.
pub fn body(body: Option<&Body>, content_type: Option<&str>, filling: &mut Filling<'_, '_>) -> shared::Body {
    let Some(body) = body.filter(|body| !body.disabled) else { return shared::Body::default() };
    let mode = MODES.iter().find(|(mode, _)| *mode == body.mode);
    mode.map_or_else(shared::Body::default, |(_, read)| read(body, content_type, filling))
}

fn raw(body: &Body, content_type: Option<&str>, filling: &mut Filling<'_, '_>) -> shared::Body {
    let Some(data) = body.raw.as_deref().filter(|data| !data.trim().is_empty()) else {
        return shared::Body::default();
    };
    let content_type = content_type.unwrap_or_else(|| language(body));
    let data = if content_type.contains("json") { filling.fill_json(data) } else { filling.fill(data) };
    read(&Sent { data: &data, content_type }, &filling.vars, filling.secrets)
}

/// The content type of a raw body's language, plain text when it names none.
fn language(body: &Body) -> &'static str {
    let named = body.options.as_ref().and_then(|options| options.pointer("/raw/language")).and_then(Value::as_str);
    let found = LANGUAGES.iter().find(|(language, _)| Some(*language) == named);
    found.map_or("text/plain", |(_, content_type)| content_type)
}

fn urlencoded(body: &Body, content_type: Option<&str>, filling: &mut Filling<'_, '_>) -> shared::Body {
    let mut fields = Map::new();
    for param in body.urlencoded.iter().filter(|param| !param.disabled) {
        let Some(name) = param.key.clone() else { continue };
        let value = filling.fill(param.value.as_deref().unwrap_or_default());
        let (name, value) = field(name, json!(value), filling.secrets);
        fields.insert(name, value);
    }
    let content_type = content_type.unwrap_or("application/x-www-form-urlencoded");
    shared::Body {
        value: Some(Value::Object(fields)),
        content_type: Some(content_type.to_owned()),
        ..shared::Body::default()
    }
}

fn graphql(body: &Body, _: Option<&str>, filling: &mut Filling<'_, '_>) -> shared::Body {
    let Some(graphql) = &body.graphql else { return shared::Body::default() };
    let query = filling.fill(graphql.query.as_deref().unwrap_or_default());
    let variables = filling.fill_json(graphql.variables.as_deref().unwrap_or_default());
    let variables = serde_json::from_str::<Value>(&variables).unwrap_or_else(|_| json!({}));
    let data = json!({ "query": query, "variables": variables }).to_string();
    read(&Sent { data: &data, content_type: "application/json" }, &filling.vars, filling.secrets)
}

fn formdata(_: &Body, _: Option<&str>, filling: &mut Filling<'_, '_>) -> shared::Body {
    filling.secrets.note("a multipart body is not imported, so it is left out".to_owned());
    shared::Body::default()
}

fn file(_: &Body, _: Option<&str>, filling: &mut Filling<'_, '_>) -> shared::Body {
    filling.secrets.note("a file body is not imported, so it is left out".to_owned());
    shared::Body::default()
}
