use serde::Serialize;

/// How serious a diagnostic is.
///
/// Run-time results that the code table calls a failure or blocked (V5xxx) are errors here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Stops the run: `vc check` exits non-zero.
    Error,
    /// Reported, but does not stop the run.
    Warning,
    /// Extra information.
    Info,
}
