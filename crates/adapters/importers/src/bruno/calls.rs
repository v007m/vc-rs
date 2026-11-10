//! A Bruno collection's requests in the form the collection importers draft from: each URL, its
//! path parameters, its enabled query and headers, where its auth sends a credential, its body
//! by mode, and a note for each script, test or assertion, which are not imported. A
//! post-response var that reads a field of the response body (`res.body.id`) is a step's save.

use indexmap::IndexMap;

use super::collection::{Auth, Body, Called, Pair, Request};
use crate::collected::{Call, Credential, GraphQl, Payload, path};

/// Reads a body of one mode.
type Mode = fn(&Body) -> Option<Payload>;

/// The readers by mode; `none` and any other mode have no body.
const MODES: [(&str, Mode); 8] = [
    ("json", json),
    ("text", text),
    ("xml", xml),
    ("sparql", sparql),
    ("formUrlEncoded", form),
    ("graphql", graphql),
    ("multipartForm", multipart),
    ("file", file),
];

/// The expressions a post-response var reads the response body through.
const BODIES: [&str; 2] = ["res.body", "res.getBody()"];

/// Each request of `called`, in order.
pub fn calls(called: &[Called<'_>]) -> Vec<Call> {
    called.iter().map(call).collect()
}

/// The enabled pairs that have a name, each with its value.
pub fn enabled<'p>(pairs: impl IntoIterator<Item = &'p Pair>) -> Vec<(String, String)> {
    let pairs = pairs.into_iter().filter(|pair| pair.enabled && !pair.name.is_empty());
    pairs.map(|pair| (pair.name.clone(), pair.value.clone())).collect()
}

/// What the request's post-response vars save: each that reads a field of the response body.
pub fn saves(request: &Request) -> IndexMap<String, String> {
    let vars = request.vars.as_ref().map(|vars| enabled(&vars.res)).unwrap_or_default();
    vars.into_iter().filter_map(|(name, value)| Some((name, saved(value.trim())?))).collect()
}

fn call(called: &Called<'_>) -> Call {
    let request = called.request;
    let queried = request.params.iter().any(|param| param.kind == "query");
    Call {
        name: called.name.to_owned(),
        method: request.method.to_ascii_uppercase(),
        url: request.url.clone(),
        path_values: params(request, "path"),
        query: queried.then(|| params(request, "query")),
        headers: enabled(called.headers.iter().copied()).into_iter().map(|(name, value)| (name, Some(value))).collect(),
        credential: called.auth.and_then(credential),
        body: request.body.as_ref().and_then(payload),
        notes: notes(request),
    }
}

/// The enabled parameters of `kind` (`query` or `path`).
fn params(request: &Request, kind: &str) -> Vec<(String, String)> {
    let params = request.params.iter().filter(|param| param.kind == kind && param.enabled);
    params.map(|param| (param.name.clone(), param.value.clone())).collect()
}

/// Where `auth` sends a credential: an api key in the query or a header, any other auth in
/// the authorization header.
fn credential(auth: &Auth) -> Option<Credential> {
    if !auth.sends() {
        return None;
    }
    let Some(key) = auth.apikey.as_ref().filter(|_| auth.mode == "apikey") else {
        return Some(Credential::Header("authorization".to_owned()));
    };
    let name = if key.key.is_empty() { "x-api-key".to_owned() } else { key.key.clone() };
    Some(if key.placement == "queryparams" { Credential::Query(name) } else { Credential::Header(name) })
}

fn payload(body: &Body) -> Option<Payload> {
    let (_, read) = MODES.iter().find(|(mode, _)| *mode == body.mode)?;
    read(body)
}

fn raw(text: Option<&String>, content_type: &'static str) -> Option<Payload> {
    Some(Payload::Raw { text: text?.clone(), content_type })
}

fn json(body: &Body) -> Option<Payload> {
    raw(body.json.as_ref(), "application/json")
}

fn text(body: &Body) -> Option<Payload> {
    raw(body.text.as_ref(), "text/plain")
}

fn xml(body: &Body) -> Option<Payload> {
    raw(body.xml.as_ref(), "application/xml")
}

fn sparql(body: &Body) -> Option<Payload> {
    raw(body.sparql.as_ref(), "application/sparql-query")
}

#[expect(clippy::unnecessary_wraps, reason = "every mode has the `Mode` signature")]
fn form(body: &Body) -> Option<Payload> {
    Some(Payload::Form(enabled(&body.form_url_encoded)))
}

fn graphql(body: &Body) -> Option<Payload> {
    body.graphql.as_ref().map(GraphQl::payload)
}

#[expect(clippy::unnecessary_wraps, reason = "every mode has the `Mode` signature")]
const fn multipart(_: &Body) -> Option<Payload> {
    Some(Payload::Left("a multipart body is not imported, so it is left out"))
}

#[expect(clippy::unnecessary_wraps, reason = "every mode has the `Mode` signature")]
const fn file(_: &Body) -> Option<Payload> {
    Some(Payload::Left("a file body is not imported, so it is left out"))
}

/// The template a post-response var's `value` saves (`res.body.items[0].id` saves
/// `${response.body.items[0].id}`), when it reads a field of the body.
fn saved(value: &str) -> Option<String> {
    let rest = BODIES.iter().find_map(|body| value.strip_prefix(body))?;
    Some(format!("${{response.body{}}}", path(rest)?))
}

/// A note for each part of the request that is not imported.
fn notes(request: &Request) -> Vec<String> {
    let script = request.script.as_ref();
    let vars = request.vars.as_ref();
    let left = [
        (script.and_then(|script| script.req.as_deref()), "the pre-request script is not imported"),
        (script.and_then(|script| script.res.as_deref()), "the post-response script is not imported"),
        (request.tests.as_deref(), "the tests are not imported"),
    ];
    let written = left.into_iter().filter(|(text, _)| text.is_some_and(|text| !text.trim().is_empty()));
    let mut notes: Vec<String> = written.map(|(_, note)| note.to_owned()).collect();
    if request.assertions.iter().any(|assertion| assertion.enabled) {
        notes.push("the assertions are not imported".to_owned());
    }
    if vars.is_some_and(|vars| vars.req.iter().any(|var| var.enabled)) {
        notes.push("the request's vars are not imported".to_owned());
    }
    notes
}
