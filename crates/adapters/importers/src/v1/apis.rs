//! One api file per v1 collection, one api per endpoint: `services/<collection>/<collection>.api.yaml`.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};
use vc_interfaces::ImportNote;

use super::source::{Scenario, Source};
use super::{Converter, MigratedFile, duration, endpoints, file_word, text};
use crate::naming::camel;

/// The api ID of each endpoint, unique within its collection.
pub struct Ids(BTreeMap<(String, String, String), String>);

impl Ids {
    /// IDs from the endpoint names without their number (`02_create_user` gives `createUser`);
    /// a name a collection has already used gets a count (`createUser2`).
    pub fn new(source: &Source) -> Self {
        let mut taken = BTreeSet::new();
        let mut ids = BTreeMap::new();
        for scenario in &source.scenarios {
            for endpoint in endpoints(&scenario.json) {
                let name = text(endpoint, "name");
                let base = api_id(name);
                let numbered = (1..).map(|count: u32| if count == 1 { base.clone() } else { format!("{base}{count}") });
                let id = numbered.into_iter().find(|id| taken.insert((scenario.collection.clone(), id.clone())));
                let key = (scenario.collection.clone(), scenario.name.clone(), name.to_owned());
                ids.insert(key, id.unwrap_or(base));
            }
        }
        Self(ids)
    }

    /// The `<collection>#<id>` of the endpoint `name` of the scenario `scenario` in `collection`.
    pub fn call(&self, collection: &str, scenario: &str, name: &str) -> Option<String> {
        let id = self.0.get(&(collection.to_owned(), scenario.to_owned(), name.to_owned()))?;
        Some(format!("{}#{id}", file_word(collection)))
    }
}

/// An endpoint's v2 name: its description, or its v1 name.
pub fn name(endpoint: &Value) -> &str {
    endpoint.get("description").and_then(Value::as_str).unwrap_or_else(|| text(endpoint, "name"))
}

/// An api file being built.
#[derive(Default)]
struct ApiFile {
    vars: Map<String, Value>,
    apis: Vec<Value>,
    todos: Vec<ImportNote>,
}

impl Converter<'_> {
    /// The api files, in collection order.
    pub(super) fn api_files(&mut self) -> Vec<MigratedFile> {
        let source = self.source;
        let mut files: BTreeMap<String, ApiFile> = BTreeMap::new();
        for scenario in &source.scenarios {
            let file = files.entry(file_word(&scenario.collection)).or_default();
            self.generated(scenario, &mut file.vars);
            for endpoint in endpoints(&scenario.json) {
                let call = self.ids.call(&scenario.collection, &scenario.name, text(endpoint, "name"));
                let call = call.unwrap_or_default();
                let (api, todos) = self.api(endpoint, &call);
                let id = call.split_once('#').map_or("", |(_, id)| id);
                file.todos.extend(todos.into_iter().map(|message| ImportNote { at: id.to_owned(), message }));
                file.apis.push(api);
            }
        }
        files.into_iter().map(|(word, file)| file.migrated(&word)).collect()
    }

    /// A scenario's `generate` as vars of its collection's api file; the first value a
    /// collection gives a name is kept.
    fn generated(&mut self, scenario: &Scenario, vars: &mut Map<String, Value>) {
        for (name, value) in self.variables(scenario.json.get("generate"), &scenario.name) {
            vars.entry(name).or_insert(value);
        }
    }

    /// The api for `endpoint`, whose `<collection>#<id>` is `call`, and what it could not keep.
    fn api(&mut self, endpoint: &Value, call: &str) -> (Value, Vec<String>) {
        let mut api = Map::new();
        api.insert("id".to_owned(), json!(call.split_once('#').map_or(call, |(_, id)| id)));
        api.insert("name".to_owned(), json!(name(endpoint)));
        let method = endpoint.get("method").and_then(Value::as_str).unwrap_or("GET");
        api.insert("method".to_owned(), json!(method.to_ascii_uppercase()));
        self.address(endpoint, call, &mut api);
        self.options(endpoint, call, &mut api);
        let todos = self.body(endpoint, call, &mut api);
        (Value::Object(api), todos)
    }

    /// `path` and `query` from the endpoint's `url`.
    fn address(&mut self, endpoint: &Value, at: &str, api: &mut Map<String, Value>) {
        let url = text(endpoint, "url");
        let (path, query) = url.split_once('?').unwrap_or((url, ""));
        api.insert("path".to_owned(), json!(self.templates.text(path, at)));
        let pairs = query.split('&').filter(|pair| !pair.is_empty());
        let query: Map<String, Value> = pairs
            .map(|pair| pair.split_once('=').unwrap_or((pair, "")))
            .map(|(name, value)| (name.to_owned(), json!(self.templates.text(value, at))))
            .collect();
        if !query.is_empty() {
            api.insert("query".to_owned(), Value::Object(query));
        }
    }

    /// `headers` (with `language` as `accept-language`), `timeout`, `auth` and `vars`.
    fn options(&mut self, endpoint: &Value, at: &str, api: &mut Map<String, Value>) {
        let headers = endpoint.get("headers").map(|headers| self.templates.value(headers, at));
        let mut headers = match headers {
            Some(Value::Object(headers)) => headers,
            _ => Map::new(),
        };
        headers.extend(endpoint.get("language").map(|language| ("accept-language".to_owned(), language.clone())));
        if !headers.is_empty() {
            api.insert("headers".to_owned(), Value::Object(headers));
        }
        let timeout = endpoint.get("timeout").and_then(Value::as_u64);
        api.extend(timeout.map(|millis| ("timeout".to_owned(), json!(duration(millis)))));
        let auth = self.accounts.of(endpoint).and_then(|account| self.accounts.auth(account));
        api.extend(auth.map(|auth| ("auth".to_owned(), json!(auth))));
        let vars = self.variables(endpoint.get("variables"), at);
        if !vars.is_empty() {
            api.insert("vars".to_owned(), Value::Object(vars));
        }
    }

    /// v1 `variables` as v2 vars: names in camelCase, regexes as Faker calls.
    pub(super) fn variables(&mut self, variables: Option<&Value>, at: &str) -> Map<String, Value> {
        let variables = variables.and_then(Value::as_object).into_iter().flatten();
        variables.map(|(name, value)| (camel(name), self.templates.variable(value, at))).collect()
    }

    /// `body` from the endpoint's `payload`, or from the payload file it names.
    fn body(&mut self, endpoint: &Value, at: &str, api: &mut Map<String, Value>) -> Vec<String> {
        let Some(payload) = endpoint.get("payload") else { return Vec::new() };
        let reference = payload.as_str().filter(|text| text.starts_with('!'));
        let Some(reference) = reference else {
            api.insert("body".to_owned(), self.templates.value(payload, at));
            return Vec::new();
        };
        let Some(file) = Source::referred(&self.source.payloads, reference) else {
            return vec![format!("payload: {reference} is not in the payloads folder; add the body by hand")];
        };
        api.insert("body".to_owned(), self.templates.value(file, at));
        Vec::new()
    }
}

impl ApiFile {
    fn migrated(self, word: &str) -> MigratedFile {
        let mut doc = Map::new();
        doc.insert("name".to_owned(), json!(word));
        doc.insert("baseUrl".to_owned(), json!("${var.apiUrl}"));
        if !self.vars.is_empty() {
            doc.insert("vars".to_owned(), Value::Object(self.vars));
        }
        doc.insert("apis".to_owned(), Value::Array(self.apis));
        MigratedFile { path: format!("services/{word}/{word}.api.yaml"), doc: Value::Object(doc), todos: self.todos }
    }
}

/// An endpoint name without its number, in camelCase; `api` when nothing is left.
fn api_id(name: &str) -> String {
    let word = name.trim_start_matches(|c: char| c.is_ascii_digit()).trim_start_matches(['_', '-', ' ', '.']);
    let id = camel(if word.is_empty() { name } else { word });
    if id.is_empty() { "api".to_owned() } else { id }
}

#[cfg(test)]
mod tests {
    use super::api_id;

    #[test]
    fn api_ids_drop_the_endpoint_number() {
        assert_eq!(api_id("01_list_users"), "listUsers");
        assert_eq!(api_id("getUser"), "getUser");
        assert_eq!(api_id("42"), "n42");
        assert_eq!(api_id(""), "api");
    }
}
