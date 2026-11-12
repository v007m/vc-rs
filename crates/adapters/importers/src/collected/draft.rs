//! A collection's requests as draft api files, one per origin their URLs name. Each request is
//! an api named after it: its path with `:name` and `{{name}}` as vars and IDs as vars, its
//! query, and its body. Auth and credential headers are never written; other headers are noted,
//! since an api file does not hold them.

use indexmap::IndexMap;
use serde_json::json;
use vc_interfaces::{DraftApi, DraftGroup, ImportDraft, ImportNote};
use vc_model::{Method, credential};

use super::payload::body;
use super::request::{Call, Credential};
use super::vars::{Filling, UNKNOWN_ORIGIN, Values};
use crate::body::Scrub;
use crate::ids::api_id;
use crate::naming::{camel, free};
use crate::origin::{group, without_user};
use crate::path_vars::templated;
use crate::secret::{DROPPED, Found, Secrets};
use crate::url::{pairs, path_and_query};

/// The api a request became: its full ID and its vars.
pub struct Requested {
    pub call: String,
    pub vars: Vec<String>,
}

/// The draft for `calls`, and what each request became, in order (none for one that is not
/// an api); `fallback` names a file whose host has no word to name it by.
pub fn draft(calls: &[Call], values: &Values, fallback: &str) -> (ImportDraft, Vec<Option<Requested>>) {
    let mut drafting = Drafting { values, fallback, groups: Vec::new(), found: Found::default(), notes: Vec::new() };
    let requested = calls.iter().map(|call| drafting.add(call)).collect();
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
    /// Adds the api `call` sends to the file for its origin.
    fn add(&mut self, call: &Call) -> Option<Requested> {
        let (origin, rest) = self.values.origin(&call.url);
        let origin = origin.map(|origin| without_user(&origin, &mut self.found.notes));
        let at = self.group(origin);
        let group = &mut self.groups.get_mut(at)?.1;
        let mut secrets = Secrets { file: &group.file_id, at: call.method.clone(), found: &mut self.found };
        let mut filling = Filling { values: self.values, secrets: &mut secrets, vars: IndexMap::new() };
        let api = api(call, &rest, &mut filling);
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

/// The api `call` sends to the path and query in `rest`, scrubbed of secrets.
fn api(call: &Call, rest: &str, filling: &mut Filling<'_, '_>) -> Result<DraftApi, String> {
    let (path, query) = path_and_query(rest);
    let path = path_of(path, &call.path_values, filling);
    let name = call.method.to_ascii_uppercase();
    filling.secrets.at = format!("{name} {path}");
    let method = Method::named(&name).ok_or_else(|| format!("an api cannot use {name}"))?;
    let mut query = query_of(call.query.as_deref(), query, filling);
    sent_credential(call.credential.as_ref(), &mut query, filling.secrets);
    let content_type = headers(&call.headers, filling.secrets);
    for note in &call.notes {
        filling.secrets.note(note.clone());
    }
    let body = body(call.body.as_ref(), content_type.as_deref(), filling);
    let mut vars = std::mem::take(&mut filling.vars);
    for (name, value) in body.vars {
        let value = filling.secrets.value(&format!("body field {name}"), value);
        vars.entry(name).or_insert(value);
    }
    let id = Some(camel(&call.name)).filter(|id| !id.is_empty()).unwrap_or_else(|| api_id(method, &path));
    Ok(DraftApi {
        id,
        name: Some(call.name.clone()).filter(|name| !name.trim().is_empty()),
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
fn path_of(path: &str, values: &[(String, String)], filling: &mut Filling<'_, '_>) -> String {
    let mut segments = Vec::new();
    for segment in path.split('/') {
        let filled = match segment.strip_prefix(':').filter(|name| !name.is_empty()) {
            Some(name) => path_variable(name, values, filling),
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

/// The var for the path variable `:name`, with its value in `values` as its default.
fn path_variable(name: &str, values: &[(String, String)], filling: &mut Filling<'_, '_>) -> String {
    let value = values.iter().find(|(key, _)| key == name).map_or("", |(_, value)| value.as_str());
    if value.starts_with("{{") && value.ends_with("}}") {
        return filling.fill(value);
    }
    let var = camel(name);
    let default = filling.secrets.text(&format!("path variable {name}"), value);
    filling.vars.entry(var.clone()).or_insert(json!(default));
    format!("${{var.{var}}}")
}

/// The query's parameters (`params`, else those in `raw`); a credential or a random value
/// becomes a secret reference.
fn query_of(params: Option<&[(String, String)]>, raw: &str, filling: &mut Filling<'_, '_>) -> IndexMap<String, String> {
    let params: Vec<(String, String)> = params.map_or_else(|| pairs(raw).collect(), <[_]>::to_vec);
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

/// What the request's auth leaves out: a credential in the query is a secret reference; one in
/// a header is noted, since the file's auth sends it.
fn sent_credential(sent: Option<&Credential>, query: &mut IndexMap<String, String>, secrets: &mut Secrets<'_>) {
    match sent {
        Some(Credential::Query(name)) => {
            query.insert(name.clone(), secrets.reference(&format!("query {name}")));
        }
        Some(Credential::Header(name)) => secrets.note(format!("header {}: {DROPPED}", name.to_ascii_lowercase())),
        None => {}
    }
}

/// The content type the headers name; a note for each of the others.
fn headers(headers: &[(String, Option<String>)], secrets: &mut Secrets<'_>) -> Option<String> {
    let mut content_type = None;
    for (name, value) in headers {
        let name = name.to_ascii_lowercase();
        if name == "content-type" {
            content_type = value.clone().filter(|value| !value.contains("{{"));
        } else if credential(&name) {
            secrets.note(format!("header {name}: {DROPPED}"));
        } else {
            secrets.note(format!("header {name} is not imported"));
        }
    }
    content_type
}
