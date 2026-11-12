use std::sync::Arc;

use vc_testkit::{FakeAuth, FakeTransport, FakeVcs, MemorySources, RecordingTransport, Recordings, status};

use crate::sample::builder;
use crate::{Engine, EngineBuilder, EngineError, TestAuthRequest};

/// `users#me` uses the bearer auth `svc`, whose token is `probe-abc`; `users#open` uses none and
/// no api uses `spare`.
const PROJECT: [(&str, &[u8]); 2] = [
    (
        "vibranium.config.yaml",
        b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\nauth:\n  - id: svc\n    type: bearer\n    token: probe-abc\n  - id: spare\n    type: bearer\n    token: spare-abc\n",
    ),
    (
        "services/users/users.api.yaml",
        b"baseUrl: ${var.baseUrl}\napis:\n  - id: open\n    path: /open\n  - id: me\n    path: /me\n    auth: svc\n",
    ),
];

fn engine(finish: impl FnOnce(EngineBuilder) -> EngineBuilder) -> (Engine, Recordings) {
    let (transport, recordings) = RecordingTransport::new(Arc::new(FakeTransport::new(|_, _| Ok(status(200)))));
    let built = finish(builder(FakeVcs::default())).sources(Arc::new(MemorySources::new(&PROJECT)));
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

#[tokio::test]
async fn an_auth_test_on_a_named_api_that_uses_another_auth_obtains_the_named_one() {
    let (engine, _) = engine(|engine| engine);
    let on = |api: &str| TestAuthRequest { api: Some(api.into()), ..auth("spare") };
    let tested = engine.test_auth(&on("users#open")).await.unwrap();
    assert_eq!((tested.api.as_str(), tested.headers.len(), tested.error), ("users#open", 1, None));
    let unknown = engine.test_auth(&on("users#nope")).await;
    assert!(matches!(unknown, Err(EngineError::Selection { .. })), "{unknown:?}");
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
