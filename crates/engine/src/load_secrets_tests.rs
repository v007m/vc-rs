//! A load run masks the example-row labels its plan holds, as `vc run` does, and a secret's
//! value given to one reaches neither its report, its ticks nor its history, its findings and
//! their sampled exchanges included, however the run ends (D326).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::sync::mpsc;

use vc_interfaces::{HistoryError, HistoryWriter, Row, RunEnd, RunId, RunStart};
use vc_model::{Keep, RenderedRequest};
use vc_testkit::{FakeTransport, FakeVcs, MemoryEnvironment, MemoryHistory, MemorySources, status};

use crate::sample::builder;
use crate::{Engine, LoadEnding, LoadReport, LoadRequest, LoadWatch, interrupts};

/// The value the secret `API_TOKEN` has; an env var, a var and a dataset row name a row with it too.
const TOKEN: &str = "Kd4R-8wNq-Yb6T-2hVz-Jc5M";

const CONFIG: &str = concat!(
    "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars:\n  baseUrl: 'http://x'\n",
    "  fromEnv: [{name: '${env.ROW_LABEL}'}]\n  fromVar: [{name: Kd4R-8wNq-Yb6T-2hVz-Jc5M}]\n",
);
const API: &str = concat!(
    "baseUrl: ${var.baseUrl}\napis:\n  - id: send\n    method: POST\n    path: /echo\n",
    "    headers: {authorization: 'Bearer ${secret.API_TOKEN}'}\n    body: {token: '${secret.API_TOKEN}'}\n",
);
const TESTS: &str = concat!(
    "tests:\n  - id: sendsFromEnv\n    examples: ${var.fromEnv}\n    call: echo#send\n",
    "  - id: sendsFromVar\n    examples: ${var.fromVar}\n    call: echo#send\n",
    "  - id: sendsFromDataset\n    examples: ${dataset.tokens}\n    call: echo#send\n",
);
const LOAD: &str = concat!(
    "mix:\n  - {test: echo.send#sendsFromEnv, weight: 1}\n  - {test: echo.send#sendsFromVar, weight: 1}\n",
    "  - {test: echo.send#sendsFromDataset, weight: 1}\nstages:\n  - {duration: 3s, users: 3}\nthinkTime: 1s\n",
    "thresholds: {errorRate: {lt: 0.5}}\n",
);

/// A seed whose 3 iterations pick each test of the 1:1:1 mix once, so every row label reaches the
/// history; a fresh one leaves a test out about 7 runs in 9.
const ALL_PICKED: u64 = 12;

/// Each thing the engine wrote to the history, as text; one that `refuses` fails each record
/// once it has noted its rows.
struct Written {
    notes: mpsc::UnboundedSender<String>,
    refuses: bool,
}

impl Written {
    fn note(&self, written: impl std::fmt::Debug) {
        self.notes.send(format!("{written:?}")).ok();
    }
}

impl HistoryWriter for Written {
    fn begin(&self, start: &RunStart) -> Result<RunId, HistoryError> {
        self.note(start);
        Ok(RunId::new(9))
    }

    fn record(&self, _: RunId, rows: Vec<Row>) -> Result<(), HistoryError> {
        let samples =
            rows.iter().filter_map(|row| if let Row::Finding(found) = row { Some(&found.samples) } else { None });
        for sample in samples.flatten() {
            let bodies = [&sample.request_body, &sample.response_body].into_iter().flatten();
            bodies.for_each(|body| self.note(format!("body {}", String::from_utf8_lossy(body))));
        }
        self.note(rows);
        if self.refuses { Err(HistoryError::Failed { reason: "the disk is full".into() }) } else { Ok(()) }
    }

    fn finish(&self, _: RunId, end: RunEnd) -> Result<(), HistoryError> {
        self.note(end);
        Ok(())
    }

    fn prune(&self, _: Keep) -> Result<usize, HistoryError> {
        Ok(0)
    }
}

/// An engine over the echo project, the secret `API_TOKEN` and the env var `ROW_LABEL` set to
/// [`TOKEN`], where every request answers 500 with its own body; and what it writes to the history.
fn engine() -> (Engine, mpsc::UnboundedReceiver<String>) {
    engine_of(LOAD, false)
}

/// [`engine`] with `load` as its scenario, and a history that `refuses` its rows or takes them.
fn engine_of(load: &str, refuses: bool) -> (Engine, mpsc::UnboundedReceiver<String>) {
    let files: [(&str, &[u8]); 6] = [
        ("vibranium.config.yaml", CONFIG.as_bytes()),
        (".env.schema", b"# the api's token\nAPI_TOKEN=\n"),
        ("services/echo/echo.api.yaml", API.as_bytes()),
        ("services/echo/send.test.yaml", TESTS.as_bytes()),
        ("datasets/tokens.json", br#"[{"name": "Kd4R-8wNq-Yb6T-2hVz-Jc5M"}]"#),
        ("load/echo/peak.load.yaml", load.as_bytes()),
    ];
    let (notes, written) = mpsc::unbounded_channel();
    let engine = builder(FakeVcs::default())
        .sources(Arc::new(MemorySources::new(&files)))
        .environment(Arc::new(MemoryEnvironment::new(&[("API_TOKEN", TOKEN), ("ROW_LABEL", TOKEN)])))
        .transport(Arc::new(FakeTransport::new(|request: &RenderedRequest, _| {
            let mut answer = status(500);
            answer.body = request.body.clone();
            Ok(answer)
        })))
        .history(Arc::new(Written { notes, refuses }), Arc::new(MemoryHistory::new()))
        .build()
        .unwrap();
    (engine, written)
}

fn request() -> LoadRequest {
    LoadRequest { scenario: "echo.peak".to_owned(), seed: Some(ALL_PICKED), ..LoadRequest::default() }
}

/// Runs `request` with its ticks watched, pressing Ctrl-C once `press` has passed if given;
/// the report, its ticks and what `written` got from the history, as text.
async fn watched(engine: &Engine, request: &LoadRequest, press: Option<Duration>) -> (LoadReport, String) {
    let (interrupter, interrupts) = interrupts();
    let (ticks, mut ticked) = mpsc::channel(1024);
    let pressing = async {
        if let Some(press) = press {
            tokio::time::sleep(press).await;
            interrupter.interrupt();
        }
    };
    let (report, ()) = tokio::join!(engine.load(request, LoadWatch { interrupts, ticks: Some(ticks) }), pressing);
    let ticks: Vec<String> = std::iter::from_fn(|| ticked.try_recv().ok()).map(|tick| format!("{tick:?}")).collect();
    assert!(!ticks.is_empty(), "each closed second is a tick");
    (report.unwrap(), ticks.join("\n"))
}

fn history(written: &mut mpsc::UnboundedReceiver<String>) -> String {
    std::iter::from_fn(|| written.try_recv().ok()).collect::<Vec<_>>().join("\n")
}

#[tokio::test]
async fn a_load_plan_s_row_labels_from_env_var_or_dataset_are_masked_once_secured() {
    let (engine, _written) = engine();
    let mut session = engine.load_session(&request()).unwrap();
    let drawn: Vec<&str> = session.prepared.plan.row_names.values().map(String::as_str).collect();
    assert_eq!(drawn, [TOKEN; 3], "each label is drawn from the secret's value");
    let (_interrupter, mut interrupts) = interrupts();
    let secured = engine.secured(&mut session.prepared, &mut interrupts).await.unwrap().unwrap();
    let masked: Vec<&str> = session.prepared.plan.row_names.values().map(String::as_str).collect();
    assert_eq!(masked, ["••••"; 3]);
    assert_eq!(secured.secrets.get("API_TOKEN").map(|secret| secret.expose().to_owned()), Some(TOKEN.to_owned()));
}

#[tokio::test(start_paused = true)]
async fn a_secret_given_to_a_load_run_reaches_neither_its_report_its_ticks_nor_its_history() {
    let (engine, mut written) = engine();
    let (report, ticks) = watched(&engine, &request(), None).await;
    assert!(report.iterations > 0 && report.errors == report.iterations, "{report:?}");
    let history = history(&mut written);
    let (json, text) = (report.json().to_string(), report.text());
    assert!(history.contains("IntervalRow { test_id: \"echo.send#sendsFrom"), "the seconds were recorded: {history}");
    assert!(history.contains("FindingRow { kind: Breach, test_id: None, api_id: None, metric: Some(\"errorRate\")"));
    for test in ["sendsFromEnv", "sendsFromVar", "sendsFromDataset"] {
        let found =
            format!("FindingRow {{ kind: Error, test_id: Some(\"echo.send#{test}\"), api_id: Some(\"echo#send\")");
        assert!(history.contains(&found), "each error kind is kept under its test and api IDs: {history}");
    }
    assert!(history.contains("samples: [ExchangeRow { action: \"call echo#send\""), "{history}");
    assert!(history.contains("(\"authorization\", \"••••\")"), "the header is masked whole: {history}");
    let bodies: Vec<&str> = history.lines().filter(|line| line.starts_with("\"body ")).collect();
    assert!(!bodies.is_empty() && bodies.iter().all(|body| body.contains("••••")), "{bodies:?}");
    for (shown, seen) in [("json", &json), ("text", &text), ("ticks", &ticks), ("history", &history)] {
        assert!(!seen.contains(TOKEN), "the {shown} holds the secret: {seen}");
    }
}

#[tokio::test(start_paused = true)]
async fn an_aborted_a_cancelled_and_a_refused_run_hand_the_history_only_masked_samples() {
    let aborting = format!("{LOAD}abortOn: [errorRate]\n");
    let cases = [
        (aborting.as_str(), false, None, LoadEnding::Aborted),
        (LOAD, false, Some(Duration::from_millis(1500)), LoadEnding::Cancelled),
        (LOAD, true, None, LoadEnding::Finished),
    ];
    for (load, refuses, press, ending) in cases {
        let (engine, mut written) = engine_of(load, refuses);
        let (report, ticks) = watched(&engine, &request(), press).await;
        assert_eq!((report.ending, report.errors > 0), (ending, true), "{report:?}");
        let refused = report.notices.iter().any(|notice| notice.contains("the disk is full"));
        assert_eq!(refused, refuses, "{:?}", report.notices);
        let history = history(&mut written);
        assert!(history.contains("samples: [ExchangeRow {"), "{ending:?}: {history}");
        assert!(history.contains("(\"authorization\", \"••••\")"), "{ending:?}: {history}");
        let seen = format!("{} {} {ticks} {history}", report.json(), report.text());
        assert!(!seen.contains(TOKEN), "{ending:?} holds the secret: {seen}");
    }
}

/// A project whose `users.me#getsMe` calls `users#me` with the client-credentials auth `svc`.
const REFRESHED: [(&str, &[u8]); 4] = [
    (
        "vibranium.config.yaml",
        b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\nauth:\n  - id: svc\n    type: oauth2ClientCredentials\n    tokenUrl: http://id/token\n    clientId: cli\n",
    ),
    ("services/users/users.api.yaml", b"baseUrl: ${var.baseUrl}\napis:\n  - id: me\n    path: /me\n    auth: svc\n"),
    ("services/users/me.test.yaml", b"tests:\n  - id: getsMe\n    call: users#me\n"),
    (
        "load/users/peak.load.yaml",
        b"mix:\n  - {test: users.me#getsMe, weight: 1}\nstages:\n  - {duration: 1s, users: 1}\n  - {duration: 3s, users: 1}\nthinkTime: 1s\n",
    ),
];

/// The `n`th token the token endpoint hands out.
fn token(n: usize) -> String {
    format!("tok-{n}-Xr9Lm2QwPz")
}

#[tokio::test(start_paused = true)]
async fn a_token_refreshed_mid_run_leaves_the_samples_sent_with_the_old_one_masked() {
    let fetched = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&fetched);
    let transport = FakeTransport::new(move |request: &RenderedRequest, _| {
        if request.uri.path() == "/token" {
            let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
            let mut answer = status(200);
            answer.body = format!(r#"{{"access_token":"{}","token_type":"bearer","expires_in":1}}"#, token(n)).into();
            return Ok(answer);
        }
        let mut answer = status(500);
        let sent = request.headers.get("authorization").and_then(|value| value.to_str().ok()).unwrap_or_default();
        answer.body = format!("sent {}", sent.trim_start_matches("Bearer ")).into();
        Ok(answer)
    });
    let (notes, mut written) = mpsc::unbounded_channel();
    let engine = builder(FakeVcs::default())
        .sources(Arc::new(MemorySources::new(&REFRESHED)))
        .transport(Arc::new(transport))
        .history(Arc::new(Written { notes, refuses: false }), Arc::new(MemoryHistory::new()))
        .build()
        .unwrap();
    let request = LoadRequest { scenario: "users.peak".to_owned(), ..LoadRequest::default() };
    let (report, ticks) = watched(&engine, &request, None).await;
    assert!(report.errors >= 2 && report.errors == report.iterations, "{report:?}");
    let fetched = fetched.load(Ordering::SeqCst);
    assert!(fetched >= 2, "the token was refreshed: {fetched}");
    let history = history(&mut written);
    let bodies: Vec<&str> = history.lines().filter(|line| line.starts_with("\"body ")).collect();
    assert_eq!(bodies.len(), usize::try_from(report.errors).unwrap().min(5), "{history}");
    assert!(bodies.iter().all(|body| *body == "\"body sent ••••\""), "{bodies:?}");
    let seen = format!("{} {} {ticks} {history}", report.json(), report.text());
    for n in 1..=fetched {
        assert!(!seen.contains(&token(n)), "token {n} of {fetched} is shown: {seen}");
    }
}
