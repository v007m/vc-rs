use std::path::{Path, PathBuf};

use clap::Args;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
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
    /// Same as `--reporter json`.
    #[arg(long, conflicts_with = "reporter")]
    json: bool,
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
    /// Fails the run on every failure, including those under `allowFailure`.
    #[arg(long)]
    strict: bool,
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
    let (stop, mut stops) = stops();
    let report = runtime.block_on(async {
        let signals = tokio::spawn(ctrl_c(stop));
        let report = execute(&located.engine, &request, &mut stops).await;
        signals.abort();
        report
    });
    close(runtime, stops.interrupted());
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
            console: if self.json { Some("json".to_owned()) } else { self.reporter.clone() },
            files: ReportFlags { formats: self.reports.clone(), out, single_file: self.single_file },
            color: use_color(&std::io::stdout()),
            failed: self.failed,
            no_cache: self.no_cache,
            strict: self.strict,
            agent: None,
            watch: None,
        }))
    }
}

/// The Ctrl-Cs a command hears, and whether it has heard one.
pub struct Stops {
    heard: UnboundedReceiver<()>,
    interrupted: bool,
}

/// A sender for [`ctrl_c`] and the [`Stops`] it feeds.
pub fn stops() -> (UnboundedSender<()>, Stops) {
    let (stop, heard) = unbounded_channel();
    (stop, Stops { heard, interrupted: false })
}

impl Stops {
    /// The next Ctrl-C, or `None` once no one sends any.
    pub async fn next(&mut self) -> Option<()> {
        let stop = self.heard.recv().await;
        self.interrupted |= stop.is_some();
        stop
    }

    /// Whether a Ctrl-C has been heard.
    pub const fn interrupted(&self) -> bool {
        self.interrupted
    }
}

/// Drops `runtime`, the one shutdown every command that runs the engine uses (D406). When the
/// command was `interrupted` (a Ctrl-C, or quitting the TUI while a run is in progress) it does
/// not wait for blocking tasks, so a read still in flight (a TLS file on a FIFO) cannot hold up
/// the exit; otherwise it waits for them, so nothing it began is cut short (D398).
pub fn close(runtime: Runtime, interrupted: bool) {
    if interrupted {
        runtime.shutdown_background();
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
pub async fn execute(engine: &Engine, request: &RunRequest, stops: &mut Stops) -> Result<RunReport, EngineError> {
    let (interrupter, interrupts) = vc_engine::interrupts();
    let run = engine.run(request, interrupts);
    tokio::pin!(run);
    loop {
        tokio::select! {
            report = &mut run => return report,
            Some(()) = stops.next() => interrupter.interrupt(),
        }
    }
}

/// Prints the run's notes, and ends with the run's exit code.
pub fn finished(report: &RunReport) -> Outcome {
    notes(report);
    Outcome::Exit(report.exit)
}

/// `warning: <notice>`, or `warning[V5118]: <rest>` for a notice that starts with its code.
fn warning(notice: &str) -> String {
    let coded = notice.split_once(": ").filter(|(code, _)| {
        code.len() == 5 && code.starts_with('V') && code[1..].bytes().all(|byte| byte.is_ascii_digit())
    });
    coded.map_or_else(|| format!("warning: {notice}"), |(code, rest)| format!("warning[{code}]: {rest}"))
}

/// Prints the run's notices, and the seed of a failed run that generated data from it.
pub fn notes(report: &RunReport) {
    for notice in &report.notices {
        eprintln!("{}", warning(notice));
    }
    if report.exit == 1 && report.generated {
        eprintln!("seed {}: pass `--seed {}` to run with the same generated data", report.seed, report.seed);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use super::{close, stops, warning};

    /// A runtime with one blocking task that has started and runs until `release` is set, or
    /// 10 seconds pass.
    fn holding(release: &Arc<AtomicBool>) -> tokio::runtime::Runtime {
        let runtime = tokio::runtime::Builder::new_multi_thread().build().unwrap();
        let (started, held) = (Arc::new(AtomicBool::new(false)), Arc::clone(release));
        let running = Arc::clone(&started);
        runtime.spawn_blocking(move || {
            running.store(true, Ordering::SeqCst);
            let begun = Instant::now();
            while !held.load(Ordering::SeqCst) && begun.elapsed() < Duration::from_secs(10) {
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let begun = Instant::now();
        while !started.load(Ordering::SeqCst) && begun.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(1));
        }
        runtime
    }

    #[test]
    fn after_a_ctrl_c_the_runtime_lets_go_of_a_blocking_read() {
        let release = Arc::new(AtomicBool::new(false));
        let runtime = holding(&release);
        let (stop, mut stops) = stops();
        stop.send(()).unwrap();
        assert_eq!(runtime.block_on(stops.next()), Some(()));
        let begun = Instant::now();
        close(runtime, stops.interrupted());
        assert!(begun.elapsed() < Duration::from_secs(5), "closed after {:?}", begun.elapsed());
        release.store(true, Ordering::SeqCst);
    }

    #[test]
    fn without_a_ctrl_c_the_runtime_waits_for_its_blocking_tasks() {
        let release = Arc::new(AtomicBool::new(false));
        let runtime = holding(&release);
        let (stop, mut stops) = stops();
        drop(stop);
        assert_eq!(runtime.block_on(stops.next()), None, "no Ctrl-C once no one sends");
        let releaser = Arc::clone(&release);
        let later = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            releaser.store(true, Ordering::SeqCst);
        });
        let begun = Instant::now();
        close(runtime, stops.interrupted());
        assert!(begun.elapsed() >= Duration::from_millis(250), "closed after {:?}", begun.elapsed());
        later.join().unwrap();
    }

    #[test]
    fn a_warning_carries_the_code_its_notice_starts_with() {
        assert_eq!(warning("V5118: plugin `report` failed: x"), "warning[V5118]: plugin `report` failed: x");
        assert_eq!(warning("history: locked"), "warning: history: locked");
        assert_eq!(warning("V51x8: odd"), "warning: V51x8: odd");
    }
}
