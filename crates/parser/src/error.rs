use thiserror::Error;

/// Why the parser could not give a result.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ParserError {
    /// The text does not parse; holds the first error's message.
    #[error("the text does not parse: {0}")]
    Unparsable(String),
}
