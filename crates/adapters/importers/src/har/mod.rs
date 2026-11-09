//! `vc import har`: the calls a browser recorded (HAR 1.2 or 1.3), as apis in one draft api file
//! per origin, with the noise filtered out and every credential left out.

mod draft;
mod entry;
mod filter;
mod ids;
mod secret;

use async_trait::async_trait;
use vc_interfaces::{ImportDraft, ImportError, ImportNote, ImportOptions, ImportSource, Importer};

/// Reads a HAR file into draft api files, one per origin it calls.
///
/// Static assets, `OPTIONS` preflights and third-party sites are filtered out, and a note at
/// `the recording` counts them. A credential header or cookie is never written; a secret in a
/// query, body or path becomes a `${secret.NAME}` reference listed in the draft's secrets.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct HarImporter;

#[async_trait]
impl Importer for HarImporter {
    async fn read(&self, source: &ImportSource, opts: &ImportOptions) -> Result<ImportDraft, ImportError> {
        let invalid = |reason: String| ImportError::Invalid { location: source.location.clone(), reason };
        let kept = filter::kept(entry::entries(&source.bytes).map_err(invalid)?);
        let mut draft = draft::draft(kept.entries.iter().map(|entry| &entry.request), &opts.file_id);
        if draft.groups.is_empty() {
            return Err(invalid(format!("it records no call to an api: {}", kept.summary)));
        }
        draft.notes.insert(0, ImportNote { at: "the recording".to_owned(), message: kept.summary });
        Ok(draft)
    }
}

#[cfg(test)]
mod tests;
