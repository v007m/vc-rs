//! `vc alerts test`: sends a sample notice through the alert channels (D257).

use std::io::Write;
use std::path::Path;

use clap::{Args, Subcommand};
use vc_engine::AlertSent;

use crate::Outcome;
use crate::explain::LayerArgs;
use crate::project;
use crate::stdout;

/// `vc alerts …`.
#[derive(Args)]
pub struct AlertsArgs {
    #[command(subcommand)]
    command: AlertsCommand,
}

#[derive(Subcommand)]
enum AlertsCommand {
    /// Sends one sample notice, titled `[test]`, through every channel of every alert rule
    /// whose profiles match. Nothing runs and nothing is recorded.
    Test {
        #[command(flatten)]
        layers: LayerArgs,
    },
}

/// `vc alerts test [--profile] [--set]`: exit 0 every channel took it, 1 one did not, 2 no
/// alert rule matches the profiles.
pub fn run(args: &AlertsArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let AlertsCommand::Test { layers } = &args.command;
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let sent = match runtime.block_on(located.engine.test_alerts(&layers.request())) {
        Ok(sent) => sent,
        Err(error) => return Ok(Outcome::Engine(error)),
    };
    if sent.is_empty() {
        let help = "add an `alerts` rule, or name the profiles one lists with `--profile`";
        return Ok(Outcome::Helped("no alert rule matches these profiles".to_owned(), Some(help.to_owned())));
    }
    let mut out = stdout::out();
    for line in sent.iter().map(line) {
        writeln!(out, "{line}")?;
    }
    let failed = sent.iter().any(|sent| sent.error.is_some());
    Ok(if failed { Outcome::Exit(1) } else { Outcome::Done })
}

/// `✓ slack (rule 1)`, or `✗ webhook (rule 2): <why>`.
fn line(sent: &AlertSent) -> String {
    let (channel, rule) = (&sent.channel, sent.rule);
    sent.error
        .as_ref()
        .map_or_else(|| format!("✓ {channel} (rule {rule})"), |error| format!("✗ {channel} (rule {rule}): {error}"))
}
