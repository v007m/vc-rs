//! `vc import bruno`: a Bruno collection, its folder of `.bru` files or the collection exported
//! as JSON, as apis in one draft api file per origin its URLs name, with every credential left
//! out, and with `--flow` a test calling its requests in collection order.

mod bru;
mod calls;
mod collection;
mod folder;

use std::sync::Arc;

use async_trait::async_trait;
use indexmap::IndexMap;
use vc_interfaces::{ImportDraft, ImportError, ImportNote, ImportOptions, ImportSource, Importer, SpecFetcher};

use collection::{Collection, Environment};

use crate::collected::{self, Values};

/// Reads a Bruno collection, its folder (with its `bruno.json`) or the collection exported from
/// Bruno as JSON, into draft api files, one per origin its URLs name. Both draft the same.
///
/// Each request is an api named after it, with `{{variables}}` as vars defaulting to the
/// collection's vars and its first environment's values. Auth and credential headers are never
/// written; an environment variable marked secret, `{{process.env.NAME}}`, a variable named like
/// a credential, or a value too random to be anything but a secret becomes a `${secret.NAME}`
/// reference listed in the draft's secrets. Scripts, tests and assertions are not imported; a
/// post-response var that reads the response body is what the `flow` test saves and sends on.
pub struct BrunoImporter {
    fetcher: Arc<dyn SpecFetcher>,
}

impl BrunoImporter {
    /// An importer that lists and reads a collection folder's files through `fetcher`.
    #[must_use]
    pub fn new(fetcher: Arc<dyn SpecFetcher>) -> Self {
        Self { fetcher }
    }

    /// The collection in `source`: its folder's files when it has no bytes and is a folder with
    /// files in it, else its bytes as the JSON export.
    async fn collection(&self, source: &ImportSource) -> Result<Collection, String> {
        if source.bytes.is_empty() {
            let listed = self.fetcher.list(&source.location).await.map_err(|error| error.to_string())?;
            if !listed.is_empty() {
                return self.folder(&source.location, &listed).await;
            }
        }
        Collection::read(&source.bytes)
    }

    /// The collection in the folder at `location`, from the files of `listed` it is read from.
    async fn folder(&self, location: &str, listed: &[String]) -> Result<Collection, String> {
        let mut files = Vec::new();
        for path in listed.iter().filter(|path| folder::reads(path)) {
            let at =
                if location.is_empty() { path.clone() } else { format!("{}/{path}", location.trim_end_matches('/')) };
            let bytes = self.fetcher.fetch(&at).await.map_err(|error| error.to_string())?;
            let text = String::from_utf8(bytes).map_err(|_| format!("{path} is not UTF-8 text"))?;
            files.push((path.clone(), text));
        }
        folder::collection(&files)
    }
}

#[async_trait]
impl Importer for BrunoImporter {
    async fn read(&self, source: &ImportSource, opts: &ImportOptions) -> Result<ImportDraft, ImportError> {
        let invalid = |reason: String| ImportError::Invalid { location: source.location.clone(), reason };
        let collection = self.collection(source).await.map_err(invalid)?;
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
        Some("give the Bruno collection's folder (the one with its bruno.json) or the collection exported as JSON")
    }

    fn reads_folders(&self) -> bool {
        true
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
