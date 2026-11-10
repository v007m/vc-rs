//! `vc tui`: the terminal UI, which needs a terminal on stdin and stdout.

use std::io::IsTerminal;
use std::path::Path;

use clap::Args;
use vc_engine::PluginStderr;
use vc_tui::TuiOptions;

use crate::{Outcome, project};

/// `vc tui`.
#[derive(Args)]
pub struct TuiArgs {
    /// The active profiles, as a comma list or repeated; later ones win.
    #[arg(long = "profile", value_name = "IDS")]
    profiles: Vec<String>,
}

/// `vc tui [--profile]`: runs until `q`; exit 0, or 2 without a terminal or a project. Plugins'
/// stderr goes to `.vibranium/plugin-logs/<id>/stderr.log`, so it cannot garble the screen.
pub fn run(args: &TuiArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        let help = "run it in a terminal; `vc run` reports to a pipe or a CI log".to_owned();
        return Ok(Outcome::Helped("vc tui needs a terminal".to_owned(), Some(help)));
    }
    let located = match project::locate_with(project, |builder| builder.plugin_stderr(PluginStderr::Log))? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let root = located.engine.root();
    let name = root.file_name().map_or_else(|| root.display().to_string(), |name| name.to_string_lossy().into());
    let mut options = TuiOptions::new(name, args.profiles.clone());
    options.base.clone_from(&located.base);
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(vc_tui::run(located.engine, options))?;
    Ok(Outcome::Done)
}
