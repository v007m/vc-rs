use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, header};
use http_body_util::BodyExt as _;
use prost::Message as _;
use serde_json::{Value, json};
use tower::ServiceExt as _;
use vc_contracts::engine::v1 as wire;
use vc_contracts::engine::v1::run_event::Kind;
use vc_contracts::results::v1 as results;
use vc_engine::{Event, RunFeed, RunStatus, Sequenced, Totals};

use crate::EngineApi;
use crate::engine_fixture::{CONFIG, SECRET, large_body_server, project};
use crate::envelope;
use crate::wire_events::WireFeed;

const RUN: &str = "/api/vibranium.engine.v1.EngineService/Run";

/// The whole stream `Run` answers `request` with, over the fixture project.
async fn stream(request: &wire::RunRequest) -> Vec<u8> {
    let (_dir, api) = project();
    stream_from(api, request).await
}

/// The whole stream `Run` answers `request` with, over `api`.
async fn stream_from(api: Arc<EngineApi>, request: &wire::RunRequest) -> Vec<u8> {
    let router = Arc::into_inner(api).unwrap().into_service().into_router();
    let request = Request::builder()
        .method(Method::POST)
        .uri(RUN)
        .header(header::CONTENT_TYPE, "application/connect+proto")
        .body(Body::from(envelope::message(&request.encode_to_vec())))
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
    response.into_body().collect().await.unwrap().to_bytes().to_vec()
}

/// The stream's events, then its end-stream JSON.
fn parse(mut body: &[u8]) -> (Vec<wire::RunEvent>, Value) {
    let mut events = Vec::new();
    while let [flags, a, b, c, d, rest @ ..] = body {
        let length = usize::try_from(u32::from_be_bytes([*a, *b, *c, *d])).unwrap();
        let (payload, next) = rest.split_at(length);
        if *flags == 2 {
            assert!(next.is_empty(), "frames after the end-stream");
            return (events, serde_json::from_slice(payload).unwrap());
        }
        events.extend(wire::RunResponse::decode(payload).unwrap().event);
        body = next;
    }
    panic!("the stream ended without its end-stream frame");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_streams_its_start_and_plan_first_and_its_report_last() {
    let body = stream(&wire::RunRequest::default()).await;
    assert!(!body.windows(SECRET.len()).any(|found| found == SECRET.as_bytes()));
    let (events, end) = parse(&body);
    assert_eq!(end, json!({}));
    let kinds: Vec<&Kind> = events.iter().filter_map(|event| event.kind.as_ref()).collect();
    let [Kind::RunStarted(started), Kind::RunPlanned(planned), .., Kind::Report(report)] = kinds.as_slice() else {
        panic!("{events:?}")
    };
    assert_eq!((started.files, started.instances, report.exit), (1, 1, 1));
    assert!(started.started_ms > 0);
    let [instance] = planned.instances.as_slice() else { panic!("{planned:?}") };
    assert_eq!((instance.test.as_str(), instance.label.as_str()), ("users.crud#listsUsers", "users.crud#listsUsers"));
    assert!(kinds.iter().any(|kind| matches!(kind, Kind::RunFinished(_))));
    let finished = |kind: &&Kind| if let Kind::TestFinished(found) = kind { found.test.clone() } else { None };
    let test = kinds.iter().find_map(finished).unwrap();
    assert_eq!((test.id.as_str(), test.status()), ("users.crud#listsUsers", results::TestStatus::Failed));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_finished_test_carries_how_long_it_ran() {
    let (events, _) = parse(&stream(&wire::RunRequest::default()).await);
    let finished = |event: &wire::RunEvent| match &event.kind {
        Some(Kind::TestFinished(found)) => found.test.clone(),
        _ => None,
    };
    let test = events.iter().find_map(finished).unwrap();
    assert!(test.duration_ms > 0.0, "{test:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_finished_test_names_its_instance_in_the_plan() {
    let (events, _) = parse(&stream(&wire::RunRequest::default()).await);
    let planned = events.iter().find_map(|event| match &event.kind {
        Some(Kind::RunPlanned(plan)) => plan.instances.first().map(|instance| instance.instance),
        _ => None,
    });
    let finished = events.iter().find_map(|event| match &event.kind {
        Some(Kind::TestFinished(found)) => Some(found.instance),
        _ => None,
    });
    assert_eq!(finished, Some(planned), "{events:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn sequence_numbers_rise_with_the_plan_beside_the_start_and_the_report_after_the_last() {
    let (events, _) = parse(&stream(&wire::RunRequest::default()).await);
    let seqs: Vec<u64> = events.iter().map(|event| event.seq).collect();
    let [start, plan, .., last, report] = seqs.as_slice() else { panic!("{seqs:?}") };
    assert_eq!((*start, *plan, *report), (1, 1, last + 1), "{seqs:?}");
    assert!(seqs[1..].windows(2).all(|pair| pair[0] < pair[1]), "{seqs:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_selection_that_matches_nothing_ends_the_stream_with_an_invalid_argument() {
    let selection = wire::Selection { ids: vec!["users.crud#listsNobody".into()], ..wire::Selection::default() };
    let request = wire::RunRequest { selection: Some(selection), ..wire::RunRequest::default() };
    let (events, end) = parse(&stream(&request).await);
    assert!(events.is_empty(), "{events:?}");
    assert_eq!(end["error"]["code"], "invalid_argument", "{end}");
}

/// The report of a run of the fixture project.
async fn a_report() -> vc_engine::RunReport {
    let (_dir, api) = project();
    let (_interrupter, interrupts) = vc_engine::interrupts();
    api.engine().run(&vc_engine::RunRequest::default(), interrupts).await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missed_notice_takes_the_last_dropped_number_and_the_report_the_one_after_the_highest() {
    let mut feed = WireFeed::default();
    let finished = |seq| {
        RunFeed::Event(Sequenced {
            seq,
            event: Event::RunFinished { status: RunStatus::Passed, totals: Totals::default() },
        })
    };
    let mut sent = Vec::new();
    for item in [
        finished(3),
        RunFeed::Missed { count: 2 },
        RunFeed::Missed { count: 1 },
        finished(7),
        RunFeed::Missed { count: 4 },
        RunFeed::Done(a_report().await),
    ] {
        sent.extend(feed.events(item).into_iter().map(|event| (event.seq, event.kind)));
    }
    let synthetic = |kind: &Option<Kind>| matches!(kind, Some(Kind::Missed(_) | Kind::Report(_)));
    let seqs: Vec<u64> = sent.iter().filter(|(_, kind)| synthetic(kind)).map(|(seq, _)| *seq).collect();
    assert_eq!(seqs, [5, 6, 11, 12], "the last of 4–5, 6 and 8–11 dropped; then the one after 11");
    let real: Vec<u64> = sent.iter().filter(|(_, kind)| !synthetic(kind)).map(|(seq, _)| *seq).collect();
    assert!(seqs.iter().all(|seq| !real.contains(seq)), "no event sent shares a synthetic number: {real:?}");
}

/// The first exchange the feed sends when the fixture's api answers `status` with a large body
/// and the config keeps `history.bodies: <bodies>`.
async fn first_exchange(status: u16, bodies: &str) -> results::Exchange {
    let (dir, api) = project();
    let config = CONFIG.replace("http://127.0.0.1:9", &large_body_server(status));
    std::fs::write(dir.path().join("vibranium.config.yaml"), format!("{config}history:\n  bodies: {bodies}\n"))
        .unwrap();
    let (events, _) = parse(&stream_from(api, &wire::RunRequest::default()).await);
    let exchange = events.iter().find_map(|event| match &event.kind {
        Some(Kind::Exchanged(exchanged)) => exchanged.exchange.clone(),
        _ => None,
    });
    exchange.unwrap_or_else(|| panic!("no exchange: {events:?}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn with_every_body_kept_an_exchange_body_past_the_history_cap_is_cut_and_says_so() {
    let exchange = first_exchange(200, "all").await;
    assert_eq!((exchange.response_body.len(), exchange.body_truncated), (64 * 1024, true));
    assert_eq!(exchange.request_body, b"");
}

#[tokio::test(flavor = "multi_thread")]
async fn with_no_bodies_kept_the_feed_sends_none() {
    let exchange = first_exchange(500, "none").await;
    let sent = (exchange.status, exchange.response_body.len(), exchange.body_truncated);
    assert_eq!(sent, (Some(500), 0, false), "left empty, as the history stores it");
}

#[tokio::test(flavor = "multi_thread")]
async fn with_failure_bodies_kept_the_feed_sends_none_even_for_a_failing_exchange() {
    let exchange = first_exchange(500, "failures").await;
    let sent = (exchange.status, exchange.response_body.len(), exchange.body_truncated);
    assert_eq!(sent, (Some(500), 0, false), "the stored run holds them once the test has failed");
}

/// The report a run streams last when the fixture's config has `reports: <reports>`.
async fn streamed_report(reports: &str) -> (tempfile::TempDir, wire::RunReport) {
    let (dir, api) = project();
    std::fs::write(dir.path().join("vibranium.config.yaml"), format!("{CONFIG}reports: {reports}\n")).unwrap();
    let (events, _) = parse(&stream_from(api, &wire::RunRequest::default()).await);
    let Some(Kind::Report(report)) = events.last().and_then(|event| event.kind.clone()) else { panic!("{events:?}") };
    (dir, report)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_report_names_the_html_file_the_run_wrote_as_text() {
    let (dir, report) = streamed_report("{formats: [ctrf, html]}").await;
    let Some(path) = report.html_report.clone() else { panic!("{report:?}") };
    let run = report.run.unwrap();
    let expected: std::path::PathBuf = [".vibranium", "reports", &run.to_string(), "index.html"].iter().collect();
    assert_eq!(std::path::Path::new(&path), expected, "relative to the project root, never where it is");
    assert!(!path.starts_with(['/', '\\']) && path.chars().nth(1) != Some(':'), "no root or drive: {path}");
    assert!(dir.path().join(&path).is_file(), "the run's own index.html, which is there: {path}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_report_names_no_html_file_written_outside_the_project() {
    let out = tempfile::tempdir().unwrap();
    let reports = format!("{{formats: [html], dir: '{}'}}", out.path().display());
    let (_dir, report) = streamed_report(&reports).await;
    assert_eq!(report.html_report, None, "a path beyond the project is never sent");
    assert!(std::fs::read_dir(out.path()).unwrap().next().is_some(), "the report is still written there");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_report_names_no_html_file_when_the_run_wrote_none() {
    let (_dir, report) = streamed_report("{formats: [ctrf, junit]}").await;
    assert_eq!((report.html_report, report.run.is_some()), (None, true), "the other files are not named");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_report_names_no_html_file_written_up_and_out_of_the_project() {
    let out = tempfile::tempdir().unwrap();
    let sibling = out.path().file_name().unwrap().to_string_lossy();
    let (_dir, report) = streamed_report(&format!("{{formats: [html], dir: '../{sibling}'}}")).await;
    assert_eq!(report.html_report, None, "`../` leaves the project, so the path is never sent");
    assert!(std::fs::read_dir(out.path()).unwrap().next().is_some(), "the report is still written there");
}
