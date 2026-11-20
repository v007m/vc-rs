use std::path::Path;
use std::sync::Arc;

use vc_adapter_http::TlsOptions;
use vc_interfaces::{CertId, LoadedTls, Secrets, SourceProvider};
use vc_model::Date;
use vc_testkit::{
    FakeVcs, FixedClock, MemoryCompileCache, MemoryEnvironment, MemorySources, MemoryTls, MemoryTlsLoader, TestPki,
};

use super::RunTls;
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
async fn run_tls(root: &Path, tls: &str, under: Under<'_>) -> Result<RunTls, EngineError> {
    run_with(EngineBuilder::new(root.to_path_buf()), tls, under).await
}

/// `http.tls` of the project `builder` builds, whose config holds `tls`, under `profile`, with `env`.
async fn run_with(builder: EngineBuilder, tls: &str, (profile, env): Under<'_>) -> Result<RunTls, EngineError> {
    let config = config(tls);
    let sources: Arc<dyn SourceProvider> =
        Arc::new(MemorySources::new(&[("vibranium.config.yaml", config.as_bytes())]));
    let engine = builder
        .sources(sources)
        .compile_cache(Arc::new(MemoryCompileCache::new()))
        .clock(Arc::new(FixedClock(Date::new(2026, 11, 5).unwrap())))
        .environment(Arc::new(MemoryEnvironment::new(env)))
        .vcs(Arc::new(FakeVcs::default()))
        .build()?;
    let request = PlanRequest { profiles: vec![profile.to_owned()], ..PlanRequest::default() };
    let prepared = engine.prepare(&request, Extra::default())?;
    engine.run_tls(&prepared, &Secrets::new()).await
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
    let mut tls = run_tls(dir.path(), CLIENTS, ("dev", &[("GOOD_KEY", key)])).await.unwrap();
    let options = http_options(&mut tls);
    assert_eq!(options.ca, [pki.ca_pem().into_bytes()]);
    let ids: Vec<_> = options.clients.iter().map(vc_adapter_http::ClientIdentity::id).collect();
    assert_eq!(ids, ["good", "bank"]);
    let certs: Vec<_> = tls.certs.values().copied().collect();
    assert_eq!((tls.certs.len(), certs.contains(&CertId::new(0)), certs.contains(&CertId::new(1))), (2, true, true));
    let hidden: Vec<_> = tls.passphrases.iter().map(|passphrase| passphrase.as_str()).collect();
    assert_eq!(hidden, ["fixture"]);
    let mut staging = run_tls(dir.path(), CLIENTS, ("staging", &[("GOOD_KEY", key)])).await.unwrap();
    assert_eq!(http_options(&mut staging).clients.len(), 3);
}

#[tokio::test]
async fn a_root_alone_loads_and_no_tls_loads_nothing() {
    let pki = TestPki::new().unwrap();
    let dir = folder(&pki);
    let inline = format!("    ca: \"{}\"\n", pki.ca_pem().replace('\n', "\\n"));
    let options = http_options(&mut run_tls(dir.path(), &inline, ("dev", &[])).await.unwrap());
    assert_eq!((options.ca, options.clients.len()), (vec![pki.ca_pem().into_bytes()], 0));
    let none = run_tls(dir.path(), "    ca: []\n", ("dev", &[])).await.unwrap();
    assert!(none.loaded.is_none() && none.certs.is_empty());
}

/// The reason `tls` stops the run with, under `dev` with `GOOD_KEY` set to `key`.
async fn refusal(dir: &Path, tls: &str, key: &str) -> String {
    match run_tls(dir, tls, ("dev", &[("GOOD_KEY", key)])).await {
        Err(EngineError::Setting { origin, reason, help }) if origin == "http.tls" && help.is_some() => reason,
        other => format!("unexpected {:?}", other.map(|tls| tls.loaded)),
    }
}

#[tokio::test]
async fn material_that_cannot_be_read_or_does_not_fit_stops_the_run() {
    let pki = TestPki::new().unwrap();
    let dir = folder(&pki);
    let key = pki.good().key_pem();
    let gone = CLIENTS.replace("certs/ca.pem", "certs/gone.pem");
    assert!(refusal(dir.path(), &gone, key).await.starts_with("V5125: `ca` certificate 1: cannot read `"));
    let gone = CLIENTS.replace("pkcs12: certs/bank.p12", "pkcs12: certs/gone.p12");
    assert!(
        refusal(dir.path(), &gone, key)
            .await
            .starts_with("V5125: client certificate `bank`, its `pkcs12`: cannot read `")
    );
    let wrong = CLIENTS.replace("passphrase: fixture", "passphrase: wrong");
    let reason = refusal(dir.path(), &wrong, key).await;
    assert!(reason.starts_with("V5126: client certificate `bank`, its `pkcs12`: the passphrase is wrong"), "{reason}");
    let reason = refusal(dir.path(), CLIENTS, "").await;
    assert!(reason.starts_with("V5125: client certificate `good`, its `key`: cannot read its file: "), "{reason}");
    let reason = refusal(dir.path(), CLIENTS, pki.other_ca().key_pem()).await;
    assert!(reason.starts_with("V5126: client certificate `good`: "), "the key is checked as it loads: {reason}");
    std::fs::write(dir.path().join("certs/bad.pem"), "not a certificate").unwrap();
    let bad = CLIENTS.replace("certs/ca.pem", "certs/bad.pem");
    let reason = refusal(dir.path(), &bad, key).await;
    assert_eq!(reason, "V5125: `ca` certificate 1: no certificate in it parses", "a `ca` entry is parsed as it loads");
    let rendered = CLIENTS.replace("cert: certs/good.pem", "cert: ${env.GOOD_KEY}");
    let reason = refusal(dir.path(), &rendered, "certs/hidden-name.pem").await;
    assert!(reason.starts_with("V5125: client certificate `good`, its `cert`: cannot read its file: "), "{reason}");
    let unset = CLIENTS.replace("${env.GOOD_KEY}", "${env.NOPE}");
    let reason = refusal(dir.path(), &unset, key).await;
    assert!(reason == "V5125: client certificate `good`, its `key`: `env.NOPE` is not set", "{reason}");
}

/// The HTTP transport's options `tls` loaded.
fn http_options(tls: &mut RunTls) -> TlsOptions {
    tls.loaded.take().and_then(LoadedTls::take).unwrap_or_default()
}

/// A builder for the project at `/project` whose TLS loader holds its CA and the bank's bundle.
fn in_memory() -> EngineBuilder {
    let root = Path::new("/project");
    let files = [(root.join("certs/ca.pem"), &b"root"[..]), (root.join("certs/bank.p12"), &b"bundle"[..])];
    EngineBuilder::new(root.to_path_buf()).tls_loader(Arc::new(MemoryTlsLoader::new(&files)))
}

#[tokio::test]
async fn the_injected_loader_reads_the_rendered_material() {
    let tls = run_with(in_memory(), CLIENTS, ("dev", &[("GOOD_KEY", "-----BEGIN PRIVATE KEY-----")])).await;
    let reason = match tls {
        Err(EngineError::Setting { reason, .. }) => reason,
        other => format!("unexpected {:?}", other.map(|tls| tls.loaded)),
    };
    assert_eq!(reason, "V5125: client certificate `good`, its `cert`: cannot read `certs/good.pem`: not found");
    let only_bank = CLIENTS.split("      - id: good").next().unwrap_or_default().to_owned()
        + "      - id: bank\n        pkcs12: certs/bank.p12\n        passphrase: fixture\n";
    let mut tls = run_with(in_memory(), &only_bank, ("dev", &[])).await.unwrap();
    let memory = tls.loaded.take().and_then(LoadedTls::take::<MemoryTls>).unwrap_or_default();
    assert_eq!((memory.ca, memory.clients), (vec![b"root".to_vec()], vec![("bank".to_owned(), Vec::new())]));
}

#[test]
fn a_value_that_does_not_render_is_quoted_only_by_the_path_it_reads() {
    let missing = vc_template::RenderError::missing("`env.NOPE` is not set");
    assert_eq!(super::render_reason(&missing), "`env.NOPE` is not set");
    let invalid = vc_template::RenderError::invalid("cannot decode \"-----BEGIN PRIVATE KEY-----\"");
    assert_eq!(super::render_reason(&invalid), "it does not render");
}
