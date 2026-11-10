//! Changes to a project's files made one edit at a time: each edit opens a file as the edits
//! before it left it, and the file's change keeps the text it had before the first.

use vc_interfaces::source_file;
use vc_model::{SourceFile, SourceFileId, SourcePath};

use crate::EngineError;
use crate::authoring::{Authoring, Opened};
use crate::edit::{FileChange, internal};

/// The files changed so far, in the order first changed.
#[derive(Debug, Default)]
pub struct Staged {
    files: Vec<FileChange>,
}

/// What an edit does to the file it opened, and what it returns.
pub type Edited<T> = (Option<FileChange>, T);

impl Staged {
    /// Runs `edit` on the file `found` as changed so far, and keeps the change it makes.
    pub fn edit<T>(
        &mut self,
        (id, file): (SourceFileId, &SourceFile),
        edit: impl FnOnce(&Opened<'_>) -> Result<Edited<T>, EngineError>,
    ) -> Result<T, EngineError> {
        let path = file.path.to_string();
        let reread = match self.files.iter().find(|change| change.path == path) {
            Some(change) => Some(reread(change)?),
            None => None,
        };
        let opened = Authoring::open((id, reread.as_ref().unwrap_or(file)))?;
        let (change, value) = edit(&opened)?;
        if let Some(change) = change {
            self.keep(change);
        }
        Ok(value)
    }

    /// Adds `change`, or makes it the text of the file it changes.
    pub fn keep(&mut self, change: FileChange) {
        match self.files.iter_mut().find(|file| file.path == change.path) {
            Some(file) => file.after = change.after,
            None => self.files.push(change),
        }
    }

    /// The changes, in path order.
    pub fn done(mut self) -> Vec<FileChange> {
        self.files.sort_by(|a, b| a.path.cmp(&b.path));
        self.files
    }
}

/// The file `change` leaves.
fn reread(change: &FileChange) -> Result<SourceFile, EngineError> {
    let path = SourcePath::new(&change.path).map_err(internal)?;
    source_file(path, change.after.clone().into_bytes()).map_err(internal)
}

#[cfg(test)]
mod tests {
    use vc_model::{SourceFileId, SourcePath};

    use super::Staged;
    use crate::edit::FileChange;

    #[test]
    fn each_edit_opens_what_the_last_one_left_and_the_change_keeps_the_first_text() {
        let path = SourcePath::new("services/a/a.api.yaml").unwrap();
        let file = vc_interfaces::source_file(path, b"apis: []\n".to_vec()).unwrap();
        let mut staged = Staged::default();
        for n in 1..=2 {
            let seen = staged
                .edit((SourceFileId::new(0), &file), |opened| {
                    let text = opened.document.text().to_owned();
                    let after = format!("{text}# {n}\n");
                    let change = FileChange {
                        path: file.path.to_string(),
                        before: Some(file.text.clone()),
                        after,
                        moved_from: None,
                    };
                    Ok((Some(change), text))
                })
                .unwrap();
            assert_eq!(seen.matches('#').count(), n - 1);
        }
        let done = staged.done();
        assert_eq!(done.len(), 1);
        let change = done.first().unwrap();
        assert_eq!((change.before.as_deref(), change.after.as_str()), (Some("apis: []\n"), "apis: []\n# 1\n# 2\n"));
    }
}
