use std::io::Write;
use std::path::Path;

use clap::{Args, ValueEnum};
use vc_engine::{ListKind, ListRequest};

use crate::Outcome;
use crate::project;
use crate::select::SelectArgs;

/// What `vc list` lists.
#[derive(Clone, Copy, ValueEnum)]
enum Kind {
    /// The selected tests, skipped ones included.
    Tests,
    /// Every api.
    Apis,
    /// Every definition file.
    Files,
}

#[derive(Args)]
pub struct ListArgs {
    /// What to list.
    kind: Kind,
    /// What to list; apis take every selector but `-t`.
    #[command(flatten)]
    select: SelectArgs,
    /// Lists only the skipped tests.
    #[arg(long)]
    skipped: bool,
    /// Prints the list as JSON on stdout, for scripts and agents.
    #[arg(long)]
    json: bool,
}

/// `vc list tests|apis|files [selectors] [--skipped] [--json]`.
pub fn run(args: &ListArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let select = match args.select.request(&located) {
        Ok(select) => select,
        Err(reason) => return Ok(Outcome::Refused(reason)),
    };
    let kind = match args.kind {
        Kind::Tests => ListKind::Tests,
        Kind::Apis => ListKind::Apis,
        Kind::Files => ListKind::Files,
    };
    let report = match located.engine.list(&ListRequest { kind, select, skipped: args.skipped }) {
        Ok(report) => report,
        Err(error) => return Ok(Outcome::Engine(error)),
    };
    for notice in &report.notices {
        eprintln!("warning: {notice}");
    }
    let mut out = std::io::stdout().lock();
    if args.json {
        writeln!(out, "{:#}", report.json())?;
    } else {
        write!(out, "{}", report.text())?;
    }
    Ok(Outcome::Done)
}
