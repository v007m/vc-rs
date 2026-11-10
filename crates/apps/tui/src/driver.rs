//! Carries out what the tabs ask of the engine, each on a task of its own, and sends what
//! comes back as updates; the UI never waits on the engine.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::task::JoinHandle;
use vc_engine::{
    Changes, Engine, EngineError, ExplainRequest, ImportRequest, Interrupter, ListKind, ListRequest, LoadRequest,
    LoadWatch, RunRequest, RunWatch, interrupts,
};

use crate::screen::{Ask, Update};

/// Feed items the run may get ahead of the UI by before it drops some (counted as missed).
const FEED: usize = 4_096;
/// The stored runs the History tab lists, and the runs its trends cover (G15).
const RUNS: usize = 30;
/// Load seconds the run may get ahead of the UI by before it drops some.
const TICKS: usize = 64;
/// Why a run or load run was not started.
const BUSY: &str = "a run or load run is in progress: x on its tab stops it";
/// How long quitting waits on the run in progress after each interrupt.
const PATIENCE: Duration = Duration::from_secs(5);
/// The interrupts that end a run outright, as `vc run` counts Ctrl-Cs.
const INTERRUPTS: usize = 3;

/// The engine side of the UI.
pub struct Driver {
    engine: Arc<Engine>,
    base: Arc<Path>,
    updates: Sender<Update>,
    running: Option<Running>,
}

/// The run or load run in progress.
struct Running {
    interrupter: Interrupter,
    task: JoinHandle<()>,
}

impl Running {
    /// Interrupts the run up to three times, as three Ctrl-Cs do: it is cancelled, then its
    /// grace period ends, then its cleanups are abandoned. It waits `patience` after each,
    /// and aborts the task if it has not ended by then.
    async fn wind_down(mut self, patience: Duration) {
        for _ in 0..INTERRUPTS {
            self.interrupter.interrupt();
            if tokio::time::timeout(patience, &mut self.task).await.is_ok() {
                return;
            }
        }
        self.task.abort();
    }
}

impl Driver {
    /// A driver for `engine`, sending what comes back into `updates`; a typed path starts
    /// from `base`.
    pub fn new(engine: Engine, base: PathBuf, updates: Sender<Update>) -> Self {
        Self { engine: Arc::new(engine), base: base.into(), updates, running: None }
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
            Ask::Skip(request) => {
                self.read(move |engine| edited(engine, engine.skip(&request), (request.target, true)));
            }
            Ask::Unskip(target) => self.read(move |engine| edited(engine, engine.unskip(&target), (target, false))),
            Ask::Scenarios => self.read(|engine| vec![scenarios(engine)]),
            Ask::Load(request) => self.load(request),
            Ask::Import(request) => self.import(*request),
            Ask::Write(changes) => self.read(move |engine| vec![written(engine, &changes)]),
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

    /// Ends the run in progress, if any, as Ctrl-C ends `vc run`: the first interrupt gives
    /// work in flight its grace period and runs the cleanups; each [`PATIENCE`] it is still
    /// going, the next interrupt hurries it, and after the third it is abandoned. Drop the
    /// updates' receiver first, so nothing waits for the UI meanwhile.
    pub async fn finish(self) {
        if let Some(running) = self.running {
            running.wind_down(PATIENCE).await;
        }
    }

    /// Whether a run or a load run is in progress.
    pub fn busy(&self) -> bool {
        self.running.as_ref().is_some_and(|running| !running.task.is_finished())
    }

    /// When a run or load run is in progress, sends back `refused` with why and is `true`: a
    /// tab that asked for one is never left waiting on it.
    fn refused(&self, refused: fn(String) -> Update) -> bool {
        if !self.busy() {
            return false;
        }
        let updates = self.updates.clone();
        tokio::spawn(async move { updates.send(refused(BUSY.to_owned())).await.ok() });
        true
    }

    /// Starts a watched run, unless a run or load run is in progress.
    fn start(&mut self, mut request: RunRequest) {
        if self.refused(Update::RunFailed) {
            return;
        }
        let (watch, feed) = mpsc::channel(FEED);
        request.watch = Some(RunWatch::new(watch));
        let (interrupter, interrupts) = interrupts();
        let (engine, updates) = (Arc::clone(&self.engine), self.updates.clone());
        let task = tokio::spawn(async move {
            let run = async move { engine.run(&request, interrupts).await };
            let (ran, ()) = tokio::join!(run, forward(feed, &updates, Update::Run));
            if let Err(error) = ran {
                updates.send(Update::RunFailed(error.to_string())).await.ok();
            }
        });
        self.running = Some(Running { interrupter, task });
    }

    /// Starts a load run with its seconds watched, unless a run or load run is in progress.
    fn load(&mut self, request: LoadRequest) {
        if self.refused(Update::LoadFailed) {
            return;
        }
        let (ticks, heard) = mpsc::channel(TICKS);
        let (interrupter, interrupts) = interrupts();
        let (engine, updates) = (Arc::clone(&self.engine), self.updates.clone());
        let task = tokio::spawn(async move {
            let load = async move { engine.load(&request, LoadWatch { interrupts, ticks: Some(ticks) }).await };
            let (loaded, ()) = tokio::join!(load, forward(heard, &updates, Update::Load));
            let update = loaded
                .map_or_else(|error| Update::LoadFailed(error.to_string()), |report| Update::Loaded(Box::new(report)));
            updates.send(update).await.ok();
        });
        self.running = Some(Running { interrupter, task });
    }

    /// Drafts an import: the spec found as `vc import` finds it, fetched and read, the files
    /// it would make passed through the compile guard, nothing written.
    fn import(&self, request: ImportRequest) {
        let (engine, base, updates) = (Arc::clone(&self.engine), Arc::clone(&self.base), self.updates.clone());
        tokio::spawn(async move {
            let drafted = drafted(&engine, &base, request).await;
            updates.send(drafted.map_or_else(|error| Update::ImportFailed(told(&error)), Update::Drafted)).await.ok();
        });
    }
}

/// The draft of `request`, its source a path in the project when it was typed as a path from
/// `base`.
async fn drafted(engine: &Engine, base: &Path, mut request: ImportRequest) -> Result<Changes, EngineError> {
    request.source = engine.spec_location(base, &request.source)?;
    engine.import(&request).await
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
        Err(error) => Update::CompareFailed { before, after, reason: error.to_string() },
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

/// The project's load scenarios: the load files `vc list files` lists.
fn scenarios(engine: &Engine) -> Update {
    match engine.list(&ListRequest { kind: ListKind::Files, ..ListRequest::default() }) {
        Ok(report) => {
            Update::Scenarios(report.items.into_iter().filter(|file| file.path.ends_with(".load.yaml")).collect())
        }
        Err(error) => Update::ScenariosFailed(error.to_string()),
    }
}

/// Writes a drafted import.
fn written(engine: &Engine, changes: &Changes) -> Update {
    match engine.write(changes) {
        Ok(()) => Update::Written(changes.titles.clone()),
        Err(error) => Update::ImportFailed(told(&error)),
    }
}

/// `error`'s text, then its help on a line of its own, as the CLI prints them.
fn told(error: &EngineError) -> String {
    error.help().map_or_else(|| error.to_string(), |help| format!("{error}\nhelp: {help}"))
}

/// Passes a watched feed on, each item as `wrap` makes it an update, until it ends or the UI
/// stops listening.
async fn forward<T>(mut feed: Receiver<T>, updates: &Sender<Update>, wrap: fn(T) -> Update) {
    while let Some(item) = feed.recv().await {
        if updates.send(wrap(item)).await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
#[path = "driver_tests.rs"]
mod tests;
