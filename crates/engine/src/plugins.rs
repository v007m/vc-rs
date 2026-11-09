//! `vc plugin add`, `list` and `remove`. A plugin is fetched into a staging folder and its
//! manifest read; once the user approves what it may reach, its `plugins:` item goes into the
//! config (through the compile guard), its permissions into `vibranium.lock`, and its files to
//! `.vibranium/plugins/<id>/`. Removing it takes all three back out.

use vc_interfaces::{Fetched, LOCK_FILE, MANIFEST_FILE, PluginManifest, PluginPermissions, PluginStoreError};
use vc_model::FileKind;
use vc_parser::{Node, write_text};

use crate::authoring::{Authoring, Opened, item, refused};
use crate::create::{DOCS, ListItem, commented_item};
use crate::edit::{Changes, FileChange, internal};
use crate::explain::did_you_mean;
use crate::plugin_lock::{self, Locked};
use crate::plugin_manifest;
use crate::{Engine, EngineError};

/// The sources `vc plugin add` takes.
const SCHEMES: [&str; 2] = ["npm:", "path:"];

/// A plugin `vc plugin add` fetched, waiting for what it asks to reach to be approved.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PluginOffer {
    /// The source as given: `npm:@acme/vibranium-report@^2`.
    pub source: String,
    /// Its manifest, with the permissions it asks for.
    pub manifest: PluginManifest,
    /// What the source resolved to: `@acme/vibranium-report@2.1.0`.
    pub resolved: String,
    /// A hash of what was fetched.
    pub integrity: String,
    fetched: Fetched,
}

/// One plugin, as `vc plugin list` shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PluginListing {
    /// Its ID.
    pub id: String,
    /// Its source: the config's, else the lock's.
    pub source: String,
    /// The installed manifest; none when it is not installed here.
    pub manifest: Option<PluginManifest>,
    /// What `vibranium.lock` lets it reach; nothing when it is not approved.
    pub granted: PluginPermissions,
    /// Whether a run can use it.
    pub state: PluginState,
}

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
}

impl Engine {
    /// Fetches the plugin at `source` (`npm:<package>` or `path:<folder>`) and reads its manifest,
    /// for the user to approve with [`Engine::plugin_approve`] or turn down with
    /// [`Engine::plugin_decline`].
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] for another kind of source, one that cannot be fetched, or a
    /// manifest that is missing or wrong; nothing is left behind.
    pub fn plugin_fetch(&self, source: &str) -> Result<PluginOffer, EngineError> {
        if !SCHEMES.iter().any(|scheme| source.starts_with(scheme)) {
            let help = "a plugin source is `npm:<package>[@<range>]` or `path:<folder>`";
            return Err(refused(format!("`{source}` is not a plugin source"), help));
        }
        let fetched = self.plugins.fetch(source).map_err(|error| {
            refused(error.to_string(), "check the source; an `npm:` source needs `npm` on the path")
        })?;
        match plugin_manifest::manifest(&fetched.manifest) {
            Ok(manifest) => Ok(PluginOffer {
                source: source.to_owned(),
                manifest,
                resolved: fetched.resolved.clone(),
                integrity: fetched.integrity.clone(),
                fetched,
            }),
            Err(reason) => {
                self.plugins.discard(&fetched);
                let help = format!("a plugin's {MANIFEST_FILE} has id, version, kind, entry.command and permissions");
                Err(refused(format!("the {MANIFEST_FILE} of `{source}`: {reason}"), &help))
            }
        }
    }

    /// Drops a fetched plugin the user turned down.
    pub fn plugin_decline(&self, offer: &PluginOffer) {
        self.plugins.discard(&offer.fetched);
    }

    /// Installs an approved plugin and returns the changes to write: its `plugins:` item in the
    /// config (none when the config has it from the same source) and its entry in
    /// `vibranium.lock` with the permissions its manifest asks for. Approving it again updates
    /// the lock. On an error the fetched plugin is dropped.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] when the config has the plugin from another source, the lock cannot
    /// be read, or the guard refuses the change; [`EngineError::Sources`] when it cannot be
    /// installed.
    pub fn plugin_approve(&self, offer: &PluginOffer) -> Result<Changes, EngineError> {
        let approved = self.approved(offer);
        if approved.is_err() {
            self.plugins.discard(&offer.fetched);
        }
        approved
    }

    fn approved(&self, offer: &PluginOffer) -> Result<Changes, EngineError> {
        let id = &offer.manifest.id;
        let project = self.authoring()?;
        let mut changes = Changes::default();
        if let Some(change) = config_item(&config(&project)?, offer)? {
            changes.files.push(change);
            changes.titles.push(format!("Add plugin {id} to the config"));
        }
        let mut changes = self.guarded(&project, changes)?;
        changes.files.push(self.lock_change(|plugins| plugin_lock::with(plugins, locked(offer)))?);
        changes.titles.push(format!("Approve what {id} may reach in {LOCK_FILE}"));
        self.plugins.install(&offer.fetched, id).map_err(|error| store_error(&error))?;
        Ok(changes)
    }

    /// Each plugin in the config or `vibranium.lock`, in ID order, with whether a run can use it.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] when the config does not parse or the lock is not valid, and
    /// [`EngineError::Sources`] when an installed manifest cannot be read.
    pub fn plugin_list(&self) -> Result<Vec<PluginListing>, EngineError> {
        let project = self.authoring()?;
        let known = (configured(config(&project)?.document.root()), self.locked()?.1);
        let mut ids: Vec<&str> = known.0.iter().map(|(id, _)| id.as_str()).collect();
        ids.extend(known.1.iter().map(|plugin| plugin.id.as_str()));
        ids.sort_unstable();
        ids.dedup();
        ids.into_iter().map(|id| Ok(listing(id, self.installed(id)?, &known))).collect()
    }

    /// The installed manifest of `id`; none when it is not installed or does not parse.
    fn installed(&self, id: &str) -> Result<Option<PluginManifest>, EngineError> {
        let text = self.plugins.manifest(id).map_err(|error| store_error(&error))?;
        Ok(text.and_then(|text| plugin_manifest::manifest(&text).ok()))
    }

    /// Takes the plugin `id` out of the config (through the guard) and `vibranium.lock`, and
    /// uninstalls it; returns the changes to write.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] when neither has it (with the close ones), the lock is not valid, or
    /// the guard refuses the change (an auth still uses it); [`EngineError::Sources`] when its
    /// folder cannot be removed.
    pub fn plugin_remove(&self, id: &str) -> Result<Changes, EngineError> {
        let project = self.authoring()?;
        let opened = config(&project)?;
        let (_, locked) = self.locked()?;
        let in_config = item(opened.document.root(), "plugins", id);
        let in_lock = locked.iter().any(|plugin| plugin.id == id);
        if in_config.is_none() && !in_lock {
            let known = configured(opened.document.root()).into_iter().map(|(id, _)| id);
            return Err(unknown(id, &known.chain(locked.into_iter().map(|plugin| plugin.id)).collect::<Vec<_>>()));
        }
        let mut changes = Changes::default();
        if let Some(node) = in_config {
            changes.files.push(
                opened.change(&[opened.document.remove(removed(opened.document.root(), node)).map_err(internal)?])?,
            );
            changes.titles.push(format!("Remove plugin {id} from the config"));
        }
        let mut changes = self.guarded(&project, changes)?;
        if in_lock {
            changes.files.push(self.lock_change(|plugins| plugins.into_iter().filter(|old| old.id != id).collect())?);
            changes.titles.push(format!("Drop what {id} may reach from {LOCK_FILE}"));
        }
        self.plugins.uninstall(id).map_err(|error| store_error(&error))?;
        Ok(changes)
    }

    /// The lock's text, if there is one, and its plugins.
    fn locked(&self) -> Result<Lock, EngineError> {
        let text = self.plugins.lock().map_err(|error| store_error(&error))?;
        let plugins = text.as_deref().map(plugin_lock::read).transpose().map_err(|reason| {
            refused(format!("{LOCK_FILE}: {reason}"), "fix it, or remove it and add the plugins again")
        })?;
        Ok((text, plugins.unwrap_or_default()))
    }

    /// The change that writes the lock with `change` made to its plugins.
    fn lock_change(&self, change: impl FnOnce(Vec<Locked>) -> Vec<Locked>) -> Result<FileChange, EngineError> {
        let (before, plugins) = self.locked()?;
        let after = plugin_lock::render(&change(plugins));
        Ok(FileChange { path: LOCK_FILE.to_owned(), before, after, moved_from: None })
    }
}

/// The project's config, opened for editing.
fn config(project: &Authoring) -> Result<Opened<'_>, EngineError> {
    let config = project.entries().find(|(_, found)| found.kind() == Some(FileKind::Config));
    Authoring::open(config.ok_or_else(|| internal("a project without its config"))?)
}

/// The config's new `plugins:` item for `offer`; none when it has the plugin from the same source.
fn config_item(opened: &Opened<'_>, offer: &PluginOffer) -> Result<Option<FileChange>, EngineError> {
    let id = &offer.manifest.id;
    if let Some(existing) = item(opened.document.root(), "plugins", id) {
        let source = existing.get("source").and_then(Node::as_str).unwrap_or_default();
        if source == offer.source {
            return Ok(None);
        }
        let help = format!("`vc plugin remove {id}` first to take it from `{}` instead", offer.source);
        return Err(refused(format!("the config has the plugin `{id}` from `{source}` already"), &help));
    }
    let yaml = format!("id: {id}\nsource: {}", write_text(&offer.source, false));
    let comment = format!("# {id} · {} plugin · keys: {DOCS}#plugins-and-alerts", offer.manifest.kind.name());
    let (change, _) = commented_item(opened, &ListItem { list: "plugins", yaml: &yaml, comment: &comment })?;
    Ok(Some(change))
}

/// Each `plugins:` item of the config: its ID and source.
fn configured(root: &Node) -> Vec<(String, String)> {
    let items = root.get("plugins").map(Node::items).unwrap_or_default();
    let source = |item: &Node| item.get("source").and_then(Node::as_str).unwrap_or_default().to_owned();
    items.iter().filter_map(|item| Some((item.get("id")?.as_str()?.to_owned(), source(item)))).collect()
}

/// `vibranium.lock`'s text, if there is one, and its plugins.
type Lock = (Option<String>, Vec<Locked>);

/// The config's plugins (ID and source) and the lock's.
type Known = (Vec<(String, String)>, Vec<Locked>);

/// How `vc plugin list` shows the plugin `id`.
fn listing(id: &str, manifest: Option<PluginManifest>, (configured, locked): &Known) -> PluginListing {
    let source = configured.iter().find(|(found, _)| found == id).map(|(_, source)| source.clone());
    let lock = locked.iter().find(|plugin| plugin.id == id);
    PluginListing {
        id: id.to_owned(),
        state: state(source.is_some(), lock, manifest.as_ref()),
        source: source.or_else(|| lock.map(|plugin| plugin.source.clone())).unwrap_or_default(),
        granted: lock.map(|plugin| plugin.permissions.clone()).unwrap_or_default(),
        manifest,
    }
}

/// What removing the plugin `item` takes out of the config: the item, or the whole `plugins:`
/// key when it is the last.
fn removed<'n>(root: &'n Node, item: &'n Node) -> &'n Node {
    root.get("plugins").filter(|list| list.items().len() == 1).unwrap_or(item)
}

/// The lock entry for an approved offer.
fn locked(offer: &PluginOffer) -> Locked {
    Locked {
        id: offer.manifest.id.clone(),
        source: offer.source.clone(),
        resolved: offer.resolved.clone(),
        integrity: offer.integrity.clone(),
        permissions: offer.manifest.permissions.clone(),
    }
}

/// Whether a run can use a plugin with these parts.
fn state(configured: bool, lock: Option<&Locked>, manifest: Option<&PluginManifest>) -> PluginState {
    let Some(lock) = lock else { return PluginState::NotApproved };
    if !configured {
        return PluginState::NotInConfig;
    }
    let Some(manifest) = manifest else { return PluginState::NotInstalled };
    let beyond = manifest.permissions.beyond(&lock.permissions);
    if beyond.is_empty() { PluginState::Ready } else { PluginState::AsksForMore(beyond) }
}

/// The refusal for an ID no plugin has, with the close ones in `known`.
fn unknown(id: &str, known: &[String]) -> EngineError {
    let near = vc_planner::closest(id, known.iter().map(String::as_str));
    let help = did_you_mean(&near).unwrap_or_else(|| "`vc plugin list` lists them".to_owned());
    refused(format!("there is no plugin `{id}`"), &help)
}

fn store_error(error: &PluginStoreError) -> EngineError {
    EngineError::Sources { reason: error.to_string() }
}

#[cfg(test)]
#[path = "plugins_tests.rs"]
mod tests;
