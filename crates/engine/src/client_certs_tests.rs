use std::path::Path;
use std::sync::Arc;

use vc_adapter_http::HttpAdapterError;
use vc_executor::RunInputs;
use vc_interfaces::{CertId, SourceProvider};
use vc_model::Date;
use vc_testkit::{FakeVcs, FixedClock, MemoryCompileCache, MemoryEnvironment, MemorySources, TestPki};

use super::{RunTls, transport_error};
use crate::plan::Extra;
use crate::{EngineBuilder, EngineError, PlanRequest};

const P12: &[u8] = include_bytes!("../../adapters/http/tests/fixtures/ec.p12");

/// The config with `tls` under `http.tls`, and the profiles `dev` (the default) and `staging`.
fn config(tls: &str) -> String {
    format!("syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n  - id: staging\nhttp:\n  tls:\n{tls}")
}

/// A profile, and the environment's variables.
type Under<'a> = (&'a str, &'a [(&'a str, &'a str)]);

/// `http.tls` of a project in `root` whose config holds `tls`, under `profile`, with `env`.
async fn run_tls(root: &Path, tls: &str, (profile, env): Under<'_>) -> Result<RunTls, EngineError> {
    let config = config(tls);
    let sources: Arc<dyn SourceProvider> =
        Arc::new(MemorySources::new(&[("vibranium.config.yaml", config.as_bytes())]));
    let engine = EngineBuilder::new(root.to_path_buf())
        .sources(sources)
        .compile_cache(Arc::new(MemoryCompileCache::new()))
        .clock(Arc::new(FixedClock(Date::new(2026, 11, 5).unwrap())))
        .environment(Arc::new(MemoryEnvironment::new(env)))
        .vcs(Arc::new(FakeVcs::default()))
        .build()?;
    let request = PlanRequest { profiles: vec![profile.to_owned()], ..PlanRequest::default() };
    let prepared = engine.prepare(&request, Extra::default())?;
    let inputs = RunInputs::new(Arc::clone(&prepared.draw.random), Arc::clone(&prepared.environment));
    engine.run_tls(&prepared, &inputs.with_profiles(prepared.profiles.clone())).await
}

/// A project folder holding the test CA, the `good` certificate and the PKCS#12 fixture.
fn folder(pki: &TestPki) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("certs")).unwrap();
    std::fs::write(dir.path().join("certs/ca.pem"), pki.ca_pem()).unwrap();
    std::fs::write(dir.path().join("certs/good.pem"), pki.good().cert_pem()).unwrap();
    std::fs::write(dir.path().join("certs/bank.p12"), P12).unwrap();
    dir
}

const CLIENTS: &str = "    ca: certs/ca.pem\n    clients:\n\
                       \x20     - id: good\n        hosts: [a.example]\n        cert: certs/good.pem\n        key: ${env.GOOD_KEY}\n\
                       \x20     - id: stage\n        profiles: [staging]\n        cert: certs/good.pem\n        key: ${env.GOOD_KEY}\n\
                       \x20     - id: bank\n        pkcs12: certs/bank.p12\n        passphrase: fixture\n";

#[tokio::test]
async fn loads_the_roots_and_the_identities_the_active_profiles_allow() {
    let pki = TestPki::new().unwrap();
    let dir = folder(&pki);
    let key = pki.good().key_pem();
    let tls = run_tls(dir.path(), CLIENTS, ("dev", &[("GOOD_KEY", key)])).await.unwrap();
    assert_eq!(tls.options.ca, [pki.ca_pem().into_bytes()]);
    let ids: Vec<_> = tls.options.clients.iter().map(vc_adapter_http::ClientIdentity::id).collect();
    assert_eq!(ids, ["good", "bank"]);
    let certs: Vec<_> = tls.certs.values().copied().collect();
    assert_eq!((tls.certs.len(), certs.contains(&CertId::new(0)), certs.contains(&CertId::new(1))), (2, true, true));
    let hidden: Vec<_> = tls.passphrases.iter().map(|passphrase| passphrase.as_str()).collect();
    assert_eq!(hidden, ["fixture"]);
    let staging = run_tls(dir.path(), CLIENTS, ("staging", &[("GOOD_KEY", key)])).await.unwrap();
    assert_eq!(staging.options.clients.len(), 3);
}

#[tokio::test]
async fn a_root_alone_loads_and_no_tls_loads_nothing() {
    let pki = TestPki::new().unwrap();
    let dir = folder(&pki);
    let inline = format!("    ca: \"{}\"\n", pki.ca_pem().replace('\n', "\\n"));
    let tls = run_tls(dir.path(), &inline, ("dev", &[])).await.unwrap();
    assert_eq!((tls.options.ca, tls.options.clients.len()), (vec![pki.ca_pem().into_bytes()], 0));
    let none = run_tls(dir.path(), "    ca: []\n", ("dev", &[])).await.unwrap();
    assert!(none.options.ca.is_empty() && none.certs.is_empty());
}

/// The reason `tls` stops the run with, under `dev` with `GOOD_KEY` set to `key`.
async fn refusal(dir: &Path, tls: &str, key: &str) -> String {
    match run_tls(dir, tls, ("dev", &[("GOOD_KEY", key)])).await {
        Err(EngineError::Setting { origin, reason, help }) if origin == "http.tls" && help.is_some() => reason,
        other => format!("unexpected {:?}", other.map(|tls| tls.options)),
    }
}

#[tokio::test]
async fn material_that_cannot_be_read_or_does_not_fit_stops_the_run() {
    let pki = TestPki::new().unwrap();
    let dir = folder(&pki);
    let key = pki.good().key_pem();
    let gone = CLIENTS.replace("certs/ca.pem", "certs/gone.pem");
    assert!(refusal(dir.path(), &gone, key).await.starts_with("V5125: a `ca` certificate: cannot read `"));
    let gone = CLIENTS.replace("pkcs12: certs/bank.p12", "pkcs12: certs/gone.p12");
    assert!(refusal(dir.path(), &gone, key).await.starts_with("V5125: client certificate `bank`: cannot read `"));
    let wrong = CLIENTS.replace("passphrase: fixture", "passphrase: wrong");
    let reason = refusal(dir.path(), &wrong, key).await;
    assert!(reason.starts_with("V5126: client certificate `bank`: the passphrase is wrong"), "{reason}");
    let reason = refusal(dir.path(), CLIENTS, "").await;
    assert!(reason.starts_with("V5125: client certificate `good`: cannot read `"), "{reason}");
    let unset = CLIENTS.replace("${env.GOOD_KEY}", "${env.NOPE}");
    let reason = refusal(dir.path(), &unset, key).await;
    assert!(reason == "V5125: client certificate `good`: `env.NOPE` is not set", "{reason}");
}

#[test]
fn transport_errors_name_the_code_and_the_material() {
    let reason = |error: &HttpAdapterError| match transport_error(error) {
        EngineError::Setting { reason, .. } => reason,
        other => format!("internal: {other}"),
    };
    let identity = HttpAdapterError::Identity { id: "good".into(), reason: "the private key is encrypted".into() };
    assert_eq!(reason(&identity), "V5125: client certificate `good`: the private key is encrypted");
    let mismatch = HttpAdapterError::Mismatch { id: "good".into(), reason: "keys do not match".into() };
    assert_eq!(reason(&mismatch), "V5126: client certificate `good`: keys do not match");
    let ca = HttpAdapterError::Certificate { index: 1, reason: "bad PEM".into() };
    assert_eq!(reason(&ca), "V5125: `ca` certificate 2: bad PEM");
    assert_eq!(reason(&HttpAdapterError::NoHost("/x".into())), "internal: the URL /x has no host");
}
