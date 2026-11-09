use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use vc_interfaces::Secrets;
use vc_model::{AlertTrigger, Program};
use vc_testkit::{FakeNotifiers, FakeTransport, FakeVcs, MemoryEnvironment, MemoryHistory, MemorySources, status};

use super::{Lookup, Senders};
use crate::sample::builder;
use crate::{Engine, EngineBuilder, RunReport, RunRequest, interrupts};

const CONFIG: &str = "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\n";
const USERS: (&str, &[u8]) = (
    "services/users/users.api.yaml",
    b"baseUrl: ${var.baseUrl}\napis:\n  - id: get\n    path: /users/1\n    expect: {status: 200}\n",
);
const CRUD: (&str, &[u8]) =
    ("services/users/crud.test.yaml", b"tags: [smoke]\ntests:\n  - id: getsUser\n    call: users#get\n");
const SCHEMA: (&str, &[u8]) = (".env.schema", b"# @sensitive\nHOOK=\n");
const HOOK: &str = "https://hooks.example/s3cr3t";

/// The project with `config`, its secret and env set, recording its runs.
fn project(config: &str) -> EngineBuilder {
    let sources = MemorySources::new(&[("vibranium.config.yaml", config.as_bytes()), USERS, CRUD, SCHEMA]);
    let history = Arc::new(MemoryHistory::new());
    builder(FakeVcs::default())
        .sources(Arc::new(sources))
        .environment(Arc::new(MemoryEnvironment::new(&[("HOOK", HOOK), ("CHANNEL", "#qa")])))
        .history(history.clone(), history)
}

/// The project with `alerts` sending to `notifiers`, its api answered by `transport`.
fn engine(alerts: &str, notifiers: FakeNotifiers, transport: FakeTransport) -> Engine {
    let config = format!("{CONFIG}alerts:\n{alerts}");
    project(&config).notifiers(Arc::new(notifiers)).transport(Arc::new(transport)).build().unwrap()
}

async fn run(engine: &Engine) -> RunReport {
    engine.run(&RunRequest::default(), interrupts().1).await.unwrap()
}

/// The run's V5117 notices.
fn unsent(report: &RunReport) -> Vec<&str> {
    report.notices.iter().map(String::as_str).filter(|notice| notice.starts_with("V5117")).collect()
}

#[tokio::test]
async fn a_failed_run_sends_its_notice_with_the_secret_filled_in() {
    let (notifiers, log) = FakeNotifiers::new(&["webhook"]);
    let engine = engine("  - notify:\n      - webhook: {url: '${secret.HOOK}'}\n", notifiers, failing_once());
    let report = run(&engine).await;
    assert_eq!((report.exit, unsent(&report)), (1, Vec::<&str>::new()), "{:?}", report.notices);
    let sent = log.take();
    let [sent] = sent.as_slice() else { panic!("{sent:?}") };
    assert_eq!((sent.channel.as_str(), &sent.settings), ("webhook", &json!({"url": HOOK})));
    let notice = &sent.notice;
    assert_eq!((notice.trigger, notice.project.as_str()), (AlertTrigger::Failure, "project"));
    let failed = notice.failures.iter().map(|failure| failure.test.as_str()).collect::<Vec<_>>();
    assert_eq!((failed, &notice.summary.profiles), (vec!["users.crud#getsUser"], &vec!["dev".to_owned()]));
}

#[tokio::test]
async fn a_pass_after_a_failure_recovers_and_a_channel_that_fails_never_changes_the_exit_code() {
    let (notifiers, log) = FakeNotifiers::new(&["webhook", "slack"]);
    let alerts = concat!(
        "  - on: [failure, recovery]\n    notify:\n",
        "      - slack: {channel: '${env.CHANNEL}'}\n      - webhook: {url: '${secret.HOOK}'}\n",
    );
    let engine = engine(alerts, notifiers.failing("slack"), failing_once());
    let (failed, passed) = (run(&engine).await, run(&engine).await);
    assert_eq!((failed.exit, passed.exit), (1, 0));
    let triggers = log.take().iter().map(|sent| sent.notice.trigger).collect::<Vec<_>>();
    assert_eq!(triggers, [AlertTrigger::Failure, AlertTrigger::Recovery]);
    assert_eq!(unsent(&passed), ["V5117: the `slack` alert was not sent: HTTP 503"]);
}

#[tokio::test]
async fn settings_that_cannot_be_filled_in_and_unknown_channels_are_warnings() {
    let (notifiers, log) = FakeNotifiers::new(&["webhook"]);
    let alerts = concat!(
        "  - notify:\n      - webhook: {url: '${env.UNSET}'}\n      - email: {to: qa@example.com}\n",
        "  - profiles: [staging]\n    notify:\n      - webhook: {url: '${secret.HOOK}'}\n",
    );
    let report = run(&engine(alerts, notifiers, failing_once())).await;
    assert_eq!(report.exit, 1);
    let unsent = unsent(&report);
    let [unfilled, unknown] = unsent.as_slice() else { panic!("{unsent:?}") };
    assert!(unfilled.contains("`env.UNSET` is not set"), "{unfilled}");
    assert_eq!(*unknown, "V5117: unknown alert channel `email`; known channels: webhook");
    assert_eq!(log.take(), [], "the staging rule is not for this run's profiles");
}

#[test]
fn a_declared_secret_read_from_env_is_refused_without_its_value() {
    let mut program = Program::default();
    program.config.secrets.declared.push("HOOK".to_owned());
    let factory = Arc::new(FakeNotifiers::new(&[]).0);
    let environment = Arc::new(MemoryEnvironment::new(&[("HOOK", HOOK), ("CHANNEL", "#qa")]));
    let senders = Senders { factory, secrets: Secrets::new(), environment };
    let lookup = Lookup { senders: &senders, program: &program };
    let refused = lookup.env("HOOK").unwrap_err().to_string();
    assert!(refused.contains("`env.HOOK` is a declared secret"), "{refused}");
    assert!(!refused.contains("s3cr3t"), "a notice never shows a setting's value: {refused}");
    assert_eq!(lookup.env("CHANNEL"), Ok(json!("#qa")));
}

/// The api fails its first request and passes every later one.
fn failing_once() -> FakeTransport {
    FakeTransport::scripted(vec![Ok(status(500)), Ok(status(200))])
}

#[tokio::test]
async fn a_webhook_posts_the_ctrf_summary_over_http() {
    let (posted, mut received) = mpsc::unbounded_channel::<Value>();
    let hook = |State(posted): State<mpsc::UnboundedSender<Value>>, body: axum::Json<Value>| async move {
        posted.send(body.0).ok();
    };
    let broken = get(|| async { (StatusCode::INTERNAL_SERVER_ERROR, "{}") });
    let routes = Router::new().route("/users/1", broken).route("/hook", post(hook)).with_state(posted);
    let listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, routes).await });
    let alerts = format!("alerts:\n  - notify:\n      - webhook: {{url: '{base}/hook'}}\n");
    let config = format!("{}{alerts}", CONFIG.replace("http://x", &base));
    let report = run(&project(&config).build().unwrap()).await;
    assert_eq!((report.exit, unsent(&report)), (1, Vec::<&str>::new()), "{:?}", report.notices);
    let body = received.try_recv().unwrap();
    assert_eq!((body["reportFormat"].as_str(), body["specVersion"].as_str()), (Some("CTRF"), Some("0.2.0")));
    assert_eq!(
        (body["results"]["tool"]["name"].as_str(), body["results"]["summary"]["failed"].as_u64()),
        (Some("vibranium"), Some(1))
    );
}
