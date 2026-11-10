//! A Postman collection v2.1, read as far as the importer uses it, and its requests in order
//! with the auth each one inherits. No type here derives `Debug`, so a value the collection
//! holds cannot reach a printout through one.

use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

use crate::collected::GraphQl;

/// The collection: its name, its folders and requests, its variables and its auth.
#[derive(Deserialize)]
pub struct Collection {
    pub info: Info,
    #[serde(default)]
    pub item: Vec<Item>,
    #[serde(default)]
    pub variable: Vec<Variable>,
    pub auth: Option<Auth>,
    #[serde(default)]
    pub event: Vec<Event>,
}

/// The collection's name and the schema it follows.
#[derive(Deserialize)]
pub struct Info {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub schema: String,
}

/// A folder (with `item`) or a request.
#[derive(Deserialize)]
pub struct Item {
    #[serde(default)]
    pub name: String,
    #[serde(default, deserialize_with = "request")]
    pub request: Option<Request>,
    #[serde(default, rename = "item")]
    pub items: Vec<Self>,
    pub auth: Option<Auth>,
    #[serde(default)]
    pub event: Vec<Event>,
}

/// A request as written; a bare URL is a `GET` of it.
#[derive(Deserialize, Default)]
pub struct Request {
    #[serde(default = "get")]
    pub method: String,
    #[serde(default)]
    pub header: Vec<Header>,
    pub url: Option<Url>,
    pub body: Option<Body>,
    pub auth: Option<Auth>,
}

/// A URL as text, or in parts.
#[derive(Deserialize)]
#[serde(untagged)]
pub enum Url {
    Raw(String),
    Parts(UrlParts),
}

/// A URL in parts: its text, or its protocol, host and path; its query and path variables.
#[derive(Deserialize)]
pub struct UrlParts {
    pub raw: Option<String>,
    pub protocol: Option<String>,
    pub host: Option<Value>,
    pub path: Option<Value>,
    #[serde(default)]
    pub query: Vec<Param>,
    #[serde(default)]
    pub variable: Vec<Variable>,
}

/// A query parameter or a form field.
#[derive(Deserialize)]
pub struct Param {
    pub key: Option<String>,
    pub value: Option<String>,
    #[serde(default)]
    pub disabled: bool,
}

/// A request header.
#[derive(Deserialize)]
pub struct Header {
    pub key: String,
    pub value: Option<String>,
    #[serde(default)]
    pub disabled: bool,
}

/// A collection or path variable; its value may be any JSON.
#[derive(Deserialize)]
pub struct Variable {
    pub key: Option<String>,
    pub value: Option<Value>,
}

/// A request body, by mode.
#[derive(Deserialize)]
pub struct Body {
    #[serde(default)]
    pub mode: String,
    pub raw: Option<String>,
    #[serde(default)]
    pub urlencoded: Vec<Param>,
    pub graphql: Option<GraphQl>,
    pub options: Option<Value>,
    #[serde(default)]
    pub disabled: bool,
}

/// An auth block: its type, and its settings under a key named after the type.
#[derive(Deserialize)]
pub struct Auth {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(flatten)]
    pub settings: Map<String, Value>,
}

/// A script run before a request (`prerequest`) or after it (`test`).
#[derive(Deserialize)]
pub struct Event {
    #[serde(default)]
    pub listen: String,
    pub script: Option<Script>,
    #[serde(default)]
    pub disabled: bool,
}

/// A script's lines.
#[derive(Deserialize)]
pub struct Script {
    pub exec: Option<Value>,
}

/// One request of the collection, in order, with what it inherits.
pub struct Called<'c> {
    pub name: &'c str,
    pub request: &'c Request,
    pub auth: Option<&'c Auth>,
    pub events: &'c [Event],
}

/// The requests of a collection in order, and how many folders hold them.
#[derive(Default)]
pub struct Walked<'c> {
    pub called: Vec<Called<'c>>,
    pub folders: usize,
}

impl Collection {
    /// The collection in `bytes`.
    ///
    /// # Errors
    ///
    /// Why `bytes` is not a Postman collection v2.1; the reason never quotes the file.
    pub fn read(bytes: &[u8]) -> Result<Self, String> {
        let not = "it is not a Postman collection v2.1: export it from Postman as Collection v2.1";
        let collection: Self = serde_json::from_slice(bytes).map_err(|_| not.to_owned())?;
        if collection.info.schema.contains("/collection/v2.1") { Ok(collection) } else { Err(not.to_owned()) }
    }

    /// Its requests in order, each with the auth of the request, else of the nearest folder or
    /// the collection that has one.
    pub fn walk(&self) -> Walked<'_> {
        let mut walked = Walked::default();
        walk(&self.item, self.auth.as_ref(), &mut walked);
        walked
    }
}

impl Auth {
    /// Whether it is an auth, and not `noauth` or one left to the folder above.
    pub fn sends(&self) -> bool {
        !matches!(self.kind.as_str(), "noauth" | "inherit")
    }

    /// The setting `key` of its type (`apikey`'s `in`), as text.
    pub fn setting(&self, key: &str) -> Option<&str> {
        let settings = self.settings.get(&self.kind)?.as_array()?;
        let found = settings.iter().find(|setting| setting.get("key").and_then(Value::as_str) == Some(key))?;
        found.get("value")?.as_str()
    }
}

impl Script {
    /// Its lines as one text.
    pub fn text(&self) -> String {
        match &self.exec {
            Some(Value::String(text)) => text.clone(),
            Some(Value::Array(lines)) => lines.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n"),
            _ => String::new(),
        }
    }
}

/// The text of an event's script of kind `listen`, when it has any.
pub fn script(events: &[Event], listen: &str) -> Option<String> {
    let event = events.iter().find(|event| event.listen == listen && !event.disabled)?;
    Some(event.script.as_ref()?.text()).filter(|text| !text.trim().is_empty())
}

/// The enabled parameters that have a name, each with its value, else empty.
pub fn enabled(params: &[Param]) -> Vec<(String, String)> {
    let params = params.iter().filter(|param| !param.disabled);
    params.filter_map(|param| Some((param.key.clone()?, param.value.clone().unwrap_or_default()))).collect()
}

fn walk<'c>(items: &'c [Item], auth: Option<&'c Auth>, walked: &mut Walked<'c>) {
    for item in items {
        let auth = item.auth.as_ref().or(auth);
        if let Some(request) = &item.request {
            let auth = request.auth.as_ref().or(auth);
            walked.called.push(Called { name: &item.name, request, auth, events: &item.event });
        } else {
            walked.folders += 1;
            walk(&item.items, auth, walked);
        }
    }
}

fn get() -> String {
    "GET".to_owned()
}

/// A request, or a bare URL as a `GET` of it.
fn request<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Request>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Written {
        Url(String),
        Full(Box<Request>),
    }
    Ok(Option::<Written>::deserialize(deserializer)?.map(|written| match written {
        Written::Url(url) => Request { method: get(), url: Some(Url::Raw(url)), ..Request::default() },
        Written::Full(request) => *request,
    }))
}
