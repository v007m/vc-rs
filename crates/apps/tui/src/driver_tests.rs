use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc::{self, Receiver};
use vc_engine::{EngineBuilder, LoadRequest, interrupts};
use vc_testkit::MemorySources;

use super::{BUSY, Driver, Running};
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
