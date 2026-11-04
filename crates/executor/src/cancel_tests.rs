use std::cell::Cell;
use std::time::Duration;

use tokio::time;
use tokio::time::Instant;

use super::{CancelTree, Ran};

const GRACE: Duration = Duration::from_secs(5);

const fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

async fn slow(value: u32, after: Duration) -> u32 {
    time::sleep(after).await;
    value
}

async fn later(after: Duration, action: impl FnOnce()) {
    time::sleep(after).await;
    action();
}

#[tokio::test(start_paused = true)]
async fn uncancelled_work_finishes() {
    assert_eq!(CancelTree::new(GRACE).guard(slow(7, ms(10_000))).await, Ran::Done(7));
}

#[tokio::test(start_paused = true)]
async fn work_in_flight_gets_the_grace_period_to_finish() {
    let tree = CancelTree::new(GRACE);
    let (ran, ()) = tokio::join!(tree.guard(slow(7, ms(1_000))), later(ms(100), || tree.cancel()));
    assert_eq!(ran, Ran::Done(7));
}

#[tokio::test(start_paused = true)]
async fn work_still_running_when_the_grace_period_ends_is_cut() {
    let tree = CancelTree::new(ms(1_000));
    let start = Instant::now();
    let (ran, ()) = tokio::join!(tree.guard(slow(7, ms(60_000))), later(ms(100), || tree.cancel()));
    assert_eq!((ran, start.elapsed()), (Ran::Cut, ms(1_100)));
}

#[tokio::test(start_paused = true)]
async fn skipping_the_grace_cuts_work_at_once() {
    let tree = CancelTree::new(GRACE);
    let start = Instant::now();
    let interrupts = async {
        later(ms(100), || tree.cancel()).await;
        later(ms(100), || tree.skip_grace()).await;
    };
    let (ran, ()) = tokio::join!(tree.guard(slow(7, ms(60_000))), interrupts);
    assert_eq!((ran, start.elapsed()), (Ran::Cut, ms(200)));
}

#[tokio::test(start_paused = true)]
async fn an_abort_stops_work_and_cleanups() {
    let tree = CancelTree::new(GRACE);
    let (ran, ()) = tokio::join!(tree.guard(slow(7, ms(60_000))), later(ms(100), || tree.abort()));
    assert_eq!(ran, Ran::Aborted);
    let child = tree.child();
    let (ran, ()) = tokio::join!(child.cleanup(slow(7, ms(60_000))), async {});
    assert_eq!(ran, Ran::Aborted, "abort is run-wide");
    assert_eq!(tree.guard(slow(7, ms(10))).await, Ran::Aborted);
    assert_eq!(CancelTree::new(GRACE).cleanup(slow(7, ms(10))).await, Ran::Done(7));
}

#[tokio::test(start_paused = true)]
async fn an_abort_during_the_grace_period_stops_work() {
    let tree = CancelTree::new(GRACE);
    let interrupts = async {
        later(ms(100), || tree.cancel()).await;
        later(ms(100), || tree.abort()).await;
    };
    let (ran, ()) = tokio::join!(tree.guard(slow(7, ms(60_000))), interrupts);
    assert_eq!(ran, Ran::Aborted);
}

#[tokio::test(start_paused = true)]
async fn work_does_not_start_in_a_cancelled_frame() {
    let tree = CancelTree::new(GRACE);
    tree.cancel();
    let started = Cell::new(false);
    assert_eq!(tree.guard(async { started.set(true) }).await, Ran::Cut);
    assert!(!started.get());
    assert!(tree.is_cancelled());
}

#[tokio::test(start_paused = true)]
async fn cleanups_run_in_a_cancelled_frame() {
    let tree = CancelTree::new(GRACE);
    tree.cancel();
    tree.skip_grace();
    let start = Instant::now();
    assert_eq!(tree.cleanup(slow(3, ms(60_000))).await, Ran::Done(3));
    assert_eq!(start.elapsed(), ms(60_000));
}

#[tokio::test(start_paused = true)]
async fn cancelling_a_frame_cancels_its_children_only() {
    let run = CancelTree::new(GRACE);
    let (file, other) = (run.child(), run.child());
    let instance = file.child();
    file.cancel();
    assert_eq!((run.is_cancelled(), file.is_cancelled(), instance.is_cancelled()), (false, true, true));
    assert!(!other.is_cancelled());
    run.cancel();
    other.cancelled().await;
    run.abort();
    instance.aborted().await;
}
