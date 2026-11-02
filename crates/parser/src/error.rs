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
    /// An edit's range is outside the text, splits a character, overlaps another edit or names
    /// another file.
    #[error("edit range {start}..{end} does not fit the text")]
    BadRange {
        /// The first byte.
        start: usize,
        /// The byte after the last.
        end: usize,
    },
}
