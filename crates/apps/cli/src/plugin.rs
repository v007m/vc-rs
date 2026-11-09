//! `vc plugin add <source>`, `vc plugin list` and `vc plugin remove <id>`: plugins and what
//! `vibranium.lock` lets them reach.

use std::io::IsTerminal;
use std::path::Path;

use clap::{Args, Subcommand};
use vc_engine::{Engine, PluginListing, PluginOffer, PluginPermissions};

use crate::{Outcome, changes, project};

/// `vc plugin …`.
#[derive(Args)]
pub struct PluginArgs {
    #[command(subcommand)]
    command: PluginCommand,
}

#[derive(Subcommand)]
enum PluginCommand {
    /// Fetches a plugin, shows what it asks to reach, and on approval installs it and records
    /// that in `vibranium.lock`.
    Add {
        /// Where it comes from: `npm:<package>[@<range>]` or `path:<folder>`.
        source: String,
        /// Approves what it asks to reach without asking; needed when stdin is not a terminal.
        #[arg(long)]
        yes: bool,
    },
    /// Lists the plugins, what each may reach, and whether a run can use it.
    List,
    /// Removes a plugin from the config and `vibranium.lock`, and uninstalls it.
    Remove {
        /// The plugin's ID, as `vc plugin list` shows it.
        id: String,
    },
}

/// `vc plugin add|list|remove`: exit 0 done, 2 refused, declined or not approved.
pub fn run(args: &PluginArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let engine = &located.engine;
    match &args.command {
        PluginCommand::Add { source, yes } => add(engine, source, *yes),
        PluginCommand::List => Ok(list(engine)),
        PluginCommand::Remove { id } => {
            Ok(changes::write(engine, engine.plugin_remove(id), &format!("✓ removed plugin {id}")))
        }
    }
}

/// Fetches, shows the offer on stderr, and installs it once approved.
fn add(engine: &Engine, source: &str, yes: bool) -> anyhow::Result<Outcome> {
    let offer = match engine.plugin_fetch(source) {
        Ok(offer) => offer,
        Err(error) => return Ok(Outcome::Engine(error)),
    };
    eprint!("{}", offered(&offer));
    let approved = if yes {
        true
    } else if std::io::stdin().is_terminal() {
        changes::confirm("Approve what it asks to reach?")?
    } else {
        engine.plugin_decline(&offer);
        let help = Some("pass --yes to approve what it asks to reach".to_owned());
        return Ok(Outcome::Helped("cannot ask for approval: stdin is not a terminal".to_owned(), help));
    };
    if !approved {
        engine.plugin_decline(&offer);
        return Ok(Outcome::Refused(format!("plugin {} not added", offer.manifest.id)));
    }
    let id = &offer.manifest.id;
    Ok(changes::write(engine, engine.plugin_approve(&offer), &format!("✓ added plugin {id} from {source}")))
}

/// `report 1.0.0 · reporter plugin from path:plugin`, then what it asks to reach.
fn offered(offer: &PluginOffer) -> String {
    let manifest = &offer.manifest;
    let args = manifest.args.iter().map(String::as_str);
    let runs: Vec<&str> = std::iter::once(manifest.command.as_str()).chain(args).collect();
    format!(
        "{} {} · {} plugin from {} ({})\n  runs: {}\n  asks to reach: {}\n",
        manifest.id,
        manifest.version,
        manifest.kind.name(),
        offer.source,
        offer.resolved,
        runs.join(" "),
        reach(&manifest.permissions)
    )
}

/// `vc plugin list`: one line for each plugin.
fn list(engine: &Engine) -> Outcome {
    match engine.plugin_list() {
        Ok(listed) if listed.is_empty() => {
            println!("no plugins; `vc plugin add <source>` adds one");
            Outcome::Done
        }
        Ok(listed) => {
            for plugin in &listed {
                println!("{}", row(plugin));
            }
            Outcome::Done
        }
        Err(error) => Outcome::Engine(error),
    }
}

/// `report  reporter 1.0.0  path:plugin  ready  may reach: secret reportToken`.
fn row(plugin: &PluginListing) -> String {
    let kind = plugin
        .manifest
        .as_ref()
        .map_or_else(|| "-".to_owned(), |manifest| format!("{} {}", manifest.kind.name(), manifest.version));
    let granted = reach(&plugin.granted);
    format!("{}  {kind}  {}  {}  may reach: {granted}", plugin.id, plugin.source, plugin.state)
}

/// `network api.acme.io, secret reportToken`, or `nothing`.
fn reach(permissions: &PluginPermissions) -> String {
    let none = PluginPermissions::default();
    let all = permissions.beyond(&none);
    if all.is_empty() { "nothing".to_owned() } else { all.join(", ") }
}
