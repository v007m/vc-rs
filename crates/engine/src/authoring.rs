//! What the authoring commands share (D107): the project's files as read now, a definition
//! found by its ID with "did you mean" when it is not there, and a change checked by the compile
//! guard before an app may write it.

use vc_diagnostics::{Diagnostics, TextEdit};
use vc_model::{FileKind, SourceFile, SourceFileId, SourceFiles};
use vc_parser::{Document, Node, Syntax};

use crate::edit::{Changes, FileChange};
use crate::{Engine, EngineError};

/// The project as an authoring command finds it: every file and its diagnostics now.
pub struct Authoring {
    pub diagnostics: Diagnostics,
    pub files: SourceFiles,
}

/// One definition file opened for editing.
pub struct Opened<'p> {
    pub file: &'p SourceFile,
    pub document: Document,
}

/// A definition named by its full ID (`users.crud#listsUsers`) or a file by its ID (`users.crud`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Named<'t> {
    pub file: &'t str,
    pub item: Option<&'t str>,
}

impl Engine {
    /// The project now, for an authoring command.
    pub(crate) fn authoring(&self) -> Result<Authoring, EngineError> {
        let compiled = self.compiled()?;
        Ok(Authoring { diagnostics: compiled.diagnostics, files: compiled.files })
    }

    /// `changes` once the compile guard passes them against `project`.
    pub(crate) fn guarded(&self, project: &Authoring, changes: Changes) -> Result<Changes, EngineError> {
        self.guard(&project.diagnostics, &changes)?;
        Ok(changes)
    }
}

impl<'t> Named<'t> {
    /// `text` split at its `#`.
    pub fn parse(text: &'t str) -> Self {
        match text.split_once('#') {
            Some((file, item)) => Self { file, item: Some(item) },
            None => Self { file: text, item: None },
        }
    }
}

impl Authoring {
    /// Each file with its number, in reading order.
    pub fn entries(&self) -> impl Iterator<Item = (SourceFileId, &SourceFile)> {
        (0..self.files.len()).filter_map(|index| {
            let id = SourceFileId::new(u32::try_from(index).ok()?);
            self.files.get(id).map(|file| (id, file))
        })
    }

    /// The file of `kind` whose ID is `file_id`.
    pub fn find(&self, kind: FileKind, file_id: &str) -> Option<(SourceFileId, &SourceFile)> {
        self.entries().find(|(_, file)| file.kind() == Some(kind) && kind.file_id(file.path.as_str()) == file_id)
    }

    /// The file `id` parsed for editing.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] when it does not parse.
    pub fn open(&self, (id, file): (SourceFileId, &SourceFile)) -> Result<Opened<'_>, EngineError> {
        let document = Document::parse(id, Syntax::of(&file.path), file.text.as_str()).map_err(|error| refused(
            format!("{} does not parse: {error}", file.path),
            "fix it first; `vc check` shows where",
        ))?;
        Ok(Opened { file, document })
    }

    /// The full ID of every item under `list` (`apis`, `tests`) in the files of `kind`, and
    /// each such file's ID, for "did you mean".
    pub fn known(&self, kind: FileKind, list: &str) -> Vec<String> {
        let files = self.entries().filter(|(_, file)| file.kind() == Some(kind));
        let mut known = Vec::new();
        for (id, file) in files {
            let file_id = kind.file_id(file.path.as_str());
            let (root, _) = vc_parser::parse(id, Syntax::of(&file.path), &file.text);
            let items = root.iter().flat_map(|root| root.get(list).map(Node::items).unwrap_or_default());
            known.extend(items.filter_map(|item| item.get("id")?.as_str().map(|item| format!("{file_id}#{item}"))));
            known.push(file_id);
        }
        known
    }

    /// The refusal for `wanted`, which names no `what`, with the close ones in `known`.
    pub fn unknown(what: &str, wanted: &str, known: &[String]) -> EngineError {
        let close = vc_planner::closest(wanted, known.iter().map(String::as_str));
        let help = match close.as_slice() {
            [] => format!("`vc list {what}s` lists them"),
            [one] => format!("did you mean `{one}`?"),
            several => format!("did you mean one of {}?", several.iter().map(|id| format!("`{id}`")).collect::<Vec<_>>().join(", ")),
        };
        refused(format!("there is no {what} `{wanted}`"), &help)
    }
}

impl Opened<'_> {
    /// The change `edits` make to this file.
    ///
    /// # Errors
    ///
    /// [`EngineError::Internal`] when an edit does not fit or the result does not parse.
    pub fn change(&self, edits: &[TextEdit]) -> Result<FileChange, EngineError> {
        let after = self.document.apply(edits).map_err(crate::edit::internal)?;
        Ok(FileChange {
            path: self.file.path.to_string(),
            before: Some(self.file.text.clone()),
            after: after.text().to_owned(),
        })
    }
}

/// The item under `list` whose `id` is `id`.
pub fn item<'d>(root: &'d Node, list: &str, id: &str) -> Option<&'d Node> {
    root.get(list)?.items().iter().find(|item| item.get("id").and_then(Node::as_str) == Some(id))
}

/// An authoring refusal: nothing was written.
pub fn refused(reason: impl Into<String>, help: &str) -> EngineError {
    EngineError::Edit { reason: reason.into(), help: Some(help.to_owned()) }
}

#[cfg(test)]
mod tests {
    use super::{Authoring, Named};
    use crate::EngineError;

    #[test]
    fn a_name_splits_at_its_hash() {
        assert_eq!(Named::parse("users.crud#listsUsers"), Named { file: "users.crud", item: Some("listsUsers") });
        assert_eq!(Named::parse("users.crud"), Named { file: "users.crud", item: None });
    }

    #[test]
    fn an_unknown_name_suggests_the_close_ones() {
        let known = ["users#list".to_owned(), "users#create".to_owned(), "users".to_owned()];
        let help = |error: EngineError| error.help().unwrap_or_default();
        assert_eq!(help(Authoring::unknown("api", "users#lst", &known)), "did you mean `users#list`?");
        assert_eq!(help(Authoring::unknown("api", "zzz", &known)), "`vc list apis` lists them");
        let two = ["a#ab".to_owned(), "a#ac".to_owned()];
        assert_eq!(help(Authoring::unknown("test", "a#aa", &two)), "did you mean one of `a#ab`, `a#ac`?");
    }
}
