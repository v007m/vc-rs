use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::{self, Receiver};
use tokio::sync::oneshot;
use tokio::time::Instant;
use vc_engine::{EngineBuilder, LoadRequest, interrupts};
use vc_testkit::MemorySources;

use super::{BUSY, Driver, PATIENCE, Running};
use crate::screen::{Ask, Update};

/// A driver over an empty project, and where its updates come out.
fn driver() -> (Driver, Receiver<Update>) {
    let engine = EngineBuilder::new(PathBuf::from("/project")).sources(Arc::new(MemorySources::new(&[]))).build();
    let (updates, received) = mpsc::channel(8);
    (Driver::new(engine.unwrap(), updates), received)
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
    driver.carry(Ask::Run(Box::default()));
    let Some(Update::RunFailed(reason)) = updates.recv().await else { panic!("the run tab hears why") };
    assert_eq!(reason, BUSY);
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
