//! What can stop the terminal UI.

/// Why `vc tui` stopped.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TuiError {
    /// The terminal could not be set up, drawn on or restored.
    #[error("the terminal failed: {0}")]
    Terminal(#[from] std::io::Error),
    /// The task reading the keyboard stopped.
    #[error("reading the keyboard stopped: {reason}")]
    Input {
        /// What went wrong.
        reason: String,
    },
}
