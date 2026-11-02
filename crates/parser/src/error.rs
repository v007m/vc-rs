use thiserror::Error;

/// Why an edit could not be made.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ParserError {
    /// The text does not parse, before or after the edit; holds the first error's message.
    #[error("the text does not parse: {0}")]
    Unparsable(String),
    /// The edit needs a different kind of node.
    #[error("expected {expected}, found {found}")]
    WrongNode {
        /// What the edit needs: "a block map".
        expected: &'static str,
        /// What it was given: "a list".
        found: &'static str,
    },
    /// The value cannot be written so that it reads back as the kind asked for.
    #[error("`{value}` cannot be written as {kind}")]
    NotOfKind {
        /// The value given.
        value: String,
        /// The kind asked for: "an integer".
        kind: &'static str,
    },
    /// The map already has the key being inserted.
    #[error("the map already has the key `{0}`")]
    DuplicateKey(String),
    /// An edit's range is outside the text, splits a character, overlaps another edit or names
    /// another file; or a node given to an edit is not part of this document.
    #[error("edit range {start}..{end} does not fit the text")]
    BadRange {
        /// The first byte.
        start: usize,
        /// The byte after the last.
        end: usize,
    },
}
