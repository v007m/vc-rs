//! Talking to a plugin: a separate program that speaks JSON-RPC 2.0, declared by its manifest
//! (`vibranium-plugin.yaml`) and approved in `vibranium.lock`.

use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use vc_diagnostics::Code;

/// How long a plugin has to answer `initialize`.
pub const INITIALIZE_DEADLINE: Duration = Duration::from_secs(5);

/// How long a plugin has to answer any other call.
pub const CALL_DEADLINE: Duration = Duration::from_secs(30);

/// The JSON-RPC error code of a request for something the plugin was not granted.
pub const PERMISSION_DENIED: i64 = -32_001;

/// The JSON-RPC error code of a call that ran past its deadline.
pub const DEADLINE_PASSED: i64 = -32_002;

/// One running plugin.
///
/// The host starts it on first use, checks it asks for no more than its manifest declares, and
/// answers its `secrets/get` requests only for the secrets the manifest lists. Every
/// implementation runs `plugin_host_suite` (D112).
#[async_trait]
pub trait PluginHost: Send + Sync {
    /// The plugin's manifest.
    fn manifest(&self) -> &PluginManifest;

    /// Calls `method` with `params` and waits up to `deadline` for the result.
    ///
    /// # Errors
    ///
    /// [`PluginError::Deadline`] past the deadline, [`PluginError::Permission`] when the plugin
    /// was refused something it needed, [`PluginError::Remote`] for any other error it answers,
    /// and [`PluginError::Crashed`] or [`PluginError::Protocol`] when it stops or garbles.
    async fn call(&self, method: &str, params: Value, deadline: Duration) -> Result<Value, PluginError>;
}

/// What a plugin is for: which methods the host calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum PluginKind {
    /// `reporter/onEvent` and `reporter/finish`: a report format.
    Reporter,
    /// `auth/credentials`: an auth type.
    Auth,
    /// `importer/read`: an import source. Named only; no host calls it yet.
    Importer,
    /// `executor/send`: a protocol. Named only; no host calls it yet.
    Executor,
}

impl PluginKind {
    /// Every kind.
    pub const ALL: [Self; 4] = [Self::Reporter, Self::Auth, Self::Importer, Self::Executor];

    /// The name a manifest's `kind` uses.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Reporter => "reporter",
            Self::Auth => "auth",
            Self::Importer => "importer",
            Self::Executor => "executor",
        }
    }

    /// The kind called `name`.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
}

/// A plugin's `vibranium-plugin.yaml`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "manifest read by the plugins adapter and shown by the engine; every user is in this repo (D160)"
)]
pub struct PluginManifest {
    /// The plugin's camelCase ID, as `plugins[].id` names it.
    pub id: String,
    /// Its version.
    pub version: String,
    /// What it is for.
    pub kind: PluginKind,
    /// The program to start (`entry.command`), run in the plugin's folder.
    pub command: String,
    /// Its arguments (`entry.args`).
    pub args: Vec<String>,
    /// What it may use.
    pub permissions: PluginPermissions,
}

/// What a plugin may use: the hosts it may reach, the secrets it may read and the paths it may
/// open. A manifest declares them; `vibranium.lock` holds the ones the user approved.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "manifest data read by the plugins adapter and the engine; every user is in this repo (D160)"
)]
pub struct PluginPermissions {
    /// Hosts, `api.example.com` or `*.example.com`.
    pub network: Vec<String>,
    /// Secret names.
    pub secrets: Vec<String>,
    /// Paths relative to the project root.
    pub files: Vec<String>,
}

impl PluginPermissions {
    /// What these ask for beyond `granted`, as `network api.example.com` or `secret apiKey`.
    #[must_use]
    pub fn beyond(&self, granted: &Self) -> Vec<String> {
        let extra = |label: &str, asked: &[String], given: &[String]| -> Vec<String> {
            asked.iter().filter(|item| !given.contains(item)).map(|item| format!("{label} {item}")).collect()
        };
        let mut extras = extra("network", &self.network, &granted.network);
        extras.extend(extra("secret", &self.secrets, &granted.secrets));
        extras.extend(extra("file", &self.files, &granted.files));
        extras
    }
}

/// Why a plugin call failed. Only the work that used the plugin fails.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PluginError {
    /// The plugin could not be started, or its manifest is not valid.
    #[error("plugin `{plugin}` could not start: {reason}")]
    Start {
        /// The plugin's ID.
        plugin: String,
        /// Why.
        reason: String,
    },
    /// The plugin asked for something its manifest or the lock does not grant (−32001).
    #[error("plugin `{plugin}` was refused {what}")]
    Permission {
        /// The plugin's ID.
        plugin: String,
        /// What it asked for: `secret apiKey`.
        what: String,
    },
    /// The plugin did not answer in time (−32002).
    #[error("plugin `{plugin}` did not answer `{method}` within {after:?}")]
    Deadline {
        /// The plugin's ID.
        plugin: String,
        /// The method called.
        method: String,
        /// The deadline it ran past.
        after: Duration,
    },
    /// The plugin answered with a JSON-RPC error.
    #[error("plugin `{plugin}` failed `{method}` ({code}): {message}")]
    Remote {
        /// The plugin's ID.
        plugin: String,
        /// The method called.
        method: String,
        /// The JSON-RPC error code.
        code: i64,
        /// Its message, redacted.
        message: String,
    },
    /// The plugin's process ended.
    #[error("plugin `{plugin}` stopped: {reason}")]
    Crashed {
        /// The plugin's ID.
        plugin: String,
        /// Its exit status, or how the pipe broke.
        reason: String,
    },
    /// The plugin sent something that is not the protocol.
    #[error("plugin `{plugin}` broke the protocol: {reason}")]
    Protocol {
        /// The plugin's ID.
        plugin: String,
        /// What was wrong.
        reason: String,
    },
}

impl PluginError {
    /// The diagnostic code: every plugin failure is V5118.
    #[must_use]
    pub const fn code(&self) -> Code {
        Code::V5118
    }

    /// The JSON-RPC error code the failure carries, if it has one.
    #[must_use]
    pub const fn rpc_code(&self) -> Option<i64> {
        match self {
            Self::Permission { .. } => Some(PERMISSION_DENIED),
            Self::Deadline { .. } => Some(DEADLINE_PASSED),
            Self::Remote { code, .. } => Some(*code),
            Self::Start { .. } | Self::Crashed { .. } | Self::Protocol { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use vc_diagnostics::Code;

    use super::{DEADLINE_PASSED, PERMISSION_DENIED, PluginError, PluginKind, PluginPermissions};

    #[test]
    fn a_kind_is_named_as_the_manifest_writes_it() {
        for kind in PluginKind::ALL {
            assert_eq!(PluginKind::named(kind.name()), Some(kind));
        }
        assert_eq!(PluginKind::named("notifier"), None);
    }

    #[test]
    fn lists_what_a_manifest_asks_beyond_the_lock() {
        let list = |items: &[&str]| items.iter().map(|item| (*item).to_owned()).collect::<Vec<_>>();
        let asked = PluginPermissions {
            network: list(&["api.acme.dev"]),
            secrets: list(&["reportToken", "apiKey"]),
            files: list(&["out"]),
        };
        let granted =
            PluginPermissions { network: list(&["api.acme.dev"]), secrets: list(&["reportToken"]), files: Vec::new() };
        assert_eq!(asked.beyond(&granted), ["secret apiKey", "file out"]);
        assert_eq!(granted.beyond(&asked), Vec::<String>::new());
    }

    #[test]
    fn errors_carry_their_rpc_codes_and_v5118() {
        let refused = PluginError::Permission { plugin: "acmeReport".into(), what: "secret apiKey".into() };
        assert_eq!(refused.to_string(), "plugin `acmeReport` was refused secret apiKey");
        assert_eq!((refused.code(), refused.rpc_code()), (Code::V5118, Some(PERMISSION_DENIED)));
        let late = PluginError::Deadline {
            plugin: "p".into(),
            method: "auth/credentials".into(),
            after: Duration::from_secs(30),
        };
        assert_eq!(late.to_string(), "plugin `p` did not answer `auth/credentials` within 30s");
        assert_eq!(late.rpc_code(), Some(DEADLINE_PASSED));
        assert_eq!(PluginError::Crashed { plugin: "p".into(), reason: "exit 1".into() }.rpc_code(), None);
    }
}
