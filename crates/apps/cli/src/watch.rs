use std::path::Path;
use std::sync::mpsc::Receiver;

use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use vc_engine::{Engine, EngineError, RunRequest, SourcePath};

use crate::run::{self, RunArgs};
use crate::{Outcome, outcome, project};

/// The exit code of a watch ended with Ctrl-C (D127).
const CANCELLED: u8 = 130;

/// `vc watch [selectors] [flags]`: runs the selection as `vc run` does, then, after each burst
/// of saves, the selected tests in the saved files and those that call an api in one (D180).
/// A save that leaves the project with errors prints them and waits for the next save. A flag
/// the first run refuses ends it with exit 2, since no save can fix it; Ctrl-C with exit 130.
pub fn run(args: &RunArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let located = match project::locate(project)? {
        Ok(located) => located,
        Err(outcome) => return Ok(outcome),
    };
    let request = match args.request(&located)? {
        Ok(request) => request,
        Err(reason) => return Ok(Outcome::Refused(reason)),
    };
    let batches = match located.engine.watch() {
        Ok(batches) => batches,
        Err(error) => return Ok(Outcome::Engine(error)),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(watch(&located.engine, request, batches))
}

/// The saves and the Ctrl-Cs a watch waits on.
struct Inbox {
    saves: UnboundedReceiver<Vec<SourcePath>>,
    stops: UnboundedReceiver<()>,
}

impl Inbox {
    /// Hands `batches` over from the watcher's thread, and starts listening for Ctrl-C.
    fn new(batches: Receiver<Vec<SourcePath>>) -> Self {
        let (save, saves) = unbounded_channel();
        std::thread::spawn(move || {
            for batch in batches {
                if save.send(batch).is_err() {
                    return;
                }
            }
        });
        let (stop, stops) = unbounded_channel();
        tokio::spawn(run::ctrl_c(stop));
        Self { saves, stops }
    }

    /// The files saved since the last run, or `None` on Ctrl-C or once the watch has ended.
    async fn saved(&mut self) -> Option<Vec<SourcePath>> {
        let mut saved = tokio::select! {
            batch = self.saves.recv() => batch?,
            _ = self.stops.recv() => return None,
        };
        while let Ok(more) = self.saves.try_recv() {
            saved.extend(more);
        }
        Some(saved)
    }
}

/// Runs, then runs again on each save, until Ctrl-C.
async fn watch(
    engine: &Engine,
    mut request: RunRequest,
    batches: Receiver<Vec<SourcePath>>,
) -> anyhow::Result<Outcome> {
    let mut inbox = Inbox::new(batches);
    loop {
        match run::execute(engine, &request, &mut inbox.stops).await {
            Ok(report) if report.exit == CANCELLED => return Ok(run::finished(&report)),
            Ok(report) => run::notes(&report),
            Err(error) if request.plan.select.saved.is_none() && from_a_flag(&error) => {
                return Ok(Outcome::Engine(error));
            }
            // The watch goes on: the error is printed and its exit code is not used.
            Err(error) => _ = outcome::finish(Ok(Outcome::Engine(error))),
        }
        eprintln!("watching for saves; press Ctrl-C to stop");
        let Some(saved) = inbox.saved().await else { return Ok(Outcome::Exit(CANCELLED)) };
        request.plan.select.saved = Some(saved);
    }
}

/// Whether `error` refuses a flag's value (`--profile nope`), which no save can fix.
fn from_a_flag(error: &EngineError) -> bool {
    matches!(error, EngineError::Setting { origin, .. } if origin.starts_with("--"))
}
