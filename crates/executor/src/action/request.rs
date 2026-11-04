//! Renders a `call:`'s request (the method, the URL with its query, the headers and the body),
//! and turns the exchange into the `request` and `response` roots.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use bytes::Bytes;
use http::header::CONTENT_TYPE;
use http::{HeaderMap, HeaderName, HeaderValue, Uri};
use indexmap::IndexMap;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC};
use serde_json::{Map, Value, json};
use vc_diagnostics::Code;
use vc_model::{Action, Api, BodySpec, Exchange, Failure, Method, RenderedRequest, Template, Timing};
use vc_template::RenderError;

use crate::Render;

/// RFC 3986 unreserved characters stay as they are in a query.
const UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

const JSON: &str = "application/json";
const TEXT: &str = "text/plain; charset=utf-8";
const OCTETS: &str = "application/octet-stream";

/// The request `action` sends to `api`: the action's query, headers and body over the api's.
pub fn rendered(render: &Render<'_>, api: &Api, action: &Action) -> Result<RenderedRequest, Failure> {
    let mut request = RenderedRequest::new(method(api.method), uri(render, api, action)?);
    request.headers = headers(render, api, action)?;
    if let Some((body, kind)) = body(render, action.body.as_ref().unwrap_or(&api.body))? {
        request.body = body;
        if !request.headers.contains_key(CONTENT_TYPE) {
            request.headers.insert(CONTENT_TYPE, HeaderValue::from_static(kind));
        }
    }
    Ok(request)
}

/// The api file's `baseUrl`, the api's `path` and the query.
fn uri(render: &Render<'_>, api: &Api, action: &Action) -> Result<Uri, Failure> {
    let base = render.program().api_groups.get(&api.group).and_then(|group| group.base_url.as_ref());
    let mut url = String::new();
    for template in base.into_iter().chain([&api.path]) {
        url.push_str(&render.text(template).map_err(failed)?);
    }
    let query = query(render, api, action)?;
    if !query.is_empty() {
        url.push(if url.contains('?') { '&' } else { '?' });
        url.push_str(&query);
    }
    url.parse::<Uri>().map_err(|error| rendering(&format!("`{url}` is not a URL: {error}")))
}

/// The api's query with the action's over it; a null value leaves its key out and a list
/// repeats it.
fn query(render: &Render<'_>, api: &Api, action: &Action) -> Result<String, Failure> {
    let mut merged: IndexMap<&str, &Template> = api.query.iter().map(|(key, value)| (key.as_str(), value)).collect();
    merged.extend(action.query.iter().map(|(key, value)| (key.as_str(), value)));
    let mut pairs = Vec::new();
    for (key, template) in merged {
        let items = match render.template(template).map_err(failed)? {
            Value::Array(items) => items,
            value => vec![value],
        };
        let items = items.iter().filter(|item| !item.is_null());
        pairs.extend(items.map(|item| format!("{}={}", encode(key), encode(&vc_template::to_text(item)))));
    }
    Ok(pairs.join("&"))
}

fn encode(text: &str) -> String {
    percent_encoding::utf8_percent_encode(text, UNRESERVED).to_string()
}

/// The config's default headers, then the api file's, the api's and the action's; a later one
/// replaces an earlier one, and a null value removes it.
fn headers(render: &Render<'_>, api: &Api, action: &Action) -> Result<HeaderMap, Failure> {
    let program = render.program();
    let group = program.api_groups.get(&api.group).map(|group| &group.headers);
    let levels = [Some(&program.config.defaults.headers), group, Some(&api.headers), Some(&action.headers)];
    let mut headers = HeaderMap::new();
    for (name, template) in levels.into_iter().flatten().flatten() {
        let name = HeaderName::try_from(name.as_str())
            .map_err(|error| rendering(&format!("`{name}` is not a header name: {error}")))?;
        match header_value(render, template)? {
            Some(value) => drop(headers.insert(name, value)),
            None => drop(headers.remove(name)),
        }
    }
    Ok(headers)
}

fn header_value(render: &Render<'_>, template: &Template) -> Result<Option<HeaderValue>, Failure> {
    let value = render.template(template).map_err(failed)?;
    if value.is_null() {
        return Ok(None);
    }
    let text = vc_template::to_text(&value);
    let value = HeaderValue::try_from(text.as_str());
    value.map(Some).map_err(|error| rendering(&format!("`{text}` is not a header value: {error}")))
}

/// A body's bytes and the `content-type` it implies.
type Body = (Bytes, &'static str);

/// The body's bytes and the `content-type` it implies.
fn body(render: &Render<'_>, spec: &BodySpec) -> Result<Option<Body>, Failure> {
    let body = match spec {
        BodySpec::None => return Ok(None),
        BodySpec::Json(value) => (Bytes::from(render.value(value).map_err(failed)?.to_string()), JSON),
        BodySpec::Text(template) => (Bytes::from(render.text(template).map_err(failed)?), TEXT),
        BodySpec::Base64(template) => (decode(&render.text(template).map_err(failed)?)?, OCTETS),
        BodySpec::Extend { base, changes, remove } => {
            let base = render.template(base).map_err(failed)?;
            let merged = vc_template::extend(base, render.value(changes).map_err(failed)?, remove);
            (Bytes::from(merged.to_string()), JSON)
        }
        BodySpec::File(_) | BodySpec::Form(_) | BodySpec::Multipart(_) => {
            return Err(rendering("`$file`, form and multipart bodies arrive in a later milestone"));
        }
    };
    Ok(Some(body))
}

fn decode(text: &str) -> Result<Bytes, Failure> {
    let bytes = STANDARD.decode(text.trim());
    bytes.map(Bytes::from).map_err(|error| rendering(&format!("the `$base64` body does not decode: {error}")))
}

/// The `request` root: the request as sent.
pub fn sent(request: &RenderedRequest) -> Value {
    let method = request.method.as_str();
    json!({ "method": method, "url": request.uri.to_string(), "headers": header_map(&request.headers), "body": body_value(&request.body) })
}

/// The `response` root, and an action id's value in later actions.
pub fn received(exchange: &Exchange) -> Value {
    let response = &exchange.response;
    json!({
        "status": response.status.as_u16(),
        "headers": header_map(&response.headers),
        "body": body_value(&response.body),
        "timing": timing(&exchange.timing),
    })
}

/// Headers by lower-case name; repeated ones joined with `, `.
fn header_map(headers: &HeaderMap) -> Value {
    let mut map = Map::new();
    for name in headers.keys() {
        let values = headers.get_all(name).iter().map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned());
        map.insert(name.as_str().to_owned(), Value::String(values.collect::<Vec<_>>().join(", ")));
    }
    Value::Object(map)
}

/// JSON when the body parses as JSON, else its text.
fn body_value(body: &Bytes) -> Value {
    serde_json::from_slice(body).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(body).into_owned()))
}

/// The phases in milliseconds; those the transport did not measure are left out.
fn timing(timing: &Timing) -> Value {
    let ms = |duration: std::time::Duration| json!(duration.as_secs_f64() * 1000.0);
    let mut map = Map::new();
    map.insert("total".into(), ms(timing.total));
    map.insert("firstByte".into(), ms(timing.first_byte));
    map.insert("download".into(), ms(timing.download));
    let phases = [("dns", timing.dns), ("connect", timing.connect), ("tls", timing.tls)];
    map.extend(phases.into_iter().filter_map(|(name, phase)| Some((name.to_owned(), ms(phase?)))));
    Value::Object(map)
}

/// A template that did not render, as a V5111 failure.
pub fn failed(error: RenderError) -> Failure {
    Failure::new(error.message).with_code(Code::V5111.to_string())
}

/// A request that cannot be built, as a V5111 failure.
pub fn rendering(message: &str) -> Failure {
    Failure::new(message).with_code(Code::V5111.to_string())
}

/// The `http` crate's name for `method`.
pub const fn method(method: Method) -> http::Method {
    match method {
        Method::Get => http::Method::GET,
        Method::Post => http::Method::POST,
        Method::Put => http::Method::PUT,
        Method::Patch => http::Method::PATCH,
        Method::Delete => http::Method::DELETE,
        Method::Head => http::Method::HEAD,
        Method::Options => http::Method::OPTIONS,
    }
}
