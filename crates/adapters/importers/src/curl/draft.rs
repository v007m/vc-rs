//! Curl requests as a draft api file: their origin a base-URL var, IDs in their paths vars,
//! their query a query and their data a body. A credential is left out with a note.

use indexmap::IndexMap;
use serde_json::json;
use vc_interfaces::{DraftApi, DraftGroup, ImportDraft, ImportNote, ImportOptions};
use vc_model::Method;

use super::body::body;
use super::options::Request;
use crate::naming::{camel, from_route};
use crate::path_vars::templated;
use crate::secrets::credential;
use crate::url::{pairs, path_and_query, split_origin};

/// What to tell the user in place of a credential.
const LEFT_OUT: &str = "a credential is never written; add it with `vc new auth`";

/// The draft for `requests`; a request that gives no api is noted as `<word> <n>`.
///
/// # Errors
///
/// The first note, when no request gives an api.
pub fn draft(requests: &[Request], opts: &ImportOptions, word: &str) -> Result<ImportDraft, String> {
    let mut drafting = Drafting::default();
    for (at, request) in requests.iter().enumerate() {
        drafting.add(request, &format!("{word} {}", at + 1));
    }
    let Drafting { origin, apis, notes } = drafting;
    let Some(origin) = origin.filter(|_| !apis.is_empty()) else {
        let why = notes.first().map(|note| format!("{}: {}", note.at, note.message));
        return Err(why.unwrap_or_else(|| "no curl command gives an api".to_owned()));
    };
    let var = format!("{}Url", camel(&opts.file_id));
    let group = DraftGroup {
        file_id: opts.file_id.clone(),
        base_url: Some(format!("${{var.{var}}}")),
        config_vars: IndexMap::from([(var, json!(origin))]),
        apis,
        ..DraftGroup::default()
    };
    Ok(ImportDraft { groups: vec![group], notes, ..ImportDraft::default() })
}

/// The apis drafted so far, the origin they share and what the user should know.
#[derive(Default)]
struct Drafting {
    origin: Option<String>,
    apis: Vec<DraftApi>,
    notes: Vec<ImportNote>,
}

impl Drafting {
    /// Adds the api `request`, the `curl` command, sends to, with its notes.
    fn add(&mut self, request: &Request, curl: &str) {
        let mut found = request.notes.clone();
        let drafted = drafted(request, &mut found).and_then(|(origin, api)| self.same_origin(origin).map(|()| api));
        let at = drafted.as_ref().map_or_else(|_| curl.to_owned(), |api| format!("{} {}", api.method.name(), api.path));
        found.extend(drafted.as_ref().err().cloned());
        self.notes.extend(found.into_iter().map(|message| ImportNote { at: at.clone(), message }));
        if let Ok(api) = drafted {
            self.keep(api, at);
        }
    }

    /// An error when `origin` is not the origin the first api calls.
    fn same_origin(&mut self, origin: String) -> Result<(), String> {
        match &self.origin {
            Some(first) if *first != origin => {
                Err(format!("it calls {origin}, not {first}: import it into another file with its own `--id`"))
            }
            Some(_) => Ok(()),
            None => {
                self.origin = Some(origin);
                Ok(())
            }
        }
    }

    /// Keeps `api` unless one with its method and path is kept, renaming it when its ID is taken.
    fn keep(&mut self, mut api: DraftApi, at: String) {
        if let Some(same) = self.apis.iter().find(|kept| (kept.method, &kept.path) == (api.method, &api.path)) {
            let message = format!("the same api as `{}`, which is kept", same.id);
            return self.notes.push(ImportNote { at, message });
        }
        while self.apis.iter().any(|kept| kept.id == api.id) {
            api.id.push('2');
        }
        self.apis.push(api);
    }
}

/// The origin `request` calls and the api it sends; what it leaves out goes in `found`.
fn drafted(request: &Request, found: &mut Vec<String>) -> Result<(String, DraftApi), String> {
    let url = request.url.as_deref().ok_or("it names no URL")?;
    let url = if url.contains("://") { url.to_owned() } else { format!("http://{url}") };
    let (origin, rest) = split_origin(&url).ok_or("its URL cannot be read")?;
    let (path, query) = path_and_query(rest);
    let method = method(request)?;
    let path = templated(if path.is_empty() { "/" } else { path });
    note_headers(request, found);
    let body = body(request, &path.vars, found);
    let data = request.data.iter().filter(|_| request.get).map(String::as_str);
    let api = DraftApi {
        id: from_route(method.name(), &path.path.replace("${var.", "{")),
        name: request.name.clone(),
        method,
        query: query_of(std::iter::once(query).chain(data), found),
        vars: path.vars.into_iter().chain(body.vars).collect(),
        path: path.path,
        body: body.value,
        content_type: body.content_type,
        ..DraftApi::default()
    };
    Ok((without_user(origin, found), api))
}

/// The method: `-X`, else HEAD for `-I`, POST when data is sent in a body, GET otherwise.
fn method(request: &Request) -> Result<Method, String> {
    let empty = request.data.is_empty() && request.form.is_empty();
    let sends = !(request.get || empty);
    let name = match &request.method {
        Some(name) => name.to_ascii_uppercase(),
        None if request.head => "HEAD".to_owned(),
        None if sends => "POST".to_owned(),
        None => "GET".to_owned(),
    };
    Method::named(&name).ok_or_else(|| format!("an api cannot use {name}"))
}

/// `origin` without the user and password a URL may hold, which only a note mentions.
fn without_user(origin: &str, found: &mut Vec<String>) -> String {
    let (scheme, host) = origin.split_once("://").unwrap_or(("http", origin));
    let Some((_, host)) = host.rsplit_once('@') else { return origin.to_owned() };
    found.push(format!("the URL's user: {LEFT_OUT}"));
    format!("{scheme}://{host}")
}

/// The credentials and headers the api leaves out; its content type is the api's own.
fn note_headers(request: &Request, found: &mut Vec<String>) {
    found.extend(request.credentials.iter().map(|option| format!("{option}: {LEFT_OUT}")));
    let names = request.headers.iter().map(|(name, _)| name.as_str());
    let (secret, other): (Vec<_>, Vec<_>) =
        names.filter(|name| !name.eq_ignore_ascii_case("content-type")).partition(|name| credential(name));
    found.extend(secret.iter().map(|name| format!("header {name}: {LEFT_OUT}")));
    if !other.is_empty() {
        found.push(format!("headers are not imported: {}", other.join(", ")));
    }
}

/// The pairs of each query, decoded; a credential is left out with a note.
fn query_of<'q>(queries: impl IntoIterator<Item = &'q str>, found: &mut Vec<String>) -> IndexMap<String, String> {
    let mut query = IndexMap::new();
    for (name, value) in queries.into_iter().flat_map(pairs) {
        if credential(&name) {
            found.push(format!("query {name}: {LEFT_OUT}"));
        } else {
            query.insert(name, value);
        }
    }
    query
}

#[cfg(test)]
#[path = "draft_tests.rs"]
mod tests;
