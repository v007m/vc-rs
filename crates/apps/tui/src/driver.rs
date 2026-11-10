//! Carries out what the tabs ask of the engine, each on a task of its own, and sends what
//! comes back as updates; the UI never waits on the engine.

use std::sync::Arc;

use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::task::JoinHandle;
use vc_engine::{
    Changes, Engine, EngineError, ExplainRequest, Interrupter, ListKind, ListRequest, RunFeed, RunRequest, RunWatch,
    interrupts,
};

use crate::screen::{Ask, Update};

/// Feed items the run may get ahead of the UI by before it drops some (counted as missed).
const FEED: usize = 4_096;
/// The stored runs the History tab lists, and the runs its trends cover (G15).
const RUNS: usize = 30;

/// The engine side of the UI.
pub struct Driver {
    engine: Arc<Engine>,
    updates: Sender<Update>,
    running: Option<Running>,
}

/// The run in progress.
struct Running {
    interrupter: Interrupter,
    task: JoinHandle<()>,
}

impl Driver {
    /// A driver for `engine`, sending what comes back into `updates`.
    pub fn new(engine: Engine, updates: Sender<Update>) -> Self {
        Self { engine: Arc::new(engine), updates, running: None }
    }

    /// Carries out `ask`.
    pub fn carry(&mut self, ask: Ask) {
        match ask {
            Ask::Run(request) => self.start(*request),
            Ask::Stop => {
                if let Some(running) = &self.running {
                    running.interrupter.interrupt();
                }
            }
            Ask::History => self.read(|engine| vec![history(engine)]),
            Ask::Compare { before, after } => self.read(move |engine| vec![compare(engine, before, after)]),
            Ask::Explore => self.read(|engine| vec![explore(engine)]),
            Ask::Explain { id, profiles } => self.read(move |engine| vec![explain(engine, id, profiles)]),
            Ask::Skip(request) => self.read(move |engine| edited(engine, engine.skip(&request), (request.target, true))),
            Ask::Unskip(target) => self.read(move |engine| edited(engine, engine.unskip(&target), (target, false))),
        }
    }

    /// Reads or writes the project or the history on a blocking thread, as file reads and
    /// SQLite do, and sends back the updates `read` makes of it.
    fn read(&self, read: impl FnOnce(&Engine) -> Vec<Update> + Send + 'static) {
        let (engine, updates) = (Arc::clone(&self.engine), self.updates.clone());
        tokio::task::spawn_blocking(move || {
            for update in read(&engine) {
                if updates.blocking_send(update).is_err() {
                    break;
                }
            }
        });
    }

    /// Ends the run in progress, if any, as one Ctrl-C ends `vc run`: work in flight gets its
    /// grace period and the cleanups run. Drop the updates' receiver first, so nothing waits
    /// for the UI meanwhile.
    pub async fn finish(self) {
        if let Some(running) = self.running {
            running.interrupter.interrupt();
            running.task.await.ok();
        }
    }

    /// Starts a watched run, unless one is in progress.
    fn start(&mut self, mut request: RunRequest) {
        if self.running.as_ref().is_some_and(|running| !running.task.is_finished()) {
            return;
        }
        let (watch, feed) = mpsc::channel(FEED);
        request.watch = Some(RunWatch::new(watch));
        let (interrupter, interrupts) = interrupts();
        let (engine, updates) = (Arc::clone(&self.engine), self.updates.clone());
        let task = tokio::spawn(async move {
            let run = async move { engine.run(&request, interrupts).await };
            let (ran, ()) = tokio::join!(run, forward(feed, &updates));
            if let Err(error) = ran {
                updates.send(Update::RunFailed(error.to_string())).await.ok();
            }
        });
        self.running = Some(Running { interrupter, task });
    }
}

/// The newest stored runs and the tests' trends over them.
fn history(engine: &Engine) -> Update {
    let read = engine.history(RUNS).and_then(|report| Ok((report, engine.trends(RUNS)?)));
    match read {
        Ok((report, trends)) => Update::History { runs: report.runs().to_vec(), trends },
        Err(error) => Update::HistoryFailed(error.to_string()),
    }
}

/// The tests of the runs numbered `before` and `after`.
fn compare(engine: &Engine, before: u64, after: u64) -> Update {
    let tests = |run: u64| engine.stored(&run.to_string()).map(|stored| (run, stored.tests().to_vec()));
    match tests(before).and_then(|before| Ok((before, tests(after)?))) {
        Ok((before, after)) => Update::Compared { before, after },
        Err(error) => Update::HistoryFailed(error.to_string()),
    }
}

/// The project's apis, then its tests.
fn explore(engine: &Engine) -> Update {
    let list = |kind| engine.list(&ListRequest { kind, ..ListRequest::default() }).map(|report| report.items);
    match list(ListKind::Apis).and_then(|apis| Ok([apis, list(ListKind::Tests)?].concat())) {
        Ok(items) => Update::Explored(items),
        Err(error) => Update::ExploreFailed(error.to_string()),
    }
}

/// How a run under `profiles` sees the test or api `id`.
fn explain(engine: &Engine, id: String, profiles: Vec<String>) -> Update {
    match engine.explain(&id, &ExplainRequest { profiles, ..ExplainRequest::default() }) {
        Ok(report) => Update::Explained { id, text: report.text },
        Err(error) => Update::ExploreFailed(error.to_string()),
    }
}

/// Writes the skip or unskip `changes` of `target`, then reads the project again.
fn edited(engine: &Engine, changes: Result<Changes, EngineError>, (target, skipped): (String, bool)) -> Vec<Update> {
    match changes.and_then(|changes| engine.write(&changes)) {
        Ok(()) => vec![Update::Quarantined { target, skipped }, explore(engine)],
        Err(error) => vec![Update::ExploreFailed(error.to_string())],
    }
}

/// Passes the run's feed on until it ends or the UI stops listening.
async fn forward(mut feed: Receiver<RunFeed>, updates: &Sender<Update>) {
    while let Some(item) = feed.recv().await {
        if updates.send(Update::Run(item)).await.is_err() {
            break;
        }
    }
}
