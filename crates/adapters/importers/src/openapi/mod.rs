//! `vc import openapi`: Swagger 2.0, `OpenAPI` 3.0 and 3.1, read as 3.1 with `oas3` (D199).

mod bundle;
mod cases;
mod creators;
mod draft;
#[cfg(test)]
mod fixture;
mod negative;
mod responses;
mod swagger;
mod upgrade;
mod values;

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Map, Value};
use vc_interfaces::{ImportDraft, ImportError, ImportOptions, ImportSource, Importer, SpecFetcher};

use self::draft::Reader;

/// The versions read, by the start of their `openapi` or `swagger` value, and how each
/// becomes 3.1.
const VERSIONS: [(&str, Upgrade); 3] = [("2.0", from_swagger), ("3.0", upgrade::to_openapi31), ("3.1", keep)];

/// How a spec of one version becomes 3.1.
type Upgrade = fn(Map<String, Value>) -> Map<String, Value>;

/// Reads an `OpenAPI` spec into a draft api file, one api per operation, and keeps the bundled
/// 3.1 document as the draft's snapshot.
pub struct OpenApiImporter {
    fetcher: Arc<dyn SpecFetcher>,
}

impl OpenApiImporter {
    /// An importer that decodes the spec, and fetches the documents it refers to, through
    /// `fetcher`.
    #[must_use]
    pub fn new(fetcher: Arc<dyn SpecFetcher>) -> Self {
        Self { fetcher }
    }
}

#[async_trait]
impl Importer for OpenApiImporter {
    async fn read(&self, source: &ImportSource, opts: &ImportOptions) -> Result<ImportDraft, ImportError> {
        let invalid = |reason: String| ImportError::Invalid { location: source.location.clone(), reason };
        let doc = self.fetcher.decode(&source.location, &source.bytes).map_err(|error| invalid(error.to_string()))?;
        let doc = bundle::bundle(self.fetcher.as_ref(), &source.location, doc).await?;
        let doc = Value::Object(as_31(doc).map_err(invalid)?);
        let spec: oas3::Spec = serde_json::from_value(doc.clone()).map_err(|error| invalid(error.to_string()))?;
        let reader = Reader { spec: &spec, doc: &doc, opts };
        let (routes, mut notes) = reader.routes();
        let (mut group, more) = reader.group(&source.location, &routes);
        notes.extend(more);
        group.tests = reader.tests(&opts.file_id, &routes, &mut group.apis);
        Ok(ImportDraft { groups: vec![group], notes, snapshot: Some(doc), ..ImportDraft::default() })
    }

    fn read_help(&self) -> Option<&'static str> {
        Some("fix the spec, or import part of it with `--include` or `--only`")
    }
}

/// `doc` as `OpenAPI` 3.1, or why it cannot be.
fn as_31(doc: Value) -> Result<Map<String, Value>, String> {
    let Value::Object(doc) = doc else { return Err("it is not an OpenAPI document".to_owned()) };
    let written = doc.get("openapi").or_else(|| doc.get("swagger"));
    let version = match written {
        Some(Value::String(version)) => version.clone(),
        Some(Value::Number(version)) => version.as_f64().map(|version| format!("{version:.1}")).unwrap_or_default(),
        _ => return Err("it names no `openapi` or `swagger` version".to_owned()),
    };
    let upgrade = VERSIONS.iter().find(|(prefix, _)| version.starts_with(prefix)).map(|(_, upgrade)| upgrade);
    let upgrade = upgrade.ok_or_else(|| format!("OpenAPI {version} is not read; 2.0, 3.0 and 3.1 are"))?;
    Ok(upgrade(doc))
}

fn from_swagger(doc: Map<String, Value>) -> Map<String, Value> {
    upgrade::to_openapi31(swagger::to_openapi3(doc))
}

const fn keep(doc: Map<String, Value>) -> Map<String, Value> {
    doc
}

#[cfg(test)]
mod tests;
