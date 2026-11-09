//! A collection's requests as draft api files, one per origin its URLs name. Each request is an
//! api named after it: its path with `:name` and `{{name}}` as vars and IDs as vars, its query,
//! and its body. Auth and credential headers are never written; other headers are noted, since
//! an api file does not hold them. Scripts are not imported, apart from the values a test
//! script saves (see [`super::script`]).

use indexmap::IndexMap;
use serde_json::{Value, json};
use vc_interfaces::{DraftApi, DraftGroup, ImportDraft, ImportNote};
use vc_model::Method;

use super::body::body;
use super::collection::{Auth, Called, Header, Url, script};
use super::script::saves;
use super::vars::{Filling, UNKNOWN_ORIGIN, Values};
use crate::body::Scrub;
use crate::ids::api_id;
use crate::naming::{camel, free};
use crate::origin::{group, without_user};
use crate::path_vars::templated;
use crate::secret::{DROPPED, Found, Secrets};
use crate::secrets::credential;
use crate::url::{pairs, path_and_query};

/// The api a request became: its full ID and its vars.
pub struct Requested {
    pub call: String,
    pub vars: Vec<String>,
}

/// The draft for `called`, and what each request became, in order (none for one that is not
/// an api); `fallback` names a file whose host has no word to name it by.
pub fn draft(called: &[Called<'_>], values: &Values, fallback: &str) -> (ImportDraft, Vec<Option<Requested>>) {
    let mut drafting = Drafting { values, fallback, groups: Vec::new(), found: Found::default(), notes: Vec::new() };
    let requested = called.iter().map(|called| drafting.add(called)).collect();
    let groups = drafting.groups.into_iter().map(|(_, group)| group).filter(|group| !group.apis.is_empty());
    let draft = ImportDraft {
        groups: groups.collect(),
        secrets: drafting.found.secrets,
        notes: drafting.notes,
        ..ImportDraft::default()
    };
    (draft, requested)
}

/// The api files drafted so far by origin (none when unknown), the secrets found and the notes.
struct Drafting<'d> {
    values: &'d Values,
    fallback: &'d str,
    groups: Vec<(Option<String>, DraftGroup)>,
    found: Found,
    notes: Vec<ImportNote>,
}

impl Drafting<'_> {
    /// Adds the api `called` sends to the file for its origin.
    fn add(&mut self, called: &Called<'_>) -> Option<Requested> {
        let (origin, rest) = self.values.origin(&url_text(called.request.url.as_ref()));
        let origin = origin.map(|origin| without_user(&origin, &mut self.found.notes));
        let at = self.group(origin);
        let group = &mut self.groups.get_mut(at)?.1;
        let mut secrets = Secrets { file: &group.file_id, at: called.request.method.clone(), found: &mut self.found };
        let mut filling = Filling { values: self.values, secrets: &mut secrets, vars: IndexMap::new() };
        let api = api(called, &rest, &mut filling);
        let at = secrets.at;
        let requested = match api {
            Ok(mut api) => {
                let apis = &group.apis;
                api.id = free(&api.id, apis.len(), |id| apis.iter().any(|kept| kept.id == id));
                let requested = Requested {
                    call: format!("{}#{}", group.file_id, api.id),
                    vars: api.vars.keys().cloned().collect(),
                };
                group.apis.push(api);
                Some(requested)
            }
            Err(message) => {
                self.found.notes.push(message);
                None
            }
        };
        self.notes.extend(self.found.notes.drain(..).map(|message| ImportNote { at: at.clone(), message }));
        requested
    }

    /// The index of the group for `origin`, added with a free file ID when it is new; an
    /// unknown origin is the local host, under `fallback`, with a note to set it.
    fn group(&mut self, origin: Option<String>) -> usize {
        if let Some(at) = self.groups.iter().position(|(kept, _)| *kept == origin) {
            return at;
        }
        let taken: Vec<&str> = self.groups.iter().map(|(_, group)| group.file_id.as_str()).collect();
        let mut draft = group(origin.as_deref().unwrap_or_default(), self.fallback, &taken);
        if origin.is_none() {
            for (var, value) in &mut draft.config_vars {
                *value = json!(UNKNOWN_ORIGIN);
                let message =
                    format!("its base URL has no value, so `{var}` is {UNKNOWN_ORIGIN}: set it in the config");
                self.notes.push(ImportNote { at: "collection".to_owned(), message });
            }
        }
        self.groups.push((origin, draft));
        self.groups.len() - 1
    }
}

/// The api `called` sends to the path and query in `rest`, scrubbed of secrets.
fn api(called: &Called<'_>, rest: &str, filling: &mut Filling<'_, '_>) -> Result<DraftApi, String> {
    let (path, query) = path_and_query(rest);
    let path = path_of(path, called.request.url.as_ref(), filling);
    let name = called.request.method.to_ascii_uppercase();
    filling.secrets.at = format!("{name} {path}");
    let method = Method::named(&name).ok_or_else(|| format!("an api cannot use {name}"))?;
    let mut query = query_of(called.request.url.as_ref(), query, filling);
    auth(called.auth, &mut query, filling.secrets);
    let content_type = headers(&called.request.header, filling.secrets);
    script_notes(called, filling.secrets);
    let body = body(called.request.body.as_ref(), content_type.as_deref(), filling);
    let mut vars = std::mem::take(&mut filling.vars);
    for (name, value) in body.vars {
        let value = filling.secrets.value(&format!("body field {name}"), value);
        vars.entry(name).or_insert(value);
    }
    let id = Some(camel(called.name)).filter(|id| !id.is_empty()).unwrap_or_else(|| api_id(method, &path));
    Ok(DraftApi {
        id,
        name: Some(called.name.to_owned()).filter(|name| !name.trim().is_empty()),
        method,
        vars,
        query,
        path,
        body: body.value.map(|value| filling.secrets.value("body", value)),
        content_type: body.content_type,
        ..DraftApi::default()
    })
}

/// The path as template text: `:name` and `{{name}}` as vars, then IDs as vars.
fn path_of(path: &str, url: Option<&Url>, filling: &mut Filling<'_, '_>) -> String {
    let mut segments = Vec::new();
    for segment in path.split('/') {
        let filled = match segment.strip_prefix(':').filter(|name| !name.is_empty()) {
            Some(name) => path_variable(name, url, filling),
            None => filling.fill(segment),
        };
        segments.push(filling.secrets.text("path segment", &filled));
    }
    let path = segments.join("/");
    let path = templated(if path.is_empty() { "/" } else { &path });
    for (var, value) in path.vars {
        filling.vars.entry(var).or_insert(value);
    }
    path.path
}

/// The var for the path variable `:name`, with the URL's value for it as its default.
fn path_variable(name: &str, url: Option<&Url>, filling: &mut Filling<'_, '_>) -> String {
    let variables = match url {
        Some(Url::Parts(parts)) => parts.variable.as_slice(),
        _ => &[],
    };
    let value = variables
        .iter()
        .find(|variable| variable.key.as_deref() == Some(name))
        .and_then(|variable| variable.value.as_ref());
    let value = match value {
        Some(Value::String(text)) => text.clone(),
        Some(value @ (Value::Number(_) | Value::Bool(_))) => value.to_string(),
        _ => String::new(),
    };
    if value.starts_with("{{") && value.ends_with("}}") {
        return filling.fill(&value);
    }
    let var = camel(name);
    let default = filling.secrets.text(&format!("path variable {name}"), &value);
    filling.vars.entry(var.clone()).or_insert(json!(default));
    format!("${{var.{var}}}")
}

/// The query's enabled parameters; a credential or a random value becomes a secret reference.
fn query_of(url: Option<&Url>, raw: &str, filling: &mut Filling<'_, '_>) -> IndexMap<String, String> {
    let params: Vec<(String, String)> = match url {
        Some(Url::Parts(parts)) => parts
            .query
            .iter()
            .filter(|param| !param.disabled)
            .filter_map(|param| Some((param.key.clone()?, param.value.clone().unwrap_or_default())))
            .collect(),
        _ => pairs(raw).collect(),
    };
    let mut query = IndexMap::new();
    for (name, value) in params {
        let place = format!("query {name}");
        let value = if credential(&name) {
            filling.secrets.reference(&place)
        } else {
            let filled = filling.fill(&value);
            filling.secrets.text(&place, &filled)
        };
        query.insert(name, value);
    }
    query
}

/// What the request's auth leaves out: an api key in the query is a secret reference; any other
/// auth is a credential header the file's auth sends.
fn auth(auth: Option<&Auth>, query: &mut IndexMap<String, String>, secrets: &mut Secrets<'_>) {
    let Some(auth) = auth.filter(|auth| auth.sends()) else { return };
    if auth.kind != "apikey" {
        secrets.note(format!("header authorization: {DROPPED}"));
        return;
    }
    let name = auth.setting("key").filter(|name| !name.is_empty()).unwrap_or("x-api-key");
    if auth.setting("in") == Some("query") {
        query.insert(name.to_owned(), secrets.reference(&format!("query {name}")));
    } else {
        secrets.note(format!("header {}: {DROPPED}", name.to_ascii_lowercase()));
    }
}

/// The content type the enabled headers name; a note for each of the others.
fn headers(headers: &[Header], secrets: &mut Secrets<'_>) -> Option<String> {
    let mut content_type = None;
    for header in headers.iter().filter(|header| !header.disabled) {
        let name = header.key.to_ascii_lowercase();
        if name == "content-type" {
            content_type = header.value.clone().filter(|value| !value.contains("{{"));
        } else if credential(&name) {
            secrets.note(format!("header {name}: {DROPPED}"));
        } else {
            secrets.note(format!("header {name} is not imported"));
        }
    }
    content_type
}

/// A note for each script of the request, which is not imported.
fn script_notes(called: &Called<'_>, secrets: &mut Secrets<'_>) {
    if script(called.events, "prerequest").is_some() {
        secrets.note("the pre-request script is not imported".to_owned());
    }
    if let Some(test) = script(called.events, "test") {
        let saved = if saves(&test).is_empty() { "" } else { ", apart from the values it saves" };
        secrets.note(format!("the test script is not imported{saved}"));
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
