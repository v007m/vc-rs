//! Plugins added and removed from the web UI's Settings (D714, D715). Each is a preview first:
//! the source fetched into staging, the config and `vibranium.lock` changes built through the
//! guard, the staging dropped, and a digest of the changes given back. A write fetches again,
//! builds the changes again and goes ahead only when they hash to that digest, then installs
//! (or uninstalls) and writes each file only while it is the text the change was made from.
//!
//! A client's source is held to less than `vc plugin add` takes: `npm:` with a registry
//! package name and range only (no `file:`, git or tarball spec, which `npm pack` would run
//! scripts for), and `path:` with a folder inside the project (V5132). A refusal never echoes
//! the source or ID a client sent.

use vc_interfaces::{MANIFEST_FILE, PluginManifest};

use crate::authoring::refused;
use crate::edit::{Changes, changes_digest, stale_unless};
use crate::plugins::{MANIFEST_HELP, PluginOffer, Unfetched};
use crate::{Engine, EngineError};

/// What a client's source may be.
const SOURCE_HELP: &str = "a plugin source is `npm:<package>[@<range>]`, a package of the npm registry, or `path:<folder>`, a folder inside the project";

/// Why a write found the changes differ from the preview's.
const STALE: &str = "the plugin change: nothing was written: the files or the plugin changed since the preview";

/// The longest npm package name.
const NAME_MAX: usize = 214;

/// A plugin added or removed, previewed or written.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PluginChange {
    /// The fetched plugin's manifest, with what it asks to reach; `None` for a removal.
    pub manifest: Option<PluginManifest>,
    /// What the source resolved to; empty for a removal.
    pub resolved: String,
    /// A hash of what was fetched; empty for a removal.
    pub integrity: String,
    /// What a run will trip over unless it is fixed.
    pub warnings: Vec<String>,
    /// The config and lock changes.
    pub changes: Changes,
    /// The digest of the changes, which the write gives back.
    pub digest: String,
}

impl Engine {
    /// The plugin at a client's `source` added: previewed, or installed and written when `write`
    /// gives the digest the preview gave. The fetched plugin is dropped unless it was installed.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] for a source a client may not name, one that cannot be fetched, a
    /// wrong manifest or a change the guard refuses; [`EngineError::Reporter`] (V5132) for a
    /// `path:` folder outside the project; [`EngineError::Stale`] when the changes differ from
    /// the preview's.
    pub fn client_plugin_add(&self, source: &str, write: Option<&str>) -> Result<PluginChange, EngineError> {
        self.client_source(source)?;
        let offer = self.offer(source).map_err(|unfetched| unfetched_error(unfetched, source))?;
        let added = self.client_added(&offer, write);
        self.plugin_decline(&offer);
        added.map_err(|error| scrubbed(error, (source, "that source")))
    }

    /// The plugin a client names by `id` removed: previewed, or written and uninstalled when
    /// `write` gives the digest the preview gave.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] for an ID that is not a plugin's or a change the guard refuses;
    /// [`EngineError::Stale`] when the changes differ from the preview's.
    pub fn client_plugin_remove(&self, id: &str, write: Option<&str>) -> Result<PluginChange, EngineError> {
        if !vc_compiler::is_camel_id(id) {
            return Err(refused(
                "that is not a plugin ID",
                "a plugin ID is a camelCase word, as the plugin list names it",
            ));
        }
        let removed = self.removal(id).and_then(|changes| {
            let digest = changes_digest(&changes);
            if let Some(expected) = write {
                stale_unless(expected, &digest, STALE)?;
                self.write_each(&changes, true)?;
                self.uninstall(id)?;
            }
            Ok(PluginChange {
                manifest: None,
                resolved: String::new(),
                integrity: String::new(),
                warnings: Vec::new(),
                changes,
                digest,
            })
        });
        removed.map_err(|error| scrubbed(error, (id, "by that ID")))
    }

    /// `offer`'s changes, and once `write` gives their digest, the plugin installed and written.
    fn client_added(&self, offer: &PluginOffer, write: Option<&str>) -> Result<PluginChange, EngineError> {
        let changes = self.approval(offer)?;
        let digest = changes_digest(&changes);
        if let Some(expected) = write {
            stale_unless(expected, &digest, STALE)?;
            self.install(offer)?;
            self.write_each(&changes, true)?;
        }
        Ok(PluginChange {
            manifest: Some(offer.manifest.clone()),
            resolved: offer.resolved.clone(),
            integrity: offer.integrity.clone(),
            warnings: offer.warnings.clone(),
            changes,
            digest,
        })
    }

    /// Refuses a source a client may not name.
    fn client_source(&self, source: &str) -> Result<(), EngineError> {
        if let Some(spec) = source.strip_prefix("npm:") {
            return if registry_spec(spec) { Ok(()) } else { Err(refused("that is not an npm package", SOURCE_HELP)) };
        }
        if let Some(folder) = source.strip_prefix("path:") {
            return self.folder_inside(folder, "a `path:` source").map(|_| ());
        }
        Err(refused("that is not a plugin source", SOURCE_HELP))
    }
}

/// Whether `spec` is a registry package: `[@scope/]name[@range]`, lowercase names, and a range
/// or tag with no `:`, `/`, `#` or `@` that would make it a file, git or tarball spec.
fn registry_spec(spec: &str) -> bool {
    let Some((scope, rest)) = scoped(spec) else { return false };
    let (name, range) = rest.split_once('@').map_or((rest, None), |(name, range)| (name, Some(range)));
    scope.is_none_or(package_word) && package_word(name) && range.is_none_or(range_text)
}

/// `spec`'s scope, when it has one, and the rest; `None` for a scope with no name after it.
fn scoped(spec: &str) -> Option<(Option<&str>, &str)> {
    spec.strip_prefix('@')
        .map_or(Some((None, spec)), |scoped| scoped.split_once('/').map(|(scope, rest)| (Some(scope), rest)))
}

/// Whether `word` is an npm name or scope: lowercase letters, digits, `-`, `.` and `_`, not
/// starting with `.`, `_` or `-`.
fn package_word(word: &str) -> bool {
    let plain = |part: char| part.is_ascii_lowercase() || part.is_ascii_digit();
    word.len() <= NAME_MAX
        && word.chars().next().is_some_and(plain)
        && word.chars().all(|part| plain(part) || matches!(part, '-' | '.' | '_'))
}

/// Whether `range` is a semver range or a dist tag.
fn range_text(range: &str) -> bool {
    !range.is_empty()
        && range.len() <= NAME_MAX
        && range.chars().all(|part| part.is_ascii_alphanumeric() || " .^~<>=*|+-".contains(part))
}

/// Why `source` gave no offer, without echoing it: npm's error code is kept, if it gave one.
fn unfetched_error(unfetched: Unfetched, source: &str) -> EngineError {
    match unfetched {
        Unfetched::Store(error) => {
            let text = error.to_string();
            let code = text.split(|part: char| !part.is_ascii_alphanumeric()).find(|word| npm_code(word));
            let reason = code.map_or_else(
                || "the plugin could not be fetched".to_owned(),
                |code| format!("the plugin could not be fetched: npm said {code}"),
            );
            let help = if source.starts_with("npm:") {
                "check the package name and range, and that `npm` is on the path and can reach its registry"
            } else {
                "check that the folder holds vibranium-plugin.yaml"
            };
            refused(reason, help)
        }
        Unfetched::Manifest(reason) => refused(format!("its {MANIFEST_FILE}: {reason}"), MANIFEST_HELP),
    }
}

/// Whether `word` is an npm error code, such as `E404` or `ENOTFOUND`.
fn npm_code(word: &str) -> bool {
    word.len() > 2
        && word.starts_with('E')
        && word.chars().all(|part| part.is_ascii_uppercase() || part.is_ascii_digit())
}

/// `error` with each quoted mention of `sent` replaced by `words`, so a refusal does not echo it.
fn scrubbed(error: EngineError, (sent, words): (&str, &str)) -> EngineError {
    let quoted = format!("`{sent}`");
    let scrub = |text: String| text.replace(&quoted, words);
    match error {
        EngineError::Edit { reason, help } => EngineError::Edit { reason: scrub(reason), help: help.map(scrub) },
        other => other,
    }
}

#[cfg(test)]
#[path = "plugin_client_tests.rs"]
mod tests;
