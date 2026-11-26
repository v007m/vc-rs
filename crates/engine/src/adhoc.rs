//! The web UI Client's ad hoc call (D540, D541): a method, URL, headers, query, body and auth
//! with no api behind them. It is compiled as an api in a file that is never written, so its
//! templates, auth, redaction and history are an api call's.

use std::borrow::Cow;

use serde_json::{Map, Value};
use vc_diagnostics::{Category, Diagnostics};
use vc_model::SourceFiles;

use crate::yaml_out::fields_item;
use crate::{AdhocCall, CallRequest, EngineError, FileChange};

/// Where the ad hoc call's api file would be; nothing is written there.
const PATH: &str = "services/vc-adhoc/vc-adhoc.api.yaml";

/// The full ID of the ad hoc call's api.
pub const API_ID: &str = "vc-adhoc#call";

/// What the call may speak; the first is the default.
const KINDS: [&str; 3] = ["http", "sse", "websocket"];

/// `diagnostics` without the convention lints (V4xxx) of the ad hoc call's file, when `changes`
/// compiled one in: nobody wrote that file, so its names and URL style are nobody's to fix
/// (D545). Every other diagnostic of it stays, as the call's own.
pub fn unlinted(diagnostics: Diagnostics, (files, changes): (&SourceFiles, &[FileChange])) -> Diagnostics {
    if !changes.iter().any(|change| change.path == PATH) {
        return diagnostics;
    }
    let ours = |diagnostic: &vc_diagnostics::Diagnostic| {
        files.get(diagnostic.primary.span.file).is_some_and(|file| file.path.as_str() == PATH)
    };
    diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code.category() != Category::Convention || !ours(diagnostic))
        .collect()
}

/// `request`, an ad hoc call's naming its api; any other as it is.
///
/// # Errors
///
/// [`EngineError::Setting`] when an ad hoc call also names an api or a draft.
pub fn resolved(request: &CallRequest) -> Result<Cow<'_, CallRequest>, EngineError> {
    if request.adhoc.is_none() {
        return Ok(Cow::Borrowed(request));
    }
    if !request.api.is_empty() || request.draft.is_some() {
        return Err(invalid("an ad hoc call names no api and no draft", "send `api` or `adhoc`, not both"));
    }
    Ok(Cow::Owned(CallRequest { api: API_ID.to_owned(), ..request.clone() }))
}

/// What the call is shown as before it is sent: its URL as written, or the api's ID.
pub fn target(request: &CallRequest) -> &str {
    request.adhoc.as_ref().map_or(request.api.as_str(), |call| call.url.as_str())
}

/// The api file `call` is compiled from.
///
/// # Errors
///
/// [`EngineError::Setting`] for a kind other than `http`, `sse` or `websocket`, a `websocket`
/// call with a method or body, a URL that is neither whole nor a template, or a JSON body that
/// is a `$` form.
pub fn file(call: &AdhocCall) -> Result<FileChange, EngineError> {
    let mut out = String::from("apis:\n");
    fields_item(&mut out, 2, &fields(call)?);
    Ok(FileChange { path: PATH.to_owned(), before: None, after: out, moved_from: None })
}

/// The api's keys, `id` first. No auth is `auth: none`, so the project's default auth is never
/// sent to a URL it was not written for.
fn fields(call: &AdhocCall) -> Result<Vec<(&'static str, Value)>, EngineError> {
    let kind = kind(call)?;
    let mut fields = vec![("id", Value::from("call"))];
    if kind != KINDS[0] {
        fields.push(("type", Value::from(kind)));
    }
    if !call.method.is_empty() {
        fields.push(("method", Value::from(call.method.as_str())));
    }
    fields.push(("path", Value::from(url(&call.url)?)));
    let auth = call.auth.as_deref().filter(|auth| !auth.is_empty()).unwrap_or("none");
    fields.push(("auth", Value::from(auth)));
    for (key, pairs) in [("headers", &call.headers), ("query", &call.query)] {
        if !pairs.is_empty() {
            fields.push((key, Value::Object(map(pairs))));
        }
    }
    if let Some(body) = &call.body {
        fields.push(("body", body_value(body)?));
    }
    Ok(fields)
}

/// The call's kind, `http` when empty; a `websocket` call has no method and no body.
fn kind(call: &AdhocCall) -> Result<&'static str, EngineError> {
    let given = if call.kind.is_empty() { KINDS[0] } else { call.kind.as_str() };
    let Some(kind) = KINDS.into_iter().find(|kind| *kind == given) else {
        return Err(invalid(&format!("no call type `{given}`"), "send `http`, `sse` or `websocket`"));
    };
    if kind == "websocket" && (!call.method.is_empty() || call.body.is_some()) {
        return Err(invalid("a `websocket` call opens with a GET and no body", "send it with no method and no body"));
    }
    Ok(kind)
}

/// `url`, when it is whole (`https://…`, `ws://…`) or starts with a template.
fn url(url: &str) -> Result<&str, EngineError> {
    if url.contains("://") || url.starts_with("${") {
        return Ok(url);
    }
    Err(invalid(&format!("`{url}` is no whole URL"), "write it as `https://api.test/users` or `${var.usersUrl}/users`"))
}

/// `pairs` as a map; a later pair of the same name wins.
fn map(pairs: &[(String, String)]) -> Map<String, Value> {
    pairs.iter().map(|(name, value)| (name.clone(), Value::from(value.as_str()))).collect()
}

/// `body` as JSON when it is a JSON map or list, else as the text it is. A map with a `$` key
/// is refused, so an ad hoc body never reads a project file (`$file`) or a payload.
fn body_value(body: &str) -> Result<Value, EngineError> {
    match serde_json::from_str::<Value>(body) {
        Ok(Value::Object(map)) if map.keys().any(|key| key.starts_with('$')) => Err(invalid(
            "an ad hoc body is JSON or text, not a `$` form",
            "send the content itself, or write the call as an api",
        )),
        Ok(value @ (Value::Object(_) | Value::Array(_))) => Ok(value),
        _ => Ok(Value::from(body)),
    }
}

/// The error of a bad ad hoc call.
fn invalid(reason: &str, help: &str) -> EngineError {
    EngineError::Setting { origin: "ad hoc call".to_owned(), reason: reason.to_owned(), help: Some(help.to_owned()) }
}

#[cfg(test)]
#[path = "adhoc_tests.rs"]
mod tests;
