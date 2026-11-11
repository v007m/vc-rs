//! `vc import har`: the calls a browser recorded (HAR 1.2 or 1.3), as apis in one draft api file
//! per origin, with the noise filtered out and every credential left out, and with `--flow` a
//! test replaying them in order.

mod draft;
mod entry;
mod filter;
mod flow;

use async_trait::async_trait;
use vc_interfaces::{ImportDraft, ImportError, ImportOptions, ImportSource, Importer};

/// Reads a HAR file into draft api files, one per origin it calls.
///
/// Static assets, `OPTIONS` preflights and third-party sites are filtered out, and the draft's
/// summary counts them. A credential header or cookie is never written; a secret in a
/// query, body or path becomes a `${secret.NAME}` reference listed in the draft's secrets. With
/// `flow`, the draft's flow calls them in recorded order, chaining values from the responses.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct HarImporter;

#[async_trait]
impl Importer for HarImporter {
    async fn read(&self, source: &ImportSource, opts: &ImportOptions) -> Result<ImportDraft, ImportError> {
        let invalid = |reason: String| ImportError::Invalid { location: source.location.clone(), reason };
        let kept = filter::kept(entry::entries(&source.bytes).map_err(invalid)?, &filter::Rules::new(&opts.recording));
        let (mut draft, recorded) = draft::draft(kept.entries.iter().map(|entry| &entry.request), &opts.file_id);
        let Some(first) = draft.groups.first() else {
            return Err(invalid(nothing_kept(&kept)));
        };
        if let Some(id) = &opts.flow {
            let steps = recorded.into_iter().zip(&kept.entries);
            let steps = steps.filter_map(|(recorded, entry)| Some((recorded?, &entry.response)));
            draft.flow = Some(flow::flow(id, &first.file_id, steps));
        }
        draft.summary = Some(kept.summary);
        Ok(draft)
    }

    fn read_help(&self) -> Option<&'static str> {
        Some(READ_HELP)
    }
}

const READ_HELP: &str = "a recording keeps the calls to the sites the profiles' base URLs are on; \
                         `--include-host <host>` keeps another host and `--keep-static` static files";

/// Why nothing was kept, with an `--include-host` for each third-party host left out.
fn nothing_kept(kept: &filter::Kept) -> String {
    let reason = format!("it records no call to an api: {}", kept.summary);
    let flags: Vec<_> = kept.third_party.iter().map(|host| format!("`--include-host {host}`")).collect();
    match flags.split_last() {
        None => reason,
        Some((last, [])) => format!("{reason}; keep the third-party host with {last}"),
        Some((last, rest)) => format!("{reason}; keep a third-party host with {} or {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests;
