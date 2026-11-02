/// Errors raised by the diagnostics crate.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DiagnosticsError {
    /// The text is not the code of any known diagnostic.
    #[error("unknown diagnostic code `{0}`")]
    UnknownCode(String),
}
