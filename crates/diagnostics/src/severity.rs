use std::fmt;

use serde::Serialize;

/// How serious a diagnostic is. Severities order from `Info` up to `Error`, so the worst of
/// several is their `max()`.
///
/// Run-time results that the code table calls a failure or blocked (V5xxx) are errors here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Extra information.
    Info,
    /// Reported, but does not stop the run.
    Warning,
    /// Must be fixed: `vc check` fails.
    Error,
}

impl fmt::Display for Severity {
    /// The lowercase name, as in `error[V2003]`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Severity;

    #[test]
    fn worst_severity_is_the_max() {
        let found = [Severity::Warning, Severity::Error, Severity::Info];
        assert_eq!(found.into_iter().max(), Some(Severity::Error));
        assert!(Severity::Info < Severity::Warning);
    }

    #[test]
    fn displays_in_lowercase() {
        let names = [Severity::Info, Severity::Warning, Severity::Error].map(|s| s.to_string());
        assert_eq!(names, ["info", "warning", "error"]);
    }
}
