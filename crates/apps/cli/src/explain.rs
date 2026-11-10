//! `vc explain <code|id>` and `vc config explain <key>`: a diagnostic code's page, a test or api
//! resolved for the active profile, and where a config value comes from (D129, D180).

use std::io::Write;
use std::path::Path;

use clap::{Args, Subcommand};
use vc_engine::{Code, Engine, EngineError, ExplainReport, ExplainRequest};

use crate::Outcome;
use crate::project;

/// The profiles and settings `explain`, `config explain` and `call` resolve with.
#[derive(Args)]
pub struct LayerArgs {
    /// The active profiles, as a comma list or repeated; later ones win.
    #[arg(long = "profile", value_name = "IDS")]
    profiles: Vec<String>,
    /// Sets one config value (`defaults.timeout=5s`, `vars.userId=7`), over every other layer
    /// (D129).
    #[arg(long = "set", value_name = "KEY=VALUE")]
    sets: Vec<String>,
}

#[derive(Args)]
pub struct ExplainArgs {
    /// A diagnostic code (`V2003`), or a test or api ID (`users.crud#createsUser`, `users#create`).
    target: String,
    #[command(flatten)]
    layers: LayerArgs,
}

/// `vc config …`.
#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Shows a config value and the layer that set it, over the values it replaced.
    Explain {
        /// The setting (`defaults.timeout`, `reports.dir`) or var (`vars.baseUrl`).
        key: String,
        #[command(flatten)]
        layers: LayerArgs,
    },
}

/// `vc explain <code|id>`: a code when it is `V` and four digits, which works outside any
/// project; a test or api otherwise. Exit 0 shown, 2 unknown.
pub fn run(args: &ExplainArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    if is_code(&args.target) {
        return code(&args.target);
    }
    explained(project, |engine| engine.explain(&args.target, &args.layers.request()))
}

/// `vc config explain <key> [--profile] [--set]`: exit 0 shown, 2 unknown key.
pub fn config(command: &ConfigCommand, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let ConfigCommand::Explain { key, layers } = command;
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    explained(project, |engine| runtime.block_on(engine.config_explain(key, &layers.request())))
}

impl LayerArgs {
    /// The profiles and settings given, as the engine takes them.
    pub fn request(&self) -> ExplainRequest {
        ExplainRequest { profiles: self.profiles.clone(), sets: self.sets.clone() }
    }
}

/// `V` followed by four ASCII digits (D180).
fn is_code(target: &str) -> bool {
    target.strip_prefix('V').is_some_and(|digits| digits.len() == 4 && digits.bytes().all(|b| b.is_ascii_digit()))
}

fn code(target: &str) -> anyhow::Result<Outcome> {
    match target.parse::<Code>() {
        Ok(code) => {
            write!(std::io::stdout().lock(), "{}", code.explain())?;
            Ok(Outcome::Done)
        }
        Err(error) => Ok(Outcome::Refused(error.to_string())),
    }
}

/// Locates the project, asks the engine, and prints its report.
fn explained(
    project: Option<&Path>,
    explain: impl FnOnce(&Engine) -> Result<ExplainReport, EngineError>,
) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let report = match explain(&located.engine) {
        Ok(report) => report,
        Err(error) => return Ok(Outcome::Engine(error)),
    };
    for notice in &report.notices {
        eprintln!("warning: {notice}");
    }
    write!(std::io::stdout().lock(), "{}", report.text)?;
    Ok(Outcome::Done)
}
