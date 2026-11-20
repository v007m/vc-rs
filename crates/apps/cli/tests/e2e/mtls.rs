//! Client certificates end to end (#105, D350–D354, D390–D393): a run reads `http.tls` when it
//! starts, trusts its `ca` and presents the identity its auth names, else the first whose
//! `hosts` match under an active profile, to a mock that asks for one and says whose it got.
//! Material that won't load, or whose parts don't fit, stops the run before any request.

use vc_mock_server::{ClientAuth, MockHandle, MockServer, MockTls};
use vc_testkit::TestPki;

use crate::{Project, said};

/// The mock over TLS, trusting the test CA and naming its `good` client, asking for a client
/// certificate as firmly as `auth` says.
async fn mock(pki: &TestPki, auth: ClientAuth) -> MockHandle {
    let server = pki.server();
    let tls = MockTls::new(vec![server.cert_der().to_vec()], server.key_der().to_vec())
        .client_auth(auth, vec![pki.ca_der().to_vec()])
        .name_client(pki.good().name(), pki.good().cert_der().to_vec());
    MockServer::start_tls(tls).await.unwrap()
}

/// `tls#whoami` with the auth `auth` (none when empty), and `tls#echo` with it too.
fn apis(auth: &str) -> String {
    let auth = if auth.is_empty() { String::new() } else { format!("    auth: {auth}\n") };
    format!(
        "baseUrl: ${{var.baseUrl}}\napis:\n  - id: whoami\n    path: /whoami\n{auth}  - id: echo\n    path: /echo\n{auth}"
    )
}

/// Tests `saysGood`, expecting the `good` client, and `isForbidden`, expecting a 403.
const TESTS: &str = "tests:\n  - id: saysGood\n    call: tls#whoami\n    expect: {body: {match: {client: good}}}\n\
                     \x20 - id: isForbidden\n    call: tls#whoami\n    expect: {status: 403}\n";

/// A project trusting the test CA through an absolute path, with the `clients` entries
/// `clients`, the auth definitions `auth` and the `tls` apis using `uses`; the `good`
/// certificate and key are in `certs/`.
fn project(pki: &TestPki, (clients, auth): (&str, &str), uses: &str) -> Project {
    let project = Project::new(&[]);
    project.write("certs/ca.pem", &pki.ca_pem());
    project.write("certs/good.pem", pki.good().cert_pem());
    project.write("certs/good.key", pki.good().key_pem());
    let ca = project.path().join("certs/ca.pem");
    let config = format!(
        "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n  - id: staging\n\
         vars: {{baseUrl: 'http://x'}}\nhttp:\n  tls:\n    ca: '{}'\n    clients:\n{clients}{auth}",
        ca.display()
    );
    project.write("vibranium.config.yaml", &config);
    project.write("services/tls/tls.api.yaml", &apis(uses));
    project.write("services/tls/who.test.yaml", TESTS);
    project
}

/// The `good` entry, sent to `hosts` under `profiles` (every profile when empty).
fn good(hosts: &str, profiles: &str) -> String {
    let profiles = if profiles.is_empty() { String::new() } else { format!("        profiles: [{profiles}]\n") };
    format!(
        "      - id: good\n        hosts: [{hosts}]\n{profiles}        cert: certs/good.pem\n        key: certs/good.key\n"
    )
}

/// `vc run` of `project` against `server`, selecting `tests` with `args`.
fn run(project: &Project, server: &MockHandle, args: &[&str]) -> std::process::Output {
    project.run(&server.base_url(), args).output().unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_host_rule_presents_the_certificate_to_a_server_the_ca_vouches_for() {
    let pki = TestPki::new().unwrap();
    let server = mock(&pki, ClientAuth::Required).await;
    let project = project(&pki, (&good("'127.0.0.1'", ""), ""), "");
    let run = run(&project, &server, &["tls.who#saysGood"]);
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_identity_goes_only_with_its_profiles() {
    let pki = TestPki::new().unwrap();
    let server = mock(&pki, ClientAuth::Optional).await;
    let project = project(&pki, (&good("'127.0.0.1'", "staging"), ""), "");
    let dev = run(&project, &server, &["tls.who#isForbidden", "--profile", "dev"]);
    assert_eq!(dev.status.code(), Some(0), "not loaded under dev: {}", said(&dev));
    let staging = run(&project, &server, &["tls.who#saysGood", "--profile", "staging"]);
    assert_eq!(staging.status.code(), Some(0), "{}", said(&staging));
}

/// `good` has no hosts, so only an auth that names it sends it.
#[tokio::test(flavor = "multi_thread")]
async fn an_auth_names_its_certificate_alongside_its_own_credentials() {
    let pki = TestPki::new().unwrap();
    let server = mock(&pki, ClientAuth::Optional).await;
    let auth = "auth:\n  - id: both\n    type: bearer\n    token: t0ken-abc\n    clientCert: good\n\
                \x20 - id: certOnly\n    type: mtls\n    clientCert: good\n";
    for (uses, wanted) in [("", "tls.who#isForbidden"), ("certOnly", "tls.who#saysGood"), ("both", "tls.who#saysGood")]
    {
        let project = project(&pki, (&good("'*.example'", ""), auth), uses);
        let run = run(&project, &server, &[wanted]);
        assert_eq!(run.status.code(), Some(0), "{uses}: {}", said(&run));
    }
    let project = project(&pki, (&good("'*.example'", ""), auth), "both");
    project.write("services/tls/echo.test.yaml", "tests:\n  - id: sendsBoth\n    call: tls#echo\n    expect: {body: {match: {headers.authorization: Bearer t0ken-abc}}}\n");
    let run = run(&project, &server, &["tls.echo#sendsBoth"]);
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
}

#[tokio::test(flavor = "multi_thread")]
async fn client_cert_none_sends_nothing_even_where_a_host_rule_matches() {
    let pki = TestPki::new().unwrap();
    let server = mock(&pki, ClientAuth::Optional).await;
    let auth = "auth:\n  - id: anonymous\n    type: bearer\n    token: t0ken-abc\n    clientCert: none\n";
    let project = project(&pki, (&good("'127.0.0.1'", ""), auth), "anonymous");
    let run = run(&project, &server, &["tls.who#isForbidden"]);
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    let required = mock(&pki, ClientAuth::Required).await;
    let refused = self::run(&project, &required, &["tls.who#isForbidden"]);
    assert_eq!(refused.status.code(), Some(1), "{}", said(&refused));
    assert!(said(&refused).contains("V5127"), "{}", said(&refused));
}

/// A named identity limited to another profile is not loaded, so the calls that need it fail.
#[tokio::test(flavor = "multi_thread")]
async fn a_named_identity_another_profile_holds_fails_its_calls() {
    let pki = TestPki::new().unwrap();
    let server = mock(&pki, ClientAuth::Optional).await;
    let auth = "auth:\n  - id: certOnly\n    type: mtls\n    clientCert: good\n";
    let project = project(&pki, (&good("'*.example'", "staging"), auth), "certOnly");
    let run = run(&project, &server, &["tls.who#saysGood"]);
    assert_eq!(run.status.code(), Some(1), "{}", said(&run));
    assert!(said(&run).contains("V5114") && said(&run).contains("--profile staging"), "{}", said(&run));
    assert!(server.stats().await.unwrap().requests.is_empty(), "no call was sent");
}

#[tokio::test(flavor = "multi_thread")]
async fn material_that_will_not_load_stops_the_run_before_any_request() {
    let pki = TestPki::new().unwrap();
    let server = mock(&pki, ClientAuth::Optional).await;
    let project = project(&pki, (&good("'127.0.0.1'", ""), ""), "");
    let other = pki.other_ca().key_pem().to_owned();
    for (key, code) in [("certs/gone.key", "V5125"), ("certs/other.key", "V5126")] {
        project.write("certs/other.key", &other);
        let config = std::fs::read_to_string(project.path().join("vibranium.config.yaml")).unwrap();
        project.write("vibranium.config.yaml", &config.replace("certs/good.key", key));
        let run = run(&project, &server, &[]);
        assert_eq!(run.status.code(), Some(2), "{key}: {}", said(&run));
        assert!(said(&run).contains(code) && said(&run).contains("`good`"), "{key}: {}", said(&run));
        assert!(!said(&run).contains("PRIVATE KEY"), "{key}: {}", said(&run));
        project.write("vibranium.config.yaml", &config);
    }
    assert!(server.stats().await.unwrap().requests.is_empty(), "no request was sent");
}
