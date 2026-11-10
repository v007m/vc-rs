//! Carries out what the tabs ask of the engine, each on a task of its own, and sends what
//! comes back as updates; the UI never waits on the engine.

use std::sync::Arc;

use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::task::JoinHandle;
use vc_engine::{Engine, Interrupter, RunFeed, RunRequest, RunWatch, interrupts};

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
            Ask::History => self.read(history),
            Ask::Compare { before, after } => self.read(move |engine| compare(engine, before, after)),
        }
    }

    /// Reads the history on a blocking thread, as SQLite does, and sends back what `read` makes
    /// of it.
    fn read(&self, read: impl FnOnce(&Engine) -> Update + Send + 'static) {
        let (engine, updates) = (Arc::clone(&self.engine), self.updates.clone());
        tokio::task::spawn_blocking(move || updates.blocking_send(read(&engine)).ok());
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

/// Passes the run's feed on until it ends or the UI stops listening.
async fn forward(mut feed: Receiver<RunFeed>, updates: &Sender<Update>) {
    while let Some(item) = feed.recv().await {
        if updates.send(Update::Run(item)).await.is_err() {
            break;
        }
    }
}
