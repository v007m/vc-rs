use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt as _;
use prost::Message as _;
use serde_json::Value;
use tower::ServiceExt as _;
use vc_contracts::engine::v1 as wire;
use vc_contracts::engine::v1::run_event::Kind;

use super::{Runs, cancel};
use crate::engine_fixture::{CONFIG, project};
use crate::{Code, envelope};

const SERVICE: &str = "/api/vibranium.engine.v1.EngineService";

/// A server on a free local port that takes every connection, says so, and never answers;
/// its base URL, and where it says so.
fn hanging_server() -> (String, Receiver<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (tell, told) = mpsc::channel();
    thread::spawn(move || {
        let mut held = Vec::new();
        for socket in listener.incoming().flatten() {
            held.push(socket);
            drop(tell.send(()));
        }
    });
    (url, told)
}

/// The service over the fixture project, its api on a server that never answers, and where
/// that server says a request has reached it.
fn hanging() -> (tempfile::TempDir, Router, Receiver<()>) {
    let (dir, api) = project();
    let (url, reached) = hanging_server();
    std::fs::write(dir.path().join("vibranium.config.yaml"), CONFIG.replace("http://127.0.0.1:9", &url)).unwrap();
    (dir, std::sync::Arc::into_inner(api).unwrap().into_service().into_router(), reached)
}

/// Waits until a run's request has reached the hanging server, so the run is in flight.
async fn in_flight(reached: &Receiver<()>) {
    for _ in 0..400 {
        if reached.try_recv().is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no request reached the server");
}

/// `Run` with `handle`, to its end: its events and its end-stream JSON.
async fn run(router: Router, handle: &str) -> (Vec<wire::RunEvent>, Value) {
    let request = wire::RunRequest { handle: handle.to_owned(), ..wire::RunRequest::default() };
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("{SERVICE}/Run"))
        .header(header::CONTENT_TYPE, "application/connect+proto")
        .body(Body::from(envelope::message(&request.encode_to_vec())))
        .unwrap();
    let body = router.oneshot(request).await.unwrap().into_body().collect().await.unwrap().to_bytes();
    parse(&body)
}

/// A stream's events, then its end-stream JSON.
fn parse(mut body: &[u8]) -> (Vec<wire::RunEvent>, Value) {
    let mut events = Vec::new();
    while let [flags, a, b, c, d, rest @ ..] = body {
        let (payload, next) = rest.split_at(usize::try_from(u32::from_be_bytes([*a, *b, *c, *d])).unwrap());
        if *flags == 2 {
            return (events, serde_json::from_slice(payload).unwrap());
        }
        events.extend(wire::RunResponse::decode(payload).unwrap().event);
        body = next;
    }
    panic!("the stream ended without its end-stream frame");
}

/// `Cancel` of `handle` at `level`: its HTTP status.
async fn cancelled(router: &Router, handle: &str, level: wire::CancelLevel) -> StatusCode {
    let request = wire::CancelRequest { run: handle.to_owned(), level: level.into() };
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("{SERVICE}/Cancel"))
        .header(header::CONTENT_TYPE, "application/proto")
        .body(Body::from(request.encode_to_vec()))
        .unwrap();
    router.clone().oneshot(request).await.unwrap().status()
}

/// The exit code of the report a run's stream ended with.
fn exit(events: &[wire::RunEvent]) -> u32 {
    let Some(Kind::Report(report)) = events.last().and_then(|event| event.kind.clone()) else { panic!("{events:?}") };
    report.exit
}

#[tokio::test(flavor = "multi_thread")]
async fn an_abandon_ends_a_hung_run_at_once_as_cancelled() {
    let (_dir, router, reached) = hanging();
    let running = tokio::spawn(run(router.clone(), "run-1"));
    in_flight(&reached).await;
    assert_eq!(cancelled(&router, "run-1", wire::CancelLevel::Abandon).await, StatusCode::OK);
    let (events, end) = tokio::time::timeout(Duration::from_secs(4), running).await.unwrap().unwrap();
    assert_eq!((exit(&events), end), (130, serde_json::json!({})), "inside the 5 s grace period");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_stop_ends_the_grace_period_as_a_second_ctrl_c() {
    let (_dir, router, reached) = hanging();
    let running = tokio::spawn(run(router.clone(), "run-2"));
    in_flight(&reached).await;
    assert_eq!(cancelled(&router, "run-2", wire::CancelLevel::Stop).await, StatusCode::OK);
    assert_eq!(cancelled(&router, "run-2", wire::CancelLevel::Stop).await, StatusCode::OK);
    let (events, _) = tokio::time::timeout(Duration::from_secs(4), running).await.unwrap().unwrap();
    assert_eq!(exit(&events), 130);
    assert_eq!(cancelled(&router, "run-2", wire::CancelLevel::Stop).await, StatusCode::NOT_FOUND, "forgotten");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_handle_a_running_run_holds_is_refused_and_a_finished_one_is_free_again() {
    let (_dir, router, reached) = hanging();
    let running = tokio::spawn(run(router.clone(), "twice"));
    in_flight(&reached).await;
    let (events, end) = run(router.clone(), "twice").await;
    assert!(events.is_empty(), "{events:?}");
    assert_eq!(end["error"]["code"], "already_exists", "{end}");
    assert_eq!(cancelled(&router, "twice", wire::CancelLevel::Abandon).await, StatusCode::OK);
    drop(tokio::time::timeout(Duration::from_secs(4), running).await.unwrap().unwrap());
    let again = tokio::spawn(run(router.clone(), "twice"));
    in_flight(&reached).await;
    assert_eq!(cancelled(&router, "twice", wire::CancelLevel::Abandon).await, StatusCode::OK);
    assert_eq!(exit(&again.await.unwrap().0), 130);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_handle_that_is_not_one_ends_the_stream_before_the_run() {
    let (_dir, router, _reached) = hanging();
    let (events, end) = run(router, "../a b").await;
    assert!(events.is_empty(), "{events:?}");
    assert_eq!(end["error"]["code"], "invalid_argument", "{end}");
}

#[tokio::test]
async fn a_cancel_names_its_level_and_a_run_that_is_running() {
    let (_dir, api) = project();
    let ask = |run: &str, level: wire::CancelLevel| wire::CancelRequest { run: run.to_owned(), level: level.into() };
    let refused = cancel(api.clone(), ask("run-1", wire::CancelLevel::Unspecified)).await.unwrap_err();
    assert_eq!(refused.code, Code::InvalidArgument, "{refused:?}");
    let gone = cancel(api.clone(), ask("run-1", wire::CancelLevel::Stop)).await.unwrap_err();
    assert_eq!((gone.code, gone.message.as_str()), (Code::NotFound, "no run `run-1` is running"));
    let long = "a".repeat(65);
    assert_eq!(cancel(api, ask(&long, wire::CancelLevel::Stop)).await.unwrap_err().code, Code::InvalidArgument);
}

#[tokio::test]
async fn the_keeper_keeps_a_run_until_it_returns_and_never_an_unnamed_one() {
    let runs = Runs::spawn();
    let (interrupter, interrupts) = vc_engine::interrupts();
    runs.open("", &interrupter).await.unwrap();
    assert!(runs.find(String::new()).await.is_none());
    runs.open("run-1", &interrupter).await.unwrap();
    assert_eq!(runs.open("run-1", &interrupter).await.unwrap_err().code, Code::AlreadyExists);
    assert!(runs.find("run-1".to_owned()).await.is_some());
    drop(interrupts);
    assert!(runs.find("run-1".to_owned()).await.is_none());
    runs.open("run-1", &interrupter).await.unwrap();
}

#[test]
fn outside_a_runtime_no_run_is_kept() {
    let runs = Runs::spawn();
    let (interrupter, _interrupts) = vc_engine::interrupts();
    let opened = tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(runs.open("a", &interrupter));
    assert_eq!(opened.unwrap_err().code, Code::Unavailable);
}
