//! `vc call <apiId>`: sends one api ad hoc with the project's auth and the active profiles and
//! prints the response (D178).

use std::io::Write;
use std::path::Path;

use clap::Args;
use vc_engine::{CallReport, CallRequest};

use crate::Outcome;
use crate::explain::LayerArgs;
use crate::project;

#[derive(Args)]
pub struct CallArgs {
    /// The api to call (`users#list`).
    api: String,
    /// Sets one var for this call (`--var userId=7`); repeat it for more. It beats the vars
    /// the api and the config define.
    #[arg(long = "var", value_name = "NAME=VALUE")]
    vars: Vec<String>,
    /// Leaves what the call made: the api's cleanup does not run afterwards.
    #[arg(long)]
    keep: bool,
    #[command(flatten)]
    layers: LayerArgs,
}

/// `vc call <apiId> [--var] [--keep] [--profile] [--set]`: exit 0 the api's expect held, 1 it
/// did not, 2 unknown api or bad flag.
pub fn run(args: &CallArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let report = match runtime.block_on(located.engine.call(&args.request())) {
        Ok(report) => report,
        Err(error) => return Ok(Outcome::Engine(error)),
    };
    shown(&report)?;
    Ok(if report.exit == 0 { Outcome::Done } else { Outcome::Exit(report.exit) })
}

impl CallArgs {
    fn request(&self) -> CallRequest {
        let layers = self.layers.request();
        let (api, vars, keep) = (self.api.clone(), self.vars.clone(), self.keep);
        CallRequest { api, vars, keep, profiles: layers.profiles, sets: layers.sets }
    }
}

fn shown(report: &CallReport) -> anyhow::Result<()> {
    for notice in &report.notices {
        eprintln!("warning: {notice}");
    }
    write!(std::io::stdout().lock(), "{}", report.text)?;
    Ok(())
}
