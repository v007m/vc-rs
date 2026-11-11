//! Every auth type a run can use without a browser, composed by the engine (D106): fixed
//! credentials (`bearer` from a secret, `basic`, `apiKey` in a header and the query), a token
//! endpoint shared by every test (`oauth2ClientCredentials`), a short-lived token that is still
//! fetched once (D193), a client-credentials `audience`, an `oauth2Refresh` `refreshToken`, a
//! `refreshOn` 401 that fetches the token again once, and a `flow` of requests with its headers
//! and cookies. `oauth2AuthorizationCode` needs a browser, so the engine's unit tests sign in
//! through a fake one instead. Last, which declared secrets a run needs set (D280).

use vc_mock_server::MockServer;

use crate::redaction::{assert_hidden, files, history_text};
use crate::{CONFIG, Project, said};

const TOKEN: &str = "Hb3N-7tYq-Wd5K-1mZr-Pg8C";

/// A project with the auth definitions `auth`, the `svc` apis `apis` and the tests `tests`; it
/// declares `API_TOKEN` only when `auth` reads it, since a declared secret must be set.
fn project(auth: &str, apis: &str, tests: &str) -> Project {
    let schema = if auth.contains("secret.API_TOKEN") { "API_TOKEN=\n" } else { "" };
    let config = format!(
        "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {{baseUrl: 'http://x'}}\n\
         history: {{bodies: all}}\nauth:\n{auth}"
    );
    let apis = format!("baseUrl: ${{var.baseUrl}}\napis:\n{apis}");
    Project::new(&[
        ("vibranium.config.yaml", &config),
        ("services/svc/svc.api.yaml", &apis),
        ("services/svc/auth.test.yaml", &format!("tests:\n{tests}")),
        (".env.schema", schema),
    ])
}

/// An api `id` echoing its request with the auth `auth`, expecting the echo to `match`.
fn echo(id: &str, auth: &str, matches: &str) -> String {
    format!("  - id: {id}\n    path: /echo\n    auth: {auth}\n    expect: {{body: {{match: {matches}}}}}\n")
}

/// One test calling each of `apis`.
fn calling(apis: &[&str]) -> String {
    apis.iter().map(|api| format!("  - id: calls{api}\n    call: svc#{api}\n")).collect::<Vec<_>>().concat()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bearer_secret_is_sent_and_shown_nowhere() {
    let server = MockServer::start().await.unwrap();
    let auth = "  - id: tok\n    type: bearer\n    token: ${secret.API_TOKEN}\n";
    let api = echo("Bearer", "tok", &format!("{{headers.authorization: 'Bearer {TOKEN}'}}"));
    let project = project(auth, &api, &calling(&["Bearer"]));
    let run = project.run(&server.base_url(), &["--report", "ctrf,junit,html"]).env("API_TOKEN", TOKEN).output();
    let run = run.unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    let history = history_text(&project.path().join(".vibranium/history.db"));
    assert!(history.contains("/echo"), "the exchange was stored");
    let mut seen = vec![("history.db rows".to_owned(), history), ("vc output".to_owned(), said(&run))];
    files(project.path().join(".vibranium").as_path(), &mut seen);
    assert_hidden(&seen, &[TOKEN]);
}

#[tokio::test(flavor = "multi_thread")]
async fn basic_and_api_key_credentials_go_where_they_are_defined() {
    let server = MockServer::start().await.unwrap();
    let auth = "  - id: user\n    type: basic\n    username: ada\n    password: lovelace\n\
                \x20 - id: inHeader\n    type: apiKey\n    in: header\n    name: x-api-key\n    value: k3y\n\
                \x20 - id: inQuery\n    type: apiKey\n    in: query\n    name: api_key\n    value: k3y\n";
    let apis = [
        echo("Basic", "user", "{headers.authorization: 'Basic YWRhOmxvdmVsYWNl'}"),
        echo("Header", "inHeader", "{\"headers['x-api-key']\": k3y}"),
        echo("Query", "inQuery", "{query.api_key: k3y}"),
    ]
    .concat();
    let project = project(auth, &apis, &calling(&["Basic", "Header", "Query"]));
    let run = project.run(&server.base_url(), &[]).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
}

/// `oauth2ClientCredentials` against the mock's token endpoint.
const CLIENT: &str = "  - id: svc\n    type: oauth2ClientCredentials\n    tokenUrl: ${var.baseUrl}/oauth/token\n\
                      \x20   clientId: vc-client\n    clientSecret: vc-secret\n";

/// `count` tests each calling `svc#me`.
fn calling_me(count: usize) -> String {
    (1..=count).map(|n| format!("  - id: callsMe{n}\n    call: svc#me\n")).collect::<Vec<_>>().concat()
}

/// `svc#me`: the mock's `GET /auth/me`, 200 only for a token it issued.
const ME: &str = "  - id: me\n    path: /auth/me\n    auth: svc\n    expect: {status: 200}\n";

#[tokio::test(flavor = "multi_thread")]
async fn every_test_shares_one_client_credentials_login() {
    let server = MockServer::start().await.unwrap();
    let run = project(CLIENT, ME, &calling_me(6)).run(&server.base_url(), &[]).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    let stats = server.stats().await.unwrap();
    assert_eq!((stats.logins, stats.count("GET /auth/me")), (1, 6));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_token_living_less_than_refresh_before_is_fetched_once() {
    let server = MockServer::start().await.unwrap();
    let client = CLIENT.replace("/oauth/token", "/oauth/token?expiresIn=30");
    let run = project(&client, ME, &calling_me(6)).run(&server.base_url(), &[]).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    let stats = server.stats().await.unwrap();
    assert_eq!((stats.logins, stats.count("GET /auth/me")), (1, 6), "half of 30s is not over (D193)");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_401_fetches_the_token_again_once() {
    let server = MockServer::start().await.unwrap();
    let reset = "  - id: forget\n    method: POST\n    path: /__reset\n    expect: {status: 204}\n";
    let tests =
        "  - id: signsInAgain\n    steps:\n      - call: svc#me\n      - call: svc#forget\n      - call: svc#me\n";
    let run = project(CLIENT, &format!("{ME}{reset}"), tests).run(&server.base_url(), &[]).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    let stats = server.stats().await.unwrap();
    assert_eq!((stats.logins, stats.count("GET /auth/me")), (1, 2), "since the reset: one 401, one login, one retry");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_flow_logs_in_once_and_applies_what_it_saved() {
    let server = MockServer::start().await.unwrap();
    let auth = "  - id: svc\n    type: flow\n    steps:\n      - id: token\n        method: POST\n\
                \x20       url: ${var.baseUrl}/auth/token\n        expect: {status: 200}\n\
                \x20       save: {accessToken: '${response.body.access_token}'}\n\
                \x20   apply:\n      headers: {authorization: 'Bearer ${var.accessToken}'}\n";
    let run = project(auth, ME, &calling_me(3)).run(&server.base_url(), &[]).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    let stats = server.stats().await.unwrap();
    assert_eq!((stats.logins, stats.count("POST /auth/token"), stats.count("GET /auth/me")), (1, 1, 3));
}

/// The `audience` goes in the token request's form: the mock's `?audience=shop-api` endpoint
/// issues a token for it and refuses any other, which fails the test with V5114 before its call.
#[tokio::test(flavor = "multi_thread")]
async fn a_client_credentials_audience_is_sent_with_the_token_request() {
    let server = MockServer::start().await.unwrap();
    let client = CLIENT.replace("/oauth/token", "/oauth/token?audience=shop-api");
    for (audience, exit) in [("shop-api", 0), ("billing-api", 1)] {
        let auth = format!("{client}    audience: {audience}\n");
        let run = project(&auth, ME, &calling_me(2)).run(&server.base_url(), &[]).output().unwrap();
        assert_eq!(run.status.code(), Some(exit), "{audience}: {}", said(&run));
        assert_eq!(said(&run).contains("V5114"), exit == 1, "{audience}: {}", said(&run));
    }
    let stats = server.stats().await.unwrap();
    assert_eq!((stats.logins, stats.count("GET /auth/me")), (1, 2), "the wrong audience got no token");
}

/// `oauth2Refresh` trading the long-lived `refreshToken` from a secret.
const REFRESH: &str = "  - id: svc\n    type: oauth2Refresh\n    tokenUrl: ${var.baseUrl}/oauth/token\n\
                       \x20   clientId: vc-client\n    clientSecret: vc-secret\n    refreshToken: ${secret.API_TOKEN}\n";

/// The run trades the refresh token once for every test. The mock rotates a refresh token out
/// when it is used, so a second run with the same one is refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_refresh_token_from_a_secret_is_traded_once_for_every_test() {
    let server = MockServer::start().await.unwrap();
    let refresh = server.refresh_token().await.unwrap();
    let project = project(REFRESH, ME, &calling_me(3));
    let run = project.run(&server.base_url(), &[]).env("API_TOKEN", &refresh).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    let stats = server.stats().await.unwrap();
    assert_eq!((stats.logins, stats.count("GET /auth/me")), (1, 3), "{stats:?}");
    let again = project.run(&server.base_url(), &[]).env("API_TOKEN", &refresh).output().unwrap();
    assert_eq!(again.status.code(), Some(1), "{}", said(&again));
    assert!(said(&again).contains("V5114"), "{}", said(&again));
}

/// A `flow` whose `apply.cookies` adds its saved token, and a fixed one, after the request's own
/// cookie, in one `Cookie` header.
#[tokio::test(flavor = "multi_thread")]
async fn a_flow_applies_its_cookies_after_the_requests_own() {
    let server = MockServer::start().await.unwrap();
    let auth = "  - id: svc\n    type: flow\n    steps:\n      - id: token\n        method: POST\n\
                \x20       url: ${var.baseUrl}/auth/token\n        save: {accessToken: '${response.body.access_token}'}\n\
                \x20   apply:\n      cookies: {session: '${var.accessToken}', tenant: eu}\n";
    let api = "  - id: Cookies\n    path: /echo\n    auth: svc\n    headers: {cookie: theme=dark}\n\
               \x20   expect: {body: {match: {headers.cookie: 'theme=dark; session=mock-token-1; tenant=eu'}}}\n";
    let run = project(auth, api, &calling(&["Cookies"])).run(&server.base_url(), &[]).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    assert_eq!(server.stats().await.unwrap().count("POST /auth/token"), 1);
}

/// Three apis, each sending one secret, and a test calling each.
const KEYS_API: &str = "baseUrl: ${var.baseUrl}\napis:\n\
                        \x20 - id: open\n    path: /echo\n    headers: {x-key: '${secret.OPEN_KEY}'}\n\
                        \x20 - id: admin\n    path: /echo\n    headers: {x-admin: '${secret.ADMIN_KEY}'}\n\
                        \x20 - id: beta\n    path: /echo\n    headers: {x-beta: '${secret.BETA_KEY}'}\n";
const KEYS_TESTS: &str = "tests:\n  - id: callsOpen\n    call: svc#open\n  - id: callsAdmin\n    call: svc#admin\n\
                          \x20 - id: callsBeta\n    call: svc#beta\n";
const KEYS_SCHEMA: &str = "OPEN_KEY=\nADMIN_KEY=\n# @optional\nBETA_KEY=\n";

/// The check before a run asks a value only of the secrets the selected tests read, and never of
/// an `# @optional` one: a test that reads an unset optional secret fails alone, with V5114.
#[tokio::test(flavor = "multi_thread")]
async fn only_the_secrets_a_selection_reads_must_be_set_and_an_optional_one_never() {
    let server = MockServer::start().await.unwrap();
    let (api, tests) = (("services/svc/svc.api.yaml", KEYS_API), ("services/svc/keys.test.yaml", KEYS_TESTS));
    let project = Project::new(&[CONFIG, api, tests, (".env.schema", KEYS_SCHEMA)]);
    let run = |args: &[&str]| project.run(&server.base_url(), args).env("OPEN_KEY", TOKEN).output().unwrap();
    let open = run(&["svc.keys#callsOpen"]);
    assert_eq!(open.status.code(), Some(0), "ADMIN_KEY is read only by callsAdmin: {}", said(&open));
    let all = run(&[]);
    assert_eq!(all.status.code(), Some(1), "{}", said(&all));
    assert!(said(&all).contains("V5114: the declared secret `ADMIN_KEY` has no value"), "{}", said(&all));
    assert!(!said(&all).contains("BETA_KEY"), "an optional secret is not required: {}", said(&all));
    assert_eq!(server.stats().await.unwrap().count("GET /echo"), 1, "the refused run sent nothing");
    let beta = run(&["svc.keys#callsOpen", "svc.keys#callsBeta"]);
    assert_eq!(beta.status.code(), Some(1), "{}", said(&beta));
    assert!(said(&beta).contains("V5114") && said(&beta).contains("BETA_KEY"), "{}", said(&beta));
    assert_eq!(server.stats().await.unwrap().count("GET /echo"), 2, "callsOpen ran; callsBeta sent nothing");
}
