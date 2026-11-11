use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tokio::sync::mpsc;
use vc_interfaces::{HistoryError, HistoryWriter, Row, RunEnd, RunId, RunStart, TransportError};
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
    concat!(
        "baseUrl: ${var.baseUrl}\napis:\n  - id: list\n    path: /users\n  - id: broken\n    path: /broken\n",
        "  - id: get\n    path: /users/1\n  - id: down\n    path: /down\n",
    )
    .as_bytes(),
);
const CRUD: (&str, &[u8]) = (
    "services/users/crud.test.yaml",
    concat!(
        "tests:\n  - id: listsUsers\n    call: users#list\n  - id: breaksUsers\n    call: users#broken\n",
        "  - id: checksSecond\n    before:\n      - call: users#list\n    steps:\n      - call: users#list\n",
        "      - call: users#get\n        expect: {status: 201}\n    after:\n      - call: users#list\n",
        "  - id: reachesDown\n    call: users#down\n",
    )
    .as_bytes(),
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
/// 500, `/down` refuses the connection, everything else answers 200.
fn engine(config: (&str, &[u8]), load: &[u8]) -> (Engine, mpsc::UnboundedReceiver<Seen>) {
    let transport = FakeTransport::new(|request: &RenderedRequest, _| match request.uri.path() {
        "/down" => Err(TransportError::Connect { host: "x:80".into(), reason: "refused".into() }),
        "/broken" => Ok(status(500)),
        _ => Ok(status(200)),
    });
    engine_over(config, load, transport)
}

/// An engine over the users project as [`engine`] makes it, sending its requests to `transport`.
fn engine_over(
    config: (&str, &[u8]),
    load: &[u8],
    transport: FakeTransport,
) -> (Engine, mpsc::UnboundedReceiver<Seen>) {
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
    let kind =
        ErrorKind { api: Some("users#broken".to_owned()), status: Some(500), code: Some(v5101()), ..failed(&report) };
    assert_eq!(report.error_kinds, [kind]);
    let text = report.text();
    assert!(text.starts_with("users.peak finished after "), "{text}");
    assert!(text.contains("\nlatency p50 ") && text.contains(" ms · p90 ") && text.contains(" ms · p95 "), "{text}");
    assert!(
        text.contains("\nbreached errorRate at 100% (lt 50%)\n") && text.contains("\nerrors users#broken 500 V5101 × "),
        "{text}"
    );
    let json = report.json();
    assert_eq!((json["exit"].as_u64(), json["ending"].as_str()), (Some(3), Some("finished")));
    let breach = json!({"metric": "errorRate", "test": null, "actual": 1.0, "unit": "share", "limit": {"lt": 0.5}});
    assert_eq!(json["breaches"], json!([breach]));
    assert!(json["p90Ms"].is_number(), "{json:#}");
    let kind = json!({"api": "users#broken", "test": null, "status": 500, "code": "V5101", "count": report.errors});
    assert_eq!(json["errorKinds"], json!([kind]));
    assert!(matches!(written(seen).last(), Some(Seen::End(RunStatus::Failed))));
}

#[tokio::test(start_paused = true)]
async fn a_breach_s_json_actual_is_the_measured_value_not_rounded() {
    let every_third = FakeTransport::new(|_: &RenderedRequest, call| Ok(status(if call % 3 == 0 { 500 } else { 200 })));
    let file = scenario("listsUsers", "{duration: 3s, users: 3}").replace("lt: 0.5", "lt: 0.01");
    let (engine, _seen) = engine_over(CONFIG, file.as_bytes(), every_third);
    let (_interrupter, interrupts) = interrupts();
    let report = load(&engine, interrupts, None).await;
    let measured = crate::load_report::rate(report.errors, report.iterations);
    assert!(report.errors > 0 && report.errors < report.iterations, "{report:?}");
    assert_eq!(report.json()["breaches"][0]["actual"].as_f64(), Some(measured), "{report:?}");
    assert!(report.text().contains("\nbreached errorRate at "), "{}", report.text());
}

/// No error kind yet: `report`'s errors, all of one kind.
fn failed(report: &LoadReport) -> ErrorKind {
    ErrorKind { count: report.errors, ..ErrorKind::default() }
}

fn v5101() -> String {
    "V5101".to_owned()
}

#[tokio::test(start_paused = true)]
async fn a_failed_check_on_a_tests_second_call_is_counted_under_that_call_not_the_ones_around_it() {
    let mut file = scenario("checksSecond", "{duration: 3s, users: 2}");
    file.extend_from_slice(b"checks: full\n");
    let (engine, _) = engine(CONFIG, &file);
    let (_interrupter, interrupts) = interrupts();
    let report = load(&engine, interrupts, None).await;
    assert!(report.errors > 0 && report.errors == report.iterations, "{report:?}");
    let kind =
        ErrorKind { api: Some("users#get".to_owned()), status: Some(200), code: Some(v5101()), ..failed(&report) };
    assert_eq!(report.error_kinds, [kind], "not its `before` call nor its `after` one");
    assert!(report.text().contains("\nerrors users#get 200 V5101 × "), "{}", report.text());
}

#[tokio::test(start_paused = true)]
async fn an_api_that_refuses_the_connection_is_counted_under_it_and_the_transport_code() {
    let (engine, _) = engine(CONFIG, &scenario("reachesDown", "{duration: 3s, users: 2}"));
    let (_interrupter, interrupts) = interrupts();
    let report = load(&engine, interrupts, None).await;
    assert!(report.errors > 0, "{report:?}");
    let code = Some("V5105".to_owned());
    let kind = ErrorKind { api: Some("users#down".to_owned()), status: None, code, ..failed(&report) };
    assert_eq!(report.error_kinds, [kind]);
    let text = report.text();
    assert!(text.contains("\nerrors users#down V5105 × ") && !text.contains("inline request"), "{text}");
}

#[tokio::test(start_paused = true)]
async fn the_history_lists_a_load_run_by_its_iterations() {
    let history = Arc::new(MemoryHistory::new());
    let peak = scenario("breaksUsers", "{duration: 3s, users: 2}");
    let sources = MemorySources::new(&[CONFIG, USERS, CRUD, ("load/users/peak.load.yaml", peak.as_slice())]);
    let engine = builder(FakeVcs::default())
        .sources(Arc::new(sources))
        .transport(Arc::new(FakeTransport::new(|_: &RenderedRequest, _| Ok(status(500)))))
        .history(history.clone(), history)
        .build()
        .unwrap();
    let (_interrupter, interrupts) = interrupts();
    let report = load(&engine, interrupts, None).await;
    let listed = engine.history(10).unwrap();
    let text = listed.text();
    let line = format!(
        "failed     load · {} iterations · {} errors  profile dev  users.peak",
        report.iterations, report.errors
    );
    assert!(report.iterations > 0 && text.contains(&line), "{text}");
    let json = listed.json();
    assert_eq!((json[0]["kind"].as_str(), json[0]["totals"]["failed"].as_u64()), (Some("load"), Some(report.errors)));
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
    let kinds: u64 = report.error_kinds.iter().map(|kind| kind.count).sum();
    assert_eq!(kinds, report.errors, "an iteration the abort drops is in no kind: {report:?}");
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

#[tokio::test(start_paused = true)]
async fn ctrl_c_counts_no_kind_for_an_iteration_it_drops() {
    let (engine, _) = engine(CONFIG, &scenario("breaksUsers", "{duration: 1m, users: 2}"));
    let (interrupter, interrupts) = interrupts();
    let press = async {
        tokio::time::sleep(Duration::from_millis(2500)).await;
        interrupter.interrupt();
    };
    let (report, ()) = tokio::join!(load(&engine, interrupts, None), press);
    assert_eq!((report.ending, report.errors > 0), (LoadEnding::Cancelled, true), "{report:?}");
    let kinds: u64 = report.error_kinds.iter().map(|kind| kind.count).sum();
    assert_eq!(kinds, report.errors, "{report:?}");
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
