use std::path::{Path, PathBuf};

use clap::Args;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use vc_engine::{Engine, EngineError, ReportFlags, RunReport, RunRequest};

use crate::Outcome;
use crate::check::use_color;
use crate::project::{self, Located};
use crate::select::PlanArgs;

#[derive(Args)]
#[expect(clippy::struct_excessive_bools, reason = "each bool is one independent command-line flag")]
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
    /// The report files to write, as a comma list or repeated: `ctrf`, `junit`, `html`. Over
    /// the config's `reports.formats`; `ctrf` when neither names any.
    #[arg(long = "report", value_name = "FORMATS")]
    reports: Vec<String>,
    /// The folder the report files go in, as they are, over the config's `reports.dir`; else
    /// each run's files go in `<reports.dir>/<run>/`.
    #[arg(long, value_name = "DIR")]
    out: Option<PathBuf>,
    /// Writes the HTML report as one self-contained file, over `reports.html.layout`.
    #[arg(long)]
    single_file: bool,
    /// Runs the tests that failed or were blocked in the last run, under its profiles.
    #[arg(long)]
    failed: bool,
    /// Runs `cache:` actions every time, neither reading nor writing the cross-run cache.
    #[arg(long)]
    no_cache: bool,
}

/// `vc run [selectors] [flags]`: exit 0 passed, 1 failed or blocked, 2 nothing ran, 130
/// cancelled (D127).
pub fn run(args: &RunArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let request = match args.request(&located)? {
        Ok(request) => request,
        Err(reason) => return Ok(Outcome::Refused(reason)),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let (stop, mut stops) = tokio::sync::mpsc::unbounded_channel();
    let report = runtime.block_on(async {
        let signals = tokio::spawn(ctrl_c(stop));
        let report = execute(&located.engine, &request, &mut stops).await;
        signals.abort();
        report
    });
    Ok(report.map_or_else(Outcome::Engine, |report| finished(&report)))
}

impl RunArgs {
    /// What these flags ask of the engine; `Ok(Err(reason))` when a selector cannot be read.
    pub fn request(&self, located: &Located) -> anyhow::Result<Result<RunRequest, String>> {
        let plan = match self.plan.request(located) {
            Ok(plan) => plan,
            Err(reason) => return Ok(Err(reason)),
        };
        let out = self.out.as_ref().map(|out| std::env::current_dir().map(|cwd| cwd.join(out))).transpose()?;
        Ok(Ok(RunRequest {
            plan,
            seed: self.seed,
            bail: self.bail,
            concurrency: self.concurrency,
            console: self.reporter.clone(),
            files: ReportFlags { formats: self.reports.clone(), out, single_file: self.single_file },
            color: use_color(&std::io::stdout()),
            failed: self.failed,
            no_cache: self.no_cache,
            agent: None,
            watch: None,
        }))
    }
}

/// Sends one value on `stop` for each Ctrl-C, until no one listens.
pub async fn ctrl_c(stop: UnboundedSender<()>) {
    while tokio::signal::ctrl_c().await.is_ok() {
        if stop.send(()).is_err() {
            return;
        }
    }
}

/// Runs `request`; each value on `stops` (a Ctrl-C) interrupts the run.
pub async fn execute(
    engine: &Engine,
    request: &RunRequest,
    stops: &mut UnboundedReceiver<()>,
) -> Result<RunReport, EngineError> {
    let (interrupter, interrupts) = vc_engine::interrupts();
    let run = engine.run(request, interrupts);
    tokio::pin!(run);
    loop {
        tokio::select! {
            report = &mut run => return report,
            Some(()) = stops.recv() => interrupter.interrupt(),
        }
    }
}

/// Prints the run's notes, and ends with the run's exit code.
pub fn finished(report: &RunReport) -> Outcome {
    notes(report);
    Outcome::Exit(report.exit)
}

/// Prints the run's notices, and the seed of a failed run.
pub fn notes(report: &RunReport) {
    for notice in &report.notices {
        eprintln!("warning: {notice}");
    }
    if report.exit == 1 {
        eprintln!("seed {}: pass `--seed {}` to run with the same generated data", report.seed, report.seed);
    }
}
