use std::sync::Arc;

use vc_testkit::{
    FakeAuth, FakeTransport, FakeVcs, MemoryEnvironment, MemorySources, RecordingTransport, Recordings, status,
};

use crate::sample::builder;
use crate::{Engine, EngineBuilder, EngineError, TestAuthRequest};

/// `users#me` uses the bearer auth `svc`, whose token is `probe-abc`; `users#open` uses none and
/// no api uses `spare`. `partner#hook` is on another host, `admin#stats` on another port, and
/// `mirror#ping` on the users' origin, written in capitals with its port.
const PROJECT: [(&str, &[u8]); 5] = [
    (
        "vibranium.config.yaml",
        b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\nauth:\n  - id: svc\n    type: bearer\n    token: probe-abc\n  - id: spare\n    type: bearer\n    token: spare-abc\n",
    ),
    (
        "services/users/users.api.yaml",
        b"baseUrl: ${var.baseUrl}\napis:\n  - id: open\n    path: /open\n  - id: me\n    path: /me\n    auth: svc\n",
    ),
    ("services/partner/partner.api.yaml", b"baseUrl: https://partner.test\napis:\n  - id: hook\n    path: /hook\n"),
    ("services/admin/admin.api.yaml", b"baseUrl: http://x:8080\napis:\n  - id: stats\n    path: /stats\n"),
    ("services/mirror/mirror.api.yaml", b"baseUrl: HTTP://X:80\napis:\n  - id: ping\n    path: /ping\n"),
];

fn engine(finish: impl FnOnce(EngineBuilder) -> EngineBuilder) -> (Engine, Recordings) {
    engine_of(&PROJECT, finish)
}

/// An engine on `project` whose transport answers `/oauth/token` with a token, and 200 otherwise.
fn engine_of(project: &[(&str, &[u8])], finish: impl FnOnce(EngineBuilder) -> EngineBuilder) -> (Engine, Recordings) {
    let (transport, recordings) = RecordingTransport::new(Arc::new(FakeTransport::new(|request, _| {
        let mut response = status(200);
        if request.uri.path() == "/oauth/token" {
            response.body = r#"{"access_token":"t0k3n","token_type":"bearer","expires_in":3600}"#.into();
        }
        Ok(response)
    })));
    let built = finish(builder(FakeVcs::default())).sources(Arc::new(MemorySources::new(project)));
    (built.transport(Arc::new(transport)).build().unwrap(), recordings)
}

fn auth(name: &str) -> TestAuthRequest {
    TestAuthRequest { auth: name.into(), ..TestAuthRequest::default() }
}

#[tokio::test]
async fn an_auth_test_names_the_credentials_of_the_first_api_that_uses_it_and_sends_nothing() {
    let (engine, recordings) = engine(|engine| engine);
    let tested = engine.test_auth(&auth("svc")).await.unwrap();
    assert_eq!((tested.api.as_str(), tested.headers.as_slice()), ("users#me", ["authorization".to_owned()].as_slice()));
    assert_eq!(
        (tested.query.len(), tested.cookies.len(), tested.expires_in, tested.error.as_ref()),
        (0, 0, None, None)
    );
    assert!(!format!("{tested:?}").contains("probe-abc"), "{tested:?}");
    assert_eq!(recordings.take().len(), 0);
}

fn on(api: &str, name: &str) -> TestAuthRequest {
    TestAuthRequest { api: Some(api.into()), ..auth(name) }
}

#[tokio::test]
async fn an_auth_test_gives_a_named_api_the_auth_only_on_an_origin_an_api_that_uses_it_sends_to() {
    let (engine, recordings) = engine(|engine| engine);
    for api in ["users#open", "mirror#ping"] {
        let tested = engine.test_auth(&on(api, "svc")).await.unwrap();
        assert_eq!((tested.api.as_str(), tested.headers.len(), tested.error), (api, 1, None));
    }
    let unknown = engine.test_auth(&on("users#nope", "svc")).await;
    assert!(matches!(unknown, Err(EngineError::Selection { .. })), "{unknown:?}");
    assert_eq!(recordings.take().len(), 0);
}

#[tokio::test]
async fn an_auth_test_refuses_to_take_an_auth_to_another_host_or_port_and_sends_nothing() {
    let (engine, recordings) = engine(|engine| engine);
    for (api, origin) in [("partner#hook", "https://partner.test:443"), ("admin#stats", "http://x:8080")] {
        let Err(EngineError::Selection { reason, help }) = engine.test_auth(&on(api, "svc")).await else {
            panic!("{api}")
        };
        assert_eq!(
            reason,
            format!("`{api}` sends to `{origin}`, where no api that uses `svc` sends; it is not given `svc`")
        );
        assert_eq!(help.as_deref(), Some("name an api on `http://x:80`, where `svc` is sent"));
    }
    let Err(EngineError::Selection { reason, .. }) = engine.test_auth(&on("users#open", "spare")).await else {
        panic!("spare")
    };
    assert_eq!(reason, "no api uses the auth `spare`");
    assert_eq!(recordings.take().len(), 0);
}

#[tokio::test]
async fn an_auth_test_refuses_an_unknown_auth_with_the_close_ones_and_one_no_api_uses() {
    let (engine, _) = engine(|engine| engine);
    let Err(EngineError::Selection { reason, help }) = engine.test_auth(&auth("svx")).await else { panic!("svx") };
    assert_eq!((reason.as_str(), help.as_deref()), ("no auth `svx`", Some("did you mean `svc`?")));
    let Err(EngineError::Selection { reason, .. }) = engine.test_auth(&auth("spare")).await else { panic!("spare") };
    assert_eq!(reason, "no api uses the auth `spare`");
}

#[tokio::test]
async fn credentials_an_auth_test_cannot_obtain_are_its_error_with_no_names() {
    let (engine, _) = engine(|engine| engine.auth(Arc::new(FakeAuth::new(&[]))));
    let tested = engine.test_auth(&auth("svc")).await.unwrap();
    let error = tested.error.clone().unwrap_or_default();
    assert!(error.contains("no auth type is called `bearer`"), "{error}");
    assert!(tested.headers.is_empty() && !error.contains("probe-abc"), "{tested:?}");
}

#[test]
fn the_project_names_its_auths_in_config_order() {
    let (engine, _) = engine(|engine| engine);
    assert_eq!(engine.project_info().unwrap().auths, ["svc", "spare"]);
}

const LEAKED: &str = "tok-fixture-leak-7c2e";

/// `users#me` uses `svc`, whose token pipes a secret that is no number through `number`.
const PIPED: [(&str, &[u8]); 3] = [
    (
        "vibranium.config.yaml",
        b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\nauth:\n  - id: svc\n    type: bearer\n    token: ${secret.SHOP_TOKEN | number}\n",
    ),
    (".env.schema", b"SHOP_TOKEN=\n"),
    ("services/users/users.api.yaml", b"baseUrl: ${var.baseUrl}\napis:\n  - id: me\n    path: /me\n    auth: svc\n"),
];

fn with_secret(engine: EngineBuilder) -> EngineBuilder {
    engine.environment(Arc::new(MemoryEnvironment::new(&[("SHOP_TOKEN", LEAKED)])))
}

#[tokio::test]
async fn an_auth_that_cannot_render_its_secret_is_an_error_with_the_secret_masked() {
    let (engine, recordings) = engine_of(&PIPED, with_secret);
    let tested = engine.test_auth(&auth("svc")).await.unwrap();
    let error = tested.error.clone().unwrap_or_default();
    assert!(error.contains("\u{2022}\u{2022}\u{2022}\u{2022}") && !error.contains(LEAKED), "{error}");
    assert!(tested.headers.is_empty() && !format!("{tested:?}").contains(LEAKED), "{tested:?}");
    assert_eq!(recordings.take().len(), 0);
}

/// `svc` fetches a token from `${var.identityUrl}`, `http://id.test` in the config; `mirror#ping`
/// is on the users' origin but sets `identityUrl` to another host in its file.
const MIRRORED: [(&str, &[u8]); 4] = [
    (
        "vibranium.config.yaml",
        b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x', identityUrl: 'http://id.test'}\nauth:\n  - id: svc\n    type: oauth2ClientCredentials\n    tokenUrl: ${var.identityUrl}/oauth/token\n    clientId: app-1\n    clientSecret: ${secret.SHOP_TOKEN}\n",
    ),
    (".env.schema", b"SHOP_TOKEN=\n"),
    ("services/users/users.api.yaml", b"baseUrl: ${var.baseUrl}\napis:\n  - id: me\n    path: /me\n    auth: svc\n"),
    (
        "services/mirror/mirror.api.yaml",
        b"baseUrl: ${var.baseUrl}\nvars:\n  identityUrl: https://elsewhere.test\napis:\n  - id: ping\n    path: /ping\n",
    ),
];

#[tokio::test]
async fn an_auth_test_through_a_named_api_renders_the_auth_as_its_sender_does() {
    let (engine, recordings) = engine_of(&MIRRORED, with_secret);
    let mirrored = engine.test_auth(&on("mirror#ping", "svc")).await.unwrap();
    let hosts: Vec<String> =
        recordings.take().iter().map(|record| record.request.uri.host().unwrap_or_default().to_owned()).collect();
    assert_eq!(hosts, ["id.test"], "the token is asked of the sender's identity host only");
    let (engine, _) = engine_of(&MIRRORED, with_secret);
    let direct = engine.test_auth(&auth("svc")).await.unwrap();
    assert_eq!(mirrored.error, None, "{mirrored:?}");
    assert_eq!(super::AuthTested { api: direct.api.clone(), ..mirrored }, direct);
}

/// No api uses `svc`; one step of a test calls `users#open` with it.
const STEPPED: [(&str, &[u8]); 3] = [
    (
        "vibranium.config.yaml",
        b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\nauth:\n  - id: svc\n    type: bearer\n    token: probe-abc\n",
    ),
    ("services/users/users.api.yaml", b"baseUrl: ${var.baseUrl}\napis:\n  - id: open\n    path: /open\n"),
    (
        "services/users/flow.test.yaml",
        b"tests:\n  - id: readsAsService\n    steps:\n      - call: users#open\n        auth: svc\n",
    ),
];

#[tokio::test]
async fn an_auth_only_a_step_uses_is_tested_through_the_api_the_step_calls() {
    let (engine, recordings) = engine_of(&STEPPED, |engine| engine);
    for request in [auth("svc"), on("users#open", "svc")] {
        let tested = engine.test_auth(&request).await.unwrap();
        assert_eq!((tested.api.as_str(), tested.headers.len(), tested.error), ("users#open", 1, None));
    }
    assert_eq!(recordings.take().len(), 0);
}

/// `users`' base URL comes from a var only the `staging` profile sets.
const UNSET: [(&str, &[u8]); 3] = [
    (
        "vibranium.config.yaml",
        b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n  - id: staging\n    vars: {usersUrl: 'http://x'}\nvars: {baseUrl: 'http://x'}\nauth:\n  - id: svc\n    type: bearer\n    token: probe-abc\n",
    ),
    ("services/users/users.api.yaml", b"baseUrl: ${var.usersUrl}\napis:\n  - id: me\n    path: /me\n    auth: svc\n"),
    ("services/mirror/mirror.api.yaml", b"baseUrl: ${var.baseUrl}\napis:\n  - id: ping\n    path: /ping\n"),
];

#[tokio::test]
async fn an_auth_test_says_no_sender_has_a_base_url_under_the_profile_and_why() {
    let (engine, recordings) = engine_of(&UNSET, |engine| engine);
    let Err(EngineError::Selection { help, .. }) = engine.test_auth(&on("mirror#ping", "svc")).await else {
        panic!("mirror")
    };
    let help = help.unwrap_or_default();
    assert!(help.starts_with("no api that uses `svc` has a base URL under the profile `dev`: "), "{help}");
    assert!(help.contains("usersUrl"), "{help}");
    assert_eq!(recordings.take().len(), 0);
}

/// `users`' base URL pipes a secret that is no number through `number`; `mirror#ping` is on
/// another path of the same host.
const URL_PIPED: [(&str, &[u8]); 4] = [
    (
        "vibranium.config.yaml",
        b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\nauth:\n  - id: svc\n    type: bearer\n    token: probe-abc\n",
    ),
    (".env.schema", b"SHOP_TOKEN=\n"),
    (
        "services/users/users.api.yaml",
        b"baseUrl: ${var.baseUrl}/${secret.SHOP_TOKEN | number}\napis:\n  - id: me\n    path: /me\n    auth: svc\n",
    ),
    ("services/mirror/mirror.api.yaml", b"baseUrl: ${var.baseUrl}\napis:\n  - id: ping\n    path: /ping\n"),
];

#[tokio::test]
async fn a_sender_url_that_cannot_render_its_secret_is_masked_in_the_refusal() {
    let (engine, recordings) = engine_of(&URL_PIPED, with_secret);
    let Err(EngineError::Selection { reason, help }) = engine.test_auth(&on("mirror#ping", "svc")).await else {
        panic!("mirror")
    };
    let help = help.unwrap_or_default();
    assert!(help.starts_with("no api that uses `svc` has a base URL under the profile `dev`: "), "{help}");
    assert!(help.contains("\u{2022}\u{2022}\u{2022}\u{2022}") && !help.contains(LEAKED), "{help}");
    assert!(!reason.contains(LEAKED), "{reason}");
    assert_eq!(recordings.take().len(), 0);
}
