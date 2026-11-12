use std::io::Write;
use std::path::Path;

use clap::Args;

use crate::Outcome;
use crate::project;
use crate::select::PlanArgs;
use crate::stdout;

#[derive(Args)]
pub struct PlanCommandArgs {
    #[command(flatten)]
    plan: PlanArgs,
    /// Prints the plan as JSON on stdout, for scripts and agents.
    #[arg(long, conflicts_with = "dot")]
    json: bool,
    /// Prints the plan as a Graphviz DOT graph.
    #[arg(long)]
    dot: bool,
}

/// `vc plan [selectors] [--json|--dot]`: what a run would do, without sending a request.
pub fn run(args: &PlanCommandArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let request = match args.plan.request(&located) {
        Ok(request) => request,
        Err(reason) => return Ok(Outcome::Refused(reason)),
    };
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let report = match runtime.block_on(located.engine.plan(&request)) {
        Ok(report) => report,
        Err(error) => return Ok(Outcome::Engine(error)),
    };
    for notice in report.notices() {
        eprintln!("warning: {notice}");
    }
    let mut out = stdout::out();
    if args.json {
        writeln!(out, "{:#}", report.json())?;
    } else if args.dot {
        write!(out, "{}", report.dot())?;
    } else {
        writeln!(out, "plan  {}", report.line())?;
        write!(out, "{}", report.tree())?;
    }
    Ok(Outcome::Done)
}
