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
