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
/// the first run refuses ends it with exit 2, since no save can fix it; Ctrl-C with exit 130; a
/// file watcher that stops with exit 70, as nothing could be watched after it (D406).
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
    let outcome = runtime.block_on(watch(&located.engine, request, batches));
    // Only a Ctrl-C ends a watch with 130; a refused flag or a stopped watcher closes normally.
    run::close(runtime, matches!(outcome, Ok(Outcome::Exit(CANCELLED))));
    outcome
}

/// What a watch waiting for saves wakes to.
enum Woke {
    /// The files saved since the last run.
    Saved(Vec<SourcePath>),
    /// A Ctrl-C.
    Stopped,
    /// The file watcher's channel closed, which no Ctrl-C did.
    Closed,
}

/// The saves and the Ctrl-Cs a watch waits on.
struct Inbox {
    saves: UnboundedReceiver<Vec<SourcePath>>,
    stops: run::Stops,
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
        let (stop, stops) = run::stops();
        tokio::spawn(run::ctrl_c(stop));
        Self { saves, stops }
    }

    /// The files saved since the last run, a Ctrl-C, or the watcher's end.
    async fn saved(&mut self) -> Woke {
        let batch = tokio::select! {
            batch = self.saves.recv() => batch,
            Some(()) = self.stops.next() => return Woke::Stopped,
        };
        let Some(mut saved) = batch else { return Woke::Closed };
        while let Ok(more) = self.saves.try_recv() {
            saved.extend(more);
        }
        Woke::Saved(saved)
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
        request.plan.select.saved = Some(match inbox.saved().await {
            Woke::Saved(saved) => saved,
            Woke::Stopped => return Ok(Outcome::Exit(CANCELLED)),
            Woke::Closed => anyhow::bail!("the file watcher stopped, so no save can be seen; start `vc watch` again"),
        });
    }
}

/// Whether `error` refuses a flag's value (`--profile nope`), which no save can fix.
fn from_a_flag(error: &EngineError) -> bool {
    matches!(error, EngineError::Setting { origin, .. } if origin.starts_with("--"))
}

#[cfg(test)]
mod tests {
    use super::{Inbox, Woke};

    #[tokio::test]
    async fn a_watcher_that_stops_sending_is_not_a_ctrl_c() {
        let (save, batches) = std::sync::mpsc::channel();
        save.send(vec![vc_engine::SourcePath::new("services/a/a.test.yaml").unwrap()]).unwrap();
        drop(save);
        let mut inbox = Inbox::new(batches);
        assert!(matches!(inbox.saved().await, Woke::Saved(saved) if saved.len() == 1), "a save sent before it closed");
        assert!(matches!(inbox.saved().await, Woke::Closed), "then the watcher is gone, which no Ctrl-C sent");
    }
}
