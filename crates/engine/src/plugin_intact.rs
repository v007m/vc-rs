//! A plugin whose installed files are checked against `vibranium.lock` on its first call, before
//! its process starts: a run hashes only the plugins it uses, off the async threads.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::OnceCell;
use vc_diagnostics::Code;
use vc_interfaces::{LOCK_FILE, PluginError, PluginHost, PluginManifest, PluginStore, PluginStoreError};

/// The prefix of an integrity the engine can check against an installed folder.
const FOLDER_HASH: &str = "blake3-";

/// `host`, checked on first use against the integrity `approved` in the lock when that is a
/// folder hash. An npm install's `sha512-…` is the tarball's, which the unpacked folder cannot
/// reproduce, so it is checked when `vc plugin add` fetches it, not here.
pub fn intact(host: impl PluginHost + 'static, approved: String, store: &Arc<dyn PluginStore>) -> Arc<dyn PluginHost> {
    if !approved.starts_with(FOLDER_HASH) {
        return Arc::new(host);
    }
    Arc::new(Intact { host: Box::new(host), approved, store: Arc::clone(store), checked: OnceCell::new() })
}

/// A plugin host that refuses every call once its installed files no longer match the lock.
struct Intact {
    host: Box<dyn PluginHost>,
    approved: String,
    store: Arc<dyn PluginStore>,
    checked: OnceCell<Result<(), String>>,
}

impl Intact {
    /// Whether the installed files still hash to the approved integrity; otherwise why not.
    async fn check(&self) -> Result<(), String> {
        let (store, id, approved) = (Arc::clone(&self.store), self.host.manifest().id.clone(), self.approved.clone());
        let hashed = tokio::task::spawn_blocking(move || unchanged(store.as_ref(), &id, &approved)).await;
        if hashed.map_err(|error| error.to_string())?.map_err(|error| error.to_string())? {
            return Ok(());
        }
        Err(format!(
            "{}: its installed files changed since {LOCK_FILE} approved them: write state to $VIB_PLUGIN_DATA; \
             re-approve with `vc plugin add` if the change was intended",
            Code::V5118
        ))
    }
}

/// Whether the installed files of the plugin `id` still hash to the integrity `approved` in the
/// lock; an integrity that is not a folder hash cannot be checked here, so it counts as
/// unchanged. It reads every installed file: call it off the async threads.
///
/// # Errors
///
/// Why the installed files cannot be read.
pub fn unchanged(store: &dyn PluginStore, id: &str, approved: &str) -> Result<bool, PluginStoreError> {
    if !approved.starts_with(FOLDER_HASH) {
        return Ok(true);
    }
    Ok(store.integrity(id)?.as_deref() == Some(approved))
}

#[async_trait]
impl PluginHost for Intact {
    fn manifest(&self) -> &PluginManifest {
        self.host.manifest()
    }

    async fn call(&self, method: &str, params: Value, deadline: Duration) -> Result<Value, PluginError> {
        if let Err(reason) = self.checked.get_or_init(|| self.check()).await {
            return Err(PluginError::Start { plugin: self.manifest().id.clone(), reason: reason.clone() });
        }
        self.host.call(method, params, deadline).await
    }

    async fn shutdown(&self) {
        self.host.shutdown().await;
    }
}
