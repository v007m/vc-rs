use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::{self, Receiver};
use tokio::sync::{oneshot, watch};
use tokio::time::Instant;
use vc_engine::{EngineBuilder, ImportRequest, LoadRequest, RunFeed, interrupts};
use vc_testkit::{FakeVcs, MemoryCompileCache, MemoryEnvironment, MemorySources};

use super::{BUSY, Driver, PANICKED, PATIENCE, Running, SETTLE, ongoing, watched};
use crate::screen::{Ask, Flight, Update};

/// A driver over an empty project, and where its updates come out.
fn driver() -> (Driver, Receiver<Update>) {
    let engine = EngineBuilder::new(PathBuf::from("/project")).sources(Arc::new(MemorySources::new(&[]))).build();
    let (updates, received) = mpsc::channel(8);
    (Driver::new(engine.unwrap(), PathBuf::new(), updates), received)
}

/// A driver over a project on disk holding `specs/orders.curl`, typed paths starting from
/// `specs`, and where its updates come out.
fn on_disk() -> (tempfile::TempDir, Driver, Receiver<Update>) {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::write(root.join("vibranium.config.yaml"), CONFIG).unwrap();
    std::fs::create_dir_all(root.join("specs")).unwrap();
    std::fs::write(root.join("specs/orders.curl"), "curl https://api.orders.test/v1/orders\n").unwrap();
    let engine = EngineBuilder::new(root.clone())
        .vcs(Arc::new(FakeVcs::default()))
        .environment(Arc::new(MemoryEnvironment::new(&[])))
        .compile_cache(Arc::new(MemoryCompileCache::new()))
        .build();
    let (updates, received) = mpsc::channel(8);
    (dir, Driver::new(engine.unwrap(), root.join("specs"), updates), received)
}

const CONFIG: &str = "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n";

/// An import of `source` with the curl importer.
fn curl(source: &str) -> Ask {
    Ask::Import(Box::new(ImportRequest {
        kind: "curl".to_owned(),
        source: source.to_owned(),
        ..ImportRequest::default()
    }))
}

#[tokio::test]
async fn a_typed_spec_path_starts_from_the_base_as_vc_import_reads_it() {
    let (_dir, mut driver, mut updates) = on_disk();
    driver.carry(curl("orders.curl"));
    let update = updates.recv().await;
    assert!(matches!(update, Some(Update::Drafted(_))), "{update:?}");
    driver.carry(curl("../outside.curl"));
    let Some(Update::ImportFailed(reason)) = updates.recv().await else { panic!("the import tab hears why") };
    assert!(reason.starts_with("../outside.curl: not a file or folder in the project at"), "{reason}");
    assert!(reason.ends_with("\nhelp: give a URL, or a path inside the project"), "{reason}");
}

/// A `kind` in progress, `work` waited on as the driver waits on a run's.
fn running(kind: Flight, work: impl Future<Output = ()> + Send + 'static) -> Running {
    let (task, done) = watched(tokio::spawn(work), mpsc::channel(1).0, Update::RunFailed);
    Running { kind, interrupter: interrupts().0, task, done, stopped: false }
}

/// A run in progress, its task `task`.
fn with_task(task: tokio::task::JoinHandle<()>) -> Running {
    Running { kind: Flight::Run, interrupter: interrupts().0, task, done: watch::channel(()).1, stopped: false }
}

/// A run in progress that never ends by itself, as a run is between its feed's `Done` and
/// the end of its task.
fn finishing() -> Running {
    running(Flight::Run, std::future::pending())
}

#[tokio::test(start_paused = true)]
async fn a_run_asked_for_while_another_is_finishing_is_refused_not_dropped() {
    let (mut driver, mut updates) = driver();
    driver.running = Some(finishing());
    let begun = Instant::now();
    driver.carry(Ask::Run(Box::default()));
    let Some(Update::RunFailed(reason)) = updates.recv().await else { panic!("the run tab hears why") };
    assert_eq!((reason.as_str(), begun.elapsed()), (BUSY, SETTLE));
}

#[tokio::test]
async fn a_run_asked_for_as_the_last_one_returns_is_carried_again_once_it_has_and_the_ui_never_waits() {
    let (mut driver, mut updates) = driver();
    let (end, ended) = oneshot::channel::<()>();
    driver.running = Some(running(Flight::Run, async move { ended.await.unwrap_or_default() }));
    driver.carry(Ask::Run(Box::default()));
    tokio::task::yield_now().await;
    assert!(updates.try_recv().is_err(), "nothing yet: the run waits off the UI");
    end.send(()).unwrap();
    let again = driver.retried().await;
    assert!(matches!(again, Some(Ask::Run(_))), "{again:?}");
    assert!(driver.running.as_ref().is_some_and(Running::ended), "the last one is over");
}

#[tokio::test]
async fn a_load_run_asked_for_while_a_run_is_in_progress_is_refused() {
    let (mut driver, mut updates) = driver();
    driver.running = Some(finishing());
    driver.carry(Ask::Load(LoadRequest::default()));
    let Some(Update::LoadFailed(reason)) = updates.recv().await else { panic!("the load tab hears why") };
    assert_eq!(reason, BUSY);
}

#[tokio::test(start_paused = true)]
async fn quitting_abandons_a_run_that_outlasts_three_interrupts() {
    let (held, dropped) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let _held = held;
        std::future::pending::<()>().await;
    });
    let begun = Instant::now();
    with_task(task).wind_down(PATIENCE).await;
    assert_eq!(begun.elapsed(), PATIENCE * 3, "a patience after each of three interrupts");
    assert!(dropped.await.is_err(), "the run's task was aborted");
}

#[tokio::test(start_paused = true)]
async fn quitting_waits_no_longer_than_the_run_takes_to_end() {
    let task = tokio::spawn(tokio::time::sleep(PATIENCE + Duration::from_secs(2)));
    let begun = Instant::now();
    with_task(task).wind_down(PATIENCE).await;
    assert_eq!(begun.elapsed(), PATIENCE + Duration::from_secs(2));
}

#[tokio::test]
async fn a_read_that_panics_is_told_to_the_tab_that_asked() {
    let (driver, mut updates) = driver();
    driver.read(Update::HistoryFailed, |_| panic!("the history broke"));
    let Some(Update::HistoryFailed(reason)) = updates.recv().await else { panic!("the history tab hears why") };
    assert_eq!(reason, format!("{PANICKED}: the history broke"));
}

#[tokio::test]
async fn a_run_or_load_run_that_panics_is_told_and_is_no_longer_in_progress() {
    let (mut driver, mut updates) = driver();
    let work = tokio::spawn(async { panic!("{}", "the run broke") });
    let (task, done) = watched(work, driver.updates.clone(), Update::RunFailed);
    driver.running = Some(Running { kind: Flight::Run, interrupter: interrupts().0, task, done, stopped: false });
    let Some(Update::RunFailed(reason)) = updates.recv().await else { panic!("the run tab hears why") };
    assert_eq!(reason, format!("{PANICKED}: the run broke"));
    tokio::task::yield_now().await;
    assert!(!driver.busy(), "a new run may start");
}

#[tokio::test(start_paused = true)]
async fn quitting_aborts_the_work_a_watched_run_waits_on() {
    let (held, dropped) = oneshot::channel::<()>();
    let work = tokio::spawn(async move {
        let _held = held;
        std::future::pending::<()>().await;
    });
    let (updates, _received) = mpsc::channel(1);
    let (task, _done) = watched(work, updates, Update::RunFailed);
    with_task(task).wind_down(PATIENCE).await;
    assert!(dropped.await.is_err(), "the run's own task was aborted with the wait on it");
}

#[tokio::test]
async fn a_stop_asked_on_one_tab_leaves_the_other_tabs_run_going() {
    let (mut driver, _updates) = driver();
    driver.running = Some(finishing());
    driver.carry(Ask::Stop(Flight::Load));
    assert!(driver.running.as_ref().is_some_and(|running| !running.stopped), "a load stop leaves a run be");
    driver.carry(Ask::Stop(Flight::Run));
    assert!(driver.running.as_ref().is_some_and(|running| running.stopped), "a run stop stops it");
}

/// Whether `update` ends a run.
const fn ends_a_run(update: &Update) -> bool {
    matches!(update, Update::RunFailed(_) | Update::Run(RunFeed::Done(_)))
}

#[tokio::test]
async fn a_run_is_heard_ending_once_as_it_returns() {
    let (_dir, mut driver, mut updates) = on_disk();
    driver.carry(Ask::Run(Box::default()));
    let mut heard = Vec::new();
    while !heard.iter().any(ends_a_run) {
        heard.push(updates.recv().await.unwrap());
    }
    driver.running.take().unwrap().task.await.unwrap();
    heard.extend(std::iter::from_fn(|| updates.try_recv().ok()));
    assert_eq!(heard.iter().filter(|update| ends_a_run(update)).count(), 1, "{heard:?}");
    let last = heard.last().unwrap();
    assert!(matches!(last, Update::Run(RunFeed::Done(_))), "its feed's Done, once, last: {heard:?}");
}

#[test]
fn a_run_feed_is_passed_on_but_for_its_end() {
    let passed = ongoing(RunFeed::Missed { count: 2 });
    assert!(matches!(passed, Some(Update::Run(RunFeed::Missed { count: 2 }))), "{passed:?}");
}
