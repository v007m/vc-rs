//! `body.schema` (D201): the JSON Schema each check names, compiled once per run from the
//! program's schemas by `vc_template::compile_schema` (the same compile `vc check` gives V2312
//! with, D307), and a body judged against it.

use std::collections::HashMap;
use std::fmt;

use serde_json::Value;
use vc_model::{Check, CheckKind, Matcher, Part, Program, TemplateValue};
use vc_template::{SchemaValidator, compile_schema};

/// Every schema the program's checks name, compiled; an entry that did not compile holds why.
#[derive(Default)]
pub struct Schemas {
    by_key: HashMap<String, Result<SchemaValidator, String>>,
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
                by_key.insert(key.to_owned(), compile_schema(&program.schemas, key).map_err(|error| error.to_string()));
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
        let unloaded = || vc_template::SchemaError::Unloaded { key: key.to_owned() }.to_string();
        let validator = self.by_key.get(key).ok_or_else(unloaded)?;
        Ok(validator.as_ref().map_err(Clone::clone)?.breaks(body))
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

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
