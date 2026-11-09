//! A Bruno collection exported as JSON, read as far as the importer uses it, and its requests
//! in order with the auth and headers each one inherits. No type here derives `Debug`, so a
//! value the collection holds cannot reach a printout through one.

use serde::Deserialize;

use crate::collected::GraphQl;

/// The item kinds that are an HTTP request; older exports name them `http-request`.
const REQUESTS: [&str; 4] = ["http", "graphql", "http-request", "graphql-request"];

/// The collection: its name, its folders and requests, its environments and what every request
/// inherits.
#[derive(Deserialize)]
pub struct Collection {
    #[serde(default)]
    pub name: String,
    pub items: Vec<Item>,
    #[serde(default)]
    pub environments: Vec<Environment>,
    pub root: Option<Root>,
}

/// A folder (with `items`) or a request, by `type`.
#[derive(Deserialize)]
pub struct Item {
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "type")]
    pub kind: String,
    pub seq: Option<f64>,
    #[serde(default)]
    pub items: Vec<Self>,
    pub root: Option<Root>,
    pub request: Option<Request>,
}

/// What a collection or folder gives every request in it.
#[derive(Deserialize)]
pub struct Root {
    pub request: Option<Shared>,
}

/// The headers, auth, scripts and vars a collection or folder sets.
#[derive(Deserialize)]
pub struct Shared {
    #[serde(default)]
    pub headers: Vec<Pair>,
    pub auth: Option<Auth>,
    pub script: Option<Script>,
    pub vars: Option<Vars>,
}

/// A request as written.
#[derive(Deserialize)]
pub struct Request {
    #[serde(default)]
    pub url: String,
    #[serde(default = "get")]
    pub method: String,
    #[serde(default)]
    pub headers: Vec<Pair>,
    #[serde(default)]
    pub params: Vec<Param>,
    pub body: Option<Body>,
    pub auth: Option<Auth>,
    pub script: Option<Script>,
    pub vars: Option<Vars>,
    pub tests: Option<String>,
    #[serde(default)]
    pub assertions: Vec<Pair>,
}

/// A header, form field, var or assertion: a name, its value and whether it is on.
#[derive(Deserialize)]
pub struct Pair {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub value: String,
    #[serde(default = "on")]
    pub enabled: bool,
}

/// A query or path parameter.
#[derive(Deserialize)]
pub struct Param {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub value: String,
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default = "on")]
    pub enabled: bool,
}

/// A request body: its mode and the text or fields of each mode.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Body {
    #[serde(default)]
    pub mode: String,
    pub json: Option<String>,
    pub text: Option<String>,
    pub xml: Option<String>,
    pub sparql: Option<String>,
    #[serde(default)]
    pub form_url_encoded: Vec<Pair>,
    pub graphql: Option<GraphQl>,
}

/// An auth block: its mode, and for an api key its name and where it goes.
#[derive(Deserialize)]
pub struct Auth {
    #[serde(default)]
    pub mode: String,
    pub apikey: Option<ApiKey>,
}

/// An api key's name and where it is sent: `header` or `queryparams`.
#[derive(Deserialize)]
pub struct ApiKey {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub placement: String,
}

/// The scripts run before a request and after its response.
#[derive(Deserialize)]
pub struct Script {
    pub req: Option<String>,
    pub res: Option<String>,
}

/// The vars set before a request and from its response.
#[derive(Deserialize)]
pub struct Vars {
    #[serde(default)]
    pub req: Vec<Pair>,
    #[serde(default)]
    pub res: Vec<Pair>,
}

/// An environment and its variables.
#[derive(Deserialize)]
pub struct Environment {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub variables: Vec<Variable>,
}

/// An environment variable; a secret one is exported without its value.
#[derive(Deserialize)]
pub struct Variable {
    #[serde(default)]
    pub name: String,
    pub value: Option<String>,
    #[serde(default = "on")]
    pub enabled: bool,
    #[serde(default)]
    pub secret: bool,
}

/// One request of the collection, in order, with what it inherits.
pub struct Called<'c> {
    pub name: &'c str,
    pub request: &'c Request,
    pub auth: Option<&'c Auth>,
    pub headers: Vec<&'c Pair>,
}

/// The requests of a collection in order, how many folders hold them and the items that are
/// not HTTP requests.
#[derive(Default)]
pub struct Walked<'c> {
    pub called: Vec<Called<'c>>,
    pub folders: usize,
    pub skipped: Vec<(&'c str, &'c str)>,
}

/// What the requests of a folder inherit.
#[derive(Clone, Default)]
struct Inherited<'c> {
    auth: Option<&'c Auth>,
    headers: Vec<&'c Pair>,
}

impl Collection {
    /// The collection in `bytes`.
    ///
    /// # Errors
    ///
    /// Why `bytes` is not a Bruno collection; the reason never quotes the file.
    pub fn read(bytes: &[u8]) -> Result<Self, String> {
        let not = "it is not a Bruno collection: export the collection as JSON from Bruno";
        serde_json::from_slice(bytes).map_err(|_| not.to_owned())
    }

    /// Its requests in order of `seq`, each with the auth of the request, else of the nearest
    /// folder or the collection that sets one, and the headers they set.
    pub fn walk(&self) -> Walked<'_> {
        let mut walked = Walked::default();
        walk(&self.items, &inherit(&Inherited::default(), self.root.as_ref()), &mut walked);
        walked
    }

    /// The vars the collection sets.
    pub fn vars(&self) -> &[Pair] {
        let vars = self.root.as_ref().and_then(|root| root.request.as_ref()).and_then(|shared| shared.vars.as_ref());
        vars.map_or(&[], |vars| &vars.req)
    }

    /// The scripts the collection runs, when it runs any.
    pub fn scripted(&self) -> bool {
        let shared = self.root.as_ref().and_then(|root| root.request.as_ref());
        shared.and_then(|shared| shared.script.as_ref()).is_some_and(Script::any)
    }
}

impl Auth {
    /// Whether it is an auth, and not `none` or one left to the folder above.
    pub fn sends(&self) -> bool {
        !matches!(self.mode.as_str(), "none" | "inherit" | "")
    }
}

impl Script {
    /// Whether either script has any text.
    pub fn any(&self) -> bool {
        [&self.req, &self.res].into_iter().flatten().any(|text| !text.trim().is_empty())
    }
}

fn walk<'c>(items: &'c [Item], inherited: &Inherited<'c>, walked: &mut Walked<'c>) {
    let mut items: Vec<&Item> = items.iter().collect();
    items.sort_by(|a, b| a.seq.unwrap_or(f64::MAX).total_cmp(&b.seq.unwrap_or(f64::MAX)));
    for item in items {
        match (&item.request, item.kind.as_str()) {
            (Some(request), kind) if REQUESTS.contains(&kind) => walked.called.push(called(item, request, inherited)),
            (_, "folder") => {
                walked.folders += 1;
                walk(&item.items, &inherit(inherited, item.root.as_ref()), walked);
            }
            (_, kind) => walked.skipped.push((&item.name, kind)),
        }
    }
}

fn called<'c>(item: &'c Item, request: &'c Request, inherited: &Inherited<'c>) -> Called<'c> {
    let own = request.auth.as_ref().filter(|auth| auth.mode != "inherit");
    let mut headers = inherited.headers.clone();
    headers.extend(&request.headers);
    Called { name: &item.name, request, auth: own.or(inherited.auth), headers }
}

/// What the requests under `root` inherit, given what its folder inherits.
fn inherit<'c>(inherited: &Inherited<'c>, root: Option<&'c Root>) -> Inherited<'c> {
    let mut inner = inherited.clone();
    let Some(shared) = root.and_then(|root| root.request.as_ref()) else { return inner };
    if let Some(auth) = shared.auth.as_ref().filter(|auth| auth.mode != "inherit") {
        inner.auth = Some(auth);
    }
    inner.headers.extend(&shared.headers);
    inner
}

fn get() -> String {
    "GET".to_owned()
}

const fn on() -> bool {
    true
}
