//! `vc check --fix`: the certain fixes of the selected diagnostics, as one guarded change.

use vc_diagnostics::{Diagnostics, TextEdit};
use vc_model::{SourceFileId, SourceFiles};
use vc_parser::{Document, Syntax};

use crate::edit::{Changes, FileChange, internal};
use crate::{CheckReport, CheckRequest, Engine, EngineError};

/// What `vc check --fix` found and would change.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct FixReport {
    /// The check as it is now.
    pub report: CheckReport,
    /// The certain fixes, applied to the files they touch; empty when there are none.
    pub changes: Changes,
}

impl Engine {
    /// Checks the project and gathers the certain fix of each selected diagnostic that has one.
    /// A fix whose edits overlap an earlier one's is left for the next run, and nothing is
    /// written: the app shows the changes and writes them.
    ///
    /// # Errors
    ///
    /// As [`Engine::check`]; [`EngineError::Edit`] when the fixes together would add an error.
    pub fn fix(&self, request: &CheckRequest) -> Result<FixReport, EngineError> {
        let compiled = self.compiled()?;
        let before = compiled.diagnostics.clone();
        let report = Self::report(compiled, request)?;
        let changes = certain_fixes(&report.diagnostics, &report.files)?;
        if !changes.is_empty() {
            self.guard(&before, &changes)?;
        }
        Ok(FixReport { report, changes })
    }
}

/// The first certain fix of each diagnostic, skipping one that overlaps a fix already taken.
fn certain_fixes(diagnostics: &Diagnostics, files: &SourceFiles) -> Result<Changes, EngineError> {
    let mut taken: Vec<&TextEdit> = Vec::new();
    let mut titles = Vec::new();
    for fix in diagnostics.iter().filter_map(|d| d.fixes.iter().find(|fix| fix.certain)) {
        if fix.edits.iter().any(|edit| taken.iter().any(|done| overlaps(done, edit))) {
            continue;
        }
        taken.extend(&fix.edits);
        titles.push(fix.title.clone());
    }
    let mut ids: Vec<SourceFileId> = taken.iter().map(|edit| edit.file).collect();
    ids.sort();
    ids.dedup();
    let changed = ids.into_iter().map(|id| changed_file(files, id, &taken));
    Ok(Changes { files: changed.collect::<Result<_, _>>()?, titles })
}

/// The file `id` with the edits in `taken` that touch it.
fn changed_file(files: &SourceFiles, id: SourceFileId, taken: &[&TextEdit]) -> Result<FileChange, EngineError> {
    let file = files.get(id).ok_or_else(|| internal(format!("a fix edits file #{} that was not read", id.index())))?;
    let edits: Vec<TextEdit> = taken.iter().filter(|edit| edit.file == id).map(|&edit| edit.clone()).collect();
    let document = Document::parse(id, Syntax::of(&file.path), file.text.as_str()).map_err(internal)?;
    let after = document.apply(&edits).map_err(internal)?;
    Ok(FileChange { path: file.path.to_string(), before: Some(file.text.clone()), after: after.text().to_owned(), moved_from: None })
}

/// Whether two edits touch the same bytes of one file; two insertions at one place overlap.
fn overlaps(a: &TextEdit, b: &TextEdit) -> bool {
    let end = |edit: &TextEdit| edit.range.end.max(edit.range.start + 1);
    a.file == b.file && a.range.start < end(b) && b.range.start < end(a)
}

#[cfg(test)]
mod tests {
    use vc_diagnostics::TextEdit;
    use vc_model::SourceFileId;

    use super::overlaps;

    fn edit(file: u32, start: usize, end: usize) -> TextEdit {
        TextEdit { file: SourceFileId::new(file), range: start..end, new_text: String::new() }
    }

    #[test]
    fn edits_overlap_only_on_shared_bytes_of_one_file() {
        assert!(overlaps(&edit(0, 2, 5), &edit(0, 4, 8)));
        assert!(overlaps(&edit(0, 3, 3), &edit(0, 3, 3)));
        assert!(overlaps(&edit(0, 3, 3), &edit(0, 2, 4)));
        assert!(!overlaps(&edit(0, 2, 5), &edit(0, 5, 8)));
        assert!(!overlaps(&edit(0, 2, 5), &edit(1, 2, 5)));
        assert!(!overlaps(&edit(0, 5, 5), &edit(0, 2, 5)));
    }
}
