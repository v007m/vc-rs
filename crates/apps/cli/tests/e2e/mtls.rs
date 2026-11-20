//! Client certificates end to end (#105, D350–D354, D390–D393): a run reads `http.tls` when it
//! starts, trusts its `ca` and presents the identity its auth names, else the first whose
//! `hosts` match under an active profile, to a mock that asks for one and says whose it got.
//! Material that won't load, or whose parts don't fit, stops the run before any request. An
//! oauth2 token request presents the auth's identity too; a PKCS#12 bundle works like a PEM
//! pair; and neither an inline key from a secret nor a passphrase shows in any output.

use vc_mock_server::{ClientAuth, MockHandle, MockServer, MockTls};
use vc_testkit::TestPki;

use crate::redaction::{assert_hidden, files, history_text};
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

/// `good` has no hosts and the mock wants a certificate on every connection, so the token
/// request fails unless it presents the one the auth names.
#[tokio::test(flavor = "multi_thread")]
async fn an_oauth2_token_request_presents_the_auths_certificate() {
    let pki = TestPki::new().unwrap();
    let server = mock(&pki, ClientAuth::Required).await;
    let auth = "auth:\n  - id: svc\n    type: oauth2ClientCredentials\n    tokenUrl: ${var.baseUrl}/oauth/token\n\
                \x20   clientId: vc-client\n    clientSecret: vc-secret\n    clientCert: good\n";
    let project = project(&pki, (&good("'*.example'", ""), auth), "svc");
    let run = run(&project, &server, &["tls.who#saysGood"]);
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
}

const PASSPHRASE: &str = "Wq7v-Lc2x-Rm9b-Ty4n";

/// `bundle`: `good` as `certs/good.p12` under `${secret.P12_PASSPHRASE}`, sent to `hosts`.
fn bundle(hosts: &str) -> String {
    format!(
        "      - id: bundle\n        hosts: [{hosts}]\n        pkcs12: certs/good.p12\n\
         \x20       passphrase: ${{secret.P12_PASSPHRASE}}\n"
    )
}

/// [`project`] with `clients`, the `bundle` file written and the secrets it reads declared.
fn bundled(pki: &TestPki, clients: &str) -> Project {
    let project = project(pki, (clients, ""), "");
    std::fs::write(project.path().join("certs/good.p12"), pki.pkcs12(pki.good(), PASSPHRASE).unwrap()).unwrap();
    project.write(".env.schema", "P12_PASSPHRASE=\nCLIENT_CERT=\nCLIENT_KEY=\n");
    project
}

/// `vc run` of `project` against `server` with `args`, the passphrase `passphrase` and `good`'s
/// certificate and key inline in the secrets.
fn secret_run(project: &Project, server: &MockHandle, (args, passphrase): (&[&str], &str)) -> std::process::Output {
    let pki_files = |name: &str| std::fs::read_to_string(project.path().join(name)).unwrap();
    let mut vc = project.run(&server.base_url(), args);
    vc.env("P12_PASSPHRASE", passphrase).env("CLIENT_CERT", pki_files("certs/good.pem"));
    vc.env("CLIENT_KEY", pki_files("certs/good.key")).output().unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pkcs12_bundle_presents_its_identity_and_a_wrong_passphrase_stops_the_run() {
    let pki = TestPki::new().unwrap();
    let server = mock(&pki, ClientAuth::Required).await;
    let project = bundled(&pki, &bundle("'127.0.0.1'"));
    let run = secret_run(&project, &server, (&["tls.who#saysGood"], PASSPHRASE));
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    let wrong = secret_run(&project, &server, (&["tls.who#saysGood"], "Zp3k-Hd8s-Qm1x"));
    assert_eq!(wrong.status.code(), Some(2), "{}", said(&wrong));
    assert!(said(&wrong).contains("V5126") && said(&wrong).contains("`bundle`"), "{}", said(&wrong));
    assert_hidden(&[("vc output".to_owned(), said(&wrong))], &["Zp3k-Hd8s-Qm1x"]);
}

/// The body of `good`'s key past its first line, whose PKCS#8 prefix every P-256 key shares.
fn key_body(pki: &TestPki) -> Vec<String> {
    let body = pki.good().key_pem().lines().filter(|line| !line.starts_with("-----"));
    body.skip(1).map(str::to_owned).collect()
}

/// `good`'s key comes inline from a secret and the bundle's passphrase from another; a failing
/// test, the text and JSON output, the CTRF and `JUnit` reports, the history and a wrong passphrase's diagnostic
/// show neither.
#[tokio::test(flavor = "multi_thread")]
async fn an_inline_key_and_a_passphrase_show_in_no_output() {
    let pki = TestPki::new().unwrap();
    let server = mock(&pki, ClientAuth::Required).await;
    let inline = "      - id: good\n        hosts: ['127.0.0.1']\n        cert: ${secret.CLIENT_CERT}\n\
                  \x20       key: ${secret.CLIENT_KEY}\n";
    let project = bundled(&pki, &format!("{inline}{}", bundle("'*.example'")));
    let text = secret_run(&project, &server, (&["--report", "ctrf,junit", "--out", "out"], PASSPHRASE));
    assert_eq!(text.status.code(), Some(1), "isForbidden fails: {}", said(&text));
    let json = secret_run(&project, &server, (&["--reporter", "json"], PASSPHRASE));
    assert_eq!(json.status.code(), Some(1), "{}", said(&json));
    let wrong = secret_run(&project, &server, (&[], "Zp3k-Hd8s-Qm1x"));
    assert_eq!(wrong.status.code(), Some(2), "{}", said(&wrong));
    let history = history_text(&project.path().join(".vibranium/history.db"));
    let output = said(&text) + &said(&json) + &said(&wrong);
    let mut seen = vec![("history.db rows".to_owned(), history), ("vc output".to_owned(), output)];
    files(project.path().join(".vibranium").as_path(), &mut seen);
    files(project.path().join("out").as_path(), &mut seen);
    let reports = ["ctrf.json", "junit.xml"].map(|name| seen.iter().any(|(path, _)| path.ends_with(name)));
    assert_eq!(reports, [true, true], "both reports were written");
    let body = key_body(&pki);
    let hidden: Vec<&str> = body.iter().map(String::as_str).chain([PASSPHRASE, "Zp3k-Hd8s-Qm1x"]).collect();
    assert_hidden(&seen, &hidden);
}

/// A key rendered from the environment is inline PEM even after a newline, and when it does
/// not fit or does not parse, the diagnostic names the client and never quotes the key (D390).
#[tokio::test(flavor = "multi_thread")]
async fn a_rendered_key_that_will_not_load_is_never_quoted() {
    let pki = TestPki::new().unwrap();
    let server = mock(&pki, ClientAuth::Optional).await;
    let project = project(&pki, (&good("'127.0.0.1'", "").replace("certs/good.key", "${env.K}"), ""), "");
    let other = pki.other_ca().key_pem();
    let body: Vec<&str> = other.lines().filter(|line| !line.starts_with("-----")).skip(1).collect();
    let cut = format!("\n{}", &other[..other.len() / 2 + 8]);
    for (key, code) in [(format!("\n{other}"), "V5126"), (cut, "V5125")] {
        let run = project.run(&server.base_url(), &[]).env("K", &key).output().unwrap();
        assert_eq!(run.status.code(), Some(2), "{}", said(&run));
        assert!(said(&run).contains(code) && said(&run).contains("`good`"), "{}", said(&run));
        let hidden: Vec<&str> = body.iter().copied().chain(["PRIVATE KEY"]).collect();
        assert_hidden(&[("vc output".to_owned(), said(&run))], &hidden);
    }
    assert!(server.stats().await.unwrap().requests.is_empty(), "no request was sent");
}

/// A value that does not render, such as `${env.K.pem}` over a K that holds a key, is named as
/// the config wrote it and never by what it read (D397).
#[tokio::test(flavor = "multi_thread")]
async fn a_value_that_does_not_render_names_its_expression_and_never_its_key() {
    let pki = TestPki::new().unwrap();
    let server = mock(&pki, ClientAuth::Optional).await;
    let project = project(&pki, (&good("'127.0.0.1'", "").replace("certs/good.key", "${env.K.pem}"), ""), "");
    let key = pki.good().key_pem();
    let run = project.run(&server.base_url(), &[]).env("K", key).output().unwrap();
    assert_eq!(run.status.code(), Some(2), "{}", said(&run));
    let want = "V5125: client certificate `good`, its `key`: `${env.K.pem}` does not render";
    assert!(said(&run).contains(want), "{}", said(&run));
    let body: Vec<&str> = key.lines().filter(|line| !line.starts_with("-----")).collect();
    let hidden: Vec<&str> = body.iter().map(|line| &line[..line.len().min(12)]).chain(["PRIVATE KEY"]).collect();
    assert_hidden(&[("vc output".to_owned(), said(&run))], &hidden);
    assert!(server.stats().await.unwrap().requests.is_empty(), "no request was sent");
}

/// A key `${env.K}` renders inline from the environment, and a test sends it, echoed back, from a
/// row labelled with it: the redactor hides it with the passphrases, so neither the output, the
/// reports nor the history show it.
#[tokio::test(flavor = "multi_thread")]
async fn a_key_rendered_from_the_environment_is_masked_where_a_run_echoes_or_labels_it() {
    let pki = TestPki::new().unwrap();
    let server = mock(&pki, ClientAuth::Required).await;
    let project = project(&pki, (&good("'127.0.0.1'", "").replace("certs/good.key", "${env.K}"), ""), "");
    project.write("services/echo/echo.api.yaml", "baseUrl: ${var.baseUrl}\napis:\n  - id: send\n    method: POST\n    path: /echo\n    body: {key: '${env.K}'}\n");
    project.write(
        "services/echo/key.test.yaml",
        "tests:\n  - id: echoesTheKey\n    examples: [{name: '${env.K}'}]\n    call: echo#send\n    expect: {status: 418}\n",
    );
    let args = ["echo.key", "--report", "ctrf,junit", "--out", "out"];
    let text = project.run(&server.base_url(), &args).env("K", pki.good().key_pem()).output().unwrap();
    assert_eq!(text.status.code(), Some(1), "the echo fails its check: {}", said(&text));
    assert!(said(&text).contains("echoesTheKey"), "{}", said(&text));
    let json =
        project.run(&server.base_url(), &["echo.key", "--reporter", "json"]).env("K", pki.good().key_pem()).output();
    let json = json.unwrap();
    assert!(said(&json).contains("\"key\""), "the echo is reported: {}", said(&json));
    let history = history_text(&project.path().join(".vibranium/history.db"));
    let mut seen = vec![("history.db rows".to_owned(), history), ("vc output".to_owned(), said(&text) + &said(&json))];
    files(project.path().join("out").as_path(), &mut seen);
    let body = key_body(&pki);
    assert_hidden(&seen, &body.iter().map(String::as_str).collect::<Vec<_>>());
}
