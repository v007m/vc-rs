//! The run's plugins (G8): a host for each `plugins:` item that `vibranium.lock` approves and
//! that is installed, its process started on first use. A plugin the run cannot use is named
//! with the reason, and fails only the work that needs it.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;
use vc_adapter_plugins::StdioPluginHost;
use vc_interfaces::{PLUGINS_DIR, PluginHost, PluginKind, Secrets};
use vc_model::{CelProgram, Program, Reference, TemplateValue};
use vc_template::{Chance, PipeRegistry, RenderError, Renderer, Resolve};

use crate::notify::Steady;
use crate::plugin_lock::Locked;
use crate::plugins::{self, PluginState};
use crate::{Engine, EngineError};

/// Why a plugin's `config` cannot read a template value.
const LITERAL_ONLY: &str = "a plugin's `config` takes literal values; the plugin reads a secret with `secrets/get`";

/// The run's plugins: a host for each one it can use, and why each other one cannot be used.
#[derive(Default)]
pub struct Hosts {
    ready: BTreeMap<String, Arc<dyn PluginHost>>,
    unusable: BTreeMap<String, String>,
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

    /// One line for each plugin the run cannot use, and why.
    pub fn notices(&self) -> Vec<String> {
        self.unusable.iter().map(|(id, reason)| format!("plugin `{id}`: {reason}")).collect()
    }
}

impl Engine {
    /// The hosts of `program`'s plugins, each able to read those of `secrets` that its manifest
    /// declares and the lock grants.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] when `vibranium.lock` is not valid, and [`EngineError::Sources`]
    /// when it or a manifest cannot be read.
    pub(crate) fn plugin_hosts(&self, program: &Program, secrets: &Secrets) -> Result<Hosts, EngineError> {
        let mut hosts = Hosts::default();
        if program.config.plugins.is_empty() {
            return Ok(hosts);
        }
        let (_, locked) = self.locked()?;
        for plugin in &program.config.plugins {
            let id = program.str(plugin.id).unwrap_or_default().to_owned();
            let lock = locked.iter().find(|entry| entry.id == id);
            let host = self.host(&id, lock)?.and_then(|host| Ok(host.config(literal(plugin.config.as_ref())?)));
            match host {
                Ok(host) => drop(hosts.ready.insert(id, Arc::new(host.secrets(secrets)))),
                Err(reason) => drop(hosts.unusable.insert(id, reason)),
            }
        }
        Ok(hosts)
    }

    /// The host of the installed plugin `id`, allowed what `lock` grants; otherwise why the run
    /// cannot use it.
    fn host(&self, id: &str, lock: Option<&Locked>) -> Result<Result<StdioPluginHost, String>, EngineError> {
        let manifest = self.installed(id)?;
        let state = plugins::state(true, lock, manifest.as_ref());
        let (PluginState::Ready, Some(lock), Some(manifest)) = (&state, lock, manifest) else {
            return Ok(Err(state.to_string()));
        };
        let dir = self.root().join(PLUGINS_DIR).join(id);
        Ok(Ok(StdioPluginHost::new(manifest, dir, lock.permissions.clone())))
    }
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
