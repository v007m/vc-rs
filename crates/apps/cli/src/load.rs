//! `vc load <scenarioId>`: runs a load scenario with live metrics.

use std::io::Write as _;
use std::path::Path;

use clap::Args;
use tokio::sync::mpsc::{self, Receiver, UnboundedReceiver};
use vc_engine::{Engine, EngineError, Event, LoadReport, LoadRequest, LoadWatch};

use crate::explain::LayerArgs;
use crate::stdout;
use crate::{Outcome, project};

/// Live lines the printer may fall behind by before ticks are dropped.
const TICKS: usize = 4;

/// `vc load …`.
#[derive(Args)]
pub struct LoadArgs {
    /// The scenario to run: its load file's ID (`checkout.peak` for
    /// `load/checkout/peak.load.yaml`).
    scenario: String,
    #[command(flatten)]
    layers: LayerArgs,
    /// Prints the final report as JSON on stdout, with no live line.
    #[arg(long)]
    json: bool,
}

/// `vc load <scenarioId> [--profile] [--set] [--json]`: a live line each second on stderr, then
/// the summary; exit 0 within thresholds, 1 blocked by a shared action, 2 unknown scenario or
/// bad flag, 3 a threshold breached, 130 cancelled (D127).
pub fn run(args: &LoadArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let layers = args.layers.request();
    let request =
        LoadRequest { scenario: args.scenario.clone(), profiles: layers.profiles, sets: layers.sets, seed: None };
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let (stop, mut stops) = mpsc::unbounded_channel();
    let report = runtime.block_on(async {
        let signals = tokio::spawn(crate::run::ctrl_c(stop));
        let report = measure((&located.engine, &request), &mut stops, !args.json).await;
        signals.abort();
        report
    });
    match report {
        Ok(report) => finished(&report, args.json),
        Err(error) => Ok(Outcome::Engine(error)),
    }
}

/// Runs `request` on `engine`, printing a live line per tick when `live`; each value on `stops`
/// (a Ctrl-C) interrupts the run.
async fn measure(
    (engine, request): (&Engine, &LoadRequest),
    stops: &mut UnboundedReceiver<()>,
    live: bool,
) -> Result<LoadReport, EngineError> {
    let (interrupter, interrupts) = vc_engine::interrupts();
    let (ticks, heard) = mpsc::channel(TICKS);
    let printer = live.then(|| tokio::spawn(print_ticks(heard)));
    let load = engine.load(request, LoadWatch { interrupts, ticks: live.then_some(ticks) });
    tokio::pin!(load);
    let report = loop {
        tokio::select! {
            report = &mut load => break report,
            Some(()) = stops.recv() => interrupter.interrupt(),
        }
    };
    if let Some(printer) = printer {
        printer.await.ok();
    }
    report
}

/// Prints each tick on stderr until the run lets go of its sender.
async fn print_ticks(mut heard: Receiver<Event>) {
    while let Some(event) = heard.recv().await {
        if let Some(line) = line(&event) {
            eprintln!("{line}");
        }
    }
}

/// `  12s  50 users  412 rps  p50 31 ms  p95 88 ms  p99 140 ms  0 errors`, then what breaches.
fn line(event: &Event) -> Option<String> {
    let Event::LoadTick { second, users, rps, p50, p90: _, p95, p99, errors, breaching } = event else {
        return None;
    };
    let (p50, p95, p99) = (p50.as_millis(), p95.as_millis(), p99.as_millis());
    let mut line =
        format!("{second:>5}s  {users} users  {rps} rps  p50 {p50} ms  p95 {p95} ms  p99 {p99} ms  {errors} errors");
    if !breaching.is_empty() {
        line.push_str("  breaching: ");
        line.push_str(&breaching.join(", "));
    }
    Some(line)
}

/// Prints the notices and the summary (or the JSON report), and ends with the run's exit code.
fn finished(report: &LoadReport, json: bool) -> anyhow::Result<Outcome> {
    for notice in &report.notices {
        eprintln!("warning: {notice}");
    }
    let mut out = stdout::out();
    if json {
        writeln!(out, "{:#}", report.json())?;
    } else {
        write!(out, "{}", report.text())?;
    }
    Ok(Outcome::Exit(report.exit))
}
