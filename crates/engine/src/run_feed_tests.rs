use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::{self, Receiver};
use vc_interfaces::HistoryReader as _;
use vc_testkit::{FakeTransport, FakeVcs, MemoryHistory, status};

use super::{RunFeed, RunWatch};
use crate::sample::builder;
use crate::{Engine, RunReport, RunRequest, interrupts};

fn project() -> (Engine, Arc<MemoryHistory>) {
    let history = Arc::new(MemoryHistory::new());
    let engine = builder(FakeVcs::default())
        .transport(Arc::new(FakeTransport::new(|_, _| Ok(status(200)))))
        .history(history.clone(), history.clone())
        .build()
        .unwrap();
    (engine, history)
}

fn watched(room: usize) -> (RunRequest, Receiver<RunFeed>) {
    let (sender, receiver) = mpsc::channel(room);
    (RunRequest { watch: Some(RunWatch::new(sender)), ..RunRequest::default() }, receiver)
}

/// Runs `request` to its end, failing the test if the run is held up.
async fn run(engine: &Engine, request: &RunRequest) -> RunReport {
    let ran = tokio::time::timeout(Duration::from_secs(10), engine.run(request, interrupts().1)).await;
    ran.expect("the run is never held up by its watcher").unwrap()
}

#[tokio::test]
async fn a_watcher_gets_every_event_in_order_then_done_once_the_history_is_written() {
    let (engine, history) = project();
    let (request, mut feed) = watched(1_024);
    let watcher = async {
        let mut seen = Vec::new();
        while let Some(next) = feed.recv().await {
            if let RunFeed::Done(report) = next {
                let finished = history.runs(1).unwrap().first().and_then(|row| row.finished_ms);
                return (seen, report, finished);
            }
            seen.push(next);
        }
        panic!("the feed ended without Done after {seen:?}");
    };
    let (report, (seen, done, finished)) = tokio::join!(run(&engine, &request), watcher);
    assert_eq!(done, report);
    assert_eq!(report.exit, 0);
    assert!(finished.is_some(), "Done comes after the reporters have finished the history");
    let seqs: Vec<u64> =
        seen.iter().filter_map(|next| if let RunFeed::Event(event) = next { Some(event.seq) } else { None }).collect();
    assert_eq!(seqs.len(), seen.len(), "nothing missed: {seen:?}");
    assert!(seqs.len() > 3 && seqs[0] == 1, "{seen:?}");
    assert!(seqs.windows(2).all(|pair| pair[1] == pair[0] + 1), "every event, in order: {seqs:?}");
}

#[tokio::test]
async fn a_full_or_closed_watch_never_holds_up_or_fails_the_run() {
    let (engine, _) = project();
    let (request, mut feed) = watched(1);
    let report = run(&engine, &request).await;
    assert_eq!(report.exit, 0);
    let Some(RunFeed::Event(first)) = feed.recv().await else { panic!("the first event is kept") };
    assert_eq!(first.seq, 1);
    let Some(RunFeed::Missed { count }) = feed.recv().await else { panic!("then what the full channel dropped") };
    assert!(count > 0);
    assert_eq!(feed.recv().await, Some(RunFeed::Done(report)));

    let (request, feed) = watched(1);
    drop(feed);
    assert_eq!(run(&engine, &request).await.exit, 0);
}

#[test]
fn a_watched_run_prints_nothing_since_its_watcher_shows_it() {
    let (watched, _feed) = watched(1);
    let agent = RunRequest { agent: Some("claude".into()), ..RunRequest::default() };
    assert_eq!((RunRequest::default().quiet(), watched.quiet(), agent.quiet()), (false, true, true));
}

#[test]
fn watches_are_equal_when_they_send_into_the_same_channel() {
    let (sender, _receiver) = mpsc::channel(1);
    let watch = RunWatch::new(sender);
    assert_eq!(watch.clone(), watch);
    assert_ne!(watch, RunWatch::new(mpsc::channel(1).0));
}
