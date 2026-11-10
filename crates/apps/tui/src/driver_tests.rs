use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::{self, Receiver};
use tokio::sync::oneshot;
use tokio::time::Instant;
use vc_engine::{EngineBuilder, ImportRequest, LoadRequest, interrupts};
use vc_testkit::{FakeVcs, MemoryCompileCache, MemoryEnvironment, MemorySources};

use super::{BUSY, Driver, PANICKED, PATIENCE, Running, watched};
use crate::screen::{Ask, Update};

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
    driver.carry(curl("orders.curl")).await;
    let update = updates.recv().await;
    assert!(matches!(update, Some(Update::Drafted(_))), "{update:?}");
    driver.carry(curl("../outside.curl")).await;
    let Some(Update::ImportFailed(reason)) = updates.recv().await else { panic!("the import tab hears why") };
    assert!(reason.starts_with("../outside.curl: not a file or folder in the project at"), "{reason}");
    assert!(reason.ends_with("\nhelp: give a URL, or a path inside the project"), "{reason}");
}

/// A run in progress that never ends by itself, as a run is between its feed's `Done` and
/// the end of its task.
fn finishing() -> Running {
    Running { interrupter: interrupts().0, task: tokio::spawn(std::future::pending()) }
}

#[tokio::test]
async fn a_run_asked_for_while_another_is_finishing_is_refused_not_dropped() {
    let (mut driver, mut updates) = driver();
    driver.running = Some(finishing());
    driver.carry(Ask::Run(Box::default())).await;
    let Some(Update::RunFailed(reason)) = updates.recv().await else { panic!("the run tab hears why") };
    assert_eq!(reason, BUSY);
}

#[tokio::test]
async fn a_run_asked_for_as_the_last_one_returns_waits_for_it_rather_than_being_refused() {
    let (mut driver, mut updates) = driver();
    let task = tokio::spawn(tokio::time::sleep(Duration::from_millis(50)));
    driver.running = Some(Running { interrupter: interrupts().0, task });
    driver.carry(Ask::Run(Box::default())).await;
    let update = updates.recv().await;
    assert!(!matches!(&update, Some(Update::RunFailed(reason)) if reason == BUSY), "{update:?}");
}

#[tokio::test]
async fn a_load_run_asked_for_while_a_run_is_in_progress_is_refused() {
    let (mut driver, mut updates) = driver();
    driver.running = Some(finishing());
    driver.carry(Ask::Load(LoadRequest::default())).await;
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
    Running { interrupter: interrupts().0, task }.wind_down(PATIENCE).await;
    assert_eq!(begun.elapsed(), PATIENCE * 3, "a patience after each of three interrupts");
    assert!(dropped.await.is_err(), "the run's task was aborted");
}

#[tokio::test(start_paused = true)]
async fn quitting_waits_no_longer_than_the_run_takes_to_end() {
    let task = tokio::spawn(tokio::time::sleep(PATIENCE + Duration::from_secs(2)));
    let begun = Instant::now();
    Running { interrupter: interrupts().0, task }.wind_down(PATIENCE).await;
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
    let task = watched(work, driver.updates.clone(), Update::RunFailed);
    driver.running = Some(Running { interrupter: interrupts().0, task });
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
    let task = watched(work, updates, Update::RunFailed);
    Running { interrupter: interrupts().0, task }.wind_down(PATIENCE).await;
    assert!(dropped.await.is_err(), "the run's own task was aborted with the wait on it");
}
