//! v1 `dependencies` as the calls a v2 test makes first (D200): a dependency's own dependencies
//! come before it, a call made twice with the same vars is made once saving what both
//! wanted, and a cached dependency (`cache: true`) is a `beforeAll` call whose saves the test
//! reads as `${var.<call>.<name>}`.

use serde_json::{Map, Value, json};

use super::source::Scenario;
use super::{Converter, endpoints, expect, text};
use crate::naming::camel;

/// How deep dependencies of dependencies are followed, against a cycle.
const DEPTH: usize = 8;

/// The calls a test makes first.
pub struct Prepared {
    /// `before`: `{call, vars, save}` in the order they run.
    pub before: Vec<Value>,
    /// `beforeAll` calls of the test file: `{id, call, save}`.
    pub before_all: Vec<Value>,
    /// The test's vars reading what a `beforeAll` call saved.
    pub vars: Map<String, Value>,
    /// What has no v2 form, with its v1 text.
    pub todos: Vec<String>,
    depth: usize,
}

impl Prepared {
    /// Calls for a test of a file whose `beforeAll` is `before_all` so far.
    pub fn new(before_all: Vec<Value>) -> Self {
        Self { before: Vec::new(), before_all, vars: Map::new(), todos: Vec::new(), depth: 0 }
    }
}

impl Converter<'_> {
    /// Adds the calls the dependencies of `endpoint`, in `scenario`, make before it.
    pub(super) fn dependencies(&mut self, scenario: &Scenario, endpoint: &Value, prepared: &mut Prepared) {
        if prepared.depth >= DEPTH {
            prepared.todos.push(format!(
                "dependencies of {} go deeper than {DEPTH}; add the rest by hand",
                text(endpoint, "name")
            ));
            return;
        }
        prepared.depth += 1;
        for dependency in endpoint.get("dependencies").and_then(Value::as_array).into_iter().flatten() {
            self.dependency(scenario, dependency, prepared);
        }
        prepared.depth -= 1;
    }

    fn dependency(&mut self, scenario: &Scenario, dependency: &Value, prepared: &mut Prepared) {
        let source = self.source;
        let collection = dependency.get("collection").and_then(Value::as_str).unwrap_or(&scenario.collection);
        let name = dependency.get("scenario").and_then(Value::as_str).unwrap_or(&scenario.name);
        let api = text(dependency, "api");
        let found = source.scenarios.iter().find(|other| other.collection == collection && other.name == name);
        let target = found.and_then(|other| Some((other, endpoints(&other.json).find(|e| text(e, "name") == api)?)));
        let Some(((other, endpoint), call)) = target.zip(self.ids.call(collection, name, api)) else {
            prepared.todos.push(format!("dependencies: {collection}/{name}/{api} is not in the project"));
            return;
        };
        self.dependencies(other, endpoint, prepared);
        let at = call.clone();
        let vars = self.variables(dependency.get("variables"), &at);
        let save = saves(dependency);
        let cached = dependency.get("cache").or_else(|| endpoint.get("cache")).and_then(Value::as_bool);
        if cached == Some(true) {
            cache(prepared, &call, (vars, save));
        } else {
            before(prepared, &call, (vars, save));
        }
        if dependency.get("repeat").and_then(Value::as_u64).is_some_and(|repeat| repeat != 1) {
            self.templates.note(&at, "a dependency's repeat has no v2 form; it is called once".to_owned());
        }
    }
}

/// A dependency call's vars and what it saves.
type Saved = (Map<String, Value>, Map<String, Value>);

/// Adds a `before` call, or merges its saves into the same call with the same vars.
fn before(prepared: &mut Prepared, call: &str, (vars, save): Saved) {
    let vars = (!vars.is_empty()).then_some(Value::Object(vars));
    let existing = prepared.before.iter_mut().find(|item| item["call"] == call && item.get("vars") == vars.as_ref());
    if let Some(Value::Object(item)) = existing {
        merge(item, save);
        return;
    }
    let mut item = Map::new();
    item.insert("call".to_owned(), json!(call));
    item.extend(vars.map(|vars| ("vars".to_owned(), vars)));
    merge(&mut item, save);
    prepared.before.push(Value::Object(item));
}

/// Adds a `beforeAll` call named like the api, and the test vars that read its saves.
fn cache(prepared: &mut Prepared, call: &str, (vars, save): Saved) {
    let id = call.split_once('#').map_or(call, |(_, id)| id).to_owned();
    for name in save.keys() {
        prepared.vars.insert(name.clone(), json!(format!("${{var.{id}.{name}}}")));
    }
    let existing = prepared.before_all.iter_mut().find(|item| item["id"] == id.as_str());
    if let Some(Value::Object(item)) = existing {
        merge(item, save);
        return;
    }
    let mut item = Map::new();
    item.insert("id".to_owned(), json!(id));
    item.insert("call".to_owned(), json!(call));
    if !vars.is_empty() {
        item.insert("vars".to_owned(), Value::Object(vars));
    }
    merge(&mut item, save);
    prepared.before_all.push(Value::Object(item));
}

/// Adds `save` to the item's `save`.
fn merge(item: &mut Map<String, Value>, save: Map<String, Value>) {
    if save.is_empty() {
        return;
    }
    if let Some(Value::Object(saved)) = item.get_mut("save") {
        saved.extend(save);
    } else {
        item.insert("save".to_owned(), Value::Object(save));
    }
}

/// A dependency's `variable` (with `path`), or its map of names to paths, as v2 saves.
fn saves(dependency: &Value) -> Map<String, Value> {
    match dependency.get("variable") {
        Some(Value::String(name)) => Map::from_iter([(camel(name), json!(saved(text(dependency, "path"))))]),
        Some(Value::Object(names)) => {
            names.iter().map(|(name, path)| (camel(name), json!(saved(path.as_str().unwrap_or_default())))).collect()
        }
        _ => Map::new(),
    }
}

/// A v1 response path (`response.items.all.id`, `id`, `response.any.code`) as a v2 save.
fn saved(path: &str) -> String {
    let path = path.strip_prefix("response").map_or(path, |rest| rest.trim_start_matches('.'));
    let random = path.split('.').any(|word| word == "any");
    let words: Vec<&str> = path.split('.').map(|word| if word == "any" { "all" } else { word }).collect();
    let key = expect::key(&words.join("."));
    let at = match key.as_str() {
        "$" => String::new(),
        key if key.starts_with('[') => key.to_owned(),
        key => format!(".{key}"),
    };
    format!("${{response.body{at}{}}}", if random { " | random" } else { "" })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{saved, saves};

    #[test]
    fn response_paths_become_saves() {
        assert_eq!(saved("response.id"), "${response.body.id}");
        assert_eq!(saved("id"), "${response.body.id}");
        assert_eq!(saved("response.items.all.id"), "${response.body.items[*].id}");
        assert_eq!(saved("response.any.code"), "${response.body[*].code | random}");
        assert_eq!(saved("response"), "${response.body}");
        assert_eq!(saved("company.anyway"), "${response.body.company.anyway}");
        assert_eq!(
            saves(&json!({"variable": "user_id", "path": "response.id"})),
            *json!({"userId": "${response.body.id}"}).as_object().unwrap()
        );
    }
}
