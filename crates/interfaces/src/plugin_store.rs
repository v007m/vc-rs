//! Where plugins live: `vc plugin add` fetches one into a staging folder, the user approves its
//! permissions, and it moves to `.vibranium/plugins/<id>/`. `vibranium.lock` records what was
//! approved; the engine writes it like any other project file and reads it back from here.

use std::path::PathBuf;

/// The folder plugins are installed into, under the project root.
pub const PLUGINS_DIR: &str = ".vibranium/plugins";

/// The file that records the approved plugins, at the project root.
pub const LOCK_FILE: &str = "vibranium.lock";

/// The manifest every plugin has at the top of its folder.
pub const MANIFEST_FILE: &str = "vibranium-plugin.yaml";

/// The installed plugins and the lock.
///
/// A source is `npm:<package>[@<range>]` or `path:<folder>` (a folder relative to the project
/// root); the engine checks the scheme before it asks.
pub trait PluginStore: Send + Sync {
    /// Fetches `source` into a staging folder.
    ///
    /// # Errors
    ///
    /// [`PluginStoreError::Fetch`] when it cannot be fetched or has no manifest; nothing is left
    /// behind.
    fn fetch(&self, source: &str) -> Result<Fetched, PluginStoreError>;

    /// Moves a fetched plugin to `.vibranium/plugins/<id>/`, replacing one installed there.
    ///
    /// # Errors
    ///
    /// [`PluginStoreError::Install`] when it cannot be moved, or `id` is not a plain word.
    fn install(&self, fetched: &Fetched, id: &str) -> Result<(), PluginStoreError>;

    /// Drops a fetched plugin that was not approved; nothing when it is gone already.
    fn discard(&self, fetched: &Fetched);

    /// Removes `.vibranium/plugins/<id>/`; nothing when it is not there.
    ///
    /// # Errors
    ///
    /// [`PluginStoreError::Install`] when it cannot be removed, or `id` is not a plain word.
    fn uninstall(&self, id: &str) -> Result<(), PluginStoreError>;

    /// The text of `vibranium.lock`; `None` when the project has none.
    ///
    /// # Errors
    ///
    /// [`PluginStoreError::Read`] when it is there but cannot be read.
    fn lock(&self) -> Result<Option<String>, PluginStoreError>;

    /// The manifest text of the installed plugin `id`; `None` when it is not installed.
    ///
    /// # Errors
    ///
    /// [`PluginStoreError::Read`] when it is there but cannot be read.
    fn manifest(&self, id: &str) -> Result<Option<String>, PluginStoreError>;
}

/// A plugin fetched into a staging folder, waiting for approval.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the plugins adapter and read by the engine; every user is in this repo (D160)"
)]
pub struct Fetched {
    /// The staging folder.
    pub staged: PathBuf,
    /// Its `vibranium-plugin.yaml`.
    pub manifest: String,
    /// What the source resolved to: `@acme/vibranium-report@2.1.0`, or the folder.
    pub resolved: String,
    /// A hash of what was fetched: npm's `sha512-…`, or `blake3-…` over a folder's files.
    pub integrity: String,
}

/// Why a plugin could not be fetched, installed or read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PluginStoreError {
    /// The source could not be fetched, or holds no manifest.
    #[error("cannot fetch `{spec}`: {reason}")]
    Fetch {
        /// The source, as written (`thiserror` reserves the name `source`).
        spec: String,
        /// Why.
        reason: String,
    },
    /// The plugin could not be moved into place or removed.
    #[error("cannot install the plugin `{id}`: {reason}")]
    Install {
        /// The plugin's ID.
        id: String,
        /// Why.
        reason: String,
    },
    /// The lock or a manifest is there but cannot be read.
    #[error("{path}: cannot be read: {reason}")]
    Read {
        /// The file, relative to the project root.
        path: String,
        /// Why.
        reason: String,
    },
}
