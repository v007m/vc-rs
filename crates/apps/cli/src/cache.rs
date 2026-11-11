//! `vc cache list` and `vc cache clear [id]`: the cross-run cache of `cache:` actions.

use std::io::Write;
use std::path::Path;

use clap::{Args, Subcommand};
use vc_engine::Engine;

use crate::Outcome;
use crate::project;
use crate::stdout;

/// `vc cache …`.
#[derive(Args)]
pub struct CacheArgs {
    #[command(subcommand)]
    command: CacheCommand,
}

#[derive(Subcommand)]
enum CacheCommand {
    /// Lists the cached results: what each caches, its profiles and when it expires.
    List {
        /// Prints the entries as JSON on stdout, for scripts and agents.
        #[arg(long)]
        json: bool,
    },
    /// Removes the cached results of one action, or all of them, running their cleanups.
    Clear {
        /// The action whose results go, as `users#create`; every entry when left out.
        id: Option<String>,
    },
}

/// `vc cache list [--json]` and `vc cache clear [id]`: exit 0 done, 2 the cache cannot be
/// opened.
pub fn run(args: &CacheArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let engine = &located.engine;
    let text = match &args.command {
        CacheCommand::List { json } => match engine.cache_list() {
            Ok(report) if *json => format!("{:#}\n", report.json()),
            Ok(report) => report.text(),
            Err(error) => return Ok(Outcome::Engine(error)),
        },
        CacheCommand::Clear { id } => match clear(engine, id.as_deref())? {
            Ok(text) => text,
            Err(error) => return Ok(Outcome::Engine(error)),
        },
    };
    write!(stdout::out(), "{text}")?;
    Ok(Outcome::Done)
}

/// Clears on a Tokio runtime, since the cleanups make requests; prints a warning for each
/// cleanup that failed.
fn clear(engine: &Engine, id: Option<&str>) -> anyhow::Result<Result<String, vc_engine::EngineError>> {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let (count, notices) = match runtime.block_on(engine.cache_clear(id)) {
        Ok(cleared) => cleared,
        Err(error) => return Ok(Err(error)),
    };
    for notice in &notices {
        eprintln!("warning: {notice}");
    }
    Ok(Ok(format!("cleared {count} {}\n", if count == 1 { "entry" } else { "entries" })))
}
