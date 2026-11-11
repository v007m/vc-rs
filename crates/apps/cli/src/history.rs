use std::io::Write;
use std::path::Path;

use clap::{Args, Subcommand};

use crate::Outcome;
use crate::project;
use crate::stdout;

#[derive(Args)]
pub struct HistoryArgs {
    #[command(subcommand)]
    command: Option<HistoryCommand>,
    /// How many runs to list, newest first.
    #[arg(long, value_name = "N", default_value_t = 20)]
    limit: usize,
    /// Prints the runs as JSON on stdout, for scripts and agents.
    #[arg(long)]
    json: bool,
}

#[derive(Subcommand)]
enum HistoryCommand {
    /// Shows one stored run and the tests in it that did not pass.
    Show {
        /// `last` or a run number from `vc history`.
        #[arg(default_value = "last")]
        run: String,
        /// Prints the run and every test as JSON on stdout.
        #[arg(long)]
        json: bool,
    },
    /// Drops the oldest stored runs and their report snapshots.
    Prune {
        /// The runs to keep, newest first; the config's `history.keep` when not given.
        #[arg(long, value_name = "N")]
        keep: Option<u32>,
    },
}

/// `vc history [--limit N] [--json]`, `vc history show [run]`, `vc history prune [--keep N]`.
pub fn run(args: &HistoryArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let engine = &located.engine;
    let (text, json) = match &args.command {
        None => match engine.history(args.limit) {
            Ok(report) => (report.text(), args.json.then(|| report.json())),
            Err(error) => return Ok(Outcome::Engine(error)),
        },
        Some(HistoryCommand::Show { run, json }) => match engine.stored(run) {
            Ok(stored) => (stored.text(), json.then(|| stored.json())),
            Err(error) => return Ok(Outcome::Engine(error)),
        },
        Some(HistoryCommand::Prune { keep }) => match engine.prune_history(*keep) {
            Ok(count) => (format!("pruned {count} {}\n", if count == 1 { "run" } else { "runs" }), None),
            Err(error) => return Ok(Outcome::Engine(error)),
        },
    };
    let mut out = stdout::out();
    match json {
        Some(json) => writeln!(out, "{json:#}")?,
        None => write!(out, "{text}")?,
    }
    Ok(Outcome::Done)
}
