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
use vc_engine::RunFeed;

use crate::engine_fixture::{SECRET, project};
use crate::envelope;
use crate::wire_events::WireFeed;

const RUN: &str = "/api/vibranium.engine.v1.EngineService/Run";

/// The whole stream `Run` answers `request` with, over the fixture project.
async fn stream(request: &wire::RunRequest) -> Vec<u8> {
    let (_dir, api) = project();
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

#[test]
fn missed_events_carry_the_last_number_seen() {
    let mut feed = WireFeed::default();
    let events = feed.events(RunFeed::Missed { count: 3 });
    let [wire::RunEvent { seq: 0, kind: Some(Kind::Missed(missed)) }] = events.as_slice() else { panic!("{events:?}") };
    assert_eq!(missed.count, 3);
}
