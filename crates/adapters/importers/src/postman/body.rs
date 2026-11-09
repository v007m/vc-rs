//! A request's body by its mode: `raw` as text with its language's content type, `urlencoded`
//! as a form, `graphql` as a query and its variables. A multipart or file body is left out with
//! a note.

use serde_json::Value;

use super::collection::{Body, enabled};
use crate::collected::Payload;

/// Reads a body of one mode.
type Mode = fn(&Body) -> Option<Payload>;

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

/// The body `body` sends, when it sends one.
pub fn payload(body: Option<&Body>) -> Option<Payload> {
    let body = body.filter(|body| !body.disabled)?;
    let (_, read) = MODES.iter().find(|(mode, _)| *mode == body.mode)?;
    read(body)
}

fn raw(body: &Body) -> Option<Payload> {
    Some(Payload::Raw { text: body.raw.clone()?, content_type: language(body) })
}

/// The content type of a raw body's language, plain text when it names none.
fn language(body: &Body) -> &'static str {
    let named = body.options.as_ref().and_then(|options| options.pointer("/raw/language")).and_then(Value::as_str);
    let found = LANGUAGES.iter().find(|(language, _)| Some(*language) == named);
    found.map_or("text/plain", |(_, content_type)| content_type)
}

#[expect(clippy::unnecessary_wraps, reason = "every mode has the `Mode` signature")]
fn urlencoded(body: &Body) -> Option<Payload> {
    Some(Payload::Form(enabled(&body.urlencoded)))
}

fn graphql(body: &Body) -> Option<Payload> {
    let graphql = body.graphql.as_ref()?;
    let query = graphql.query.clone().unwrap_or_default();
    Some(Payload::GraphQl { query, variables: graphql.variables.clone().unwrap_or_default() })
}

#[expect(clippy::unnecessary_wraps, reason = "every mode has the `Mode` signature")]
const fn formdata(_: &Body) -> Option<Payload> {
    Some(Payload::Left("a multipart body is not imported, so it is left out"))
}

#[expect(clippy::unnecessary_wraps, reason = "every mode has the `Mode` signature")]
const fn file(_: &Body) -> Option<Payload> {
    Some(Payload::Left("a file body is not imported, so it is left out"))
}
