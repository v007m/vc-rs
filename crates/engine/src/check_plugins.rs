//! `vc check`'s plugin problems, which only the engine sees, as the compiler reads neither
//! `vibranium.lock` nor the installed manifests: a `plugins` item a run would refuse (V5120),
//! and a `reports.formats` name that is an installed plugin of another kind (V2302).

use vc_diagnostics::{Code, Diagnostic};
use vc_interfaces::PluginKind;
use vc_model::{Program, SourceFiles};
use vc_parser::{Node, Syntax};

use crate::authoring::item;
use crate::plugin_hosts::{a_kind, literal};
use crate::plugin_lock::Locked;
use crate::plugins::{self, PluginState};
use crate::{Engine, check_alerts};

/// What `vc plugin list` and `vc plugin add` do about a plugin a run refuses.
const LIST_HELP: &str =
    "`vc plugin list` shows what each plugin needs; `vc plugin add <source>` installs and approves one";

impl Engine {
    /// The V5120 warning for each `plugins` item a run cannot use, by the tests the run makes
    /// before it starts one, and V2302 for each `reports.formats` name that is an installed
    /// plugin other than a reporter.
    pub(crate) fn plugin_problems(&self, program: &Program, files: &SourceFiles) -> Vec<Diagnostic> {
        let Some(root) = config_root(files).filter(|_| !program.config.plugins.is_empty()) else {
            return Vec::new();
        };
        let locked = self.locked().map(|(_, locked)| locked).map_err(|error| error.to_string());
        let mut found = Vec::new();
        let mut ids = Vec::new();
        for plugin in &program.config.plugins {
            let Some((id, node)) = program.str(plugin.id).and_then(|id| Some((id, item(&root, "plugins", id)?))) else {
                continue;
            };
            ids.push(id);
            let problem = self.refused(id, &locked).map_or_else(
                || literal(plugin.config.as_ref()).err().map(|reason| templated((node, id), &reason)),
                |reason| Some(unusable((node, id), &reason, LIST_HELP)),
            );
            found.extend(problem);
        }
        found.extend(self.misused_formats(&root, &ids));
        found
    }

    /// Why a run would not start the plugin `id`: the lock cannot be read, or the plugin is not
    /// approved, not installed or asks for more than the lock grants.
    fn refused(&self, id: &str, locked: &Result<Vec<Locked>, String>) -> Option<String> {
        let locked = match locked {
            Ok(locked) => locked,
            Err(error) => return Some(error.clone()),
        };
        let manifest = match self.installed(id) {
            Ok(manifest) => manifest,
            Err(error) => return Some(error.to_string()),
        };
        let state = plugins::state(true, locked.iter().find(|entry| entry.id == id), manifest.as_ref());
        (state != PluginState::Ready).then(|| state.to_string())
    }

    /// V2302 for each `reports.formats` name among the plugin `ids` whose installed manifest is
    /// not a reporter's.
    fn misused_formats(&self, root: &Node, ids: &[&str]) -> Vec<Diagnostic> {
        let formats = root.get("reports").and_then(|reports| reports.get("formats")).map(Node::items);
        let named = formats.unwrap_or_default().iter().filter_map(|node| Some((node, node.as_str()?)));
        let plugins = named.filter(|(_, name)| ids.contains(name));
        let kinds = plugins.filter_map(|(node, name)| Some((node, name, self.installed(name).ok()??.kind)));
        kinds
            .filter(|(_, _, kind)| *kind != PluginKind::Reporter)
            .map(|(node, name, kind)| misused(node, name, kind))
            .collect()
    }
}

/// The config's root, when it parses.
fn config_root(files: &SourceFiles) -> Option<Node> {
    let (id, file) = check_alerts::config(files)?;
    vc_parser::parse(id, Syntax::of(&file.path), &file.text).0
}

/// V5120 at the plugin's `id`.
fn unusable((node, id): (&Node, &str), reason: &str, help: &str) -> Diagnostic {
    let span = node.get("id").map_or(node.span, |at| at.span);
    refusal(Diagnostic::new(Code::V5120, span, format!("a run cannot use plugin `{id}`: {reason}")), help)
        .label("a run leaves it out")
}

/// V5120 at a `config` that holds a `${…}` value.
fn templated((node, id): (&Node, &str), reason: &str) -> Diagnostic {
    let span = node.get("config").or_else(|| node.get("id")).map_or(node.span, |at| at.span);
    let help = "write literal values in `config`; the plugin reads a secret it was granted with `secrets/get`";
    refusal(Diagnostic::new(Code::V5120, span, format!("a run cannot use plugin `{id}`: {reason}")), help)
        .label("not literal")
}

/// `diagnostic` with what a run does without the plugin, and `help`.
fn refusal(diagnostic: Diagnostic, help: &str) -> Diagnostic {
    diagnostic.note("the run goes on; the work that needs the plugin fails, and its report is not written").help(help)
}

/// V2302 at a `reports.formats` name that is a plugin of another kind.
fn misused(node: &Node, name: &str, kind: PluginKind) -> Diagnostic {
    let message = format!("`{name}` is {} plugin, not a reporter, so it writes no report", a_kind(kind));
    Diagnostic::new(Code::V2302, node.span, message)
        .label("not a reporter")
        .help(format!("`reports.formats` takes ctrf, junit, html and reporter plugins' IDs: remove `{name}`"))
}

#[cfg(test)]
#[path = "check_plugins_tests.rs"]
mod tests;
