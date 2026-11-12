//! `vc mcp`: the MCP server for AI agents, over stdio (D197). Stdout carries the protocol, so
//! nothing else is printed there.

use std::path::Path;

use clap::Args;
use vc_mcp::{McpError, VcServer};

use crate::explain::LayerArgs;
use crate::{Outcome, project};

/// `vc mcp`.
#[derive(Args)]
pub struct McpArgs {
    #[command(flatten)]
    layers: LayerArgs,
}

/// What a client that broke the handshake should send instead.
const HANDSHAKE: &str = "send `initialize` first, and `notifications/initialized` once it is answered";

/// `vc mcp [--profile] [--set]`: serves until the agent closes stdin; exit 0, or 2 when the
/// project cannot be found, a profile or `--set` is bad, or the client breaks the handshake.
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
    match runtime.block_on(server.serve_stdio()) {
        Ok(()) => Ok(Outcome::Done),
        Err(error @ McpError::Handshake { .. }) => Ok(Outcome::Helped(error.to_string(), Some(HANDSHAKE.to_owned()))),
        Err(error) => Err(error.into()),
    }
}
