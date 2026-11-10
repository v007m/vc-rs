//! `vc import bruno`: a Bruno collection exported as JSON, as apis in one draft api file per
//! origin its URLs name, with every credential left out, and with `--flow` a test calling its
//! requests in collection order.

mod calls;
mod collection;

use async_trait::async_trait;
use indexmap::IndexMap;
use vc_interfaces::{ImportDraft, ImportError, ImportNote, ImportOptions, ImportSource, Importer};

use collection::{Collection, Environment};

use crate::collected::{self, Values};

/// Reads a Bruno collection, exported from Bruno as JSON, into draft api files, one per origin
/// its URLs name.
///
/// Each request is an api named after it, with `{{variables}}` as vars defaulting to the
/// collection's vars and its first environment's values. Auth and credential headers are never
/// written; an environment variable marked secret, `{{process.env.NAME}}`, a variable named like
/// a credential, or a value too random to be anything but a secret becomes a `${secret.NAME}`
/// reference listed in the draft's secrets. Scripts, tests and assertions are not imported; a
/// post-response var that reads the response body is what the `flow` test saves and sends on.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct BrunoImporter;

#[async_trait]
impl Importer for BrunoImporter {
    async fn read(&self, source: &ImportSource, opts: &ImportOptions) -> Result<ImportDraft, ImportError> {
        let invalid = |reason: String| ImportError::Invalid { location: source.location.clone(), reason };
        let collection = Collection::read(&source.bytes).map_err(invalid)?;
        let walked = collection.walk();
        let saves: Vec<_> = walked.called.iter().map(|called| calls::saves(called.request)).collect();
        let values = values(&collection, saves.iter().flat_map(IndexMap::keys).cloned().collect());
        let (mut draft, requested) = collected::draft(&calls::calls(&walked.called), &values, &opts.file_id);
        let summary = collected::summary(&collection.name, walked.called.len(), walked.folders);
        let Some(first) = draft.groups.first() else {
            return Err(invalid(format!("it holds no request to an api: {summary}")));
        };
        if let Some(id) = &opts.flow {
            let steps = requested.into_iter().zip(saves).filter_map(|(requested, save)| Some((requested?, save)));
            draft.flow = Some(collected::flow(id, &first.file_id, steps));
        }
        draft.notes.extend(collection_notes(&collection, &walked.skipped));
        draft.summary = Some(summary);
        Ok(draft)
    }

    fn source_help(&self) -> Option<&'static str> {
        Some("export the collection as JSON from Bruno: import reads one file, not a collection folder")
    }
}

/// The collection's vars and its first environment's, which win; the environment's variables
/// marked secret are never written.
fn values(collection: &Collection, saved: Vec<String>) -> Values {
    let mut values: IndexMap<String, String> = calls::enabled(collection.vars()).into_iter().collect();
    let environment = collection.environments.first().map(|environment| environment.variables.as_slice());
    let variables = environment.unwrap_or_default().iter().filter(|variable| variable.enabled);
    let mut secret = Vec::new();
    for variable in variables {
        if variable.secret {
            secret.push(variable.name.clone());
        } else if let Some(value) = variable.value.clone().filter(|value| !value.is_empty()) {
            values.insert(variable.name.clone(), value);
        }
    }
    values.retain(|_, value| !value.is_empty());
    Values::new(values, saved).with_secrets(secret)
}

/// A note for each part of the collection that is not imported: its scripts, its environments
/// after the first and its items that are not HTTP requests.
fn collection_notes(collection: &Collection, skipped: &[(&str, &str)]) -> Vec<ImportNote> {
    let note = |message: String| ImportNote { at: "collection".to_owned(), message };
    let mut notes = Vec::new();
    if collection.scripted() {
        notes.push(note("the collection's scripts are not imported".to_owned()));
    }
    if let [first, others @ ..] = collection.environments.as_slice()
        && !others.is_empty()
    {
        let others: Vec<_> = others.iter().map(|other: &Environment| other.name.as_str()).collect();
        notes.push(note(format!(
            "the vars default to environment {}; {} is not imported",
            first.name,
            others.join(", ")
        )));
    }
    let skipped = skipped.iter().map(|(name, kind)| note(format!("{name} is a {kind} request, which is not imported")));
    notes.extend(skipped);
    notes
}

#[cfg(test)]
mod tests;
