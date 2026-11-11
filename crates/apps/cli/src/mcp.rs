//! `vc mcp`: the MCP server for AI agents, over stdio (D197). Stdout carries the protocol, so
//! nothing else is printed there.

use std::path::Path;

use clap::Args;
use vc_mcp::VcServer;

use crate::explain::LayerArgs;
use crate::{Outcome, project};

/// `vc mcp`.
#[derive(Args)]
pub struct McpArgs {
    #[command(flatten)]
    layers: LayerArgs,
}

/// `vc mcp [--profile] [--set]`: serves until the agent closes stdin; exit 0, or 2 when the
/// project cannot be found or a profile or `--set` is bad.
pub fn run(args: &McpArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let layers = args.layers.request();
    if let Err(error) = located.engine.check_layers(&layers) {
        return Ok(Outcome::Engine(error));
    }
    let server = VcServer::new(located.engine, layers.profiles, layers.sets);
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(server.serve_stdio())?;
    Ok(Outcome::Done)
}
