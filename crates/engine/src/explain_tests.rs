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

/// Config headers and an auth, under an api file that sets its own.
const SAMPLE_CONFIG: (&str, &[u8]) = (
    "vibranium.config.yaml",
    concat!(
        "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\n",
        "defaults:\n  headers: {x-trace: config, accept: application/json}\n",
        "auth:\n  - id: svc\n    type: bearer\n    token: abc\n",
    )
    .as_bytes(),
);
const SAMPLE: (&str, &[u8]) = (
    "services/sample/sample.api.yaml",
    concat!(
        "baseUrl: ${var.baseUrl}\nauth: svc\nheaders: {x-trace: file, X-Team: core}\napis:\n",
        "  - id: createUser\n    method: POST\n    path: /users\n    save: {userId: '${response.body.id}'}\n",
        "  - id: getUser\n    path: /users/${var.userId | urlEncode}\n    vars: {userId: '0'}\n",
        "    headers: {Accept: text/plain}\n    auth: none\n",
    )
    .as_bytes(),
);
const BASICS: (&str, &[u8]) = (
    "services/sample/basics.test.yaml",
    concat!(
        "vars: {userId: from-file}\nbeforeAll:\n  - command: echo seed\nafterAll:\n  - command: echo done\n",
        "tests:\n  - id: checksUser\n    steps:\n      - call: sample#getUser\n        vars: {userId: a b}\n",
        "        headers: {X-TRACE: step}\n        expect:\n          body:\n            equals: {id: 1}\n",
        "            match: {name: '${var.userId}'}\n          timing: {total: {lt: 500ms}}\n",
        "          cel:\n            echoes the path: response.body.path == '/users'\n",
        "  - id: readsSavedUser\n    before:\n      - call: sample#createUser\n    after:\n      - command: echo after\n",
        "    steps:\n      - call: sample#createUser\n      - call: sample#getUser\n        vars: {a: '${var.userId}'}\n",
    )
    .as_bytes(),
);

fn sample() -> Engine {
    let sources = MemorySources::new(&[SAMPLE_CONFIG, SAMPLE, BASICS]);
    builder(FakeVcs::default()).sources(Arc::new(sources)).build().unwrap()
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
        "    userId  0  (api users#remove)\n  expect\n    status: 2xx  (default)\n",
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

#[test]
fn a_step_shows_its_headers_auth_and_the_values_its_checks_expect() {
    let text = explained(&sample(), "sample.basics#checksUser", &ExplainRequest::default());
    let expected = concat!(
        "sample.basics#checksUser\nprofile  dev\nbeforeAll  command\ncall sample#getUser\n",
        "  GET http://x/users/a%20b\n  vars\n",
        "    baseUrl  http://x  (vibranium.config.yaml)\n    userId  a b  (call)\n",
        "  headers\n    X-TRACE  step  (call)\n    Accept  text/plain  (api sample#getUser)\n",
        "    X-Team  core  (file sample)\n",
        "  auth  none  (api sample#getUser)\n",
        "  expect\n    status: 2xx  (default)\n",
        "    match name: a b  (step call sample#getUser)\n",
        "    equals: {id: 1}  (step call sample#getUser)\n",
        "    timing total: lt 500ms  (step call sample#getUser)\n",
        "    cel echoes the path: response.body.path == '/users'  (step call sample#getUser)\n",
        "afterAll  command\n",
    );
    assert_eq!(text, expected);
}

#[test]
fn a_test_shows_its_lifecycle_and_the_values_earlier_calls_save() {
    let text = explained(&sample(), "sample.basics#readsSavedUser", &ExplainRequest::default());
    let saved = "    userId  ${response.body.id}  (saved by call sample#createUser)\n";
    let file_headers = "  headers\n    x-trace  file  (file sample)\n";
    let expected = [
        "sample.basics#readsSavedUser\nprofile  dev\nbeforeAll  command\nbefore  call sample#createUser\n",
        "call sample#createUser\n  POST http://x/users\n  vars\n    baseUrl  http://x  (vibranium.config.yaml)\n",
        saved,
        file_headers,
        "    accept  application/json  (vibranium.config.yaml)\n    X-Team  core  (file sample)\n",
        "  auth  svc  (file sample)\n  expect\n    status: 2xx  (default)\n",
        "call sample#getUser\n  GET http://x/users/${var.userId | urlEncode}\n  vars\n",
        "    baseUrl  http://x  (vibranium.config.yaml)\n",
        saved,
        "    a  ${var.userId}  (call)\n",
        file_headers,
        "    Accept  text/plain  (api sample#getUser)\n    X-Team  core  (file sample)\n",
        "  auth  none  (api sample#getUser)\n  expect\n    status: 2xx  (default)\n",
        "after  command\nafterAll  command\n",
    ];
    assert_eq!(text, expected.concat());
}
