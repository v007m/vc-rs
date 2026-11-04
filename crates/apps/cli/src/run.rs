use std::path::{Path, PathBuf};

use clap::Args;
use vc_engine::{Engine, EngineError, RunReport, RunRequest};

use crate::Outcome;
use crate::check::use_color;
use crate::project;
use crate::select::PlanArgs;

#[derive(Args)]
pub struct RunArgs {
    #[command(flatten)]
    plan: PlanArgs,
    /// The seed for generated data; a failing run prints its seed to pass here.
    #[arg(long, value_name = "N")]
    seed: Option<u64>,
    /// Starts no more tests after the first failure.
    #[arg(long)]
    bail: bool,
    /// Test instances at once, over `defaults.concurrency`.
    #[arg(long, value_name = "N")]
    concurrency: Option<u32>,
    /// The console format: `spec` (the default) or `json`.
    #[arg(long, value_name = "FORMAT")]
    reporter: Option<String>,
    /// The report files to write, as a comma list or repeated: `ctrf`, `junit`. Over the
    /// config's `reports.formats`; `ctrf` when neither names any.
    #[arg(long = "report", value_name = "FORMATS")]
    reports: Vec<String>,
    /// The folder the report files go in, over the config's `reports.dir`.
    #[arg(long, value_name = "DIR")]
    out: Option<PathBuf>,
}

/// `vc run [selectors] [flags]`: exit 0 passed, 1 failed or blocked, 2 nothing ran, 130
/// cancelled (D127).
pub fn run(args: &RunArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let plan = match args.plan.request(&located) {
        Ok(plan) => plan,
        Err(reason) => return Ok(Outcome::Refused(reason)),
    };
    let request = RunRequest {
        plan,
        seed: args.seed,
        bail: args.bail,
        concurrency: args.concurrency,
        console: args.reporter.clone(),
        reports: args.reports.clone(),
        out: args.out.as_ref().map(|out| std::env::current_dir().map(|cwd| cwd.join(out))).transpose()?,
        color: use_color(&std::io::stdout()),
    };
    let report = match execute(&located.engine, &request)? {
        Ok(report) => report,
        Err(error) => return Ok(Outcome::Engine(error)),
    };
    for notice in &report.notices {
        eprintln!("warning: {notice}");
    }
    if report.exit == 1 {
        eprintln!("seed {}: pass `--seed {}` to run with the same generated data", report.seed, report.seed);
    }
    Ok(Outcome::Exit(report.exit))
}

/// Runs `request` on a Tokio runtime; each Ctrl-C interrupts the run.
fn execute(engine: &Engine, request: &RunRequest) -> anyhow::Result<Result<RunReport, EngineError>> {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    Ok(runtime.block_on(async {
        let (interrupter, interrupts) = vc_engine::interrupts();
        let signals = tokio::spawn(async move {
            while tokio::signal::ctrl_c().await.is_ok() {
                interrupter.interrupt();
            }
        });
        let report = engine.run(request, interrupts).await;
        signals.abort();
        report
    }))
}
