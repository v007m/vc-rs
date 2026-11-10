//! A Bruno collection folder read into the [`Collection`] its JSON export reads into, so both
//! draft the same: `bruno.json` names it, `collection.bru` is what every request inherits,
//! `environments/<name>.bru` are its environments, each folder (named by its `folder.bru`, else
//! by itself) holds requests, one per `.bru` file. A secret environment variable is only named
//! in a `.bru` file (Bruno keeps its value elsewhere), so there is none to leave out.

use std::path::Path;

use serde::Deserialize;

use super::bru::Bru;
use super::collection::{ApiKey, Auth, Body, Collection, Environment, Item, Param, Request, Root, Script, Shared};
use super::collection::{Variable, Vars};
use crate::collected::GraphQl;

/// The file that makes a folder a Bruno collection.
pub const CONFIG: &str = "bruno.json";

/// The block that holds a request's URL, by its method.
const METHODS: [&str; 9] = ["get", "post", "put", "delete", "patch", "options", "head", "connect", "trace"];

/// The folder that holds the environments, at the top of the collection.
const ENVIRONMENTS: &str = "environments/";

/// A folder's own file, which names it and says what its requests inherit.
const FOLDER: &str = "folder.bru";

/// The collection's own file, which says what every request inherits.
const ROOT: &str = "collection.bru";

/// `bruno.json`, as far as it is read: the collection's name and the paths Bruno leaves out.
#[derive(Deserialize)]
struct Config {
    #[serde(default)]
    name: String,
    #[serde(default)]
    ignore: Vec<String>,
}

/// The parsed `.bru` files of a collection folder, by path relative to it.
struct Files(Vec<(String, Bru)>);

/// Whether a folder's file at `path` is one the collection is read from.
pub fn reads(path: &str) -> bool {
    path == CONFIG || bru(path)
}

/// Whether `path` names a `.bru` file, whatever the case of its extension.
fn bru(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|extension| extension.eq_ignore_ascii_case("bru"))
}

/// The collection in a folder whose files `files` holds, each by its path relative to the folder,
/// with its text.
///
/// # Errors
///
/// Why the folder is not a Bruno collection, or which file cannot be read; the reason never
/// quotes a file.
pub fn collection(files: &[(String, String)]) -> Result<Collection, String> {
    let config = files.iter().find(|(path, _)| path == CONFIG);
    let config = config.ok_or("it is not a Bruno collection: the folder has no bruno.json")?;
    let config: Config = serde_json::from_str(&config.1).map_err(|_| "its bruno.json is not JSON".to_owned())?;
    let files = Files::parse(files, &config.ignore)?;
    let root = files.get(ROOT).map(|bru| Root { request: Some(shared(bru)) });
    Ok(Collection { name: config.name, items: files.items(""), environments: files.environments(), root })
}

impl Files {
    fn parse(files: &[(String, String)], ignore: &[String]) -> Result<Self, String> {
        let ignored = |path: &str| ignore.iter().any(|ignored| path == ignored || inside(path, ignored));
        let read = files.iter().filter(|(path, _)| bru(path) && !ignored(path));
        let parsed = read.map(|(path, text)| {
            Bru::parse(text).map(|bru| (path.clone(), bru)).map_err(|why| format!("{path}: {why}"))
        });
        parsed.collect::<Result<Vec<_>, String>>().map(Self)
    }

    fn get(&self, path: &str) -> Option<&Bru> {
        self.0.iter().find(|(file, _)| file == path).map(|(_, bru)| bru)
    }

    /// The requests and folders directly in the folder `dir` (`""` for the top, else ending
    /// in `/`), in path order.
    fn items(&self, dir: &str) -> Vec<Item> {
        let mut items = Vec::new();
        let mut folders: Vec<&str> = Vec::new();
        for (rest, bru) in self.under(dir) {
            match rest.split_once('/') {
                Some((folder, _)) if !folders.contains(&folder) => folders.push(folder),
                None if rest != FOLDER => items.push(request_item(rest, bru)),
                Some(_) | None => {}
            }
        }
        items.extend(folders.into_iter().map(|folder| self.folder(&format!("{dir}{folder}/"), folder)));
        items
    }

    /// The files under the folder `dir`, each by its path relative to it; at the top, neither
    /// `collection.bru` nor the environments, which are not requests.
    fn under<'f>(&'f self, dir: &'f str) -> impl Iterator<Item = (&'f str, &'f Bru)> {
        let top = dir.is_empty();
        let requests =
            self.0.iter().filter(move |(path, _)| !(top && (path == ROOT || path.starts_with(ENVIRONMENTS))));
        requests.filter_map(move |(path, bru)| Some((path.strip_prefix(dir)?, bru)))
    }

    fn folder(&self, dir: &str, name: &str) -> Item {
        let own = self.get(&format!("{dir}{FOLDER}"));
        let meta = |key: &str| own.and_then(|bru| bru.value("meta", key));
        Item {
            name: meta("name").unwrap_or_else(|| name.to_owned()),
            kind: "folder".to_owned(),
            seq: meta("seq").and_then(|seq| seq.parse().ok()),
            items: self.items(dir),
            root: own.map(|bru| Root { request: Some(shared(bru)) }),
            request: None,
        }
    }

    /// The environments, by file name.
    fn environments(&self) -> Vec<Environment> {
        let named = self.0.iter().filter_map(|(path, bru)| Some((path.strip_prefix(ENVIRONMENTS)?, bru)));
        let named =
            named.filter_map(|(file, bru)| Some((file.strip_suffix(".bru").filter(|name| !name.contains('/'))?, bru)));
        named.map(|(name, bru)| environment(name, bru)).collect()
    }
}

/// Whether `path` is inside the folder `folder`.
fn inside(path: &str, folder: &str) -> bool {
    path.strip_prefix(folder).is_some_and(|rest| rest.starts_with('/'))
}

/// A request, or an item of another kind (`grpc`) with no request, from its file.
fn request_item(file: &str, bru: &Bru) -> Item {
    let meta = |key: &str| bru.value("meta", key);
    Item {
        name: meta("name").unwrap_or_else(|| file.trim_end_matches(".bru").to_owned()),
        kind: meta("type").unwrap_or_else(|| "http".to_owned()),
        seq: meta("seq").and_then(|seq| seq.parse().ok()),
        items: Vec::new(),
        root: None,
        request: request(bru),
    }
}

fn request(bru: &Bru) -> Option<Request> {
    let method = METHODS.into_iter().find(|method| bru.has(method))?;
    let mut written = params(bru, "params:query", "query");
    written.extend(params(bru, "query", "query"));
    written.extend(params(bru, "params:path", "path"));
    Some(Request {
        url: bru.value(method, "url").unwrap_or_default(),
        method: method.to_ascii_uppercase(),
        headers: bru.pairs("headers"),
        params: written,
        body: bru.value(method, "body").map(|mode| body(bru, mode)),
        auth: bru.value(method, "auth").map(|mode| auth(bru, mode)),
        script: script(bru),
        vars: vars(bru),
        tests: bru.text("tests"),
        assertions: bru.pairs("assert"),
    })
}

/// The entries of the block `name` as parameters of `kind` (`query` or `path`).
fn params(bru: &Bru, name: &str, kind: &str) -> Vec<Param> {
    let pairs = bru.pairs(name).into_iter();
    pairs
        .map(|pair| Param { name: pair.name, value: pair.value, kind: kind.to_owned(), enabled: pair.enabled })
        .collect()
}

fn body(bru: &Bru, mode: String) -> Body {
    let graphql =
        bru.text("body:graphql").map(|query| GraphQl { query: Some(query), variables: bru.text("body:graphql:vars") });
    Body {
        mode,
        json: bru.text("body:json"),
        text: bru.text("body:text"),
        xml: bru.text("body:xml"),
        sparql: bru.text("body:sparql"),
        form_url_encoded: bru.pairs("body:form-urlencoded"),
        graphql,
    }
}

fn auth(bru: &Bru, mode: String) -> Auth {
    let apikey = bru.has("auth:apikey").then(|| ApiKey {
        key: bru.value("auth:apikey", "key").unwrap_or_default(),
        placement: bru.value("auth:apikey", "placement").unwrap_or_default(),
    });
    Auth { mode, apikey }
}

/// What a collection's or folder's file gives every request in it.
fn shared(bru: &Bru) -> Shared {
    let auth = bru.value("auth", "mode").map(|mode| auth(bru, mode));
    Shared { headers: bru.pairs("headers"), auth, script: script(bru), vars: vars(bru) }
}

fn script(bru: &Bru) -> Option<Script> {
    let (req, res) = (bru.text("script:pre-request"), bru.text("script:post-response"));
    (req.is_some() || res.is_some()).then_some(Script { req, res })
}

fn vars(bru: &Bru) -> Option<Vars> {
    let (req, res) = (bru.pairs("vars:pre-request"), bru.pairs("vars:post-response"));
    (!req.is_empty() || !res.is_empty()).then_some(Vars { req, res })
}

/// An environment named `name`: its `vars`, then the secret ones `vars:secret` names, whose
/// values Bruno does not keep in the file.
fn environment(name: &str, bru: &Bru) -> Environment {
    let vars = bru.pairs("vars").into_iter();
    let plain =
        vars.map(|pair| Variable { name: pair.name, value: Some(pair.value), enabled: pair.enabled, secret: false });
    let secret = bru.list("vars:secret").into_iter();
    let secret = secret.map(|pair| Variable { name: pair.name, value: None, enabled: pair.enabled, secret: true });
    Environment { name: name.to_owned(), variables: plain.chain(secret).collect() }
}

#[cfg(test)]
#[path = "folder_tests.rs"]
mod tests;
