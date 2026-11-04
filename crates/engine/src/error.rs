/// Why the engine could not do what it was asked.
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
}
