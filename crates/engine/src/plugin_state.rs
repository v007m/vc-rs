//! Whether a run can use a plugin, and if not, why: what `vc plugin list`, `vc check` and a run
//! say about each plugin.

use std::fmt;

use vc_diagnostics::Code;
use vc_interfaces::{LOCK_FILE, MANIFEST_FILE, PluginManifest};

use crate::plugin_lock::Locked;

/// Whether a run can use a plugin, and if not, why.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PluginState {
    /// In the config, approved and installed, asking for no more than it was granted.
    Ready,
    /// Approved but not installed here (`.vibranium/` is not committed): add it again.
    NotInstalled,
    /// In the config but not in `vibranium.lock`.
    NotApproved,
    /// Its installed manifest asks for these beyond what the lock grants: `secret apiKey`.
    AsksForMore(Vec<String>),
    /// In `vibranium.lock` but not in the config.
    NotInConfig,
    /// Its installed files no longer hash to what `vibranium.lock` approved (V5118): a run
    /// refuses to start it.
    Changed,
    /// The program its `entry.command` names cannot be found: a run cannot start it.
    NoEntry(String),
}

impl PluginState {
    /// The state as one camelCase word, as the web UI keys it: `ready`, `notInstalled`, ...
    #[must_use]
    pub const fn word(&self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::NotInstalled => "notInstalled",
            Self::NotApproved => "notApproved",
            Self::AsksForMore(_) => "asksForMore",
            Self::NotInConfig => "notInConfig",
            Self::Changed => "changed",
            Self::NoEntry(_) => "noEntry",
        }
    }
}

impl fmt::Display for PluginState {
    /// `ready`, or what is wrong and how to fix it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ready => f.write_str("ready"),
            Self::NotInstalled => f.write_str("not installed: `vc plugin add` it again"),
            Self::NotApproved => f.write_str("not approved: `vc plugin add` it"),
            Self::AsksForMore(more) => write!(f, "asks for more: {}; `vc plugin add` it again", more.join(", ")),
            Self::NotInConfig => f.write_str("not in the config: `vc plugin remove` it"),
            Self::Changed => write!(
                f,
                "{}: its installed files changed since {LOCK_FILE} approved them; `vc plugin add` it again",
                Code::V5118
            ),
            Self::NoEntry(command) => write!(f, "{}", no_entry(command)),
        }
    }
}

/// Why a run cannot start a plugin whose entry program `command` cannot be found.
pub fn no_entry(command: &str) -> String {
    format!("its entry program `{command}` cannot be found: install it, or fix `entry.command` in its {MANIFEST_FILE}")
}

/// Whether a run can use a plugin with these parts.
pub fn state(configured: bool, lock: Option<&Locked>, manifest: Option<&PluginManifest>) -> PluginState {
    let Some(lock) = lock else { return PluginState::NotApproved };
    if !configured {
        return PluginState::NotInConfig;
    }
    let Some(manifest) = manifest else { return PluginState::NotInstalled };
    let beyond = manifest.permissions.beyond(&lock.permissions);
    if beyond.is_empty() { PluginState::Ready } else { PluginState::AsksForMore(beyond) }
}
