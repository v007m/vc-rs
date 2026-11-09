//! The lines a recording's import prints: what was read, where each host went, what became a
//! secret reference, what `.env.schema` gains, the flow, then the notes, one line per message.

use indexmap::IndexMap;
use vc_compiler::camel_id;
use vc_interfaces::{DraftGroup, ImportDraft, ImportNote, SecretFinding};

/// How a note on a dropped credential header ends; its header goes on the secrets line.
const MAPPED: &str = ": never written; it maps to the file's auth";

/// Where one host of the recording went.
#[derive(Debug, Default)]
pub struct Placed {
    pub host: String,
    pub file: String,
    pub existing: bool,
    pub matched: usize,
    pub added: usize,
    /// The file's own auth, which a dropped credential header maps to.
    pub auth: Option<String>,
}

/// What the import wrote besides the api files: the `.env.schema` line and the flow line.
pub struct Written<'w> {
    pub placed: &'w [Placed],
    pub schema: Option<String>,
    pub flow: Option<String>,
}

impl Placed {
    /// `group`, going to the api file `file`, which exists or is new.
    pub fn of(group: &DraftGroup, file: &str, existing: bool) -> Self {
        let origin = group.config_vars.values().next().and_then(serde_json::Value::as_str).unwrap_or_default();
        let host = origin.split_once("://").map_or(origin, |(_, host)| host).to_owned();
        Self { host, file: file.to_owned(), existing, ..Self::default() }
    }

    /// `  api.shop.test → shop (existing)  2 matched, 1 new · auth shopToken`; the auth part
    /// only when a credential header was dropped.
    fn line(&self, dropped: bool) -> String {
        let counts = [(self.matched, "matched"), (self.added, "new")];
        let counts: Vec<_> = counts.iter().filter(|(n, _)| *n > 0).map(|(n, what)| format!("{n} {what}")).collect();
        let state = if self.existing { "existing" } else { "new" };
        let line = format!("  {} → {} ({state})  {}", self.host, self.file, counts.join(", "));
        if !dropped {
            return line;
        }
        let auth = self.auth.as_ref().map_or_else(
            || {
                let suggested = camel_id(&format!("{} auth", self.file.rsplit('.').next().unwrap_or(&self.file)));
                format!(" · no auth yet: `vc new auth {suggested} --type bearer`")
            },
            |auth| format!(" · auth {auth}"),
        );
        format!("{line}{auth}")
    }
}

/// Every line, the first `Import <n> apis from <source>`.
pub fn titles(draft: &ImportDraft, source: &str, written: &Written<'_>) -> Vec<String> {
    let count: usize = draft.groups.iter().map(|group| group.apis.len()).sum();
    let mut titles = vec![format!("Import {count} apis from {source}")];
    titles.extend(draft.summary.clone());
    let headers = dropped(&draft.notes);
    titles.extend(written.placed.iter().map(|placed| placed.line(!headers.is_empty())));
    titles.extend(secrets(&headers, &draft.secrets));
    titles.extend(written.schema.clone());
    titles.extend(written.flow.clone());
    titles.extend(noted(&draft.notes));
    titles
}

/// The credential headers the notes say were dropped, each once, in order.
fn dropped(notes: &[ImportNote]) -> Vec<&str> {
    let mut headers = Vec::new();
    for note in notes {
        let header = note.message.strip_suffix(MAPPED).and_then(|rest| rest.strip_prefix("header "));
        if let Some(header) = header.filter(|header| !headers.contains(header)) {
            headers.push(header);
        }
    }
    headers
}

/// `secrets  header authorization, cookie dropped → each file's auth · query api_key →
/// ${secret.PAY_API_KEY}`; none when nothing was.
fn secrets(headers: &[&str], found: &[SecretFinding]) -> Option<String> {
    let mut parts = Vec::new();
    if !headers.is_empty() {
        parts.push(format!("header {} dropped → each file's auth", headers.join(", ")));
    }
    for secret in found {
        let place = secret.found_at.split_once(": ").map_or(secret.found_at.as_str(), |(_, place)| place);
        parts.push(format!("{place} → ${{secret.{}}}", secret.name));
    }
    (!parts.is_empty()).then(|| format!("secrets  {}", parts.join(" · ")))
}

/// The notes other than dropped headers, one line per message: `at: message`, or
/// `at and 3 more: message` when several places share it.
fn noted(notes: &[ImportNote]) -> Vec<String> {
    let mut grouped: IndexMap<&str, Vec<&str>> = IndexMap::new();
    for note in notes.iter().filter(|note| !note.message.starts_with("header ") || !note.message.ends_with(MAPPED)) {
        grouped.entry(note.message.as_str()).or_default().push(note.at.as_str());
    }
    let line = |(message, places): (&str, Vec<&str>)| match places.as_slice() {
        [one] => format!("{one}: {message}"),
        [first, rest @ ..] => format!("{first} and {} more: {message}", rest.len()),
        [] => message.to_owned(),
    };
    grouped.into_iter().map(line).collect()
}

#[cfg(test)]
mod tests {
    use vc_interfaces::ImportNote;

    use super::{Placed, dropped, noted};

    fn note(at: &str, message: &str) -> ImportNote {
        ImportNote { at: at.to_owned(), message: message.to_owned() }
    }

    #[test]
    fn notes_sharing_a_message_are_one_line_and_dropped_headers_leave_them() {
        let notes = [
            note("POST /a", "header cookie: never written; it maps to the file's auth"),
            note("POST /a", "a multipart body is not imported, so it is left out"),
            note("POST /b", "a multipart body is not imported, so it is left out"),
            note("GET /c", "header cookie: never written; it maps to the file's auth"),
            note("GET /c", "recorded 2 times, merged into one api"),
        ];
        assert_eq!(dropped(&notes), ["cookie"]);
        assert_eq!(
            noted(&notes),
            [
                "POST /a and 1 more: a multipart body is not imported, so it is left out",
                "GET /c: recorded 2 times, merged into one api"
            ]
        );
    }

    #[test]
    fn a_host_line_names_its_file_counts_and_auth() {
        let placed = Placed { host: "pay.test".to_owned(), file: "pay".to_owned(), added: 2, ..Placed::default() };
        assert_eq!(placed.line(false), "  pay.test → pay (new)  2 new");
        assert_eq!(
            placed.line(true),
            "  pay.test → pay (new)  2 new · no auth yet: `vc new auth payAuth --type bearer`"
        );
        let shop = Placed { existing: true, matched: 1, auth: Some("shopToken".to_owned()), ..placed };
        assert_eq!(shop.line(true), "  pay.test → pay (existing)  1 matched, 2 new · auth shopToken");
    }
}
