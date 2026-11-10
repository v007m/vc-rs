//! One test file per v1 scenario, one test per endpoint: `services/<collection>/<area>.test.yaml`,
//! the area being the scenario name without its collection (`users_crud` gives `crud`). A test
//! calls its endpoint's api after the calls its dependencies make; `repeat-until` is a `poll`.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};
use vc_interfaces::ImportNote;

use super::apis::name;
use super::deps::Prepared;
use super::source::Scenario;
use super::{Converter, MigratedFile, duration, endpoints, file_word, text};
use crate::naming::third_person;

/// How long a `repeat-until` polls when its endpoint has no `repeat`.
const POLL_TIMEOUT: &str = "30s";

/// The wait between polls when the endpoint has no `repeat-delay`, in milliseconds.
const POLL_EVERY: u64 = 1000;

/// Endpoint keys with no v2 form, and the note that says what became of them.
const DROPPED: [(&str, &str); 2] = [
    ("delay", "delay has no v2 form; the call is made without waiting"),
    ("async", "async has no v2 form; tests in a file run in parallel already"),
];

/// A test file being built.
#[derive(Default)]
struct TestFile {
    id: String,
    before_all: Vec<Value>,
    tests: Vec<Value>,
    todos: Vec<ImportNote>,
    taken: BTreeSet<String>,
}

impl Converter<'_> {
    /// The test files, in scenario order.
    pub(super) fn test_files(&mut self) -> Vec<MigratedFile> {
        let source = self.source;
        let mut paths = BTreeSet::new();
        source.scenarios.iter().map(|scenario| self.test_file(scenario, &mut paths)).collect()
    }

    fn test_file(&mut self, scenario: &Scenario, paths: &mut BTreeSet<String>) -> MigratedFile {
        let path = path(scenario, paths);
        let id = path.trim_start_matches("services/").trim_end_matches(".test.yaml").replace('/', ".");
        let mut file = TestFile { id, ..TestFile::default() };
        for endpoint in endpoints(&scenario.json) {
            self.test(scenario, endpoint, &mut file);
        }
        let scripts = scripts(&scenario.json).into_iter().map(|message| ImportNote { at: String::new(), message });
        file.todos.extend(scripts);
        let vars = self.account_vars(scenario);
        file.migrated(scenario, vars, path)
    }

    /// The `variables` of the accounts the scenario's endpoints are sent as.
    fn account_vars(&mut self, scenario: &Scenario) -> Map<String, Value> {
        let accounts: BTreeSet<&str> = endpoints(&scenario.json).filter_map(|e| self.accounts.of(e)).collect();
        let mut vars = Map::new();
        for account in accounts {
            let variables = self.accounts.variables(account).cloned().map(Value::Object);
            vars.extend(self.variables(variables.as_ref(), &scenario.name));
        }
        vars
    }

    /// The test for `endpoint`, with the calls its dependencies make first.
    fn test(&mut self, scenario: &Scenario, endpoint: &Value, file: &mut TestFile) {
        let call = self.ids.call(&scenario.collection, &scenario.name, text(endpoint, "name")).unwrap_or_default();
        let id = file.test_id(&call);
        let at = format!("{}#{id}", file.id);
        let mut prepared = Prepared::new(std::mem::take(&mut file.before_all));
        self.dependencies(scenario, endpoint, &mut prepared);
        file.before_all = std::mem::take(&mut prepared.before_all);
        let mut test = Map::new();
        test.insert("id".to_owned(), json!(id));
        test.insert("name".to_owned(), json!(name(endpoint)));
        insert(&mut test, "vars", Value::Object(prepared.vars));
        if endpoint.get("ignore").and_then(Value::as_bool) == Some(true) {
            test.insert("skip".to_owned(), json!("ignored in v1"));
        }
        insert(&mut test, "before", Value::Array(prepared.before));
        test.insert("call".to_owned(), json!(call));
        let mut todos = self.runs(endpoint, &at, &mut test);
        todos.extend(prepared.todos);
        todos.extend(scripts(endpoint));
        file.todos.extend(todos.into_iter().map(|message| ImportNote { at: id.clone(), message }));
        file.tests.push(Value::Object(test));
        self.dropped(endpoint, &at);
    }

    /// `repeat`, `poll` and `expect`, and what they could not keep.
    fn runs(&mut self, endpoint: &Value, at: &str, test: &mut Map<String, Value>) -> Vec<String> {
        let mut todos = Vec::new();
        let repeat = endpoint.get("repeat").and_then(Value::as_u64);
        if let Some(until) = endpoint.get("repeat-until") {
            todos.extend(self.poll(endpoint, until, (at, test)));
        } else if let Some(times) = repeat.filter(|times| *times > 1) {
            test.insert("repeat".to_owned(), json!(times));
            if endpoint.get("repeat-delay").is_some() {
                self.templates
                    .note(at, "repeat-delay has no v2 form for a repeat; the calls run one after another".to_owned());
            }
        }
        if let Some(expect) = endpoint.get("expect") {
            let expected = self.expected(expect, at);
            test.extend(expected.expect.map(|expect| ("expect".to_owned(), expect)));
            todos.extend(expected.todos);
        }
        todos
    }

    /// `repeat-until` as a `poll` every `repeat-delay`, for as long as `repeat` polls would take.
    fn poll(&mut self, endpoint: &Value, until: &Value, (at, test): (&str, &mut Map<String, Value>)) -> Vec<String> {
        let expected = self.expected(until, at);
        let every = endpoint.get("repeat-delay").and_then(Value::as_u64).unwrap_or(POLL_EVERY);
        let repeat = endpoint.get("repeat").and_then(Value::as_u64);
        let timeout = repeat.map_or_else(|| POLL_TIMEOUT.to_owned(), |times| duration(times.saturating_mul(every)));
        let until = expected.expect.unwrap_or_else(|| json!({}));
        test.insert("poll".to_owned(), json!({"until": until, "every": duration(every), "timeout": timeout}));
        expected.todos.into_iter().map(|todo| format!("repeat-until: {todo}")).collect()
    }

    /// A note for each endpoint key that was dropped.
    fn dropped(&mut self, endpoint: &Value, at: &str) {
        for (key, note) in DROPPED {
            if endpoint.get(key).is_some_and(|value| !matches!(value, Value::Bool(false) | Value::Null)) {
                self.templates.note(at, note.to_owned());
            }
        }
    }
}

impl TestFile {
    /// The test ID for the api `call`: its ID with a third-person verb, unique in the file.
    fn test_id(&mut self, call: &str) -> String {
        let base = third_person(call.split_once('#').map_or(call, |(_, id)| id));
        let numbered = (1..).map(|count: u32| if count == 1 { base.clone() } else { format!("{base}{count}") });
        numbered.into_iter().find(|id| self.taken.insert(id.clone())).unwrap_or(base)
    }

    fn migrated(self, scenario: &Scenario, vars: Map<String, Value>, path: String) -> MigratedFile {
        let mut doc = Map::new();
        doc.insert("name".to_owned(), json!(scenario.name));
        let tags = text(&scenario.json, "tags").split(',').map(str::trim).filter(|tag| !tag.is_empty());
        insert(&mut doc, "tags", tags.map(|tag| json!(file_word(tag))).collect());
        insert(&mut doc, "vars", Value::Object(vars));
        insert(&mut doc, "beforeAll", Value::Array(self.before_all));
        doc.insert("tests".to_owned(), Value::Array(self.tests));
        MigratedFile { path, doc: Value::Object(doc), todos: self.todos }
    }
}

/// The test file's path, unique among `paths`.
fn path(scenario: &Scenario, paths: &mut BTreeSet<String>) -> String {
    let word = file_word(&scenario.collection);
    let prefix = format!("{}_", scenario.collection);
    let area = file_word(scenario.name.strip_prefix(&prefix).unwrap_or(&scenario.name));
    let numbered = (1..).map(|count: u32| if count == 1 { area.clone() } else { format!("{area}-{count}") });
    let mut paths_of = numbered.map(|area| format!("services/{word}/{area}.test.yaml"));
    paths_of.find(|path| paths.insert(path.clone())).unwrap_or_default()
}

/// Inserts `value` at `key` unless it is an empty list or map.
fn insert(map: &mut Map<String, Value>, key: &str, value: Value) {
    let empty = match &value {
        Value::Array(items) => items.is_empty(),
        Value::Object(entries) => entries.is_empty(),
        _ => false,
    };
    if !empty {
        map.insert(key.to_owned(), value);
    }
}

/// The v1 `scripts` of a scenario or endpoint, each as `scripts.<hook>: <its code on one line>`.
fn scripts(value: &Value) -> Vec<String> {
    let scripts = value.get("scripts").and_then(Value::as_object).into_iter().flatten();
    let one_line = |code: &Value| code.as_str().unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ");
    scripts.map(|(hook, code)| format!("scripts.{hook}: {}", one_line(code))).collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::json;

    use super::{path, scripts};
    use crate::v1::source::Scenario;

    #[test]
    fn a_scenario_is_a_test_file_named_after_its_area() {
        let scenario = |name: &str| Scenario { collection: "users".to_owned(), name: name.to_owned(), json: json!({}) };
        let mut paths = BTreeSet::new();
        assert_eq!(path(&scenario("users_crud"), &mut paths), "services/users/crud.test.yaml");
        assert_eq!(path(&scenario("crud"), &mut paths), "services/users/crud-2.test.yaml");
        assert_eq!(path(&scenario("Admin Flows"), &mut paths), "services/users/admin-flows.test.yaml");
    }

    #[test]
    fn scripts_are_kept_on_one_line() {
        let endpoint = json!({"scripts": {"after-endpoint": "if (x) {\n  y();\n}"}});
        assert_eq!(scripts(&endpoint), ["scripts.after-endpoint: if (x) { y(); }"]);
    }
}
