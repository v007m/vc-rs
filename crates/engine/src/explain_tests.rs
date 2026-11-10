use std::sync::Arc;

use vc_testkit::{FakeVcs, MemorySources};

use crate::sample::{builder, engine};
use crate::{Engine, EngineError, ExplainRequest};

const CONFIG: (&str, &[u8]) = (
    "vibranium.config.yaml",
    b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n    vars: {region: eu}\nvars: {baseUrl: 'http://x', region: us, version: v0}\n",
);
const USERS: (&str, &[u8]) = (
    "services/users/users.api.yaml",
    b"baseUrl: ${var.baseUrl}\nvars: {version: v1}\nprofiles:\n  - id: dev\n    vars: {version: v2}\napis:\n  - id: create\n    method: POST\n    path: /${var.version}/users\n    expect: {status: 201}\n    cleanup: {call: remove, vars: {userId: '${response.body.id}'}}\n  - id: remove\n    method: DELETE\n    path: /users/${var.userId}\n    vars: {userId: 0}\n",
);
const CRUD: (&str, &[u8]) = (
    "services/users/crud.test.yaml",
    b"vars: {userId: 3}\ntests:\n  - id: makesUsers\n    vars: {userId: 4}\n    steps:\n      - call: users#create\n        vars: {userId: \"${var.region}-${env.USER_TAG | default('x')}\"}\n      - command: echo done\n",
);

fn project() -> Engine {
    builder(FakeVcs::default()).sources(Arc::new(MemorySources::new(&[CONFIG, USERS, CRUD]))).build().unwrap()
}

fn explained(engine: &Engine, id: &str, request: &ExplainRequest) -> String {
    engine.explain(id, request).unwrap().text
}

#[test]
fn a_test_shows_each_call_with_its_url_vars_checks_and_cleanup() {
    let text = explained(&project(), "users.crud#makesUsers", &ExplainRequest::default());
    let call = concat!(
        "users.crud#makesUsers\nprofile  dev\ncall users#create\n  POST http://x/v2/users\n  vars\n",
        "    baseUrl  http://x  (vibranium.config.yaml)\n",
        "    region  eu  (profile dev)\n",
        "    version  v2  (file users, profile dev)\n",
        "    userId  eu-${env.USER_TAG | default('x')}  (call)\n",
        "  expect\n    status: 201  (api users#create)\n  cleanup  users#remove\n",
    );
    assert!(text.starts_with(call), "{text}");
    assert!(text.ends_with("\ncommand\n"), "a step that is no call is named alone: {text}");
}

#[test]
fn an_api_shows_its_own_levels_under_the_profiles_and_settings_given() {
    let request = ExplainRequest { profiles: vec!["dev".into()], sets: vec!["vars.baseUrl=http://y".into()] };
    let text = explained(&project(), "users#remove", &request);
    let expected = concat!(
        "users#remove\nprofile  dev\n  DELETE http://y/users/0\n  vars\n",
        "    baseUrl  http://y  (--set)\n    region  eu  (profile dev)\n    version  v2  (file users, profile dev)\n",
        "    userId  0  (api users#remove)\n  expect\n",
    );
    assert_eq!(text, expected);
}

#[test]
fn the_sample_names_its_definitions() {
    let text = explained(&engine(FakeVcs::default()), "users#list", &ExplainRequest::default());
    assert!(text.starts_with("users#list  List users\nprofile  dev\n  GET http://x/users\n"), "{text}");
    let text = explained(&engine(FakeVcs::default()), "users.crud#listsUsers", &ExplainRequest::default());
    assert!(text.starts_with("users.crud#listsUsers  Lists users\nprofile  dev\ncall users#list\n"), "{text}");
}

#[test]
fn an_unknown_id_is_refused_with_the_closest_ones() {
    let error = engine(FakeVcs::default()).explain("users.crud#listUsers", &ExplainRequest::default()).unwrap_err();
    let help = Some("did you mean `users.crud#listsUsers` or `users.crud#skipsUsers`?".to_owned());
    assert_eq!(error, EngineError::Selection { reason: "no test or api `users.crud#listUsers`".into(), help });
    let error = engine(FakeVcs::default()).explain("zzz", &ExplainRequest::default()).unwrap_err();
    assert_eq!(error.help(), None);
}

#[test]
fn an_unknown_profile_is_refused() {
    let request = ExplainRequest { profiles: vec!["prod".into()], ..ExplainRequest::default() };
    assert!(matches!(project().explain("users#remove", &request), Err(EngineError::Setting { .. })));
}
