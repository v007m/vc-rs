use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use vc_interfaces::{
    Environment, HistoryError, HistoryWriter, Row, RunEnd, RunId, RunStart, Secret, TlsError, TlsPart, TlsSpec,
    Transport, TransportFactory,
};
use vc_model::Keep;
use vc_testkit::{FakeTransport, FakeVcs, MemoryEnvironment, MemoryHistory, MemorySources, status};

use crate::plan::Extra;
use crate::sample::builder;
use crate::{CallRequest, Engine, EngineError, LoadEnding, LoadRequest, LoadWatch, PlanRequest, RunRequest};

/// The passphrase `P12_PASS` holds, which a row label reads too.
const PASS: &str = "Wq3-fixture-Lp8";

/// A config whose row label and `bank` bundle's passphrase read `passphrase`.
fn config(passphrase: &str) -> String {
    format!(
        "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars:\n  rows: [{{name: '${{env.P12_PASS}}'}}]\n\
         http:\n  tls:\n    clients:\n      - id: bank\n        pkcs12: certs/bank.p12\n        passphrase: {passphrase}\n"
    )
}

const API: &str = "baseUrl: http://x\napis:\n  - id: send\n    method: GET\n    path: /echo\n";
const TESTS: &str = "tests:\n  - id: sendsRows\n    examples: ${var.rows}\n    call: echo#send\n";
const LOAD: &str = "mix:\n  - {test: echo.send#sendsRows, weight: 1}\nstages:\n  - {duration: 1s, users: 1}\n";

/// A transport factory that refuses with `refusal` or builds a transport answering 200, once
/// `released` is set or a few seconds have passed; it notes in `started` that it began.
#[derive(Default)]
struct Loader {
    refusal: Option<TlsError>,
    held: bool,
    started: Arc<AtomicBool>,
    released: Arc<AtomicBool>,
}

impl TransportFactory for Loader {
    fn build(&self, _: &TlsSpec, _: &dyn Environment) -> Result<Arc<dyn Transport>, TlsError> {
        self.started.store(true, Ordering::SeqCst);
        let until = Instant::now() + Duration::from_secs(5);
        while self.held && !self.released.load(Ordering::SeqCst) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        let transport: Arc<dyn Transport> = Arc::new(FakeTransport::new(|_, _| Ok(status(200))));
        self.refusal.clone().map_or_else(|| Ok(transport), Err)
    }
}

/// Each run the engine began in the history, as its kind.
struct Begun(mpsc::UnboundedSender<String>);

impl HistoryWriter for Begun {
    fn begin(&self, start: &RunStart) -> Result<RunId, HistoryError> {
        self.0.send(format!("{start:?}")).ok();
        Ok(RunId::new(9))
    }

    fn record(&self, _: RunId, _: Vec<Row>) -> Result<(), HistoryError> {
        Ok(())
    }

    fn finish(&self, _: RunId, _: RunEnd) -> Result<(), HistoryError> {
        Ok(())
    }

    fn prune(&self, _: Keep) -> Result<usize, HistoryError> {
        Ok(0)
    }
}

/// An engine over the echo project with `config`, `P12_PASS` set to [`PASS`] and `loader`; and
/// the runs it begins in the history.
fn engine(config: &str, loader: Loader) -> (Engine, mpsc::UnboundedReceiver<String>) {
    let files: [(&str, &[u8]); 5] = [
        ("vibranium.config.yaml", config.as_bytes()),
        (".env.schema", b"# a bundle's passphrase\nNOPE=\n"),
        ("services/echo/echo.api.yaml", API.as_bytes()),
        ("services/echo/send.test.yaml", TESTS.as_bytes()),
        ("load/echo/peak.load.yaml", LOAD.as_bytes()),
    ];
    let (begun, runs) = mpsc::unbounded_channel();
    let engine = builder(FakeVcs::default())
        .sources(Arc::new(MemorySources::new(&files)))
        .environment(Arc::new(MemoryEnvironment::new(&[("P12_PASS", PASS)])))
        .transport_factory(Arc::new(loader))
        .history(Arc::new(Begun(begun)), Arc::new(MemoryHistory::new()))
        .build()
        .unwrap();
    (engine, runs)
}

fn load_request() -> LoadRequest {
    LoadRequest { scenario: "echo.peak".to_owned(), ..LoadRequest::default() }
}

fn call_request() -> CallRequest {
    CallRequest { api: "echo#send".to_owned(), ..CallRequest::default() }
}

#[tokio::test]
async fn a_row_label_holding_a_bundles_passphrase_is_masked_once_secured() {
    let (engine, _runs) = engine(&config("${env.P12_PASS}"), Loader::default());
    let mut prepared = engine.prepare(&PlanRequest::default(), Extra::default()).unwrap();
    let drawn: Vec<&str> = prepared.plan.row_names.values().map(String::as_str).collect();
    assert_eq!(drawn, [PASS], "the label is drawn from the env var the passphrase reads");
    let (_interrupter, mut interrupts) = crate::interrupts();
    let (_, tls) = engine.secured(&mut prepared, &mut interrupts).await.unwrap().unwrap();
    let masked: Vec<&str> = prepared.plan.row_names.values().map(String::as_str).collect();
    assert_eq!(masked, ["••••"]);
    assert_eq!(tls.passphrases.iter().map(Secret::expose).collect::<Vec<_>>(), [PASS]);
}

/// The error of each entry point over `config` with `loader`, and whether any began a run.
async fn refused(config: &str, loader: Loader) -> Vec<(String, bool)> {
    let (engine, mut begun) = engine(config, loader);
    let mut seen = Vec::new();
    let mut note = |error: Option<EngineError>| {
        seen.push((error.map(|error| error.to_string()).unwrap_or_default(), begun.try_recv().is_ok()));
    };
    let (_interrupter, interrupts) = crate::interrupts();
    note(engine.run(&RunRequest::default(), interrupts).await.err());
    let (_interrupter, interrupts) = crate::interrupts();
    note(engine.load(&load_request(), LoadWatch { interrupts, ticks: None }).await.err());
    note(engine.call(&call_request()).await.err());
    seen
}

/// A loader that refuses `bank`'s bundle as `Unusable`, else as a `Mismatch`.
fn refusing(unusable: bool) -> Loader {
    let (part, reason) = (TlsPart::Client { id: "bank".into(), field: Some("pkcs12") }, "it does not open".to_owned());
    let refusal = if unusable { TlsError::Unusable { part, reason } } else { TlsError::Mismatch { part, reason } };
    Loader { refusal: Some(refusal), ..Loader::default() }
}

#[tokio::test]
async fn a_tls_or_secret_refusal_stops_run_load_and_call_before_any_run_begins() {
    let cases = [
        ("V5125", refused(&config(PASS), refusing(true)).await),
        ("V5126", refused(&config(PASS), refusing(false)).await),
        ("V5114", refused(&config("${secret.NOPE}"), Loader::default()).await),
    ];
    for (code, seen) in cases {
        for (error, began) in seen {
            assert!(error.contains(code), "{code}: {error}");
            assert!(!began, "{code} began a run: {error}");
        }
    }
}

/// A loader that refuses once released, and the flags of its start and its release.
fn held() -> (Loader, Arc<AtomicBool>, Arc<AtomicBool>) {
    let loader = Loader { held: true, ..refusing(false) };
    let (started, released) = (Arc::clone(&loader.started), Arc::clone(&loader.released));
    (loader, started, released)
}

/// Waits until `started` is set, a bounded number of times, then interrupts.
async fn press_once(started: &AtomicBool, interrupter: crate::Interrupter) {
    for _ in 0..2000 {
        if started.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    interrupter.interrupt();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_interrupt_while_http_tls_loads_cancels_run_and_load_before_any_run_begins() {
    let (slow, started, released) = held();
    let (engine, mut begun) = engine(&config(PASS), slow);
    let (run, load) = (RunRequest::default(), load_request());
    let (interrupter, interrupts) = crate::interrupts();
    let (report, ()) = tokio::join!(engine.run(&run, interrupts), press_once(&started, interrupter));
    let report = report.unwrap();
    assert!(!released.load(Ordering::SeqCst), "the run did not wait for the load");
    assert_eq!((report.exit, report.run.is_some()), (130, false));
    started.store(false, Ordering::SeqCst);
    let (interrupter, interrupts) = crate::interrupts();
    let watch = LoadWatch { interrupts, ticks: None };
    let (cancelled, ()) = tokio::join!(engine.load(&load, watch), press_once(&started, interrupter));
    let cancelled = cancelled.unwrap();
    released.store(true, Ordering::SeqCst);
    assert_eq!((cancelled.ending, cancelled.exit), (LoadEnding::Cancelled, 130));
    assert!(begun.try_recv().is_err(), "no run began");
}
