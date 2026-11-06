//! `vc skip` and `vc unskip`: write or remove the `skip` of a test, or of a test file at its top
//! (D77). Unskipping a skip `vc skip` wrote gives back the original bytes.

use vc_diagnostics::TextEdit;
use vc_model::{Date, FileKind};
use vc_parser::{Node, write_text};

use crate::authoring::{Authoring, Named, Opened, item, refused};
use crate::edit::Changes;
use crate::{Engine, EngineError};

/// What `vc skip` asks for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct SkipRequest {
    /// A test (`users.crud#listsUsers`) or a test file (`users.crud`).
    pub target: String,
    /// Why, for the report.
    pub reason: String,
    /// The last day of the skip, `YYYY-MM-DD`; none for the short form.
    pub until: Option<String>,
}

impl Engine {
    /// The change that skips a test or a test file: `skip: <reason>`, or `skip:` with `reason`
    /// and `until` when a date is given, as the test's last key or the file's first.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] for a blank reason, a date that is not one, an unknown target
    /// (with the close ones), a target already skipped, or a change the guard refuses.
    pub fn skip(&self, request: &SkipRequest) -> Result<Changes, EngineError> {
        if request.reason.trim().is_empty() {
            return Err(refused("a skip needs a reason", "say why, for the report: `--reason \"flaky upstream\"`"));
        }
        if let Some(until) = request.until.as_deref().filter(|until| Date::parse(until).is_none()) {
            return Err(refused(format!("`{until}` is not a date"), "write it as YYYY-MM-DD: `--until 2026-12-01`"));
        }
        let project = self.authoring()?;
        let opened = target(&project, &request.target)?;
        let host = host(&opened, &request.target)?;
        if host.get("skip").is_some() {
            let help = format!("`vc unskip {}` first", request.target);
            return Err(refused(format!("`{}` is already skipped", request.target), &help));
        }
        let edit = skip_edit(&opened, host, request)?;
        let changes = Changes { files: vec![opened.change(&[edit])?], titles: vec![format!("Skip {}", request.target)] };
        self.guarded(&project, changes)
    }

    /// The change that removes the `skip` of a test or a test file, comment included.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] for an unknown target or one that is not skipped.
    pub fn unskip(&self, target_text: &str) -> Result<Changes, EngineError> {
        let project = self.authoring()?;
        let opened = target(&project, target_text)?;
        let skip = host(&opened, target_text)?
            .get("skip")
            .ok_or_else(|| refused(format!("`{target_text}` is not skipped"), "`vc list tests --skipped` lists the skips"))?;
        let edit = opened.document.remove(skip).map_err(|error| refused(format!("cannot remove this skip: {error}"), "remove it by hand"))?;
        let changes = Changes { files: vec![opened.change(&[edit])?], titles: vec![format!("Unskip {target_text}")] };
        self.guarded(&project, changes)
    }
}

/// The test file `text` names, opened.
fn target<'p>(project: &'p Authoring, text: &str) -> Result<Opened<'p>, EngineError> {
    let named = Named::parse(text);
    match project.find(FileKind::Test, named.file) {
        Some(found) => project.open(found),
        None => Err(Authoring::unknown("test", text, &project.known(FileKind::Test, "tests"))),
    }
}

/// The map that holds the skip: the test `text` names, or the file's root for a file ID.
fn host<'d>(opened: &'d Opened<'_>, text: &str) -> Result<&'d Node, EngineError> {
    let root = opened.document.root();
    match Named::parse(text).item {
        None => Ok(root),
        Some(id) => item(root, "tests", id).ok_or_else(|| {
            let ids = root.get("tests").map(Node::items).unwrap_or_default().iter();
            let file = Named::parse(text).file;
            let known: Vec<String> = ids.filter_map(|test| test.get("id")?.as_str().map(|id| format!("{file}#{id}"))).collect();
            Authoring::unknown("test", text, &known)
        }),
    }
}

/// The edit that adds the skip to `host`.
fn skip_edit(opened: &Opened<'_>, host: &Node, request: &SkipRequest) -> Result<TextEdit, EngineError> {
    let document = &opened.document;
    let reason = request.reason.trim();
    let cannot = |error: vc_parser::ParserError| refused(format!("cannot add a skip here: {error}"), "add it by hand");
    let is_file = Named::parse(&request.target).item.is_none();
    match (&request.until, is_file) {
        (None, false) => document.insert_entry(host, "skip", reason).map_err(cannot),
        (Some(until), false) => document.insert_yaml(host, "skip", &long_form(reason, until, "")).map_err(cannot),
        (None, true) => document.insert_first(host, "skip", reason).map_err(cannot),
        (Some(until), true) => {
            let at = document.insert_first(host, "skip", reason).map_err(cannot)?;
            let newline = if document.text().contains("\r\n") { "\r\n" } else { "\n" };
            let text = format!("skip:\n{}", long_form(reason, until, "  ")).replace('\n', newline);
            Ok(TextEdit { new_text: text, ..at })
        }
    }
}

/// `reason` and `until` as a YAML map, each line after `indent` and ending in a newline.
fn long_form(reason: &str, until: &str, indent: &str) -> String {
    format!("{indent}reason: {}\n{indent}until: {}\n", write_text(reason, false), write_text(until, false))
}

#[cfg(test)]
mod tests {
    use super::long_form;

    #[test]
    fn the_long_form_quotes_a_reason_only_when_yaml_needs_it() {
        assert_eq!(long_form("upstream down", "2099-01-01", ""), "reason: upstream down\nuntil: 2099-01-01\n");
        assert_eq!(long_form("see: #12", "2099-01-01", "  "), "  reason: \"see: #12\"\n  until: 2099-01-01\n");
    }
}
