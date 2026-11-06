//! `vc new api`: an api appended to the api file its ID derives from, its path parameters
//! written as vars; from a full URL, the origin becomes a base-URL var in the config (D177).

use vc_compiler::camel_id;
use vc_model::FileKind;
use vc_parser::{Node, write_text};

use crate::authoring::{Authoring, refused};
use crate::create::{DOCS, NewItem, checked};
use crate::edit::{FileChange, internal};
use crate::{Added, Engine, EngineError};

/// What `vc new api` asks for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct NewApi {
    /// The api's full ID: `users#deactivate`.
    pub id: String,
    /// The HTTP method; GET when none.
    pub method: Option<String>,
    /// The path, `{name}` for each parameter: `/users/{id}/deactivate`.
    pub path: Option<String>,
    /// A full URL instead of a path: `https://api.example.com/users?q=ann`.
    pub url: Option<String>,
}

/// Where an api sends its request: the origin (empty for a bare path), path and query.
#[derive(Debug, PartialEq, Eq)]
struct Target<'u> {
    origin: &'u str,
    path: &'u str,
    query: Vec<(&'u str, &'u str)>,
}

impl Engine {
    /// The change that appends an api to the api file its ID derives from, creating that file
    /// when there is none. A `--url` origin becomes a config var ending in `Url`, reused when one
    /// already holds it, and the new file's `baseUrl`.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] for an ID that breaks a convention (with the one it suggests), an
    /// ID the file already has, neither or both of a path and a URL, a URL that is not a full
    /// one, an existing file with another base URL, or a change the guard refuses.
    pub fn new_api(&self, request: &NewApi) -> Result<Added, EngineError> {
        let named = checked(&request.id)?;
        let target = target(request)?;
        let project = self.authoring()?;
        let (root, also) = match target.origin {
            "" => (String::new(), Vec::new()),
            origin => based(&project, named.file, origin)?,
        };
        let method = request.method.as_deref().unwrap_or("GET").to_ascii_uppercase();
        let yaml = api_yaml(named.item.unwrap_or_default(), &method, &target);
        let comment = format!("# {} · {method} {} · keys: {DOCS}#api-files", request.id, target.path);
        self.added(&project, NewItem { kind: FileKind::Api, list: "apis", named, yaml, comment, root, also })
    }
}

/// The request's path or URL, split.
fn target(request: &NewApi) -> Result<Target<'_>, EngineError> {
    match (&request.path, &request.url) {
        (Some(path), None) => Ok(Target { origin: "", path, query: Vec::new() }),
        (None, Some(url)) => split(url),
        _ => Err(refused(
            "an api needs one of `--path` and `--url`",
            "`--path /users/{id}` in a file with a base URL, or `--url https://api.example.com/users`",
        )),
    }
}

/// A full URL as its origin, path and query; a fragment is dropped.
fn split(url: &str) -> Result<Target<'_>, EngineError> {
    let bad = || refused(format!("`{url}` is not a full URL"), "give its scheme and host: `--url https://api.example.com/users`");
    let url = url.split('#').next().unwrap_or(url);
    let host = url.find("://").ok_or_else(bad)? + 3;
    let end = url.get(host..).and_then(|rest| rest.find(['/', '?'])).map_or(url.len(), |at| host + at);
    let (origin, rest) = url.split_at_checked(end).filter(|_| end > host).ok_or_else(bad)?;
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let pairs = query.split('&').filter(|pair| !pair.is_empty());
    let query = pairs.map(|pair| pair.split_once('=').unwrap_or((pair, ""))).collect();
    Ok(Target { origin, path: if path.is_empty() { "/" } else { path }, query })
}

/// The root lines of a new api file for `origin`, and the config change that adds its var.
/// An existing file must already take its base URL from that var.
fn based(project: &Authoring, file: &str, origin: &str) -> Result<(String, Vec<FileChange>), EngineError> {
    let (name, change) = base_var(project, file, origin)?;
    let wanted = format!("${{var.{name}}}");
    if let Some(found) = project.find(FileKind::Api, file) {
        let opened = project.open(found)?;
        let base = opened.document.root().get("baseUrl").and_then(Node::as_str).unwrap_or("none");
        if base != wanted {
            let help = format!("use `--path` to add to {}, or another file ID for {origin}", found.1.path);
            return Err(refused(format!("`{file}` takes its base URL from {base}, not {wanted}"), &help));
        }
    }
    Ok((format!("baseUrl: {wanted}\n"), change.into_iter().collect()))
}

/// The config var that holds `origin`: one already set to it, or `<last word>Url` with the
/// change that adds it to the config's `vars`.
fn base_var(project: &Authoring, file: &str, origin: &str) -> Result<(String, Option<FileChange>), EngineError> {
    let config = project.entries().find(|(_, found)| found.kind() == Some(FileKind::Config));
    let opened = project.open(config.ok_or_else(|| internal("a project without its config"))?)?;
    let (document, root) = (&opened.document, opened.document.root());
    let vars = root.get("vars");
    let entries = vars.map(Node::entries).unwrap_or_default();
    if let Some(same) = entries.iter().find(|entry| entry.value.as_str() == Some(origin)) {
        return Ok((same.key.clone(), None));
    }
    let name = format!("{}Url", camel_id(file.rsplit('.').next().unwrap_or(file)));
    if entries.iter().any(|entry| entry.key == name) {
        let help = format!("set `vars.{name}` to {origin}, or add the api by `--path`");
        return Err(refused(format!("the config var `{name}` holds another URL"), &help));
    }
    let added = match vars {
        Some(vars) => document.insert_flow_entry(vars, &name, origin).or_else(|_| document.insert_entry(vars, &name, origin)),
        None => document.insert_yaml(root, "vars", &format!("{name}: {}", write_text(origin, false))),
    };
    let edit = added.map_err(|error| refused(format!("cannot add `{name}` to the config: {error}"), "add it by hand"))?;
    Ok((name, Some(opened.change(&[edit])?)))
}

/// The api's YAML: its ID, its method unless GET, its path with vars, and its query.
fn api_yaml(id: &str, method: &str, target: &Target<'_>) -> String {
    let mut lines = vec![format!("id: {id}")];
    lines.extend((method != "GET").then(|| format!("method: {}", write_text(method, false))));
    lines.push(format!("path: {}", write_text(&as_vars(target.path), false)));
    lines.extend((!target.query.is_empty()).then(|| "query:".to_owned()));
    lines.extend(target.query.iter().map(|(key, value)| format!("  {}: {}", write_text(key, false), write_text(value, false))));
    lines.join("\n")
}

/// `path` with each `{name}` written as `${var.name}`; a `${…}` template stays as it is.
fn as_vars(path: &str) -> String {
    let mut out = String::with_capacity(path.len() + 8);
    let mut before = ' ';
    for c in path.chars() {
        match c {
            '{' if before != '$' => out.push_str("${var."),
            _ => out.push(c),
        }
        before = c;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Target, api_yaml, as_vars, split};

    #[test]
    fn path_parameters_become_vars() {
        assert_eq!(as_vars("/users/{id}/deactivate"), "/users/${var.id}/deactivate");
        assert_eq!(as_vars("/users/${var.id}"), "/users/${var.id}");
    }

    #[test]
    fn a_url_splits_into_origin_path_and_query() {
        let target = split("https://api.example.com/users?q=ann&all#top").ok();
        let query = vec![("q", "ann"), ("all", "")];
        assert_eq!(target, Some(Target { origin: "https://api.example.com", path: "/users", query }));
        let bare = split("http://localhost:7421").ok();
        assert_eq!(bare, Some(Target { origin: "http://localhost:7421", path: "/", query: Vec::new() }));
        assert!(split("api.example.com/users").is_err() && split("https:///users").is_err());
    }

    #[test]
    fn an_api_is_written_with_its_method_only_when_not_get() {
        let target = Target { origin: "", path: "/users/{id}", query: vec![("q", "ann")] };
        assert_eq!(api_yaml("find", "GET", &target), "id: find\npath: /users/${var.id}\nquery:\n  q: ann");
        let post = Target { origin: "", path: "/users", query: Vec::new() };
        assert_eq!(api_yaml("create", "POST", &post), "id: create\nmethod: POST\npath: /users");
    }
}
