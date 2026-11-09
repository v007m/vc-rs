//! `vc new api <id> --from-curl '<curl>'`: one copied curl command as an api, read by the
//! `curl` importer. Its origin becomes a base-URL var as with `--url`, IDs in its path and the
//! fields of its JSON body become vars, and no credential is written.

use vc_adapter_importers::ImporterRegistry;
use vc_interfaces::{DraftApi, ImportDraft, ImportOptions, ImportSource};
use vc_model::FileKind;

use crate::authoring::refused;
use crate::create::{DOCS, NewItem, checked};
use crate::create_api::based;
use crate::edit::internal;
use crate::import_write::api_fields;
use crate::yaml_out::entry;
use crate::{Added, Engine, EngineError};

/// What the importer's errors are told by.
const LOCATION: &str = "the curl command";

impl Engine {
    /// The change that appends the api one curl command describes to the api file `id`
    /// derives from, creating that file when there is none. Each title after the first is a
    /// note on what was left out, a credential named but never its value.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] for an ID that breaks a convention or that the file already has,
    /// a text that is not one curl command, an existing file with another base URL, or a change
    /// the guard refuses.
    pub async fn new_api_from_curl(&self, id: &str, curl: &str) -> Result<Added, EngineError> {
        let named = checked(id)?;
        let draft = self.read_curl(named.file, curl).await?;
        let (api, origin) = only_api(&draft)?;
        let project = self.authoring()?;
        let (root, also) = based(&project, named.file, origin)?;
        let item = named.item.unwrap_or_default();
        let yaml = api_yaml(&DraftApi { id: item.to_owned(), ..api.clone() });
        let comment = format!("# {id} · {} {} · keys: {DOCS}#api-files", api.method.name(), api.path);
        let new = NewItem { kind: FileKind::Api, list: "apis", named, yaml, comment, root, also, next: None };
        let mut added = self.added(&project, new)?;
        added.changes.titles.extend(draft.notes.iter().map(|note| format!("{}: {}", note.at, note.message)));
        Ok(added)
    }

    async fn read_curl(&self, file: &str, curl: &str) -> Result<ImportDraft, EngineError> {
        let registry = ImporterRegistry::builtin();
        let importer = registry.create("curl", self.fetcher()?).map_err(|error| internal(error.to_string()))?;
        let source = ImportSource { location: LOCATION.to_owned(), bytes: curl.as_bytes().to_vec() };
        let options = ImportOptions { file_id: file.to_owned(), ..ImportOptions::default() };
        let help = "give one command as copied, starting `curl`, with its URL";
        importer.read(&source, &options).await.map_err(|error| refused(error.to_string(), help))
    }
}

/// The one api the draft holds and the origin it calls.
fn only_api(draft: &ImportDraft) -> Result<(&DraftApi, &str), EngineError> {
    let group = draft.groups.first().ok_or_else(|| internal("a curl draft without its file"))?;
    let origin = group.config_vars.values().next().and_then(|origin| origin.as_str());
    match (group.apis.as_slice(), origin) {
        ([api], Some(origin)) => Ok((api, origin)),
        (apis, _) => Err(refused(
            format!("{LOCATION} gives {} apis, and `vc new api` adds one", apis.len()),
            "keep one command, or write them to a file for `vc import curl <file>`",
        )),
    }
}

/// The api's YAML, unindented: its fields in the house order (D139).
fn api_yaml(api: &DraftApi) -> String {
    let mut out = String::new();
    for (key, value) in api_fields(api) {
        entry(&mut out, 0, (key, &value));
    }
    out.trim_end().to_owned()
}

#[cfg(test)]
#[path = "create_curl_tests.rs"]
mod tests;
