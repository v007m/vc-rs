//! The run's plugins (G8): a host for each `plugins:` item that `vibranium.lock` approves and
//! that is installed, its files checked and its process started on first use. A plugin the run
//! cannot use is named with the reason, and fails only the work that needs it.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::broadcast;
use vc_adapter_plugins::StdioPluginHost;
use vc_adapter_system::SystemEnvironment;
use vc_interfaces::{PLUGINS_DIR, PluginHost, PluginKind, PluginStderr, Secrets};
use vc_model::{CelProgram, Program, Reference, TemplateValue};
use vc_template::{Chance, PipeRegistry, RenderError, Renderer, Resolve};

use crate::Engine;
use crate::notify::Steady;
use crate::plugin_intact::intact;
use crate::plugin_lock::Locked;
use crate::plugins::{self, PluginState};

/// Where each plugin keeps its own files, in a folder named by its id: its install folder is
/// checked against the lock, so it must not change.
/// Each folder on the way from the project root is made as the plugin starts, and a link among
/// them keeps it from starting.
const PLUGIN_DATA: &str = ".vibranium/plugin-data";

/// Where the host keeps each plugin's stderr log ([`PluginStderr::Log`]), in a folder named by
/// its id: never under [`PLUGIN_DATA`], which the plugin writes to. Each folder on the way from
/// the project root is made as the plugin starts, and refused when it is a link.
const PLUGIN_LOGS: &str = ".vibranium/plugin-logs";

/// The most lines the run's plugins can say as they start before the run hears them: two each
/// at most.
const HEARD: usize = 64;

/// The process environment variable that lets plugins run unconfined where they cannot be
/// confined, when `1` or `true` (D260). It is never read from `.env`, which the project writes.
pub const ALLOW_UNCONFINED_ENV: &str = "VIB_PLUGINS_ALLOW_UNCONFINED";

/// Why a plugin's `config` cannot read a template value.
const LITERAL_ONLY: &str = "a plugin's `config` takes literal values; the plugin reads a secret with `secrets/get`";

/// The run's plugins: a host for each one it can use, why each other one cannot be used, and
/// where they say why one starts with its stderr dropped.
#[derive(Clone)]
pub struct Hosts {
    ready: BTreeMap<String, Arc<dyn PluginHost>>,
    unusable: BTreeMap<String, String>,
    told: broadcast::Sender<String>,
}

impl Default for Hosts {
    fn default() -> Self {
        let (told, _) = broadcast::channel(HEARD);
        Self { ready: BTreeMap::new(), unusable: BTreeMap::new(), told }
    }
}

impl Hosts {
    /// The plugin `id`, when it is a `kind` plugin the run can use; otherwise why not.
    pub fn get(&self, id: &str, kind: PluginKind) -> Result<Arc<dyn PluginHost>, String> {
        if let Some(reason) = self.unusable.get(id) {
            return Err(format!("plugin `{id}`: {reason}"));
        }
        let host = self.ready.get(id).ok_or_else(|| format!("there is no plugin `{id}` in the config"))?;
        let found = host.manifest().kind;
        if found != kind {
            return Err(format!("plugin `{id}` is a `{}` plugin, not `{}`", found.name(), kind.name()));
        }
        Ok(Arc::clone(host))
    }

    /// The `kind` plugins the run can use.
    pub fn of(&self, kind: PluginKind) -> impl Iterator<Item = Arc<dyn PluginHost>> + '_ {
        self.ready.values().filter(move |host| host.manifest().kind == kind).cloned()
    }

    /// Each plugin the run cannot use, and why.
    pub fn unusable(&self) -> impl Iterator<Item = (&str, &str)> {
        self.unusable.iter().map(|(id, reason)| (id.as_str(), reason.as_str()))
    }

    /// One line for each plugin the run cannot use, and why, but for the reporter plugins in
    /// `reported`: the run's reports name each of those once, with its report not written.
    pub fn notices(&self, reported: &[String]) -> Vec<String> {
        let unusable = self.unusable.iter().filter(|(id, _)| !reported.contains(*id));
        unusable.map(|(id, reason)| format!("plugin `{id}`: {reason}")).collect()
    }

    /// What the plugins say from now on as they start: a line for each whose stderr is dropped
    /// as its log cannot be made, and for each that runs unconfined (D260), and why. A plugin
    /// never started says nothing.
    pub fn heard(&self) -> broadcast::Receiver<String> {
        self.told.subscribe()
    }

    /// Ends every plugin the run started, once the run is over, and waits until each has
    /// exited or been killed ([`PluginHost::shutdown`]), so none is cut off when `vc` exits.
    pub async fn shutdown(&self) {
        let mut ending = tokio::task::JoinSet::new();
        for host in self.ready.values() {
            let host = Arc::clone(host);
            drop(ending.spawn(async move { host.shutdown().await }));
        }
        while ending.join_next().await.is_some() {}
    }
}

impl Engine {
    /// The hosts of `program`'s plugins, each able to read those of `secrets` that its manifest
    /// declares and the lock grants. A lock or manifest that cannot be read makes the plugins it
    /// concerns unusable, with the reason; installed files that no longer match the lock fail
    /// the plugin's first call.
    pub(crate) fn plugin_hosts(&self, program: &Program, secrets: &Secrets) -> Hosts {
        let mut hosts = Hosts::default();
        if program.config.plugins.is_empty() {
            return hosts;
        }
        let locked = self.locked().map(|(_, locked)| locked).map_err(|error| error.to_string());
        let unconfined = self.unconfined_allowed();
        for plugin in &program.config.plugins {
            let id = program.str(plugin.id).unwrap_or_default().to_owned();
            let host = locked
                .as_ref()
                .map_err(Clone::clone)
                .and_then(|locked| self.host(&id, locked.iter().find(|entry| entry.id == id)))
                .and_then(|(host, approved)| Ok((host.config(literal(plugin.config.as_ref())?), approved)));
            match host {
                Ok((host, approved)) => {
                    let host = host.secrets(secrets).told(hosts.told.clone()).allow_unconfined(unconfined);
                    let host = intact(host, approved, &self.plugins);
                    drop(hosts.ready.insert(id, host));
                }
                Err(reason) => drop(hosts.unusable.insert(id, reason)),
            }
        }
        hosts
    }

    /// Whether the user lets plugins run unconfined where they cannot be confined (D260):
    /// [`ALLOW_UNCONFINED_ENV`] in the process environment, or `plugins.allowUnconfined: true` in
    /// the user config. Never the project's config, a profile or `.env`, so a cloned project
    /// cannot switch confinement off; a user config that cannot be read allows nothing.
    pub(crate) fn unconfined_allowed(&self) -> bool {
        let process = self.environment.clone().unwrap_or_else(|| Arc::new(SystemEnvironment::capture()));
        let set = process.var(ALLOW_UNCONFINED_ENV).is_some_and(|value| value == "1" || value == "true");
        set || self.user_layer(process.as_ref()).is_ok_and(|user| user.allows_unconfined_plugins())
    }

    /// The host of the installed plugin `id`, allowed what `lock` grants and keeping its own
    /// files under [`PLUGIN_DATA`], with the integrity the lock approved; otherwise why the run
    /// cannot use it.
    fn host(&self, id: &str, lock: Option<&Locked>) -> Result<(StdioPluginHost, String), String> {
        let manifest = self.installed(id).map_err(|error| error.to_string())?;
        let state = plugins::state(true, lock, manifest.as_ref());
        let (PluginState::Ready, Some(lock), Some(manifest)) = (&state, lock, manifest) else {
            return Err(state.to_string());
        };
        let dir = self.root().join(PLUGINS_DIR).join(id);
        let host = StdioPluginHost::new(manifest, dir, lock.permissions.clone())
            .data(self.root(), Path::new(PLUGIN_DATA).join(id))
            .confined(self.root(), Arc::clone(&self.confiner));
        let logged = self.plugin_processes.stderr == PluginStderr::Log;
        let host = if logged { host.stderr_log(self.root(), Path::new(PLUGIN_LOGS).join(id)) } else { host };
        Ok((host, lock.integrity.clone()))
    }
}

/// Runs `vc` as the plugin launcher when `args` (this process's, the program first) ask it to.
///
/// It confines this process to the plugin's permissions and becomes the plugin, returning only
/// why it could not; `None` when they do not, and the program goes on as usual (D254).
pub fn launch_plugin(args: impl Iterator<Item = OsString>) -> Option<String> {
    vc_adapter_plugins::launch(args)
}

/// A plugin's `config`, which holds literal values only, so it never carries a secret the lock
/// did not grant.
fn literal(config: Option<&TemplateValue>) -> Result<Value, String> {
    let Some(config) = config else { return Ok(Value::Null) };
    let pipes = PipeRegistry::builtin();
    Renderer::new(&pipes, &Literal).value(config).map_err(|error| format!("its `config`: {error}"))
}

/// Answers no lookups: a plugin's `config` has no `${…}` values.
struct Literal;

impl Resolve for Literal {
    fn resolve(&self, _reference: &Reference) -> Result<Value, RenderError> {
        Err(RenderError::invalid(LITERAL_ONLY))
    }

    fn evaluate(&self, _program: &CelProgram) -> Result<Value, RenderError> {
        Err(RenderError::invalid(LITERAL_ONLY))
    }

    fn chance(&self) -> &dyn Chance {
        &Steady
    }
}
