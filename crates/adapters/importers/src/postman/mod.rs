//! `vc import postman`: a Postman collection (v2.1) as apis in one draft api file per origin its
//! URLs name, with every credential left out, and with `--flow` a test calling its requests in
//! collection order.

mod body;
mod calls;
mod collection;
mod script;

use async_trait::async_trait;
use indexmap::IndexMap;
use vc_interfaces::{ImportDraft, ImportError, ImportNote, ImportOptions, ImportSource, Importer};

use collection::{Collection, script};

use crate::collected::{self, Values};

/// Reads a Postman collection v2.1 into draft api files, one per origin its URLs name.
///
/// Each request is an api named after it, with `{{variables}}` as vars defaulting to the
/// collection's values. Auth and credential headers are never written; a variable named like a
/// credential, or a value too random to be anything but a secret, becomes a `${secret.NAME}`
/// reference listed in the draft's secrets. Scripts are not imported, apart from the values a
/// test script saves from the response, which the `flow` test saves and sends on.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct PostmanImporter;

#[async_trait]
impl Importer for PostmanImporter {
    async fn read(&self, source: &ImportSource, opts: &ImportOptions) -> Result<ImportDraft, ImportError> {
        let invalid = |reason: String| ImportError::Invalid { location: source.location.clone(), reason };
        let collection = Collection::read(&source.bytes).map_err(invalid)?;
        let walked = collection.walk();
        let saves: Vec<_> = walked.called.iter().map(|called| saved(called.events)).collect();
        let values =
            Values::new(calls::values(&collection.variable), saves.iter().flat_map(IndexMap::keys).cloned().collect());
        let (mut draft, requested) = collected::draft(&calls::calls(&walked.called), &values, &opts.file_id);
        let summary = collected::summary(&collection.info.name, walked.called.len(), walked.folders);
        let Some(first) = draft.groups.first() else {
            return Err(invalid(format!("it holds no request to an api: {summary}")));
        };
        if let Some(id) = &opts.flow {
            let steps = requested.into_iter().zip(saves).filter_map(|(requested, save)| Some((requested?, save)));
            draft.flow = Some(collected::flow(id, &first.file_id, steps));
        }
        if script(&collection.event, "prerequest").or_else(|| script(&collection.event, "test")).is_some() {
            let message = "the collection's scripts are not imported".to_owned();
            draft.notes.push(ImportNote { at: "collection".to_owned(), message });
        }
        draft.summary = Some(summary);
        Ok(draft)
    }
}

/// What a request's test script saves.
fn saved(events: &[collection::Event]) -> IndexMap<String, String> {
    script(events, "test").map(|test| script::saves(&test)).unwrap_or_default()
}

#[cfg(test)]
mod tests;
