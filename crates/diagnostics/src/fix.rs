use std::ops::Range;

use serde::Serialize;
use vc_model::{SourceFileId, SourceSpan};

/// A change that resolves a diagnostic: byte-range replacements, possibly across several files.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Fix {
    /// What the fix does: "Replace with orders#place".
    pub title: String,
    /// Whether `vc check --fix` may apply it without asking for each fix.
    pub certain: bool,
    /// The replacements, applied together.
    pub edits: Vec<TextEdit>,
}

impl Fix {
    /// A fix that is the only sensible one, so it can be applied without asking.
    #[must_use]
    pub fn certain(title: impl Into<String>, edits: Vec<TextEdit>) -> Self {
        Self { title: title.into(), certain: true, edits }
    }

    /// A fix that is likely but not certain, so the user confirms it.
    #[must_use]
    pub fn suggested(title: impl Into<String>, edits: Vec<TextEdit>) -> Self {
        Self { title: title.into(), certain: false, edits }
    }
}

/// Replaces a byte range of one file with new text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct TextEdit {
    /// The file to change.
    pub file: SourceFileId,
    /// The bytes to replace; an empty range inserts.
    pub range: Range<usize>,
    /// The text to put in their place.
    pub new_text: String,
}

impl TextEdit {
    /// Replaces the bytes `span` covers with `new_text`.
    #[must_use]
    pub fn replace(span: SourceSpan, new_text: impl Into<String>) -> Self {
        Self { file: span.file, range: span.byte_range(), new_text: new_text.into() }
    }
}

#[cfg(test)]
mod tests {
    use vc_model::{Pos, SourceFileId, SourceSpan};

    use super::{Fix, TextEdit};

    #[test]
    fn replace_takes_file_and_bytes_from_the_span() {
        let span = SourceSpan::new(SourceFileId::new(3), Pos::new(1, 5, 4), Pos::new(1, 9, 8));
        let edit = TextEdit::replace(span, "orders#place");
        assert_eq!(edit, TextEdit { file: SourceFileId::new(3), range: 4..8, new_text: "orders#place".to_owned() });
    }

    #[test]
    fn certain_and_suggested_differ_only_in_certainty() {
        assert!(Fix::certain("Add the unit", Vec::new()).certain);
        assert!(!Fix::suggested("Add the unit", Vec::new()).certain);
    }
}
