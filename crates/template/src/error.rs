use vc_diagnostics::Diagnostic;
use vc_model::ModelError;

/// What can go wrong in this crate.
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    /// A template the user wrote does not parse; the diagnostic says where and why.
    #[error("{}", .0.message)]
    Syntax(Box<Diagnostic>),
    /// An internal limit, such as a full string table; not the user's to fix.
    #[error(transparent)]
    Internal(#[from] ModelError),
    /// A pipe registered under a name already taken.
    #[error("pipe `{0}` is already registered")]
    DuplicatePipe(String),
    /// A plugin pipe registered without a `prefix.`.
    #[error("pipe `{0}` needs a plugin prefix, such as `acme.{0}`")]
    UnprefixedPipe(String),
}

/// Why a value could not be rendered at run time; the executor reports it as V5111.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct RenderError {
    /// What went wrong, with the path and the value found.
    pub message: String,
    /// Set when the value is absent, so a `default` pipe may stand in for it.
    pub missing: bool,
}

impl RenderError {
    /// A value that is not there.
    #[must_use]
    pub fn missing(message: impl Into<String>) -> Self {
        Self { message: message.into(), missing: true }
    }

    /// A value that is there but cannot be used.
    #[must_use]
    pub fn invalid(message: impl Into<String>) -> Self {
        Self { message: message.into(), missing: false }
    }
}
