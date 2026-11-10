//! Carries out what the tabs ask of the engine, each on a task of its own, and sends what
//! comes back as updates; the UI never waits on the engine.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::sync::watch;
use tokio::task::{JoinError, JoinHandle};
use vc_engine::{
    Changes, Engine, EngineError, ExplainRequest, FileKind, ImportRequest, Interrupter, ListKind, ListRequest,
    LoadRequest, LoadWatch, RunFeed, RunRequest, RunWatch, interrupts,
};

use crate::screen::{Ask, Flight, Update};

/// Feed items the run may get ahead of the UI by before it drops some (counted as missed).
const FEED: usize = 4_096;
/// The stored runs the History tab lists, and the runs its trends cover (G15).
const RUNS: usize = 30;
/// Load seconds the run may get ahead of the UI by before it drops some.
const TICKS: usize = 64;
/// Why a run or load run was not started.
const BUSY: &str = "a run or load run is in progress: x on its tab stops it";
/// How long a run or load run asked for waits, off the UI, on the last one's task to return,
/// as it does just after its feed's end, before it is refused.
const SETTLE: Duration = Duration::from_millis(250);
/// Runs or load runs that waited on the last one and may be carried out again, ahead of the UI.
const RETRIES: usize = 8;
/// How long quitting waits on the run in progress after each interrupt.
const PATIENCE: Duration = Duration::from_secs(5);
/// The interrupts that end a run outright, as `vc run` counts Ctrl-Cs.
const INTERRUPTS: usize = 3;
/// What a tab is told when the work it asked for panicked.
const PANICKED: &str = "internal error, please report it";

/// The engine side of the UI.
pub struct Driver {
    engine: Arc<Engine>,
    base: Arc<Path>,
    updates: Sender<Update>,
    running: Option<Running>,
    retry: Sender<Ask>,
    retried: Receiver<Ask>,
}

/// The run or load run in progress: which it is, how to interrupt it, its task, which `done`
/// hears the end of, and whether it was asked to stop.
struct Running {
    kind: Flight,
    interrupter: Interrupter,
    task: JoinHandle<()>,
    done: watch::Receiver<()>,
    stopped: bool,
}

impl Running {
    /// Whether its task is over, the wait on its work included.
    fn ended(&self) -> bool {
        self.done.has_changed().is_err()
    }

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
        let (retry, retried) = mpsc::channel(RETRIES);
        Self { engine: Arc::new(engine), base: base.into(), updates, running: None, retry, retried }
    }

    /// Carries out `ask` without waiting on it. A run or load run asked for while the last one
    /// is still returning is handed back by [`Driver::retried`] once it has, to be carried out
    /// again, or refused should that take longer than [`SETTLE`].
    pub fn carry(&mut self, ask: Ask) {
        let Some(ask) = self.settled(ask) else { return };
        match ask {
            Ask::Run(request) => self.start(*request),
            Ask::Stop(kind) => self.stop(kind),
            Ask::History => self.read(Update::HistoryFailed, |engine| vec![history(engine)]),
            Ask::Compare { before, after } => self.read(
                move |reason| Update::CompareFailed { before, after, reason },
                move |engine| vec![compare(engine, before, after)],
            ),
            Ask::Explore => self.read(Update::ExploreFailed, |engine| vec![explore(engine)]),
            Ask::Explain { id, profiles } => {
                let asked = id.clone();
                let failed = move |reason| Update::ExplainFailed { id: asked, reason };
                self.read(failed, move |engine| vec![explain(engine, id, profiles)]);
            }
            Ask::Skip(request) => self
                .read(Update::SkipFailed, move |engine| edited(engine, engine.skip(&request), (request.target, true))),
            Ask::Unskip(target) => {
                self.read(Update::SkipFailed, move |engine| edited(engine, engine.unskip(&target), (target, false)));
            }
            Ask::Scenarios => self.read(Update::ScenariosFailed, |engine| vec![scenarios(engine)]),
            Ask::Load(request) => self.load(request),
            Ask::Import(request) => {
                let base = Arc::clone(&self.base);
                self.read(Update::ImportFailed, move |engine| vec![draft(engine, &base, *request)]);
            }
            Ask::Write(changes) => self.read(Update::ImportFailed, move |engine| vec![written(engine, &changes)]),
        }
    }

    /// Reads or writes the project or the history on a blocking thread, as file reads and
    /// SQLite do, and sends back the updates `read` makes of it, or `failed` should it panic.
    fn read(
        &self,
        failed: impl FnOnce(String) -> Update + Send + 'static,
        read: impl FnOnce(&Engine) -> Vec<Update> + Send + 'static,
    ) {
        let (engine, updates) = (Arc::clone(&self.engine), self.updates.clone());
        let work = tokio::task::spawn_blocking(move || {
            for update in read(&engine) {
                if updates.blocking_send(update).is_err() {
                    break;
                }
            }
        });
        drop(watched(work, self.updates.clone(), failed));
    }

    /// A run or load run that waited on the last one to return, to be carried out again.
    pub async fn retried(&mut self) -> Option<Ask> {
        self.retried.recv().await
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

    /// `ask`, when it can be carried out now: it is not a run or load run, or none is in
    /// progress. Otherwise a task waits up to [`SETTLE`] for the one in progress to return, then
    /// hands `ask` back through [`Driver::retried`], or sends back why it is refused: a tab that
    /// asked for one is never left waiting on it, nor is the UI.
    fn settled(&self, ask: Ask) -> Option<Ask> {
        let refused: fn(String) -> Update = match &ask {
            Ask::Run(_) => Update::RunFailed,
            Ask::Load(_) => Update::LoadFailed,
            _ => return Some(ask),
        };
        let Some(running) = self.running.as_ref().filter(|running| !running.ended()) else { return Some(ask) };
        let (mut done, retry, updates) = (running.done.clone(), self.retry.clone(), self.updates.clone());
        tokio::spawn(async move {
            if tokio::time::timeout(SETTLE, done.changed()).await.is_ok() {
                retry.send(ask).await.ok();
            } else {
                updates.send(refused(BUSY.to_owned())).await.ok();
            }
        });
        None
    }

    /// Interrupts the run or load run in progress when it is a `kind`: once, as one Ctrl-C
    /// does; asked again, as many more times as end it outright, its cleanups abandoned.
    fn stop(&mut self, kind: Flight) {
        let Some(running) = self.running.as_mut().filter(|running| running.kind == kind) else { return };
        let times = if running.stopped { INTERRUPTS - 1 } else { 1 };
        running.stopped = true;
        for _ in 0..times {
            running.interrupter.interrupt();
        }
    }

    /// Starts a watched run; none is in progress.
    fn start(&mut self, mut request: RunRequest) {
        let (watch, feed) = mpsc::channel(FEED);
        request.watch = Some(RunWatch::new(watch));
        let (interrupter, interrupts) = interrupts();
        let (engine, updates) = (Arc::clone(&self.engine), self.updates.clone());
        let work = tokio::spawn(async move {
            let run = async move { engine.run(&request, interrupts).await };
            let (ran, ()) = tokio::join!(run, forward(feed, &updates, ongoing));
            let end = ran
                .map_or_else(|error| Update::RunFailed(error.to_string()), |report| Update::Run(RunFeed::Done(report)));
            updates.send(end).await.ok();
        });
        let (task, done) = watched(work, self.updates.clone(), Update::RunFailed);
        self.running = Some(Running { kind: Flight::Run, interrupter, task, done, stopped: false });
    }

    /// Starts a load run with its seconds watched; none is in progress.
    fn load(&mut self, request: LoadRequest) {
        let (ticks, heard) = mpsc::channel(TICKS);
        let (interrupter, interrupts) = interrupts();
        let (engine, updates) = (Arc::clone(&self.engine), self.updates.clone());
        let work = tokio::spawn(async move {
            let load = async move { engine.load(&request, LoadWatch { interrupts, ticks: Some(ticks) }).await };
            let (loaded, ()) = tokio::join!(load, forward(heard, &updates, |tick| Some(Update::Load(tick))));
            let update = loaded
                .map_or_else(|error| Update::LoadFailed(error.to_string()), |report| Update::Loaded(Box::new(report)));
            updates.send(update).await.ok();
        });
        let (task, done) = watched(work, self.updates.clone(), Update::LoadFailed);
        self.running = Some(Running { kind: Flight::Load, interrupter, task, done, stopped: false });
    }
}

/// A task that is aborted when this is dropped, as when the task waiting on it is aborted.
struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Waits on `work` and sends `failed` with why should it panic, so the tab that asked for it
/// is never left waiting; aborting the wait aborts `work`. The receiver hears the wait end.
fn watched(
    work: JoinHandle<()>,
    updates: Sender<Update>,
    failed: impl FnOnce(String) -> Update + Send + 'static,
) -> (JoinHandle<()>, watch::Receiver<()>) {
    let (ended, done) = watch::channel(());
    let task = tokio::spawn(async move {
        let _ended = ended;
        let mut work = AbortOnDrop(work);
        if let Err(error) = (&mut work.0).await
            && let Some(reason) = panicked(error)
        {
            updates.send(failed(reason)).await.ok();
        }
    });
    (task, done)
}

/// What a task's panic said, when it panicked rather than being aborted.
fn panicked(error: JoinError) -> Option<String> {
    let payload = error.try_into_panic().ok()?;
    let said = payload.downcast_ref::<&str>().map(|said| (*said).to_owned());
    let said = said.or_else(|| payload.downcast_ref::<String>().cloned()).unwrap_or_default();
    Some(format!("{PANICKED}: {said}"))
}

/// Drafts an import: the spec found as `vc import` finds it, fetched and read, the files it
/// would make passed through the compile guard, nothing written. It runs on the blocking
/// thread a read is carried out on.
fn draft(engine: &Engine, base: &Path, request: ImportRequest) -> Update {
    let drafted = tokio::runtime::Handle::current().block_on(drafted(engine, base, request));
    drafted.map_or_else(|error| Update::ImportFailed(told(&error)), Update::Drafted)
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

/// The project's apis, then its tests, from one read of it.
fn explore(engine: &Engine) -> Update {
    match engine.catalog() {
        Ok(report) => Update::Explored { items: report.items, notices: report.notices },
        Err(error) => Update::ExploreFailed(error.to_string()),
    }
}

/// How a run under `profiles` sees the test or api `id`.
fn explain(engine: &Engine, id: String, profiles: Vec<String>) -> Update {
    match engine.explain(&id, &ExplainRequest { profiles, ..ExplainRequest::default() }) {
        Ok(report) => Update::Explained { id, text: report.text },
        Err(error) => Update::ExplainFailed { id, reason: error.to_string() },
    }
}

/// Writes the skip or unskip `changes` of `target`, then reads the project again.
fn edited(engine: &Engine, changes: Result<Changes, EngineError>, (target, skipped): (String, bool)) -> Vec<Update> {
    match changes.and_then(|changes| engine.write(&changes)) {
        Ok(()) => vec![Update::Quarantined { target, skipped }, explore(engine)],
        Err(error) => vec![Update::SkipFailed(error.to_string())],
    }
}

/// The project's load scenarios: the load files `vc list files` lists.
fn scenarios(engine: &Engine) -> Update {
    match engine.list(&ListRequest { kind: ListKind::Files, ..ListRequest::default() }) {
        Ok(report) => {
            Update::Scenarios(report.items.into_iter().filter(|file| file.kind() == Some(FileKind::Load)).collect())
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

/// Passes a watched feed on, each item as `wrap` makes it an update (none drops it), until it
/// ends or the UI stops listening.
async fn forward<T>(mut feed: Receiver<T>, updates: &Sender<Update>, wrap: fn(T) -> Option<Update>) {
    while let Some(item) = feed.recv().await {
        if let Some(update) = wrap(item)
            && updates.send(update).await.is_err()
        {
            break;
        }
    }
}

/// A run's feed item as an update, but for its `Done`: a run is ended once, when
/// [`Engine::run`] returns, with its report or why it failed, so a run that fails after its
/// feed ends is never heard ending twice, the second taken for a run started in between.
fn ongoing(item: RunFeed) -> Option<Update> {
    (!matches!(item, RunFeed::Done(_))).then_some(Update::Run(item))
}

#[cfg(test)]
#[path = "driver_tests.rs"]
mod tests;
