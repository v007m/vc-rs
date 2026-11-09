//! What an authoring command would write, and the compile guard every such write passes (D107):
//! a change that would add an error `vc check` reports is refused, so nothing is written.

use vc_diagnostics::{Diagnostic, Diagnostics};
use vc_model::{SourceFile, SourceFiles, SourcePath};

use crate::project::sources_error;
use crate::{Engine, EngineError};

/// One definition file an authoring command changes or creates.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FileChange {
    /// Where the file is, relative to the project root, with `/` separators.
    pub path: String,
    /// Its text now; `None` for a new file.
    pub before: Option<String>,
    /// Its text after the change.
    pub after: String,
    /// Where the file was before a move (`vc rename` of a file ID); written to `path`, then
    /// removed from here.
    pub moved_from: Option<String>,
}

/// What an authoring command would write. The engine never writes it: the app shows it, asks
/// when it should, and writes each file's `after` text to its path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Changes {
    /// The files, in path order.
    pub files: Vec<FileChange>,
    /// What each change does, one line each: `Rename to listsUsers`.
    pub titles: Vec<String>,
}

impl FileChange {
    /// The change as a unified diff with three lines of context, headed by the path.
    #[must_use]
    pub fn diff(&self) -> String {
        let before = self.before.as_deref().unwrap_or_default();
        let diff = similar::TextDiff::from_lines(before, &self.after);
        let was = self.moved_from.as_ref().unwrap_or(&self.path);
        let old = if self.before.is_some() { format!("a/{was}") } else { "/dev/null".to_owned() };
        diff.unified_diff().context_radius(3).header(&old, &format!("b/{}", self.path)).to_string()
    }
}

impl Changes {
    /// Whether nothing would change.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

impl Engine {
    /// Writes `changes`, as a guarded engine call returned them, through the source writer.
    /// Apps cannot build [`Changes`] themselves, so only text the guard passed is written.
    ///
    /// # Errors
    ///
    /// [`EngineError::Sources`] naming the first file that could not be written; the files
    /// before it are written. A moved file is written at its new path before the old one is
    /// removed.
    pub fn write(&self, changes: &Changes) -> Result<(), EngineError> {
        let writer = self.writer.as_ref().ok_or_else(|| internal("this engine has no folder to write into"))?;
        for change in &changes.files {
            let path = SourcePath::new(&change.path).map_err(internal)?;
            writer.write(&path, &change.after).map_err(sources_error)?;
            if let Some(was) = &change.moved_from {
                writer.remove(&SourcePath::new(was).map_err(internal)?).map_err(sources_error)?;
            }
        }
        Ok(())
    }

    /// Compiles the project as it would be after `changes` and refuses them when that adds an
    /// error: one whose code and message are not among the errors in `before`, the project's
    /// diagnostics now.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] naming the first new error; as [`Engine::check`] when the project
    /// cannot be read.
    pub(crate) fn guard(&self, before: &Diagnostics, changes: &Changes) -> Result<(), EngineError> {
        let after = self.compiled_with(&changes.files)?;
        let known = |new: &&Diagnostic| before.iter().any(|old| same_error(old, new));
        match after.diagnostics.iter().filter(|d| d.is_error()).find(|new| !known(new)) {
            None => Ok(()),
            Some(new) => Err(refusal(new, &after.files)),
        }
    }
}

/// `files` with each change in place: a changed file replaced, a new one added, a moved one
/// taken from where it was.
///
/// # Errors
///
/// [`EngineError::Internal`] when a change's path is not a project path.
pub fn overlay(files: &mut Vec<SourceFile>, changes: &[FileChange]) -> Result<(), EngineError> {
    for change in changes {
        let path = SourcePath::new(&change.path).map_err(internal)?;
        let file = vc_interfaces::source_file(path, change.after.clone().into_bytes()).map_err(internal)?;
        files.retain(|old| old.path != file.path && Some(old.path.as_str()) != change.moved_from.as_deref());
        files.push(file);
    }
    Ok(())
}

/// Whether `old` and `new` are the same error, wherever each is.
fn same_error(old: &Diagnostic, new: &Diagnostic) -> bool {
    old.is_error() && old.code == new.code && old.message == new.message
}

/// The refusal for a change that would add `error`.
fn refusal(error: &Diagnostic, files: &SourceFiles) -> EngineError {
    let span = error.primary.span;
    let path = files.get(span.file).map_or_else(String::new, |file| file.path.to_string());
    let reason = format!(
        "nothing was written: the change would add error[{}] at {path}:{}: {}",
        error.code, span.start.line, error.message
    );
    let help = error.help.first().cloned().or_else(|| Some("run `vc check` to see the project's problems".to_owned()));
    EngineError::Edit { reason, help }
}

/// An error that should not happen, as a bug to report.
pub fn internal(error: impl std::fmt::Display) -> EngineError {
    EngineError::Internal { reason: error.to_string() }
}

#[cfg(test)]
mod tests {
    use super::{Changes, FileChange};

    fn change(before: Option<&str>, after: &str) -> FileChange {
        let (path, before, after) = ("services/a/b.test.yaml".to_owned(), before.map(str::to_owned), after.to_owned());
        FileChange { path, before, after, moved_from: None }
    }

    #[test]
    fn a_diff_shows_the_changed_lines_with_the_path() {
        let diff = change(Some("a: 1\nb: 2\n"), "a: 1\nb: 3\n").diff();
        assert!(diff.contains("--- a/services/a/b.test.yaml\n+++ b/services/a/b.test.yaml\n"), "{diff}");
        assert!(diff.contains("-b: 2\n+b: 3\n") && diff.contains(" a: 1\n"), "{diff}");
    }

    #[test]
    fn a_moved_file_diffs_from_where_it_was() {
        let moved =
            FileChange { moved_from: Some("services/a/old.test.yaml".to_owned()), ..change(Some("a: 1\n"), "a: 2\n") };
        assert!(
            moved.diff().contains("--- a/services/a/old.test.yaml\n+++ b/services/a/b.test.yaml\n"),
            "{}",
            moved.diff()
        );
    }

    #[test]
    fn a_new_file_diffs_from_nothing() {
        let diff = change(None, "a: 1\n").diff();
        assert!(diff.starts_with("--- /dev/null\n") && diff.contains("+a: 1\n"), "{diff}");
    }

    #[test]
    fn changes_are_empty_without_files() {
        assert!(Changes::default().is_empty());
        assert!(!Changes { files: vec![change(None, "")], titles: Vec::new() }.is_empty());
    }
}
