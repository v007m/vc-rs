use serde_json::{Value, json};
use vc_model::FileKind;

/// The JSON Schema of one kind of definition file, for editors (`vc schema export`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileSchema {
    /// The kind of file the schema describes.
    pub kind: FileKind,
    /// The schema itself (JSON Schema 2020-12).
    pub schema: Value,
}

/// Where the published schemas live; `<kind>.schema.json` is appended.
const BASE_URL: &str = "https://raw.githubusercontent.com/v007m/vc-rs/master/schema/";

/// One schema per [`FileKind`], in [`FileKind::ALL`] order.
///
/// They are meant to come from the same shape declarations as the shape-check stage, so the
/// editors and the checker agree; until those exist, each kind is a bare object schema.
#[must_use]
pub fn schemas() -> Vec<FileSchema> {
    FileKind::ALL.into_iter().map(|kind| FileSchema { kind, schema: schema_of(kind) }).collect()
}

fn schema_of(kind: FileKind) -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": format!("{BASE_URL}{}.schema.json", kind.name()),
        "title": format!("Vibranium {} file", kind.name()),
        "type": "object",
    })
}

#[cfg(test)]
mod tests {
    use vc_model::FileKind;

    use super::schemas;

    #[test]
    fn has_one_object_schema_per_kind() {
        let all = schemas();
        assert_eq!(all.iter().map(|s| s.kind).collect::<Vec<_>>(), FileKind::ALL);
        for schema in all {
            assert_eq!(schema.schema["type"], "object");
            let id = schema.schema["$id"].as_str().unwrap();
            assert!(id.ends_with(&format!("/{}.schema.json", schema.kind.name())), "{id}");
        }
    }
}
