//! `body.schema` (D201): the JSON Schema each check names, compiled once per run from the
//! program's schemas, and a body judged against it. The draft is the schema's `$schema` (draft-04
//! to 2020-12), 2020-12 when it has none. Remote and file `$ref`s are never fetched.

use std::collections::HashMap;
use std::fmt;

use indexmap::IndexMap;
use jsonschema::{Registry, ValidationError, Validator};
use serde_json::{Value, json};
use vc_model::{Check, CheckKind, Matcher, Part, Program, TemplateValue};

/// The URI a schema file is known by while a `#/pointer` into it compiles.
const DOCUMENT: &str = "urn:vc:schema";

/// Every schema the program's checks name, compiled; an entry that did not compile holds why.
#[derive(Default)]
pub struct Schemas {
    by_key: HashMap<String, Result<Validator, String>>,
}

impl fmt::Debug for Schemas {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut keys: Vec<_> = self.by_key.keys().collect();
        keys.sort_unstable();
        f.debug_set().entries(keys).finish()
    }
}

impl Schemas {
    /// The schemas of every `body.schema` check in `program`'s calls and polls, each compiled once.
    #[must_use]
    pub fn compile(program: &Program) -> Self {
        let checks = program.actions.iter().flat_map(|action| {
            let until = action.poll.iter().flat_map(|poll| &poll.until.checks);
            action.expect.checks.iter().chain(until)
        });
        let mut by_key = HashMap::new();
        for key in checks.filter_map(key) {
            if !by_key.contains_key(key) {
                by_key.insert(key.to_owned(), compiled(&program.schemas, key));
            }
        }
        Self { by_key }
    }

    /// What `body` breaks the schema named `key` with, each as `<path>: <reason>`; none when it
    /// matches.
    ///
    /// # Errors
    ///
    /// A message when no schema was loaded for `key` or it is not a valid JSON Schema.
    pub fn check(&self, key: &str, body: &Value) -> Result<Vec<String>, String> {
        let validator = self.by_key.get(key).ok_or_else(|| unloaded(key))?;
        let validator = validator.as_ref().map_err(Clone::clone)?;
        Ok(validator.iter_errors(body).map(|error| broken(&error)).collect())
    }
}

/// The key a `body.schema` check names its schema by: its matcher's one literal.
pub fn key(check: &Check) -> Option<&str> {
    let Matcher::Equals(TemplateValue::String(template)) = &check.matcher else { return None };
    match (check.kind, &template.parts[..]) {
        (CheckKind::Schema, [Part::Literal(key)]) => Some(key),
        _ => None,
    }
}

/// The schema `key` names: a whole document or inline schema, or a `#/pointer` into a document.
fn compiled(schemas: &IndexMap<String, Value>, key: &str) -> Result<Validator, String> {
    let invalid = |error: String| format!("the schema `{key}` is not a valid JSON Schema: {error}");
    if let Some(schema) = schemas.get(key) {
        return jsonschema::validator_for(schema).map_err(|error| invalid(error.to_string()));
    }
    let (path, pointer) = key.split_once('#').ok_or_else(|| unloaded(key))?;
    let document = schemas.get(path).ok_or_else(|| unloaded(key))?;
    fragment(document, pointer).map_err(invalid)
}

/// The schema at `pointer` in `document`, with the document's own `$ref`s and draft kept.
fn fragment(document: &Value, pointer: &str) -> Result<Validator, String> {
    let registry = Registry::new().add(DOCUMENT, document).and_then(jsonschema::RegistryBuilder::prepare);
    let registry = registry.map_err(|error| error.to_string())?;
    let root = json!({ "$ref": format!("{DOCUMENT}#{pointer}") });
    jsonschema::options().with_registry(&registry).build(&root).map_err(|error| error.to_string())
}

fn unloaded(key: &str) -> String {
    format!("no schema is loaded for `{key}`")
}

/// One way the body breaks its schema, where it does; the body's own values are left out.
fn broken(error: &ValidationError<'_>) -> String {
    let path = error.instance_path().as_str();
    format!("{}: {}", if path.is_empty() { "/" } else { path }, error.masked())
}

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
