use std::fs;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, header};
use http_body_util::BodyExt as _;
use prost::Message as _;
use serde_json::{Value, json};
use tower::ServiceExt as _;
use vc_contracts::engine::v1 as wire;
use vc_contracts::engine::v1::run_load_response::Kind;

use crate::EngineApi;
use crate::engine_fixture::{CONFIG, SECRET, project, serving};
use crate::envelope;

const RUN_LOAD: &str = "/api/vibranium.engine.v1.EngineService/RunLoad";
const SCENARIO: &str = "load/users/peak.load.yaml";
const PEAK: &str = "mix:\n  - {test: users.crud#listsUsers, weight: 1}\nstages:\n  - {duration: 0s, users: 1}\n  - {duration: 1s, users: 1}\nthinkTime: 100ms\nthresholds: {errorRate: {lt: 0.5}}\n";

/// The fixture project with the scenario `users.peak`, its api answered by a local server.
fn loaded() -> (tempfile::TempDir, Arc<EngineApi>) {
    let (dir, api) = project();
    let config = CONFIG.replace("http://127.0.0.1:9", &serving(200, b"[]".to_vec()));
    fs::write(dir.path().join("vibranium.config.yaml"), config).unwrap();
    fs::create_dir_all(dir.path().join("load/users")).unwrap();
    fs::write(dir.path().join(SCENARIO), PEAK).unwrap();
    (dir, api)
}

/// The whole stream `RunLoad` answers `request` with, over `api`.
async fn stream(api: Arc<EngineApi>, request: &wire::RunLoadRequest) -> Vec<u8> {
    let router = Arc::into_inner(api).unwrap().into_service().into_router();
    let request = Request::builder()
        .method(Method::POST)
        .uri(RUN_LOAD)
        .header(header::CONTENT_TYPE, "application/connect+proto")
        .body(Body::from(envelope::message(&request.encode_to_vec())))
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
    response.into_body().collect().await.unwrap().to_bytes().to_vec()
}

/// The stream's messages, then its end-stream JSON.
fn parse(mut body: &[u8]) -> (Vec<Kind>, Value) {
    let mut kinds = Vec::new();
    while let [flags, a, b, c, d, rest @ ..] = body {
        let length = usize::try_from(u32::from_be_bytes([*a, *b, *c, *d])).unwrap();
        let (payload, next) = rest.split_at(length);
        if *flags == 2 {
            assert!(next.is_empty(), "frames after the end-stream");
            return (kinds, serde_json::from_slice(payload).unwrap());
        }
        kinds.extend(wire::RunLoadResponse::decode(payload).unwrap().kind);
        body = next;
    }
    panic!("the stream ended without its end-stream frame");
}

fn peak() -> wire::RunLoadRequest {
    wire::RunLoadRequest { scenario: "users.peak".into(), seed: Some(7), ..wire::RunLoadRequest::default() }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_load_run_streams_its_ticks_then_its_report() {
    let (_dir, api) = loaded();
    let body = stream(api, &peak()).await;
    assert!(!body.windows(SECRET.len()).any(|found| found == SECRET.as_bytes()));
    let (kinds, end) = parse(&body);
    assert_eq!(end, json!({}));
    let [ticks @ .., Kind::Report(report)] = kinds.as_slice() else { panic!("{kinds:?}") };
    assert!(!ticks.is_empty() && ticks.iter().all(|kind| matches!(kind, Kind::Tick(_))), "{kinds:?}");
    let Kind::Tick(first) = &ticks[0] else { panic!("{ticks:?}") };
    assert_eq!(first.second, 1, "{first:?}");
    assert!(first.host.is_some(), "{first:?}");
    assert_eq!((report.scenario.as_str(), report.ending(), report.exit), ("users.peak", wire::LoadEnding::Finished, 0));
    assert_eq!((report.seed, report.errors), (7, 0), "{report:?}");
    assert!(report.iterations > 0 && report.run.is_some(), "{report:?}");
    let [test] = report.tests.as_slice() else { panic!("{report:?}") };
    assert_eq!(test.test, "users.crud#listsUsers");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_scenario_that_is_not_there_ends_the_stream_with_an_error_and_nothing_sent() {
    let (_dir, api) = loaded();
    let request = wire::RunLoadRequest { scenario: "users.none".into(), ..wire::RunLoadRequest::default() };
    let (kinds, end) = parse(&stream(api, &request).await);
    assert!(kinds.is_empty(), "{kinds:?}");
    assert!(end["error"]["code"].is_string(), "{end}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_handle_that_is_not_a_name_is_refused_before_the_run_starts() {
    let (_dir, api) = loaded();
    let request = wire::RunLoadRequest { handle: "../peak".into(), ..peak() };
    let (kinds, end) = parse(&stream(api, &request).await);
    assert!(kinds.is_empty(), "{kinds:?}");
    assert_eq!(end["error"]["code"], "invalid_argument", "{end}");
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_load_run_whose_reports_dir_set_leaves_the_project_is_refused_before_it_takes_its_handle() {
    let outside = tempfile::tempdir().unwrap();
    let absolute = outside.path().to_string_lossy().into_owned();
    for dir in [absolute.as_str(), "../elsewhere", "linked/ci"] {
        let (root, api) = loaded();
        std::os::unix::fs::symlink(outside.path(), root.path().join("linked")).unwrap();
        let sets = vec![format!("reports.dir={dir}")];
        let request = wire::RunLoadRequest { sets, handle: "../peak".into(), ..peak() };
        let (kinds, end) = parse(&stream(api, &request).await);
        assert!(kinds.is_empty(), "{dir}: {kinds:?}");
        assert_eq!(end["error"]["code"], "invalid_argument", "{dir}: {end}");
        let message = end["error"]["message"].as_str().unwrap();
        assert!(message.starts_with("V5132") && !message.contains(&absolute), "{dir}: {message}");
    }
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0, "nothing is written outside");
}
