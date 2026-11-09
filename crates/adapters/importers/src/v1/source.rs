//! The files of a v1 project: `config.json`, and the scenarios, payloads and schemas in the
//! folder its `tests_directory` names.

use std::collections::BTreeMap;

use serde_json::Value;
use vc_interfaces::ImportError;

/// The v1 config, at the top of the project folder.
const CONFIG: &str = "config.json";
/// The tests directory when the config names none.
const TESTS: &str = "Vibranium-Tests";

/// One v1 scenario file: `scenarios/<collection>/<name>.json`.
pub struct Scenario {
    /// The folder it is in, which v1 calls its collection.
    pub collection: String,
    /// Its `name`, or its file name when it has none.
    pub name: String,
    /// The whole file.
    pub json: Value,
}

/// A v1 project, read.
pub struct Source {
    /// `config.json`.
    pub config: Value,
    /// The scenarios, in path order.
    pub scenarios: Vec<Scenario>,
    /// The payload files, by the name a `!collection/name` reference gives.
    pub payloads: BTreeMap<String, Value>,
    /// The JSON schema files, by the name a `!collection/name` reference gives.
    pub schemas: BTreeMap<String, Value>,
}

impl Source {
    /// The project in `files` (path relative to the v1 folder → text).
    ///
    /// # Errors
    ///
    /// [`ImportError::Invalid`] when there is no `config.json`, or a file it reads is not JSON.
    pub fn read(files: &BTreeMap<String, String>) -> Result<Self, ImportError> {
        let config = parsed(files, CONFIG)?;
        let tests = config.get("tests_directory").and_then(Value::as_str).unwrap_or(TESTS).trim_matches('/');
        let mut folders: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
        for path in files.keys() {
            if let Some((folder, name)) = within(path, tests) {
                folders.entry(folder.to_owned()).or_default().insert(name.to_owned(), parsed(files, path)?);
            }
        }
        let mut take = |folder: &str| folders.remove(folder).unwrap_or_default();
        let scenarios = take("scenarios").into_iter().map(scenario).collect();
        Ok(Self { config, scenarios, payloads: take("payloads"), schemas: take("schemas") })
    }

    /// The payload or schema a `!collection/name` reference names; `None` for any other text.
    pub fn referred<'s>(files: &'s BTreeMap<String, Value>, reference: &str) -> Option<&'s Value> {
        reference.strip_prefix('!').and_then(|name| files.get(name.trim_end_matches(".json")))
    }
}

/// The folder (`scenarios`, `payloads`, `schemas`) and name of a JSON file under `tests`.
fn within<'p>(path: &'p str, tests: &str) -> Option<(&'p str, &'p str)> {
    let rest = path.strip_prefix(tests)?.strip_prefix('/')?;
    let (folder, file) = rest.split_once('/')?;
    Some((folder, file.strip_suffix(".json")?))
}

/// The scenario at `collection/name`; a scenario outside a folder is a collection of its own.
fn scenario((path, json): (String, Value)) -> Scenario {
    let (collection, file) = path.rsplit_once('/').unwrap_or((path.as_str(), path.as_str()));
    let name = json.get("name").and_then(Value::as_str).unwrap_or(file).to_owned();
    let collection = collection.split('/').next().unwrap_or(collection).to_owned();
    Scenario { collection, name, json }
}

/// The JSON at `path`.
fn parsed(files: &BTreeMap<String, String>, path: &str) -> Result<Value, ImportError> {
    let invalid = |reason: String| ImportError::Invalid { location: path.to_owned(), reason };
    let text = files.get(path).ok_or_else(|| invalid("it is not there; give the folder of a v1 project".to_owned()))?;
    serde_json::from_str(text).map_err(|error| invalid(format!("it is not JSON: {error}")))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::Source;

    fn files(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
        entries.iter().map(|(path, text)| ((*path).to_owned(), (*text).to_owned())).collect()
    }

    #[test]
    fn scenarios_payloads_and_schemas_are_read_from_the_tests_directory() {
        let read = Source::read(&files(&[
            ("config.json", r#"{"tests_directory": "T"}"#),
            ("T/scenarios/users/users_crud.json", r#"{"name": "crud"}"#),
            ("T/scenarios/loose.json", "{}"),
            ("T/payloads/users/new.json", r#"{"a": 1}"#),
            ("T/schemas/users/user.json", r#"{"type": "object"}"#),
            ("other/scenarios/x/y.json", "not read"),
        ]))
        .unwrap();
        let scenarios: Vec<_> = read.scenarios.iter().map(|s| (s.collection.as_str(), s.name.as_str())).collect();
        assert_eq!(scenarios, [("loose", "loose"), ("users", "crud")]);
        assert_eq!(Source::referred(&read.payloads, "!users/new"), Some(&json!({"a": 1})));
        assert_eq!(Source::referred(&read.schemas, "!users/user.json"), Some(&json!({"type": "object"})));
        assert_eq!(Source::referred(&read.schemas, "users/user"), None);
    }

    #[test]
    fn a_folder_without_a_config_or_with_broken_json_is_refused() {
        let missing = Source::read(&files(&[])).err().unwrap().to_string();
        assert!(missing.contains("config.json") && missing.contains("v1 project"), "{missing}");
        let broken = Source::read(&files(&[("config.json", "{}"), ("Vibranium-Tests/scenarios/a/b.json", "{")]));
        assert!(broken.err().unwrap().to_string().contains("Vibranium-Tests/scenarios/a/b.json"));
    }
}
