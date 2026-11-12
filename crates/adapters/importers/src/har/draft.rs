//! Kept requests as draft api files: one per origin, named after its host's first label other
//! than `www` and `api`, with the origin a base-URL var. Each request becomes an api (IDs in its
//! path as vars, its query, its body read by the shared body rules); one with the method and
//! templated path of a kept api is merged into it. Credential headers and cookies are never
//! written, and secrets become references (see [`super::secret`]).

use indexmap::IndexMap;
use serde_json::Value;
use vc_interfaces::{DraftApi, DraftGroup, ImportDraft, ImportNote};
use vc_model::{Method, credential};

use super::entry::Request;
use super::flow::Recorded;
use crate::body::{Body, Scrub, Sent, read};
use crate::ids::api_id;
use crate::naming::free;
use crate::origin::{group, without_user};
use crate::path_vars::templated;
use crate::secret::{DROPPED, Found, Secrets};
use crate::url::{pairs, path_and_query, split_origin};

/// The draft for `requests`, and what each one called, in order (none for a request that is
/// not an api's); `fallback` names a file whose host has no word to name it by.
pub fn draft<'r>(
    requests: impl IntoIterator<Item = &'r Request>,
    fallback: &str,
) -> (ImportDraft, Vec<Option<Recorded>>) {
    let mut drafting = Drafting::default();
    let recorded = requests.into_iter().map(|request| drafting.add(request, fallback)).collect();
    (drafting.done(), recorded)
}

/// One api file being drafted, and how often each of its apis was recorded.
struct Group {
    origin: String,
    draft: DraftGroup,
    recorded: Vec<usize>,
}

/// The api files drafted so far, the secrets found and what the user should know.
#[derive(Default)]
struct Drafting {
    groups: Vec<Group>,
    found: Found,
    notes: Vec<ImportNote>,
}

impl Drafting {
    /// Adds the api `request` calls to the file for its origin, or merges it into a kept one;
    /// the notes of a merged request are left out, since the kept one has them.
    fn add(&mut self, request: &Request, fallback: &str) -> Option<Recorded> {
        let (origin, rest) = split_origin(&request.url)?;
        let origin = without_user(origin, &mut self.found.notes);
        let at = self.group(&origin, fallback);
        let group = self.groups.get_mut(at)?;
        let mut secrets = Secrets { file: &group.draft.file_id, at: request.method.clone(), found: &mut self.found };
        let api = api(request, rest, &mut secrets);
        let at = secrets.at;
        let recorded = match api {
            Ok(api) => Some(group.record(api)),
            Err(message) => {
                self.found.notes.push(message);
                None
            }
        };
        if recorded.as_ref().is_some_and(|(_, merged)| *merged) {
            self.found.notes.clear();
        }
        self.notes.extend(self.found.notes.drain(..).map(|message| ImportNote { at: at.clone(), message }));
        recorded.map(|(recorded, _)| recorded)
    }

    /// The index of the group for `origin`, added with a free file ID when it is new.
    fn group(&mut self, origin: &str, fallback: &str) -> usize {
        if let Some(at) = self.groups.iter().position(|group| group.origin == origin) {
            return at;
        }
        let taken: Vec<&str> = self.groups.iter().map(|group| group.draft.file_id.as_str()).collect();
        let draft = group(origin, fallback, &taken);
        self.groups.push(Group { origin: origin.to_owned(), draft, recorded: Vec::new() });
        self.groups.len() - 1
    }

    /// The draft: groups with an api, each api recorded more than once noted as merged.
    fn done(self) -> ImportDraft {
        let mut notes = self.notes;
        let mut groups = Vec::new();
        for group in self.groups.into_iter().filter(|group| !group.draft.apis.is_empty()) {
            for (api, count) in group.draft.apis.iter().zip(&group.recorded).filter(|(_, count)| **count > 1) {
                let at = format!("{} {}", api.method.name(), api.path);
                notes.push(ImportNote { at, message: format!("recorded {count} times, merged into one api") });
            }
            groups.push(group.draft);
        }
        ImportDraft { groups, secrets: self.found.secrets, notes, ..ImportDraft::default() }
    }
}

impl Group {
    /// What `api` called: a kept api with its method and path, counted as recorded once more
    /// (`true`), else `api` itself, kept with a free ID.
    fn record(&mut self, mut api: DraftApi) -> (Recorded, bool) {
        let same = self.draft.apis.iter().position(|kept| (kept.method, &kept.path) == (api.method, &api.path));
        if let Some(at) = same
            && let (Some(kept), Some(count)) = (self.draft.apis.get(at), self.recorded.get_mut(at))
        {
            *count += 1;
            let call = format!("{}#{}", self.draft.file_id, kept.id);
            return (Recorded { call, vars: api.vars, defaults: kept.vars.clone() }, true);
        }
        let apis = &self.draft.apis;
        api.id = free(&api.id, apis.len(), |id| apis.iter().any(|kept| kept.id == id));
        let call = format!("{}#{}", self.draft.file_id, api.id);
        let recorded = Recorded { call, vars: api.vars.clone(), defaults: api.vars.clone() };
        self.draft.apis.push(api);
        self.recorded.push(1);
        (recorded, false)
    }
}

/// The api `request` sends to the path and query in `rest`, scrubbed of secrets.
fn api(request: &Request, rest: &str, secrets: &mut Secrets<'_>) -> Result<DraftApi, String> {
    let (path, query) = path_and_query(rest);
    let path = path.split('/').map(|segment| secrets.text("path segment", segment)).collect::<Vec<_>>().join("/");
    let path = templated(if path.is_empty() { "/" } else { &path });
    let name = request.method.to_ascii_uppercase();
    secrets.at = format!("{name} {}", path.path);
    let method = Method::named(&name).ok_or_else(|| format!("an api cannot use {name}"))?;
    note_headers(request, secrets);
    let body = body(request, &path.vars, secrets);
    let vars = path.vars.into_iter().chain(body.vars).map(|(name, value)| {
        let value = secrets.value(&format!("body field {name}"), value);
        (name, value)
    });
    Ok(DraftApi {
        id: api_id(method, &path.path),
        method,
        vars: vars.collect(),
        query: query_of(query, secrets),
        path: path.path,
        body: body.value.map(|value| secrets.value("body", value)),
        content_type: body.content_type,
        ..DraftApi::default()
    })
}

/// A note for each header named like a credential, `cookie` among them.
fn note_headers(request: &Request, secrets: &mut Secrets<'_>) {
    let mut named: Vec<String> = Vec::new();
    for header in &request.headers {
        let name = header.name.to_ascii_lowercase();
        if credential(&name) && !named.contains(&name) {
            named.push(name);
        }
    }
    for name in named {
        secrets.note(format!("header {name}: {DROPPED}"));
    }
}

/// The recorded body, read by the shared rules; a multipart one is left out with a note.
fn body(request: &Request, taken: &IndexMap<String, Value>, secrets: &mut Secrets<'_>) -> Body {
    let Some(data) = &request.post_data else { return Body::default() };
    let Some(text) = data.text.as_deref().filter(|text| !text.is_empty()) else { return Body::default() };
    if data.mime_type.to_ascii_lowercase().starts_with("multipart/") {
        secrets.note("a multipart body is not imported, so it is left out".to_owned());
        return Body::default();
    }
    let content_type = Some(data.mime_type.as_str()).filter(|kind| !kind.is_empty());
    read(&Sent { data: text, content_type: content_type.unwrap_or("text/plain") }, taken, secrets)
}

/// The query's pairs, decoded; a credential or a random value becomes a secret reference.
fn query_of(query: &str, secrets: &mut Secrets<'_>) -> IndexMap<String, String> {
    let mut read = IndexMap::new();
    for (name, value) in pairs(query) {
        let place = format!("query {name}");
        let value = if credential(&name) { secrets.reference(&place) } else { secrets.text(&place, &value) };
        read.insert(name, value);
    }
    read
}
