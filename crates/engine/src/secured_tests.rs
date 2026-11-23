use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::mpsc;
use vc_interfaces::{
    CommandError, CommandOutput, CommandRunner, CommandSpec, Environment, HistoryError, HistoryWriter, Row, RunEnd,
    RunId, RunStart, Secret, TlsError, TlsFinding, TlsPart, TlsProbe, TlsSpec, Transport, TransportFactory,
    TransportPorts,
};
use vc_model::Keep;
use vc_testkit::{FakeTransport, FakeVcs, MemoryCacheStore, MemoryEnvironment, MemoryHistory, MemorySources, status};

use crate::plan::Extra;
use crate::sample::builder;
use crate::{
    CallRequest, Engine, EngineBuilder, EngineError, LoadEnding, LoadRequest, LoadWatch, PlanRequest, RunRequest,
    TestAuthRequest,
};

/// The passphrase `P12_PASS` holds, which a row label reads too.
const PASS: &str = "Wq3-fixture-Lp8";

/// A config whose row label and `bank` bundle's passphrase read `passphrase`, with the bearer
/// auth `svc`.
fn config(passphrase: &str) -> String {
    format!(
        "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars:\n  rows: [{{name: '${{env.P12_PASS}}'}}]\n\
         auth:\n  - id: svc\n    type: bearer\n    token: t0k3n\n\
         http:\n  tls:\n    clients:\n      - id: bank\n        pkcs12: certs/bank.p12\n        passphrase: {passphrase}\n"
    )
}

/// [`config`] with one alert rule, whose webhook every run's end sends.
fn alerting(passphrase: &str) -> String {
    format!("{}alerts:\n  - notify:\n      - webhook: {{url: 'http://hook'}}\n", config(passphrase))
}

const API: &str = "baseUrl: http://x\napis:\n  - id: send\n    method: GET\n    path: /echo\n    auth: svc\n";
const TESTS: &str = "tests:\n  - id: sendsRows\n    examples: ${var.rows}\n    call: echo#send\n";
const LOAD: &str = "mix:\n  - {test: echo.send#sendsRows, weight: 1}\nstages:\n  - {duration: 1s, users: 1}\n";

/// A transport factory that refuses with `refusal` or builds a transport answering 200, once
/// `released` is set or a few seconds have passed; it notes in `started` that it began, and
/// counts its builds in `builds`. Its inspections find `findings`, and count in `inspected`
/// those whose probes hold `bank`'s bundle with the passphrase rendered.
#[derive(Default)]
struct Loader {
    refusal: Option<TlsError>,
    held: bool,
    started: Arc<AtomicBool>,
    released: Arc<AtomicBool>,
    builds: Arc<AtomicUsize>,
    findings: Vec<TlsFinding>,
    inspected: Arc<AtomicUsize>,
}

impl TransportFactory for Loader {
    fn build(&self, _: &TlsSpec, _: &dyn Environment) -> Result<TransportPorts, TlsError> {
        self.started.store(true, Ordering::SeqCst);
        self.builds.fetch_add(1, Ordering::SeqCst);
        let until = Instant::now() + Duration::from_secs(5);
        while self.held && !self.released.load(Ordering::SeqCst) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        let transport: Arc<dyn Transport> = Arc::new(FakeTransport::new(|_, _| Ok(status(200))));
        self.refusal.clone().map_or_else(|| Ok(transport.into()), Err)
    }

    fn inspect(&self, probes: &[TlsProbe]) -> Vec<TlsFinding> {
        let rendered = |probe: &TlsProbe| {
            matches!(probe, TlsProbe::Pkcs12 { id, passphrase: Some(passphrase), .. }
                if id == "bank" && passphrase.expose() == PASS)
        };
        if probes.iter().any(rendered) {
            self.inspected.fetch_add(1, Ordering::SeqCst);
        }
        self.findings.clone()
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

/// A command runner that notes in its flag that it began, and never ends.
struct Hung(Arc<AtomicBool>);

#[async_trait]
impl CommandRunner for Hung {
    async fn run(&self, _: &CommandSpec) -> Result<CommandOutput, CommandError> {
        self.0.store(true, Ordering::SeqCst);
        std::future::pending().await
    }
}

/// An engine over the echo project with `config`, `P12_PASS` set to [`PASS`] and `loader`; and
/// the runs it begins in the history.
fn engine(config: &str, loader: Loader) -> (Engine, mpsc::UnboundedReceiver<String>) {
    engine_with(config, loader, |builder| builder)
}

/// [`engine`], with `finish` applied to its builder.
fn engine_with(
    config: &str,
    loader: Loader,
    finish: impl FnOnce(EngineBuilder) -> EngineBuilder,
) -> (Engine, mpsc::UnboundedReceiver<String>) {
    let files: [(&str, &[u8]); 5] = [
        ("vibranium.config.yaml", config.as_bytes()),
        (".env.schema", b"# a bundle's passphrase\nNOPE=\n"),
        ("services/echo/echo.api.yaml", API.as_bytes()),
        ("services/echo/send.test.yaml", TESTS.as_bytes()),
        ("load/echo/peak.load.yaml", LOAD.as_bytes()),
    ];
    let (begun, runs) = mpsc::unbounded_channel();
    let engine = finish(builder(FakeVcs::default()))
        .sources(Arc::new(MemorySources::new(&files)))
        .environment(Arc::new(MemoryEnvironment::new(&[("P12_PASS", PASS)])))
        .transport_factory(Arc::new(loader))
        .history(Arc::new(Begun(begun)), Arc::new(MemoryHistory::new()))
        .cache(Arc::new(MemoryCacheStore::new()))
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
    let secured = engine.secured(&mut prepared, &mut interrupts).await.unwrap().unwrap();
    let masked: Vec<&str> = prepared.plan.row_names.values().map(String::as_str).collect();
    assert_eq!(masked, ["••••"]);
    assert_eq!(secured.hidden.iter().map(Secret::expose).collect::<Vec<_>>(), [PASS]);
}

/// The error of each entry point over `config` with `loader` (run, load, call, `vc test auth`
/// and a cache clear), and whether any began a run.
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
    note(engine.test_auth(&TestAuthRequest { auth: "svc".to_owned(), ..TestAuthRequest::default() }).await.err());
    note(engine.cache_clear(None).await.err());
    assert_eq!(seen.len(), 5, "every entry point is refused");
    seen
}

/// A loader that refuses `bank`'s bundle as `Unusable`, else as a `Mismatch`.
fn refusing(unusable: bool) -> Loader {
    let (part, reason) = (TlsPart::Client { id: "bank".into(), field: Some("pkcs12") }, "it does not open".to_owned());
    let refusal = if unusable { TlsError::Unusable { part, reason } } else { TlsError::Mismatch { part, reason } };
    Loader { refusal: Some(refusal), ..Loader::default() }
}

#[tokio::test]
async fn a_tls_or_secret_refusal_stops_every_entry_point_before_any_run_begins() {
    let cases = [
        ("V5125", refused(&config(PASS), refusing(true)).await),
        ("V5126", refused(&config(PASS), refusing(false)).await),
        ("V5114", refused(&config("${secret.NOPE}"), Loader::default()).await),
        ("V5125", refused(&alerting(PASS), refusing(true)).await),
        ("V5114", refused(&alerting("${secret.NOPE}"), Loader::default()).await),
    ];
    for (code, seen) in cases {
        for (error, began) in seen {
            assert!(error.contains(code), "{code}: {error}");
            assert!(!began, "{code} began a run: {error}");
        }
    }
}

#[tokio::test]
async fn a_run_or_load_with_alerts_builds_only_the_transport_it_secured() {
    let loader = Loader::default();
    let builds = Arc::clone(&loader.builds);
    let (engine, _runs) = engine(&alerting(PASS), loader);
    engine.run(&RunRequest::default(), crate::interrupts().1).await.unwrap();
    assert_eq!(builds.swap(0, Ordering::SeqCst), 1, "the run's alerts use its secured transport");
    let watch = LoadWatch { interrupts: crate::interrupts().1, ticks: None };
    engine.load(&load_request(), watch).await.unwrap();
    assert_eq!(builds.load(Ordering::SeqCst), 1, "the load's alerts use its secured transport");
}

#[tokio::test]
async fn a_transport_that_cannot_be_built_stops_an_alerting_project_before_any_run_begins() {
    let refusal = TlsError::Transport { reason: "no client".to_owned() };
    let seen = refused(&alerting(PASS), Loader { refusal: Some(refusal), ..Loader::default() }).await;
    for (error, began) in seen {
        assert!(error.contains("no client"), "{error}");
        assert!(!began, "a run began: {error}");
    }
}

/// The 5th of November 2026, the sample clock's day, as days since 1970.
const TODAY_DAYS: i64 = 20_762;

#[tokio::test]
async fn run_load_call_and_test_auth_note_what_the_built_material_holds() {
    let part = |field| TlsPart::Client { id: "bank".into(), field: Some(field) };
    let day = 86_400;
    let findings = vec![
        TlsFinding::Validity { part: part("pkcs12"), not_before: 0, not_after: (TODAY_DAYS + 10) * day },
        TlsFinding::Exposed { part: part("pkcs12"), mode: 0o644 },
        TlsFinding::Validity { part: part("pkcs12"), not_before: 0, not_after: (TODAY_DAYS + 31) * day },
        TlsFinding::Unusable { part: part("pkcs12"), reason: "a build refuses it first".into() },
    ];
    let loader = Loader { findings, ..Loader::default() };
    let inspected = Arc::clone(&loader.inspected);
    let (secured, _runs) = engine(&config("${env.P12_PASS}"), loader);
    let (_interrupter, interrupts) = crate::interrupts();
    let run = secured.run(&RunRequest::default(), interrupts).await.unwrap().notices;
    let (_interrupter, interrupts) = crate::interrupts();
    let load = secured.load(&load_request(), LoadWatch { interrupts, ticks: None }).await.unwrap().notices;
    let call = secured.call(&call_request()).await.unwrap().notices;
    let auth = TestAuthRequest { auth: "svc".to_owned(), ..TestAuthRequest::default() };
    let auth = secured.test_auth(&auth).await.unwrap().notices;
    let want = [
        "V3107: client certificate `bank`, its `pkcs12` expires on 2026-11-15, within 30 days; renew it before then \
         and point the config at the new one; after that servers refuse it at the handshake",
        "V4012: client certificate `bank`, its `pkcs12`: its file is open to other users (mode 644); make it \
         readable by you alone with `chmod 600`",
    ];
    for (entry, notices) in [("run", run), ("load", load), ("call", call), ("test auth", auth)] {
        let ours = notices.iter().filter(|notice| notice.starts_with("V3") || notice.starts_with("V4"));
        assert_eq!(ours.collect::<Vec<_>>(), want, "{entry}");
    }
    assert_eq!(inspected.load(Ordering::SeqCst), 4, "each inspects the rendered bundle once");
    let loader =
        Loader { findings: vec![TlsFinding::Exposed { part: part("pkcs12"), mode: 0o644 }], ..Loader::default() };
    let inspected = Arc::clone(&loader.inspected);
    let (plain, _runs) = engine(&untlsed(), loader);
    let (_interrupter, interrupts) = crate::interrupts();
    let notices = plain.run(&RunRequest::default(), interrupts).await.unwrap().notices;
    assert!(notices.iter().all(|notice| !notice.starts_with("V4012")), "no http.tls, nothing inspected: {notices:?}");
    assert_eq!(inspected.load(Ordering::SeqCst), 0);
}

/// [`config`] without its `http.tls`.
fn untlsed() -> String {
    config(PASS).split("http:\n").next().unwrap_or_default().to_owned()
}

#[tokio::test]
async fn a_refusal_when_http_tls_is_not_set_is_internal_not_a_setting() {
    for (error, began) in refused(&untlsed(), refusing(true)).await {
        assert!(error.contains("it does not open"), "{error}");
        assert!(!error.contains("V5125") && !error.contains("http.tls"), "no http.tls to blame: {error}");
        assert!(!began, "a run began: {error}");
    }
}

#[tokio::test]
async fn the_plain_transport_is_built_off_the_runtime_and_its_refusal_is_internal() {
    let (slow, started, released) = held();
    let (engine, _runs) = engine(&config(PASS), slow);
    let environment: Arc<dyn Environment> = Arc::new(MemoryEnvironment::new(&[]));
    let release = async {
        for _ in 0..2000 {
            if started.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        released.store(true, Ordering::SeqCst);
    };
    let begun = Instant::now();
    let (built, ()) = tokio::join!(engine.transport(environment), release);
    assert!(begun.elapsed() < Duration::from_secs(4), "the build blocked the runtime");
    assert!(matches!(built, Err(EngineError::Internal { .. })), "{:?}", built.err());
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

#[tokio::test(flavor = "multi_thread")]
async fn an_interrupt_while_the_secrets_load_cancels_run_and_load_before_any_run_begins() {
    let config = config("${secret.NOPE}").replacen('\n', "\nsecrets: {provider: varlock}\n", 1);
    let started = Arc::new(AtomicBool::new(false));
    let hung = Arc::new(Hung(Arc::clone(&started)));
    let (engine, mut begun) = engine_with(&config, Loader::default(), |builder| builder.commands(hung));
    let (run, load) = (RunRequest::default(), load_request());
    let (interrupter, interrupts) = crate::interrupts();
    let (report, ()) = tokio::join!(engine.run(&run, interrupts), press_once(&started, interrupter));
    let report = report.unwrap();
    assert_eq!((report.exit, report.run.is_some()), (130, false));
    assert!(started.swap(false, Ordering::SeqCst), "the secrets were loading");
    let (interrupter, interrupts) = crate::interrupts();
    let watch = LoadWatch { interrupts, ticks: None };
    let (cancelled, ()) = tokio::join!(engine.load(&load, watch), press_once(&started, interrupter));
    assert_eq!(cancelled.map(|report| (report.ending, report.exit)).ok(), Some((LoadEnding::Cancelled, 130)));
    assert!(started.load(Ordering::SeqCst) && begun.try_recv().is_err(), "the load secrets loaded, and no run began");
}
