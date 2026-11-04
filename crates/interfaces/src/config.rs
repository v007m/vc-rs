/// One setting from one config layer: `defaults.concurrency = 4`, from `--set`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConfigEntry {
    /// The setting's dotted key, as written in `vibranium.config.yaml` (`reports.dir`), or
    /// `profile` for the active profiles.
    pub key: String,
    /// The value as text; the engine reads it as the key's type.
    pub value: String,
    /// Where it was set, for messages and `vc config explain`: `.env`, `VIB_PROFILE`, `--set`.
    pub origin: String,
}

impl ConfigEntry {
    /// `key` set to `value` by `origin`.
    #[must_use]
    pub fn new(key: impl Into<String>, value: impl Into<String>, origin: impl Into<String>) -> Self {
        Self { key: key.into(), value: value.into(), origin: origin.into() }
    }
}

/// One config layer above the project's own files (D129).
///
/// The layers are `.env`, the `VIB_*` environment variables and `--set`. The engine reads them
/// lowest first, so a later entry for the same key wins, and every value keeps its origin.
pub trait ConfigSource: Send + Sync {
    /// The layer's settings, in the order written.
    ///
    /// # Errors
    ///
    /// [`ConfigSourceError`] when the layer cannot be read or an entry is not a setting.
    fn entries(&self) -> Result<Vec<ConfigEntry>, ConfigSourceError>;
}

/// Why a config layer could not be read. The user can act on each one.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigSourceError {
    /// The layer's file exists but cannot be read.
    #[error("cannot read {origin}: {reason}")]
    Unreadable {
        /// The layer, such as `.env`.
        origin: String,
        /// What went wrong.
        reason: String,
    },
    /// An entry is not `key=value`.
    #[error("{origin}: `{entry}` is not key=value")]
    Malformed {
        /// The layer, such as `--set`.
        origin: String,
        /// The entry as written.
        entry: String,
    },
}
