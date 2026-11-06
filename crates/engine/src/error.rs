use crate::CheckReport;

/// Why the engine could not do what it was asked.
///
/// Every variant but [`EngineError::Internal`] is something the user can fix; the CLI shows
/// it as `error: …` with [`EngineError::help`] and exits 2.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EngineError {
    /// No folder at or above the start folder holds a `vibranium.config.yaml`.
    #[error("no vibranium.config.yaml in {start} or any folder above it")]
    NoProject {
        /// Where the search started.
        start: String,
    },
    /// The project's files cannot be searched at all.
    #[error("{reason}")]
    Sources {
        /// What went wrong.
        reason: String,
    },
    /// A path given to the check selects no definition file.
    #[error("{path}: no definition file is at or under this path")]
    NoFilesMatch {
        /// The path, relative to the project root.
        path: String,
    },
    /// The schemas could not be written as JSON.
    #[error("cannot write the schemas: {reason}")]
    Schema {
        /// What went wrong.
        reason: String,
    },
    /// The definitions have errors, so nothing can be planned or run; the report holds them.
    #[error("the project has {} error(s); nothing ran", .0.summary().errors)]
    Invalid(Box<CheckReport>),
    /// The selectors or flags select nothing that can run.
    #[error("{reason}")]
    Selection {
        /// What is wrong.
        reason: String,
        /// What to do instead, when the engine knows.
        help: Option<String>,
    },
    /// A setting from `.env`, `VIB_*`, `--set` or a flag is not one the run knows, or its
    /// value does not fit.
    #[error("{origin}: {reason}")]
    Setting {
        /// The layer that set it: `.env`, `VIB_PROFILE`, `--set`, `--concurrency`.
        origin: String,
        /// What is wrong.
        reason: String,
        /// What to do instead, when the engine knows.
        help: Option<String>,
    },
    /// A reporter name is unknown, not built yet, or given to the wrong flag.
    #[error("{reason}")]
    Reporter {
        /// What is wrong.
        reason: String,
        /// What to do instead.
        help: Option<String>,
    },
    /// The run history cannot be read or written, or holds no such run.
    #[error("history: {reason}")]
    History {
        /// What is wrong.
        reason: String,
        /// What to do instead, when the engine knows.
        help: Option<String>,
    },
    /// The cross-run cache has no key or cannot be opened or read.
    #[error("cache: {reason}")]
    Cache {
        /// What is wrong.
        reason: String,
        /// What to do instead, when the engine knows.
        help: Option<String>,
    },
    /// Something inside the engine failed; this is a bug to report, not a user error.
    #[error("{reason}")]
    Internal {
        /// What went wrong.
        reason: String,
    },
}

impl EngineError {
    /// The `help:` line to show under the error, when there is one.
    #[must_use]
    pub fn help(&self) -> Option<String> {
        match self {
            Self::NoProject { .. } => {
                Some("run it inside a project, pass `--project <dir>`, or create one with `vc init`".to_owned())
            }
            Self::Selection { help, .. }
            | Self::Setting { help, .. }
            | Self::Reporter { help, .. }
            | Self::History { help, .. }
            | Self::Cache { help, .. } => help.clone(),
            _ => None,
        }
    }

    /// Whether this is a bug in the engine rather than something the user can fix.
    #[must_use]
    pub const fn is_internal(&self) -> bool {
        matches!(self, Self::Internal { .. })
    }
}
