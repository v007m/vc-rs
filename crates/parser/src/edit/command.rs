//! Edits applied as one step, with the step that undoes them.

use vc_diagnostics::TextEdit;

use super::Document;
use crate::ParserError;

/// Edits to one document, applied together and undone together (D107).
///
/// ```
/// use vc_model::SourceFileId;
/// use vc_parser::{Document, EditCommand, Syntax};
///
/// let doc = Document::parse(SourceFileId::new(0), Syntax::Yaml, "a: 1  # one\nb: 2\n").unwrap();
/// let edit = doc.set_scalar(doc.root().get("a").unwrap(), "10").unwrap();
/// let (after, undo) = EditCommand::new(vec![edit]).apply(&doc).unwrap();
/// assert_eq!(after.text(), "a: 10  # one\nb: 2\n");
/// assert_eq!(undo.apply(&after).unwrap().0.text(), doc.text());
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditCommand {
    edits: Vec<TextEdit>,
}

impl EditCommand {
    /// A command making `edits`, which must not overlap.
    #[must_use]
    pub const fn new(edits: Vec<TextEdit>) -> Self {
        Self { edits }
    }

    /// The edits, in the order given.
    #[must_use]
    pub fn edits(&self) -> &[TextEdit] {
        &self.edits
    }

    /// `document` with the edits made, and the command that gives back its exact bytes.
    ///
    /// # Errors
    /// As [`Document::apply`].
    pub fn apply(&self, document: &Document) -> Result<(Document, Self), ParserError> {
        let changed = document.apply(&self.edits)?;
        Ok((changed, self.inverse(document.text())))
    }

    /// The edits that turn the changed text back into `before`: each one puts the old bytes back
    /// where the new text landed, after the shifts made by the edits before it.
    fn inverse(&self, before: &str) -> Self {
        let mut sorted: Vec<&TextEdit> = self.edits.iter().collect();
        sorted.sort_by_key(|edit| (edit.range.start, edit.range.end));
        let (mut added, mut removed) = (0, 0);
        let undo = sorted.into_iter().map(|edit| {
            let start = edit.range.start + added - removed;
            added += edit.new_text.len();
            removed += edit.range.len();
            let old = before.get(edit.range.clone()).unwrap_or_default().to_owned();
            TextEdit { file: edit.file, range: start..start + edit.new_text.len(), new_text: old }
        });
        Self { edits: undo.collect() }
    }
}
