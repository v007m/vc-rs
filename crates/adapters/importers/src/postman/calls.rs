//! A collection's requests in the form the collection importers draft from: each URL as text,
//! its path variables, its enabled query and headers, where its auth sends a credential, its
//! body, and a note for each script, which is not imported apart from the values a test script
//! saves (see [`super::script`]).

use indexmap::IndexMap;
use serde_json::Value;

use super::body::payload;
use super::collection::{Auth, Called, Url, Variable, enabled, script};
use super::script::saves;
use crate::collected::{Call, Credential};

/// Each request of `called`, in order.
pub fn calls(called: &[Called<'_>]) -> Vec<Call> {
    called.iter().map(call).collect()
}

/// The variables with a value, as text.
pub fn values(variables: &[Variable]) -> IndexMap<String, String> {
    let values = variables.iter().filter_map(|variable| Some((variable.key.clone()?, text(variable.value.as_ref()))));
    values.filter(|(_, value)| !value.is_empty()).collect()
}

fn call(called: &Called<'_>) -> Call {
    let request = called.request;
    let url = request.url.as_ref();
    let headers = request.header.iter().filter(|header| !header.disabled);
    Call {
        name: called.name.to_owned(),
        method: request.method.clone(),
        url: url_text(url),
        path_values: path_values(url),
        query: query(url),
        headers: headers.map(|header| (header.key.clone(), header.value.clone())).collect(),
        credential: called.auth.and_then(credential),
        body: payload(request.body.as_ref()),
        notes: script_notes(called),
    }
}

/// The URL as text: as written, else from its parts.
fn url_text(url: Option<&Url>) -> String {
    let parts = match url {
        None => return String::new(),
        Some(Url::Raw(raw)) => return raw.clone(),
        Some(Url::Parts(parts)) => parts,
    };
    if let Some(raw) = &parts.raw {
        return raw.clone();
    }
    let joined = |value: Option<&Value>, by: &str| match value {
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(by),
        Some(Value::String(text)) => text.clone(),
        _ => String::new(),
    };
    let host = joined(parts.host.as_ref(), ".");
    let scheme = parts.protocol.as_deref().map(|protocol| format!("{protocol}://")).unwrap_or_default();
    format!("{scheme}{host}/{}", joined(parts.path.as_ref(), "/"))
}

/// The enabled query parameters of a URL in parts; none for one written as text.
fn query(url: Option<&Url>) -> Option<Vec<(String, String)>> {
    let Some(Url::Parts(parts)) = url else { return None };
    Some(enabled(&parts.query))
}

/// The value of each path variable the URL names.
fn path_values(url: Option<&Url>) -> Vec<(String, String)> {
    let Some(Url::Parts(parts)) = url else { return Vec::new() };
    let values = parts.variable.iter();
    values.filter_map(|variable| Some((variable.key.clone()?, text(variable.value.as_ref())))).collect()
}

/// A variable's value as text; one that is not text, a number or a flag is empty.
fn text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(value @ (Value::Number(_) | Value::Bool(_))) => value.to_string(),
        _ => String::new(),
    }
}

/// Where `auth` sends a credential: an api key in the query or a header, any other auth in
/// the authorization header.
fn credential(auth: &Auth) -> Option<Credential> {
    if !auth.sends() {
        return None;
    }
    if auth.kind != "apikey" {
        return Some(Credential::Header("authorization".to_owned()));
    }
    let name = auth.setting("key").filter(|name| !name.is_empty()).unwrap_or("x-api-key").to_owned();
    Some(if auth.setting("in") == Some("query") { Credential::Query(name) } else { Credential::Header(name) })
}

/// A note for each script of the request, which is not imported.
fn script_notes(called: &Called<'_>) -> Vec<String> {
    let mut notes = Vec::new();
    if script(called.events, "prerequest").is_some() {
        notes.push("the pre-request script is not imported".to_owned());
    }
    if let Some(test) = script(called.events, "test") {
        let saved = if saves(&test).is_empty() { "" } else { ", apart from the values it saves" };
        notes.push(format!("the test script is not imported{saved}"));
    }
    notes
}
