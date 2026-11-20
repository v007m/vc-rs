use std::path::Path;
use std::sync::Arc;

use tokio::sync::mpsc::UnboundedReceiver;
use vc_interfaces::{CertId, Secret, Secrets, SourceProvider};
use vc_model::Date;
use vc_testkit::{
    FakeTransport, FakeVcs, FixedClock, MemoryCompileCache, MemoryEnvironment, MemorySources, MemoryTls,
    MemoryTransports, TestPki, status,
};

use crate::plan::Extra;
use crate::secured::Secured;
use crate::{EngineBuilder, EngineError, PlanRequest};

const P12: &[u8] = include_bytes!("../../adapters/http/tests/fixtures/ec.p12");

/// The config with `tls` under `http.tls`, and the profiles `dev` (the default) and `staging`.
fn config(tls: &str) -> String {
    format!("syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n  - id: staging\nhttp:\n  tls:\n{tls}")
}

/// A profile, and the environment's variables.
type Under<'a> = (&'a str, &'a [(&'a str, &'a str)]);

/// `http.tls` of a project in `root` whose config holds `tls`, under `profile`, with `env`.
async fn run_tls(root: &Path, tls: &str, under: Under<'_>) -> Result<Secured, EngineError> {
    run_with(EngineBuilder::new(root.to_path_buf()), tls, under).await
}

/// `http.tls` of the project `builder` builds, whose config holds `tls`, under `profile`, with `env`.
async fn run_with(builder: EngineBuilder, tls: &str, (profile, env): Under<'_>) -> Result<Secured, EngineError> {
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
    engine.with_tls(&prepared, Secrets::new()).await
}

/// A builder for the project at `root` whose transport factory holds `files`, under `root`, and
/// what each of its builds read.
fn memory(root: &Path, files: &[(&str, &[u8])]) -> (EngineBuilder, UnboundedReceiver<MemoryTls>) {
    let files: Vec<_> = files.iter().map(|(path, bytes)| (root.join(path), *bytes)).collect();
    let (factory, read) = MemoryTransports::new(&files, Arc::new(FakeTransport::new(|_, _| Ok(status(200)))));
    (EngineBuilder::new(root.to_path_buf()).transport_factory(Arc::new(factory)), read)
}

/// A factory holding the test CA, the `good` certificate and the PKCS#12 fixture.
fn memory_pki(pki: &TestPki) -> (EngineBuilder, UnboundedReceiver<MemoryTls>) {
    let (ca, good) = (pki.ca_pem(), pki.good().cert_pem());
    memory(
        Path::new("/project"),
        &[("certs/ca.pem", ca.as_bytes()), ("certs/good.pem", good.as_bytes()), ("certs/bank.p12", P12)],
    )
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
async fn builds_with_the_roots_and_the_identities_the_active_profiles_allow() {
    let pki = TestPki::new().unwrap();
    let key = pki.good().key_pem();
    let (builder, mut read) = memory_pki(&pki);
    let tls = run_with(builder, CLIENTS, ("dev", &[("GOOD_KEY", key)])).await.unwrap();
    let built = read.try_recv().unwrap();
    assert_eq!(built.ca, [pki.ca_pem().into_bytes()]);
    let clients = [("good".to_owned(), vec!["a.example".to_owned()]), ("bank".to_owned(), Vec::new())];
    assert_eq!(built.clients, clients);
    let certs: Vec<_> = tls.certs.values().copied().collect();
    assert_eq!((tls.certs.len(), certs.contains(&CertId::new(0)), certs.contains(&CertId::new(1))), (2, true, true));
    let hidden: Vec<_> = tls.hidden.iter().map(Secret::expose).collect();
    assert_eq!(hidden.first(), Some(&key), "the key rendered from the environment (D401)");
    assert_eq!(hidden.last(), Some(&"fixture"), "and the passphrase");
    let (builder, mut read) = memory_pki(&pki);
    run_with(builder, CLIENTS, ("staging", &[("GOOD_KEY", key)])).await.unwrap();
    assert_eq!(read.try_recv().unwrap().clients.len(), 3);
    let dir = folder(&pki);
    assert!(run_tls(dir.path(), CLIENTS, ("dev", &[("GOOD_KEY", key)])).await.is_ok(), "the HTTP factory builds it");
}

#[tokio::test]
async fn a_root_alone_builds_and_no_tls_builds_a_plain_transport() {
    let pki = TestPki::new().unwrap();
    let inline = format!("    ca: \"{}\"\n", pki.ca_pem().replace('\r', "\\r").replace('\n', "\\n"));
    let (builder, mut read) = memory_pki(&pki);
    run_with(builder, &inline, ("dev", &[])).await.unwrap();
    let built = read.try_recv().unwrap();
    assert_eq!((built.ca, built.clients.len()), (vec![pki.ca_pem().into_bytes()], 0));
    let (builder, mut read) = memory_pki(&pki);
    let none = run_with(builder, "    ca: []\n", ("dev", &[])).await.unwrap();
    assert_eq!((read.try_recv().unwrap(), none.certs.len()), (MemoryTls::default(), 0));
}

/// The reason `tls` stops the run with, under `dev` with `GOOD_KEY` set to `key`.
async fn refusal(dir: &Path, tls: &str, key: &str) -> String {
    match run_tls(dir, tls, ("dev", &[("GOOD_KEY", key)])).await {
        Err(EngineError::Setting { origin, reason, help }) if origin == "http.tls" && help.is_some() => reason,
        other => format!("unexpected {:?}", other.err()),
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
    assert!(reason == "V5125: client certificate `good`, its `key`: `${env.NOPE}` does not render", "{reason}");
    let deeper = CLIENTS.replace("${env.GOOD_KEY}", "-----BEGIN ${env.GOOD_KEY.pem | trim}");
    let reason = refusal(dir.path(), &deeper, key).await;
    let want = "V5125: client certificate `good`, its `key`: `${env.GOOD_KEY.pem | trim}` does not render";
    assert!(reason == want, "only the expression, never the text around it or what it read: {reason}");
}

#[tokio::test]
async fn a_rendered_path_is_trimmed_and_a_pkcs12_value_is_always_a_path() {
    let pki = TestPki::new().unwrap();
    let rendered = "    ca: ${env.CA}\n    clients:\n      - id: bank\n        pkcs12: ${env.P12}\n\
                    \x20       passphrase: fixture\n";
    let env = [("CA", " certs/ca.pem\n"), ("P12", "\tcerts/bank.p12 ")];
    let (builder, mut read) = memory_pki(&pki);
    run_with(builder, rendered, ("dev", &env)).await.unwrap();
    let built = read.try_recv().unwrap();
    assert_eq!((built.ca.len(), built.clients.len()), (1, 1), "both paths are read once trimmed");
    let env = [("CA", "certs/ca.pem"), ("P12", "-----BEGIN PKCS12-----\nMIIB\n")];
    let reason = match run_with(memory_pki(&pki).0, rendered, ("dev", &env)).await {
        Err(EngineError::Setting { reason, .. }) => reason,
        other => format!("unexpected {:?}", other.err()),
    };
    assert!(reason.starts_with("V5125: client certificate `bank`, its `pkcs12`: cannot read its file: "), "{reason}");
}

#[tokio::test]
async fn the_injected_factory_reads_the_rendered_material() {
    let files: [(&str, &[u8]); 2] = [("certs/ca.pem", b"root"), ("certs/bank.p12", b"bundle")];
    let (builder, _read) = memory(Path::new("/project"), &files);
    let tls = run_with(builder, CLIENTS, ("dev", &[("GOOD_KEY", "-----BEGIN PRIVATE KEY-----")])).await;
    let reason = match tls {
        Err(EngineError::Setting { reason, .. }) => reason,
        other => format!("unexpected {:?}", other.err()),
    };
    assert_eq!(reason, "V5125: client certificate `good`, its `cert`: cannot read `certs/good.pem`: not found");
    let only_bank = CLIENTS.split("      - id: good").next().unwrap_or_default().to_owned()
        + "      - id: bank\n        pkcs12: certs/bank.p12\n        passphrase: fixture\n";
    let (builder, mut read) = memory(Path::new("/project"), &files);
    run_with(builder, &only_bank, ("dev", &[])).await.unwrap();
    let built = read.try_recv().unwrap();
    assert_eq!((built.ca, built.clients), (vec![b"root".to_vec()], vec![("bank".to_owned(), Vec::new())]));
}

#[test]
fn a_transport_that_cannot_be_built_for_another_reason_is_internal() {
    let error = super::tls_error(&vc_interfaces::TlsError::Transport { reason: "no native roots".into() });
    assert!(matches!(&error, EngineError::Internal { reason } if reason == "no native roots"), "{error:?}");
}

#[test]
fn a_held_key_hides_its_trimmed_and_lf_copies_and_each_long_body_line() {
    let key = "-----BEGIN PRIVATE KEY-----\r\nMIGHAgEAMBMGByqGSM49AgEGCCqGSM49\r\nAwEHBG0wawIBAQQg\r\nAwEHBG0wawIBAQ=\r\n\
               -----END PRIVATE KEY-----\r\n";
    let held = format!(" {key}");
    let inline = |text: &str| vc_interfaces::MaterialSource::Inline(Secret::new(text));
    let material = vc_interfaces::ClientSource::Pem { cert: inline("c"), key: inline(&held) };
    let client = vc_interfaces::ClientSpec { id: "good".into(), hosts: Vec::new(), material };
    let hidden: Vec<String> = super::hidden(&client).iter().map(|secret| secret.expose().to_owned()).collect();
    let trimmed = key.trim();
    let lf = |text: &str| text.replace("\r\n", "\n");
    let lines = ["MIGHAgEAMBMGByqGSM49AgEGCCqGSM49", "AwEHBG0wawIBAQQg"];
    let want = [held.clone(), trimmed.to_owned(), lf(&held), lf(trimmed)].into_iter().chain(lines.map(str::to_owned));
    assert_eq!(hidden, want.collect::<Vec<_>>(), "a 15-character line and the PEM lines are not hidden");
    let passphrase = vc_interfaces::ClientSource::Pkcs12 { bundle: inline("b"), passphrase: Secret::new("pw") };
    let bundle = vc_interfaces::ClientSpec { id: "bank".into(), hosts: Vec::new(), material: passphrase };
    assert_eq!(super::hidden(&bundle).iter().map(Secret::expose).collect::<Vec<_>>(), ["pw"], "each form once");
}
