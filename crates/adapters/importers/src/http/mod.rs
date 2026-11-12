//! `vc import http`: one api per request of a `.http` file (VS Code REST Client, `JetBrains`
//! HTTP Client), drafted by the curl rules once its `{{…}}` are resolved.

mod parse;
mod templates;

use async_trait::async_trait;
use indexmap::IndexSet;
use serde_json::{Value, json};
use vc_interfaces::{DraftApi, ImportDraft, ImportError, ImportNote, ImportOptions, ImportSource, Importer};
use vc_model::credential;

use self::parse::Written;
use self::templates::Templates;
use crate::curl::{Request, draft};

/// What a request is called in a note when it gives no api: `request 2`.
const WORD: &str = "request";

/// The origin of a request whose URL starts with a variable the file does not define.
const UNKNOWN_ORIGIN: &str = "http://localhost";

/// What a var starts with in drafted text.
const VAR: &str = "${var.";

/// Reads a `.http` file into a draft api file.
///
/// It drafts one api per distinct method and path, all sent to one origin, as `vc import curl`
/// does; a request's `###` title (else its `# @name`) is the api's name. A file variable is
/// written as its value, except one named like a credential, which is never written.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct HttpImporter;

#[async_trait]
impl Importer for HttpImporter {
    async fn read(&self, source: &ImportSource, opts: &ImportOptions) -> Result<ImportDraft, ImportError> {
        let invalid = |reason: String| ImportError::Invalid { location: source.location.clone(), reason };
        let text = std::str::from_utf8(&source.bytes).map_err(|_| invalid("it is not UTF-8 text".to_owned()))?;
        let file = parse::read(text);
        if file.requests.is_empty() {
            return Err(invalid("it holds no request".to_owned()));
        }
        addressed(&file.requests).map_err(invalid)?;
        let templates = Templates::new(&file.variables);
        let requests: Vec<Request> = file.requests.into_iter().map(|written| request(written, &templates)).collect();
        let mut draft = draft(&requests, opts, WORD).map_err(invalid)?;
        let mut notes = Vec::new();
        draft.groups.iter_mut().flat_map(|group| group.apis.iter_mut()).for_each(|api| unset(api, &mut notes));
        draft.notes.extend(notes);
        Ok(draft)
    }
}

/// Whether every request names a URL: a full one, a path, or one starting with a variable.
fn addressed(requests: &[Written]) -> Result<(), String> {
    let url = |written: &Written| written.url.starts_with(['/', '{']) || written.url.contains("://");
    match requests.iter().enumerate().find(|(_, written)| !url(written)) {
        Some((at, written)) => Err(format!("request {}: {:?} is not a URL", at + 1, written.url)),
        None => Ok(()),
    }
}

/// `written` as the curl request it sends, its `{{…}}` resolved.
fn request(written: Written, templates: &Templates) -> Request {
    let Written { name, method, url, headers, body, mut notes } = written;
    let headers: Vec<(String, String)> =
        headers.into_iter().map(|header| filled(header, templates, &mut notes)).collect();
    let url = located(&templates.text(&url, &mut notes), &headers, &mut notes);
    let body = body.map(|body| templates.text(&body, &mut notes));
    let typed = headers.iter().any(|(name, _)| name.eq_ignore_ascii_case("content-type"));
    let json = !typed && body.as_deref().is_some_and(|body| serde_json::from_str::<Value>(body).is_ok());
    let mut headers = headers;
    if !typed && !json && body.is_some() {
        headers.push(("content-type".to_owned(), "text/plain".to_owned()));
    }
    let method = Some(method.unwrap_or_else(|| "GET".to_owned()));
    Request {
        name,
        method,
        url: Some(url),
        headers,
        data: body.into_iter().collect(),
        json,
        notes,
        ..Request::default()
    }
}

/// A header with its value's `{{…}}` resolved; a credential's value is left as it is, since
/// the draft never writes it.
fn filled((name, value): (String, String), templates: &Templates, notes: &mut Vec<String>) -> (String, String) {
    let value = if credential(&name) { value } else { templates.text(&value, notes) };
    (name, value)
}

/// A full URL: a path alone is sent to its `Host` header; one starting with a variable the file
/// does not define is sent to localhost, with a note.
fn located(url: &str, headers: &[(String, String)], notes: &mut Vec<String>) -> String {
    if let Some((name, rest)) = url.strip_prefix(VAR).and_then(|rest| rest.split_once('}')) {
        notes.push(format!("{{{{{name}}}}} is not defined in the file; set the base-URL var to its value"));
        return format!("{UNKNOWN_ORIGIN}{rest}");
    }
    let host = headers.iter().find(|(name, _)| name.eq_ignore_ascii_case("host")).map(|(_, host)| host);
    match host {
        Some(host) if url.starts_with('/') => format!("http://{host}{url}"),
        _ => url.to_owned(),
    }
}

/// Gives `api` each var it reads that nothing defines, empty, with a note: one it does not
/// set, and one a body field set to itself (`"sku": "{{sku}}"` gives the var `sku`).
fn unset(api: &mut DraftApi, notes: &mut Vec<ImportNote>) {
    let itself = |(name, value): &(&String, &Value)| value.as_str() == Some(format!("{VAR}{name}}}").as_str());
    let mut empty: IndexSet<String> = api.vars.iter().filter(itself).map(|(name, _)| name.clone()).collect();
    empty.extend(read(api).into_iter().filter(|name| !api.vars.contains_key(name)));
    let at = format!("{} {}", api.method.name(), api.path);
    for name in empty {
        if !credential(&name) {
            notes.push(ImportNote {
                at: at.clone(),
                message: format!("{name} is not defined in the file; give it a value"),
            });
        }
        api.vars.insert(name, json!(""));
    }
}

/// The names of the vars `api` reads in its path, query, body and var values.
fn read(api: &DraftApi) -> Vec<String> {
    let query = api.query.values().map(|value| json!(value));
    let values = api.vars.values().cloned().chain(api.body.clone()).chain(query).chain([json!(api.path)]);
    values.flat_map(|value| names(&value.to_string())).collect()
}

/// The names of the vars `text` reads.
fn names(text: &str) -> Vec<String> {
    let reads = text.split(VAR).skip(1);
    let names =
        reads.map(|rest| rest.split(|c: char| !c.is_ascii_alphanumeric() && c != '_').next().unwrap_or_default());
    names.filter(|name| !name.is_empty()).map(str::to_owned).collect()
}

#[cfg(test)]
mod tests;
