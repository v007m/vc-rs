//! `vc load <scenarioId>`: runs a load scenario with live metrics.

use std::path::Path;

use clap::Args;
use vc_engine::LoadRequest;

use crate::explain::LayerArgs;
use crate::{Outcome, project};

/// `vc load …`.
#[derive(Args)]
pub struct LoadArgs {
    /// The scenario to run (`checkout.peak#browses`).
    scenario: String,
    #[command(flatten)]
    layers: LayerArgs,
}

/// `vc load <scenarioId> [--profile] [--set]`: exit 0 within thresholds, 3 a threshold
/// breached, 2 unknown scenario or bad flag (D127).
pub fn run(args: &LoadArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let layers = args.layers.request();
    let request = LoadRequest { scenario: args.scenario.clone(), profiles: layers.profiles, sets: layers.sets };
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    Ok(match runtime.block_on(located.engine.load(&request)) {
        Ok(report) => Outcome::Exit(report.exit),
        Err(error) => Outcome::Engine(error),
    })
}
