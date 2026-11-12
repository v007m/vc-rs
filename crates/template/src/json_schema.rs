//! `body.schema` (D201): the JSON Schema a key names in the program's schemas, compiled the one way
//! `vc check` (V2312, D307) and a run (V5101) both use. The draft is the schema's `$schema`
//! (draft-04 to 2020-12), 2020-12 when it has none. Remote and file `$ref`s are never fetched.

use std::fmt;

use indexmap::IndexMap;
use jsonschema::error::ValidationErrorKind;
use jsonschema::{ReferencingError, Registry, ValidationError, Validator};
use serde_json::{Value, json};

/// The URI a schema file is known by while a `#/pointer` into it compiles.
const DOCUMENT: &str = "urn:vc:schema";

/// A compiled JSON Schema that judges response bodies.
pub struct SchemaValidator(Validator);

impl fmt::Debug for SchemaValidator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SchemaValidator")
    }
}

impl SchemaValidator {
    /// What `body` breaks the schema with, each as `<path>: <reason>`; none when it matches. The
    /// body's own values are left out.
    #[must_use]
    pub fn breaks(&self, body: &Value) -> Vec<String> {
        self.0.iter_errors(body).map(|error| broken(&error)).collect()
    }
}

/// Why the schema a key names cannot judge a body.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SchemaError {
    /// No schema is held under the key: a file the compile was not given, or a templated path.
    #[error("no schema is loaded for `{key}`")]
    Unloaded {
        /// The key the check names.
        key: String,
    },
    /// The schema is not a valid JSON Schema.
    #[error("the schema `{key}` is not a valid JSON Schema: {reason}")]
    Invalid {
        /// The key the check names.
        key: String,
        /// Where in the schema (or its file) it breaks, as a JSON pointer; empty when the whole
        /// schema is at fault, such as a `$ref` that leads nowhere.
        at: String,
        /// The validator's reason.
        reason: String,
    },
}

/// The schema `key` names in `schemas`, compiled: a whole document or inline schema, or a
/// `#/pointer` into a document, with the document's own `$ref`s and draft kept.
///
/// # Errors
///
/// [`SchemaError::Unloaded`] when `schemas` holds nothing for `key`, and
/// [`SchemaError::Invalid`] when what it holds is not a valid JSON Schema.
pub fn compile_schema(schemas: &IndexMap<String, Value>, key: &str) -> Result<SchemaValidator, SchemaError> {
    let unloaded = || SchemaError::Unloaded { key: key.to_owned() };
    let invalid = |(at, reason): Fault| SchemaError::Invalid { key: key.to_owned(), at, reason };
    if let Some(schema) = schemas.get(key) {
        return jsonschema::validator_for(schema).map(SchemaValidator).map_err(|error| invalid(fault(&error)));
    }
    let (path, pointer) = key.split_once('#').ok_or_else(unloaded)?;
    let document = schemas.get(path).ok_or_else(unloaded)?;
    fragment(document, pointer).map(SchemaValidator).map_err(invalid)
}

/// Where a schema breaks, as a JSON pointer, and why.
type Fault = (String, String);

/// The schema at `pointer` in `document`.
fn fragment(document: &Value, pointer: &str) -> Result<Validator, Fault> {
    let registry = Registry::new().add(DOCUMENT, document).and_then(jsonschema::RegistryBuilder::prepare);
    let registry = registry.map_err(|error| (String::new(), referenced(&error)))?;
    let root = json!({ "$ref": format!("{DOCUMENT}#{pointer}") });
    jsonschema::options().with_registry(&registry).build(&root).map_err(|error| fault(&error))
}

fn fault(error: &ValidationError<'_>) -> Fault {
    let reason = match error.kind() {
        ValidationErrorKind::Referencing(error) => referenced(error),
        _ => error.to_string(),
    };
    (error.instance_path().as_str().to_owned(), reason)
}

/// Why a `$ref` does not resolve: one outside the file is never fetched.
fn referenced(error: &ReferencingError) -> String {
    match error {
        ReferencingError::Unretrievable { uri, .. } => {
            format!("`$ref` `{uri}` is outside the file, and vc never fetches one")
        }
        other => other.to_string(),
    }
}

/// One way the body breaks its schema, where it does.
fn broken(error: &ValidationError<'_>) -> String {
    let path = error.instance_path().as_str();
    format!("{}: {}", if path.is_empty() { "/" } else { path }, error.masked())
}

#[cfg(test)]
#[path = "json_schema_tests.rs"]
mod tests;
