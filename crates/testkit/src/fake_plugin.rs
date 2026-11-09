use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use vc_interfaces::{PluginError, PluginHost, PluginManifest, Secrets};

/// The JSON-RPC code of a method the plugin does not have.
const METHOD_NOT_FOUND: i64 = -32_601;

/// What a [`FakePlugin`] answers for one method, given the params and the secrets it may read.
pub type PluginScript = dyn Fn(&Value, &Secrets) -> Result<Value, PluginError> + Send + Sync;

/// A [`PluginHost`] that answers each method from a script and starts no process.
///
/// It keeps only the run's secrets its manifest declares, as a real host would.
#[derive(Clone)]
pub struct FakePlugin {
    manifest: PluginManifest,
    secrets: Secrets,
    scripts: BTreeMap<String, Arc<PluginScript>>,
    stalls: BTreeSet<String>,
}

impl FakePlugin {
    /// A plugin with `manifest`, given those of `secrets` the manifest declares.
    #[must_use]
    pub fn new(manifest: PluginManifest, secrets: &Secrets) -> Self {
        let mut granted = Secrets::new();
        let declared = |name: &str| manifest.permissions.secrets.iter().any(|secret| secret == name);
        for (name, secret) in secrets.iter().filter(|(name, _)| declared(name)) {
            granted.insert(name, secret.clone());
        }
        Self { manifest, secrets: granted, scripts: BTreeMap::new(), stalls: BTreeSet::new() }
    }

    /// This plugin, answering `method` with `script`.
    #[must_use]
    pub fn on(
        mut self,
        method: &str,
        script: impl Fn(&Value, &Secrets) -> Result<Value, PluginError> + Send + Sync + 'static,
    ) -> Self {
        self.scripts.insert(method.to_owned(), Arc::new(script));
        self
    }

    /// This plugin, never answering `method`.
    #[must_use]
    pub fn stalling(mut self, method: &str) -> Self {
        self.stalls.insert(method.to_owned());
        self
    }

    fn error(&self, method: &str, code: i64, message: &str) -> PluginError {
        let plugin = self.manifest.id.clone();
        PluginError::Remote { plugin, method: method.to_owned(), code, message: message.to_owned() }
    }
}

impl fmt::Debug for FakePlugin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FakePlugin")
            .field("id", &self.manifest.id)
            .field("methods", &self.scripts.keys())
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl PluginHost for FakePlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn call(&self, method: &str, params: Value, deadline: Duration) -> Result<Value, PluginError> {
        if self.stalls.contains(method) {
            tokio::time::sleep(deadline).await;
            let plugin = self.manifest.id.clone();
            return Err(PluginError::Deadline { plugin, method: method.to_owned(), after: deadline });
        }
        let script =
            self.scripts.get(method).ok_or_else(|| self.error(method, METHOD_NOT_FOUND, "method not found"))?;
        script(&params, &self.secrets)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vc_interfaces::{PluginError, PluginHost, Secrets};

    use super::FakePlugin;
    use crate::{plugin_host_suite, suite_plugin_manifest};

    fn suite_fake(secrets: &Secrets) -> FakePlugin {
        FakePlugin::new(suite_plugin_manifest(), secrets)
            .on("suite/echo", |params, _| Ok(params.clone()))
            .on("suite/secret", |params, secrets| {
                let name = params["name"].as_str().unwrap_or_default();
                let refused =
                    || PluginError::Permission { plugin: "suitePlugin".into(), what: format!("secret {name}") };
                secrets.get(name).map(|secret| json!(secret.expose())).ok_or_else(refused)
            })
            .stalling("suite/stall")
    }

    #[tokio::test]
    async fn fake_plugin_meets_the_contract() {
        plugin_host_suite(&mut |secrets| Box::new(suite_fake(secrets)) as Box<dyn PluginHost>).await;
        let debug = format!("{:?}", suite_fake(&Secrets::new()));
        assert!(debug.contains("suitePlugin") && debug.contains("suite/echo"), "{debug}");
    }
}
