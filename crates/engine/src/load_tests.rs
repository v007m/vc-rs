use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use vc_interfaces::{HistoryError, HistoryWriter, Row, RunEnd, RunId, RunStart};
use vc_loadtest::Ending;
use vc_model::{AlertTrigger, Event, Keep, RenderedRequest, RunStatus};
use vc_testkit::{FakeNotifiers, FakeTransport, FakeVcs, MemoryHistory, MemorySources, status};

use super::exit;
use crate::sample::builder;
use crate::{Engine, EngineError, ErrorKind, Interrupts, LoadEnding, LoadReport, LoadRequest, LoadWatch, interrupts};

const CONFIG: (&str, &[u8]) = (
    "vibranium.config.yaml",
    b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\n",
);
const BLOCKED: (&str, &[u8]) = (
    "vibranium.config.yaml",
    b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\nbeforeRun:\n  - call: users#broken\n",
);
const USERS: (&str, &[u8]) = (
    "services/users/users.api.yaml",
    b"baseUrl: ${var.baseUrl}\napis:\n  - id: list\n    path: /users\n  - id: broken\n    path: /broken\n",
);
const CRUD: (&str, &[u8]) = (
    "services/users/crud.test.yaml",
    b"tests:\n  - id: listsUsers\n    call: users#list\n  - id: breaksUsers\n    call: users#broken\n",
);

/// A scenario `users.peak` mixing only `test`, through `stage`, failing above half errors.
fn scenario(test: &str, stage: &str) -> Vec<u8> {
    let mix = format!("mix:\n  - {{test: users.crud#{test}, weight: 1}}\n");
    format!("{mix}stages:\n  - {stage}\nthinkTime: 1s\nthresholds: {{errorRate: {{lt: 0.5}}}}\n").into_bytes()
}

/// What the engine wrote to the history.
#[derive(Debug)]
#[expect(clippy::large_enum_variant, reason = "a test's short record of what was written")]
enum Seen {
    Start(String),
    Row(Row),
    End(RunStatus),
}

struct Written(mpsc::UnboundedSender<Seen>);

impl HistoryWriter for Written {
    fn begin(&self, start: &RunStart) -> Result<RunId, HistoryError> {
        self.0.send(Seen::Start(start.kind.clone())).ok();
        Ok(RunId::new(9))
    }

    fn record(&self, _: RunId, rows: Vec<Row>) -> Result<(), HistoryError> {
        for row in rows {
            self.0.send(Seen::Row(row)).ok();
        }
        Ok(())
    }

    fn finish(&self, _: RunId, end: RunEnd) -> Result<(), HistoryError> {
        self.0.send(Seen::End(end.status)).ok();
        Ok(())
    }

    fn prune(&self, _: Keep) -> Result<usize, HistoryError> {
        Ok(0)
    }
}

/// An engine over the users project with `config` and the load file `load`; `/broken` answers
/// 500, everything else 200.
fn engine(config: (&str, &[u8]), load: &[u8]) -> (Engine, mpsc::UnboundedReceiver<Seen>) {
    let transport = FakeTransport::new(|request: &RenderedRequest, _| {
        Ok(status(if request.uri.path() == "/broken" { 500 } else { 200 }))
    });
    let (writer, seen) = mpsc::unbounded_channel();
    let sources = MemorySources::new(&[config, USERS, CRUD, ("load/users/peak.load.yaml", load)]);
    let engine = builder(FakeVcs::default())
        .sources(Arc::new(sources))
        .transport(Arc::new(transport))
        .history(Arc::new(Written(writer)), Arc::new(MemoryHistory::new()))
        .build()
        .unwrap();
    (engine, seen)
}

async fn load(engine: &Engine, interrupts: Interrupts, ticks: Option<mpsc::Sender<Event>>) -> LoadReport {
    let request = LoadRequest { scenario: "users.peak".to_owned(), ..LoadRequest::default() };
    engine.load(&request, LoadWatch { interrupts, ticks }).await.unwrap()
}

fn written(mut seen: mpsc::UnboundedReceiver<Seen>) -> Vec<Seen> {
    std::iter::from_fn(|| seen.try_recv().ok()).collect()
}

#[tokio::test(start_paused = true)]
async fn runs_the_stages_writing_each_second_and_ticking_the_live_view() {
    let (engine, seen) = engine(CONFIG, &scenario("listsUsers", "{duration: 3s, users: 2}"));
    let (ticks, mut heard) = mpsc::channel(64);
    let (_interrupter, interrupts) = interrupts();
    let report = load(&engine, interrupts, Some(ticks)).await;
    assert_eq!((report.exit, report.ending, report.errors), (0, LoadEnding::Finished, 0), "{report:?}");
    assert!(report.iterations > 0 && report.seconds > 0 && report.rps > 0.0, "{report:?}");
    assert!(report.breaches.is_empty() && report.error_kinds.is_empty() && report.blocked.is_none());
    assert_eq!((report.run_id, report.scenario.as_str()), (Some(9), "users.peak"));
    let written = written(seen);
    assert!(matches!(written.first(), Some(Seen::Start(kind)) if kind == "load"), "{written:?}");
    assert!(matches!(written.last(), Some(Seen::End(RunStatus::Passed))), "{written:?}");
    let counted: u64 = written
        .iter()
        .filter_map(|seen| if let Seen::Row(Row::Interval(row)) = seen { Some(row) } else { None })
        .inspect(|row| assert!(row.test_id == "users.crud#listsUsers" && !row.histogram.is_empty()))
        .map(|row| row.count)
        .sum();
    assert_eq!(counted, report.iterations, "every iteration is in a stored second");
    let Ok(Event::LoadTick { users, errors, breaching, .. }) = heard.try_recv() else { panic!("a tick") };
    assert!(users <= 2 && errors == 0 && breaching.is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_failing_api_breaches_the_error_rate_and_is_counted_by_kind() {
    let (engine, seen) = engine(CONFIG, &scenario("breaksUsers", "{duration: 3s, users: 2}"));
    let (_interrupter, interrupts) = interrupts();
    let report = load(&engine, interrupts, None).await;
    assert_eq!((report.exit, report.ending), (3, LoadEnding::Finished), "{report:?}");
    assert!(report.errors > 0 && report.errors == report.iterations, "{report:?}");
    let metrics: Vec<_> = report.breaches.iter().map(|breach| (breach.metric.as_str(), breach.test.clone())).collect();
    assert_eq!(metrics, [("errorRate", None)]);
    let kind = ErrorKind { api: Some("users#broken".to_owned()), status: Some(500), count: report.errors };
    assert_eq!(report.error_kinds, [kind]);
    let text = report.text();
    assert!(text.starts_with("users.peak finished after "), "{text}");
    assert!(
        text.contains("\nbreached errorRate at 1.000\n") && text.contains("\nerrors users#broken 500 × "),
        "{text}"
    );
    let json = report.json();
    assert_eq!((json["exit"].as_u64(), json["ending"].as_str()), (Some(3), Some("finished")));
    assert_eq!(json["errorKinds"][0]["api"].as_str(), Some("users#broken"));
    assert!(matches!(written(seen).last(), Some(Seen::End(RunStatus::Failed))));
}

#[tokio::test(start_paused = true)]
async fn a_breached_threshold_sends_the_threshold_alert_naming_it() {
    let (notifiers, log) = FakeNotifiers::new(&["webhook"]);
    let alerts = "alerts:\n  - on: [threshold]\n    notify:\n      - webhook: {url: 'http://hook'}\n";
    let config = format!("{}{alerts}", String::from_utf8_lossy(CONFIG.1));
    let peak = scenario("breaksUsers", "{duration: 3s, users: 2}");
    let sources = [(CONFIG.0, config.as_bytes()), USERS, CRUD, ("load/users/peak.load.yaml", peak.as_slice())];
    let engine = builder(FakeVcs::default())
        .sources(Arc::new(MemorySources::new(&sources)))
        .transport(Arc::new(FakeTransport::new(|_: &RenderedRequest, _| Ok(status(500)))))
        .history(Arc::new(MemoryHistory::new()), Arc::new(MemoryHistory::new()))
        .notifiers(Arc::new(notifiers))
        .build()
        .unwrap();
    let (_interrupter, interrupts) = interrupts();
    let report = load(&engine, interrupts, None).await;
    assert_eq!(report.exit, 3, "{report:?}");
    let sent = log.take();
    let [sent] = sent.as_slice() else { panic!("{sent:?}") };
    assert_eq!(
        (sent.notice.trigger, sent.notice.breaches.as_slice()),
        (AlertTrigger::Threshold, ["errorRate".to_owned()].as_slice())
    );
}

#[tokio::test(start_paused = true)]
async fn an_abort_on_breach_stops_the_run_early_and_exits_3() {
    let mut file = scenario("breaksUsers", "{duration: 1m, users: 2}");
    file.extend_from_slice(b"abortOn: [errorRate]\n");
    let (engine, seen) = engine(CONFIG, &file);
    let (_interrupter, interrupts) = interrupts();
    let report = load(&engine, interrupts, None).await;
    assert_eq!((report.exit, report.ending), (3, LoadEnding::Aborted), "{report:?}");
    assert!(report.seconds < 60 && report.errors > 0, "{report:?}");
    assert!(report.text().starts_with("users.peak aborted after "), "{}", report.text());
    assert_eq!(report.json()["ending"].as_str(), Some("aborted"));
    assert!(matches!(written(seen).last(), Some(Seen::End(RunStatus::Failed))));
}

#[tokio::test(start_paused = true)]
async fn a_failed_before_run_blocks_the_users() {
    let (engine, seen) = engine(BLOCKED, &scenario("listsUsers", "{duration: 3s, users: 2}"));
    let (_interrupter, interrupts) = interrupts();
    let report = load(&engine, interrupts, None).await;
    assert_eq!((report.exit, report.ending, report.iterations), (1, LoadEnding::Blocked, 0), "{report:?}");
    let blocked = report.blocked.clone().unwrap_or_default();
    assert!(blocked.starts_with("V5106: the shared action `call users#broken` failed: "), "{blocked}");
    assert!(report.text().ends_with(&format!("\nblocked: {blocked}\n")), "{}", report.text());
    assert!(matches!(written(seen).last(), Some(Seen::End(RunStatus::Failed))));
}

#[tokio::test(start_paused = true)]
async fn ctrl_c_stops_the_users_and_exits_130() {
    let (engine, seen) = engine(CONFIG, &scenario("listsUsers", "{duration: 1m, users: 1}"));
    let (interrupter, interrupts) = interrupts();
    let press = async {
        tokio::time::sleep(Duration::from_millis(2500)).await;
        interrupter.interrupt();
    };
    let (report, ()) = tokio::join!(load(&engine, interrupts, None), press);
    assert_eq!((report.exit, report.ending), (130, LoadEnding::Cancelled), "{report:?}");
    assert!(report.seconds < 60, "{report:?}");
    assert!(matches!(written(seen).last(), Some(Seen::End(RunStatus::Cancelled))));
}

#[tokio::test]
async fn an_unknown_scenario_names_the_known_ones() {
    let (engine, _) = engine(CONFIG, &scenario("listsUsers", "{duration: 1s, users: 1}"));
    let request = LoadRequest { scenario: "users.missing".to_owned(), ..LoadRequest::default() };
    let (_interrupter, interrupts) = interrupts();
    let error = engine.load(&request, LoadWatch { interrupts, ticks: None }).await.unwrap_err();
    let EngineError::Selection { reason, help } = error else { panic!("{error:?}") };
    assert_eq!(reason, "no load scenario has the ID `users.missing`");
    assert_eq!(help.as_deref(), Some("the scenarios are: users.peak"));
}

#[test]
fn exits_3_on_a_whole_run_breach_and_130_on_ctrl_c() {
    assert_eq!(exit(Ending::Finished, false), (0, RunStatus::Passed));
    assert_eq!(exit(Ending::Aborted, false), (0, RunStatus::Passed));
    assert_eq!(exit(Ending::Finished, true), (3, RunStatus::Failed));
    assert_eq!(exit(Ending::Cancelled, true), (130, RunStatus::Cancelled));
}
