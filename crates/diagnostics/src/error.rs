/// Errors raised by the diagnostics crate.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum DiagnosticsError {
    /// The text is not the code of any known diagnostic.
    #[error("unknown diagnostic code `{0}`")]
    UnknownCode(String),
    /// A span or edit points at a file the renderer was not given.
    #[error("no source file #{0} to render against")]
    UnknownFile(u32),
    /// The JSON renderer could not write its output.
    #[error("cannot write diagnostics as JSON: {0}")]
    Json(String),
}
