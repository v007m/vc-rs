//! A load run masks the example-row labels its plan holds, as `vc run` does, and a secret's
//! value given to one reaches neither its report nor its history.

use std::sync::Arc;

use tokio::sync::mpsc;

use vc_interfaces::{HistoryError, HistoryWriter, Row, RunEnd, RunId, RunStart};
use vc_model::{Keep, RenderedRequest};
use vc_testkit::{FakeTransport, FakeVcs, MemoryEnvironment, MemoryHistory, MemorySources, status};

use crate::sample::builder;
use crate::{Engine, LoadRequest, LoadWatch, interrupts};

/// The value the secret `API_TOKEN` has; an env var, a var and a dataset row name a row with it too.
const TOKEN: &str = "Kd4R-8wNq-Yb6T-2hVz-Jc5M";

const CONFIG: &str = concat!(
    "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars:\n  baseUrl: 'http://x'\n",
    "  fromEnv: [{name: '${env.ROW_LABEL}'}]\n  fromVar: [{name: Kd4R-8wNq-Yb6T-2hVz-Jc5M}]\n",
);
const API: &str = "baseUrl: ${var.baseUrl}\napis:\n  - id: send\n    method: POST\n    path: /echo\n";
const TESTS: &str = concat!(
    "tests:\n  - id: sendsFromEnv\n    examples: ${var.fromEnv}\n    call: echo#send\n",
    "  - id: sendsFromVar\n    examples: ${var.fromVar}\n    call: echo#send\n",
    "  - id: sendsFromDataset\n    examples: ${dataset.tokens}\n    call: echo#send\n",
);
const LOAD: &str = concat!(
    "mix:\n  - {test: echo.send#sendsFromEnv, weight: 1}\n  - {test: echo.send#sendsFromVar, weight: 1}\n",
    "  - {test: echo.send#sendsFromDataset, weight: 1}\nstages:\n  - {duration: 3s, users: 3}\nthinkTime: 1s\n",
);

/// Each thing the engine wrote to the history, as text.
struct Written(mpsc::UnboundedSender<String>);

impl Written {
    fn note(&self, written: impl std::fmt::Debug) {
        self.0.send(format!("{written:?}")).ok();
    }
}

impl HistoryWriter for Written {
    fn begin(&self, start: &RunStart) -> Result<RunId, HistoryError> {
        self.note(start);
        Ok(RunId::new(9))
    }

    fn record(&self, _: RunId, rows: Vec<Row>) -> Result<(), HistoryError> {
        self.note(rows);
        Ok(())
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
/// [`TOKEN`], where every request answers 500; and what it writes to the history.
fn engine() -> (Engine, mpsc::UnboundedReceiver<String>) {
    let files: [(&str, &[u8]); 6] = [
        ("vibranium.config.yaml", CONFIG.as_bytes()),
        (".env.schema", b"# the api's token\nAPI_TOKEN=\n"),
        ("services/echo/echo.api.yaml", API.as_bytes()),
        ("services/echo/send.test.yaml", TESTS.as_bytes()),
        ("datasets/tokens.json", br#"[{"name": "Kd4R-8wNq-Yb6T-2hVz-Jc5M"}]"#),
        ("load/echo/peak.load.yaml", LOAD.as_bytes()),
    ];
    let (writer, written) = mpsc::unbounded_channel();
    let engine = builder(FakeVcs::default())
        .sources(Arc::new(MemorySources::new(&files)))
        .environment(Arc::new(MemoryEnvironment::new(&[("API_TOKEN", TOKEN), ("ROW_LABEL", TOKEN)])))
        .transport(Arc::new(FakeTransport::new(|_: &RenderedRequest, _| Ok(status(500)))))
        .history(Arc::new(Written(writer)), Arc::new(MemoryHistory::new()))
        .build()
        .unwrap();
    (engine, written)
}

fn request() -> LoadRequest {
    LoadRequest { scenario: "echo.peak".to_owned(), ..LoadRequest::default() }
}

#[tokio::test]
async fn a_load_plan_s_row_labels_from_env_var_or_dataset_are_masked_once_secured() {
    let (engine, _written) = engine();
    let session = engine.load_session(&request()).unwrap();
    let drawn: Vec<&str> = session.prepared.plan.row_names.values().map(String::as_str).collect();
    assert_eq!(drawn, [TOKEN; 3], "each label is drawn from the secret's value");
    let (prepared, secrets) = engine.secured(session.prepared).await.unwrap();
    let masked: Vec<&str> = prepared.plan.row_names.values().map(String::as_str).collect();
    assert_eq!(masked, ["••••"; 3]);
    assert_eq!(secrets.get("API_TOKEN").map(|secret| secret.expose().to_owned()), Some(TOKEN.to_owned()));
}

#[tokio::test(start_paused = true)]
async fn a_secret_given_to_a_load_run_reaches_neither_its_report_nor_its_history() {
    let (engine, mut written) = engine();
    let (_interrupter, interrupts) = interrupts();
    let report = engine.load(&request(), LoadWatch { interrupts, ticks: None }).await.unwrap();
    assert!(report.iterations > 0 && report.errors == report.iterations, "{report:?}");
    let history: String = std::iter::from_fn(|| written.try_recv().ok()).collect::<Vec<_>>().join("\n");
    let (json, text) = (report.json().to_string(), report.text());
    assert!(history.contains("IntervalRow { test_id: \"echo.send#sendsFrom"), "the seconds were recorded: {history}");
    for (shown, seen) in [("json", &json), ("text", &text), ("history", &history)] {
        assert!(!seen.contains(TOKEN), "the {shown} holds the secret: {seen}");
    }
}
