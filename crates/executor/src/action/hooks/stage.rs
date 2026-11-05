//! `beforeRequest` hooks as the pipeline's hooks [`RequestStage`]: after auth, on every attempt.

use async_trait::async_trait;
use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue, Uri};
use serde_json::{Map, Value, json};
use tokio::sync::watch;
use vc_interfaces::TransportError;
use vc_model::{Failure, HookRef, RenderedRequest};

use super::{HookBridge, failure, kind, returned};
use crate::RequestStage;
use crate::action::request;

/// Runs an api's `beforeRequest` hooks in order. Each gets `{request}` with the request's
/// `method`, `url`, `path`, `query` (decoded, a list for a repeated key), `headers` and `body`,
/// and returns the request to send, or nothing to send it as it is. A changed `url` wins over a
/// changed `path` or `query`; the method cannot change.
///
/// A hook that fails stops the attempt. Its failure, with its code, goes to the call through
/// `refusals`, since a [`TransportError`] cannot carry it.
pub struct HookStage {
    bridge: HookBridge,
    hooks: Vec<HookRef>,
    refusals: watch::Sender<Option<Failure>>,
}

impl HookStage {
    /// A stage running `hooks` through `bridge`, sending each failure to `refusals`.
    pub const fn new(bridge: HookBridge, hooks: Vec<HookRef>, refusals: watch::Sender<Option<Failure>>) -> Self {
        Self { bridge, hooks, refusals }
    }

    fn refuse(&self, failure: Failure) -> TransportError {
        let reason = failure.message.clone();
        drop(self.refusals.send_replace(Some(failure)));
        TransportError::InvalidRequest { reason }
    }
}

#[async_trait]
impl RequestStage for HookStage {
    async fn apply(&self, request: RenderedRequest) -> Result<RenderedRequest, TransportError> {
        let mut request = request;
        for hook in &self.hooks {
            let shown = shown(&request);
            let input = json!({ "request": shown.clone() });
            let value = self.bridge.call(hook, input).await.map_err(|error| self.refuse(failure(hook, &error)))?;
            request = changed(request, (&shown, value)).map_err(|what| self.refuse(returned(hook, &what)))?;
        }
        Ok(request)
    }
}

/// The request as a hook sees it: the `request` root, with its `path` and `query`.
pub fn shown(request: &RenderedRequest) -> Value {
    let mut shown = request::sent(request);
    if let Value::Object(fields) = &mut shown {
        fields.insert("path".to_owned(), Value::String(request.uri.path().to_owned()));
        fields.insert("query".to_owned(), query_shown(&request.uri));
    }
    shown
}

/// `request` with what a hook changed in `old`, as `new` returns it.
fn changed(mut request: RenderedRequest, (old, new): (&Value, Value)) -> Result<RenderedRequest, String> {
    let new = match new {
        Value::Null => return Ok(request),
        Value::Object(new) => new,
        other => return Err(format!("returned {}, not a request", kind(&other))),
    };
    let field = |key: &str| new.get(key).filter(|value| old.get(key) != Some(*value));
    if let Some(headers) = field("headers") {
        request.headers = header_map(headers)?;
    }
    if let Some(body) = field("body") {
        request.body = body_bytes(body);
    }
    request.uri = url(&request.uri, (old, &new))?;
    Ok(request)
}

/// The URL to send to: a changed `url`, else the old one with a changed `path` or `query`.
fn url(uri: &Uri, (old, new): (&Value, &Map<String, Value>)) -> Result<Uri, String> {
    let field = |key: &str| new.get(key).filter(|value| old.get(key) != Some(*value));
    if let Some(url) = field("url") {
        return parse(&vc_template::to_text(url));
    }
    if field("path").is_none() && field("query").is_none() {
        return Ok(uri.clone());
    }
    let path = new.get("path").map_or_else(|| uri.path().to_owned(), vc_template::to_text);
    let query = new.get("query").map(query_text).unwrap_or_default();
    let scheme = uri.scheme_str().unwrap_or("http");
    let authority = uri.authority().map_or("", http::uri::Authority::as_str);
    let query = if query.is_empty() { String::new() } else { format!("?{query}") };
    parse(&format!("{scheme}://{authority}{path}{query}"))
}

fn parse(url: &str) -> Result<Uri, String> {
    url.parse::<Uri>().map_err(|error| format!("returned the URL `{url}`, which does not parse: {error}"))
}

/// The query's pairs, decoded, by key; a key given more than once has a list.
fn query_shown(uri: &Uri) -> Value {
    let mut pairs = Map::new();
    for pair in uri.query().unwrap_or_default().split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let (key, value) = (decode(key), Value::String(decode(value)));
        match pairs.get_mut(&key) {
            Some(Value::Array(items)) => items.push(value),
            Some(first) => *first = Value::Array(vec![first.take(), value]),
            None => drop(pairs.insert(key, value)),
        }
    }
    Value::Object(pairs)
}

fn decode(text: &str) -> String {
    percent_encoding::percent_decode_str(text).decode_utf8_lossy().into_owned()
}

/// A query from a hook's `query` object: a list repeats its key and a null leaves it out.
fn query_text(query: &Value) -> String {
    let Value::Object(given) = query else { return String::new() };
    let mut pairs = Vec::new();
    for (key, value) in given {
        let items = if let Value::Array(items) = value { items.as_slice() } else { std::slice::from_ref(value) };
        let items = items.iter().filter(|item| !item.is_null());
        pairs.extend(
            items.map(|item| format!("{}={}", request::encode(key), request::encode(&vc_template::to_text(item)))),
        );
    }
    pairs.join("&")
}

/// The headers of a hook's `headers` object; a null value leaves its header out.
fn header_map(given: &Value) -> Result<HeaderMap, String> {
    let Value::Object(given) = given else { return Err(format!("returned `headers` as {}", kind(given))) };
    let mut headers = HeaderMap::new();
    for (name, value) in given.iter().filter(|(_, value)| !value.is_null()) {
        let name = HeaderName::try_from(name.as_str())
            .map_err(|error| format!("returned `{name}`, which is not a header name: {error}"))?;
        let text = vc_template::to_text(value);
        let value = HeaderValue::try_from(text.as_str())
            .map_err(|error| format!("returned `{text}`, which is not a header value: {error}"))?;
        headers.append(name, value);
    }
    Ok(headers)
}

/// A hook's `body`: a string as its text, null as none, anything else as JSON.
fn body_bytes(body: &Value) -> Bytes {
    match body {
        Value::Null => Bytes::new(),
        Value::String(text) => Bytes::from(text.clone()),
        other => Bytes::from(other.to_string()),
    }
}
