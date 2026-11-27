//! What refers to one definition file's items from the rest of the project: the list a person
//! confirms before the web UI moves the file to the trash (D575, D683).

use vc_model::{SourceFile, SourceFileId};
use vc_parser::{Node, references};

use crate::authoring::Authoring;
use crate::rename::item_ids;
use crate::{Engine, EngineError};

/// One `call:` or `test:` in another file that names an item of the file asked about.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct FileReference {
    /// The file the reference is in, relative to the project root.
    pub path: String,
    /// The line it is on, from 1.
    pub line: u32,
    /// What holds it: the full ID of the test, api or action it is in (`orders.flow#checksOut`),
    /// or the top-level key (`beforeRun`) when that has no ID.
    pub from: String,
    /// The item it names, by full ID (`users#list`).
    pub target: String,
}

/// The references to one file's items, and the files that could not be read for them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct FileReferences {
    /// In path order, then file order.
    pub references: Vec<FileReference>,
    /// The definition files that do not parse, so were not searched.
    pub unread: Vec<String>,
}

impl Engine {
    /// Every `call:` and `test:` outside the api, test or load file at `path` that names one of
    /// its items, short forms resolved against the file they are in. A file that does not parse
    /// is named in `unread` rather than failing the list.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] as [`Engine::trash_file`]; [`EngineError::NoFilesMatch`] when the
    /// project has no file there; [`EngineError::Sources`] when the project cannot be read.
    pub fn file_references(&self, path: &str) -> Result<FileReferences, EngineError> {
        let path = self.definition_path(path)?;
        let project = self.authoring()?;
        let own = project.entries().find(|(_, file)| file.path == path);
        let own = own.ok_or_else(|| EngineError::NoFilesMatch { path: path.to_string() })?;
        let file_id = own.1.path.file_id().unwrap_or_default();
        let opened = Authoring::open(own)?;
        let targets: Vec<String> =
            item_ids(opened.document.root()).iter().map(|id| format!("{file_id}#{id}")).collect();
        let mut found = FileReferences::default();
        for other in project.entries().filter(|(_, file)| file.path != path && file.kind().is_some()) {
            match Authoring::open(other) {
                Ok(opened) => found.references.extend(referring(other, opened.document.root(), &targets)),
                Err(_) => found.unread.push(other.1.path.to_string()),
            }
        }
        Ok(found)
    }
}

/// The references in `root`, the file `file`, to one of `targets`, each with what holds it.
fn referring((_, file): (SourceFileId, &SourceFile), root: &Node, targets: &[String]) -> Vec<FileReference> {
    let file_id = file.path.file_id().unwrap_or_default();
    let mut found = Vec::new();
    for entry in root.entries() {
        let holders: Vec<(&Node, String)> = if entry.value.items().is_empty() {
            vec![(&entry.value, entry.key.clone())]
        } else {
            entry.value.items().iter().map(|item| (item, holder(&file_id, item, &entry.key))).collect()
        };
        for (node, from) in holders {
            let named = references(node).into_iter().filter_map(|value| Some((value, full(&file_id, value.as_str()?))));
            found.extend(named.filter(|(_, target)| targets.contains(target)).map(|(value, target)| FileReference {
                path: file.path.to_string(),
                line: value.span.start.line,
                from: from.clone(),
                target,
            }));
        }
    }
    found
}

/// The full ID of `item`, in the file `file_id`, or the list `key` it is in when it has none.
fn holder(file_id: &str, item: &Node, key: &str) -> String {
    item.get("id").and_then(Node::as_str).map_or_else(|| key.to_owned(), |id| format!("{file_id}#{id}"))
}

/// `text`, a reference in the file `file_id`, as a full ID: a short one names an item there.
fn full(file_id: &str, text: &str) -> String {
    if text.contains('#') { text.to_owned() } else { format!("{file_id}#{text}") }
}

#[cfg(test)]
#[path = "file_refs_tests.rs"]
mod tests;
