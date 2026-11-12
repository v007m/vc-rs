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
use vc_model::{
    Action, ActionId, Api, BodySpec, Exchange, Failure, InlineRequest, Method, Part, Reference, RenderedRequest,
    Template, Timing,
};
use vc_template::RenderError;

use crate::Render;
use crate::action::encode::{self, Encoder, Encoding};
use crate::action::media::{self, OCTETS};
use crate::action::path;

/// RFC 3986 unreserved characters stay as they are in a query.
const UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

pub const JSON: &str = "application/json";
const TEXT: &str = "text/plain; charset=utf-8";

/// The request action `id` sends to `api`: the action's query, headers and body over the api's.
pub fn rendered(render: &Render<'_>, api: &Api, (id, action): (ActionId, &Action)) -> Result<RenderedRequest, Failure> {
    let mut request = RenderedRequest::new(method(api.method), uri(render, api, action)?);
    request.headers = headers(render, api, action)?;
    with_body(render, request, (id, action.body.as_ref().unwrap_or(&api.body)))
}

/// The request an inline step (a `flow` auth's) sends: its method, URL, query, headers and
/// body as written, with the config's default headers under its own.
pub fn inline(render: &Render<'_>, (id, inline): (ActionId, &InlineRequest)) -> Result<RenderedRequest, Failure> {
    let url = render.text(&inline.url).map_err(failed)?;
    let uri = with_query(url, &query(render, [&inline.query])?)?;
    let mut request = RenderedRequest::new(method(inline.method), uri);
    let levels = [&render.program().config.defaults.headers, &inline.headers];
    request.headers = header_levels(render, levels.into_iter().flatten())?;
    with_body(render, request, (id, &inline.body))
}

/// `request` with the body `spec` that action `id` sends, in the encoding its `content-type`
/// names, and the `content-type` the body implies unless one is set.
fn with_body(
    render: &Render<'_>,
    mut request: RenderedRequest,
    (action, spec): (ActionId, &BodySpec),
) -> Result<RenderedRequest, Failure> {
    let written = request.headers.get(CONTENT_TYPE).cloned();
    let sending = Sending::new(action, written.as_ref());
    if let Some((body, kind)) = body(render, spec, &sending)? {
        request.body = body;
        match kind {
            ContentType::Replaced(value) => drop(request.headers.insert(CONTENT_TYPE, value)),
            ContentType::Implied(kind) if written.is_none() => {
                drop(request.headers.insert(CONTENT_TYPE, HeaderValue::from_static(kind)));
            }
            ContentType::Implied(_) => {}
        }
    }
    Ok(request)
}

/// The api file's `baseUrl` as written, the api's `path` with each value encoded (D278), and
/// the query.
pub fn uri(render: &Render<'_>, api: &Api, action: &Action) -> Result<Uri, Failure> {
    let base = render.program().api_groups.get(&api.group).and_then(|group| group.base_url.as_ref());
    let mut url = base.map(|base| render.text(base)).transpose().map_err(failed)?.unwrap_or_default();
    url.push_str(&path::rendered(render, &api.path)?);
    if base.is_none() && !url.contains("://") {
        return Err(unbased(render, api, &url));
    }
    with_query(url, &query(render, [&api.query, &action.query])?)
}

/// The V5111 failure of `api`, whose file has no `baseUrl`, sending to the relative `url`.
fn unbased(render: &Render<'_>, api: &Api, url: &str) -> Failure {
    let program = render.program();
    let name = program.str(api.id.sym()).unwrap_or_default();
    let file = name.split_once('#').map_or(name, |(file, _)| file);
    let var = format!("{}Url", file.rsplit('.').next().unwrap_or(file));
    let names = render.values().profile_names();
    let profiles = if names.is_empty() { "no profile".to_owned() } else { format!("profile {}", names.join(", ")) };
    rendering(&format!(
        "`{name}` sends to `{url}`, which is not a full URL: no `baseUrl` is set for `{file}` ({profiles}); \
         help: add `baseUrl: ${{var.{var}}}` to its api file and set `{var}` in the config, a profile, or with \
         `--set vars.{var}=http://…`"
    ))
}

/// `url` with `query` appended, parsed.
fn with_query(mut url: String, query: &str) -> Result<Uri, Failure> {
    if !query.is_empty() {
        url.push(if url.contains('?') { '&' } else { '?' });
        url.push_str(query);
    }
    url.parse::<Uri>().map_err(|error| rendering(&format!("`{url}` is not a URL: {error}")))
}

/// The query `levels` make, each over the ones before it; a null value leaves its key out and
/// a list repeats it.
fn query<const N: usize>(render: &Render<'_>, levels: [&IndexMap<String, Template>; N]) -> Result<String, Failure> {
    let merged: IndexMap<&str, &Template> =
        levels.into_iter().flatten().map(|(key, value)| (key.as_str(), value)).collect();
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

/// `reference` as a template of its own.
pub fn lone(reference: Reference) -> Template {
    Template { parts: vec![Part::Ref(reference)] }
}

/// `text` percent-encoded for a query, leaving only the unreserved characters.
pub fn encode(text: &str) -> String {
    percent_encoding::utf8_percent_encode(text, UNRESERVED).to_string()
}

/// The config's default headers, then the api file's, the api's and the action's; a later one
/// replaces an earlier one, and a null value removes it.
fn headers(render: &Render<'_>, api: &Api, action: &Action) -> Result<HeaderMap, Failure> {
    let program = render.program();
    let group = program.api_groups.get(&api.group).map(|group| &group.headers);
    let levels = [Some(&program.config.defaults.headers), group, Some(&api.headers), Some(&action.headers)];
    header_levels(render, levels.into_iter().flatten().flatten())
}

/// `templates` rendered in order into one map; a later one replaces an earlier one, and a
/// null value removes it.
fn header_levels<'t>(
    render: &Render<'_>,
    templates: impl Iterator<Item = (&'t String, &'t Template)>,
) -> Result<HeaderMap, Failure> {
    let mut headers = HeaderMap::new();
    for (name, template) in templates {
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

/// The `content-type` a body is sent with.
enum ContentType {
    /// The one the body implies, sent unless one is written.
    Implied(&'static str),
    /// The one written, rewritten by its encoder: a multipart boundary added (D274).
    Replaced(HeaderValue),
}

/// A body's bytes and its `content-type`.
type Body = (Bytes, ContentType);

/// How an action's body is sent: by which action, and the encoder its written `content-type`
/// names with that header.
struct Sending<'h> {
    action: ActionId,
    encode: Option<(Encoder, &'h HeaderValue)>,
}

impl<'h> Sending<'h> {
    /// How action `action` sends its body under the written `content_type`.
    fn new(action: ActionId, content_type: Option<&'h HeaderValue>) -> Self {
        let encode = content_type.and_then(|written| Some((encode::encoder(written)?, written)));
        Self { action, encode }
    }
}

/// The body's bytes and its `content-type`; a map is encoded by the encoder the `content-type`
/// names, if any.
fn body(render: &Render<'_>, spec: &BodySpec, sending: &Sending<'_>) -> Result<Option<Body>, Failure> {
    let body = match spec {
        BodySpec::None => return Ok(None),
        BodySpec::Json(value) => structured(render, render.value(value).map_err(failed)?, sending)?,
        BodySpec::Text(template) => text_body(render, template, sending)?,
        BodySpec::Base64(template) => (decode(&render.text(template).map_err(failed)?)?, ContentType::Implied(OCTETS)),
        BodySpec::Extend { base, changes, remove } => {
            let base = render.template(base).map_err(failed)?;
            let value = vc_template::extend(base, render.value(changes).map_err(failed)?, remove).map_err(failed)?;
            structured(render, value, sending)?
        }
        BodySpec::File(path) => file(render, path, sending.action)?,
        BodySpec::Form(_) | BodySpec::Multipart(_) => {
            return Err(rendering("form and multipart bodies are written as a map with a `content-type`"));
        }
    };
    Ok(Some(body))
}

/// `value` as JSON, or a map in the encoding its `content-type` names (D273, D274).
fn structured(render: &Render<'_>, value: Value, sending: &Sending<'_>) -> Result<Body, Failure> {
    match (value, sending.encode) {
        (Value::Object(fields), Some((encode, content_type))) => {
            let inputs = render.values().inputs();
            let (files, random) = (&inputs.bodies, inputs.random.as_ref());
            let encoding = Encoding { files, random, content_type, action: sending.action };
            let (bytes, rewritten) = encode(&fields, &encoding)?;
            Ok((bytes, rewritten.map_or(ContentType::Implied(JSON), ContentType::Replaced)))
        }
        (value, _) => Ok((Bytes::from(value.to_string()), ContentType::Implied(JSON))),
    }
}

/// The bytes of the `$file` at `path`, which the engine read before the run, and the
/// `content-type` its extension implies (D272).
fn file(render: &Render<'_>, path: &Template, action: ActionId) -> Result<Body, Failure> {
    let path = path.as_literal().ok_or_else(|| rendering("a `$file` body path cannot hold `${…}` references"))?;
    let read = media::file(&render.values().inputs().bodies, (action, ""), path)?;
    let (bytes, kind) = read.ok_or_else(|| media::not_read((action, ""), path))?;
    Ok((bytes, ContentType::Implied(kind)))
}

/// A text body; a lone reference to an object or array (`body: ${var.payload}`) is sent as a
/// [`structured`] body.
fn text_body(render: &Render<'_>, template: &Template, sending: &Sending<'_>) -> Result<Body, Failure> {
    let value = render.template(template).map_err(failed)?;
    if value.is_object() || value.is_array() {
        return structured(render, value, sending);
    }
    Ok((Bytes::from(vc_template::to_text(&value)), ContentType::Implied(TEXT)))
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

/// The phases in milliseconds. A phase that did not happen (a reused connection, an IP literal,
/// plain HTTP) reads 0; the exchange itself still holds `None` for it.
fn timing(timing: &Timing) -> Value {
    let ms = |duration: std::time::Duration| json!(duration.as_secs_f64() * 1000.0);
    let mut map = Map::new();
    map.insert("total".into(), ms(timing.total));
    map.insert("firstByte".into(), ms(timing.first_byte));
    map.insert("download".into(), ms(timing.download));
    let phases = [("dns", timing.dns), ("connect", timing.connect), ("tls", timing.tls)];
    map.extend(phases.into_iter().map(|(name, phase)| (name.to_owned(), ms(phase.unwrap_or_default()))));
    Value::Object(map)
}

/// A template that did not render, as a failure under its code, V5111 unless it has one.
pub fn failed(error: RenderError) -> Failure {
    Failure::new(error.message).with_code(error.code.unwrap_or(Code::V5111).to_string())
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
