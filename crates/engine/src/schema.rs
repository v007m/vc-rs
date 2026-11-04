use serde_json::{Map, Value};
use vc_model::FileKind;

use crate::{Engine, EngineError};

/// One kind's JSON Schema, ready to write to a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaFile {
    /// The kind of definition file the schema describes.
    pub kind: FileKind,
    /// `<kind>.schema.json`.
    pub file_name: String,
    /// The schema as pretty-printed JSON, ending in a newline.
    pub text: String,
}

impl Engine {
    /// The JSON Schema of each kind of definition file, in [`FileKind::ALL`] order, for
    /// editors. Needs no project.
    ///
    /// # Errors
    ///
    /// [`EngineError::Schema`] when a schema cannot be written as JSON.
    pub fn schema_export() -> Result<Vec<SchemaFile>, EngineError> {
        vc_compiler::schemas()
            .into_iter()
            .map(|schema| {
                let file_name = format!("{}.schema.json", schema.kind.name());
                Ok(SchemaFile { kind: schema.kind, file_name, text: pretty(&schema.schema)? })
            })
            .collect()
    }

    /// Every schema in one JSON object keyed by kind name (`config`, `api`, `test`, `load`).
    ///
    /// # Errors
    ///
    /// [`EngineError::Schema`] when the schemas cannot be written as JSON.
    pub fn schema_bundle() -> Result<String, EngineError> {
        let bundle: Map<String, Value> =
            vc_compiler::schemas().into_iter().map(|schema| (schema.kind.name().to_owned(), schema.schema)).collect();
        pretty(&Value::Object(bundle))
    }
}

fn pretty(value: &Value) -> Result<String, EngineError> {
    let text =
        serde_json::to_string_pretty(value).map_err(|error| EngineError::Schema { reason: error.to_string() })?;
    Ok(text + "\n")
}

#[cfg(test)]
mod tests {
    use crate::{Engine, FileKind};

    #[test]
    fn exports_one_schema_file_per_kind() {
        let files = Engine::schema_export().unwrap();
        let names: Vec<_> = files.iter().map(|file| file.file_name.as_str()).collect();
        assert_eq!(names, ["config.schema.json", "api.schema.json", "test.schema.json", "load.schema.json"]);
        assert_eq!(files[1].kind, FileKind::Api);
        assert!(files[1].text.contains("\"$id\"") && files[1].text.ends_with("}\n"));
    }

    #[test]
    fn bundles_every_schema_by_kind_name() {
        let bundle: serde_json::Value = serde_json::from_str(&Engine::schema_bundle().unwrap()).unwrap();
        let kinds: Vec<_> = bundle.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(kinds.len(), 4);
        assert!(["config", "api", "test", "load"].iter().all(|kind| kinds.contains(kind)));
    }
}
