//! The demo's users store, sign-in routes and specs (D331), called by the real `vc` on the real
//! `vc demo`: a user goes round create, read, replace, patch and delete; a flow login and a
//! client-credentials grant each get one token that every test reuses; `/auth/me` refuses a
//! call without one; and the tests `vc import openapi` drafts from the served spec all pass.

use serde_json::Value;

use crate::demo_routes::{Demo, run};

/// `users#create` (saving `userId`, deleted after), `#get`, `#replace`, `#patch` and `#delete`.
const USERS: (&str, &str) = (
    "services/users/users.api.yaml",
    concat!(
        "baseUrl: ${var.baseUrl}\napis:\n",
        "  - id: create\n    method: POST\n    path: /users\n    body: {name: Ada, email: ada@example.com}\n",
        "    expect: {status: 201}\n    save: {userId: '${response.body.id}'}\n",
        "    cleanup: {call: delete, vars: {userId: '${response.body.id}'}}\n",
        "  - id: get\n    path: /users/${var.userId}\n",
        "  - id: replace\n    method: PUT\n    path: /users/${var.userId}\n",
        "    body: {name: Ada Lovelace, email: ada@example.org}\n",
        "  - id: patch\n    method: PATCH\n    path: /users/${var.userId}\n    body: {role: admin}\n",
        "  - id: delete\n    method: DELETE\n    path: /users/${var.userId}\n    expect: {status: [204, 404]}\n",
    ),
);

/// The CRUD round trip, one test a step.
const CRUD: (&str, &str) = (
    "services/users/crud.test.yaml",
    concat!(
        "tests:\n",
        "  - id: createsUser\n    call: users#create\n",
        "    expect: {body: {match: {name: Ada, email: ada@example.com, id: {type: string}}}}\n",
        "  - id: readsCreatedUser\n    before: [{call: users#create}]\n    call: users#get\n",
        "    expect: {status: 200, body: {match: {id: '${var.userId}', name: Ada}}}\n",
        "  - id: replacesAndPatchesUser\n    before: [{call: users#create}, {call: users#replace}]\n",
        "    call: users#patch\n",
        "    expect: {status: 200, body: {match: {name: Ada Lovelace, email: ada@example.org, role: admin}}}\n",
        "  - id: forgetsDeletedUser\n    before: [{call: users#create}, {call: users#delete, expect: {status: 204}}]\n",
        "    call: users#get\n    expect: {status: 404}\n",
        "  - id: keepsAdaFixed\n    call: users#patch\n    vars: {userId: '1'}\n    expect: {status: 403}\n",
    ),
);

/// `count` tests calling `svc#me` and `count` calling `svc#echo`, which sees the token sent.
fn calling(count: usize) -> String {
    let me = (1..=count).map(|n| format!("  - id: callsMe{n}\n    call: svc#me\n"));
    let echo = (1..=count).map(|n| {
        format!("  - id: sendsTheOneToken{n}\n    call: svc#echo\n    expect: {{body: {{match: {{headers.authorization: 'Bearer demo-token-1'}}}}}}\n")
    });
    format!("tests:\n{}", me.chain(echo).collect::<String>())
}

/// `svc#me` and `svc#echo`, signed in with `svc`, the config's one auth definition.
const SIGNED_IN: (&str, &str) = (
    "services/svc/svc.api.yaml",
    concat!(
        "baseUrl: ${var.baseUrl}\napis:\n",
        "  - id: me\n    path: /auth/me\n    auth: svc\n    expect: {status: 200, body: {match: {user: demo}}}\n",
        "  - id: echo\n    path: /echo\n    auth: svc\n",
    ),
);

/// A run of [`calling`] three times each, signed in with `auth`: every test passes.
fn passes_with(auth: &str) {
    let demo = Demo::start();
    let tests = calling(3);
    let project = demo.project(&format!("auth:\n{auth}"), &[SIGNED_IN, ("services/svc/me.test.yaml", &tests)]);
    let (code, finished) = run(&project);
    assert_eq!(code, Some(0), "{finished:#?}");
    assert_eq!(finished.len(), 6);
    assert!(finished.iter().all(|test| test["status"] == "passed"), "{finished:#?}");
}

#[test]
fn a_user_goes_round_create_read_replace_patch_and_delete() {
    let demo = Demo::start();
    let (code, finished) = run(&demo.project("", &[USERS, CRUD]));
    assert_eq!(code, Some(0), "{finished:#?}");
    assert_eq!(finished.len(), 5);
    assert!(finished.iter().all(|test| test["status"] == "passed"), "{finished:#?}");
}

#[test]
fn a_bearer_login_is_made_once_and_its_token_reused() {
    passes_with(
        "  - id: svc\n    type: flow\n    steps:\n      - id: login\n        method: POST\n\
         \x20       url: ${var.baseUrl}/auth/token\n        body: {username: ada, password: lovelace}\n\
         \x20       expect: {status: 200}\n        save: {accessToken: '${response.body.access_token}'}\n\
         \x20   apply:\n      headers: {authorization: 'Bearer ${var.accessToken}'}\n",
    );
}

#[test]
fn a_client_credentials_token_is_fetched_once_and_reused() {
    passes_with(
        "  - id: svc\n    type: oauth2ClientCredentials\n    tokenUrl: ${var.baseUrl}/oauth/token\n\
         \x20   clientId: vc-client\n    clientSecret: vc-secret\n    scopes: [users.read]\n",
    );
}

#[test]
fn auth_me_refuses_a_call_without_a_token() {
    let demo = Demo::start();
    let apis = "baseUrl: ${var.baseUrl}\napis:\n  - id: me\n    path: /auth/me\n";
    let tests = "tests:\n  - id: refusesNoToken\n    call: svc#me\n\
                 \x20   expect: {status: 401, body: {match: {error: a valid bearer token is needed}}}\n";
    let files = [("services/svc/svc.api.yaml", apis), ("services/svc/me.test.yaml", tests)];
    let (code, finished) = run(&demo.project("", &files));
    assert_eq!((code, &finished[0]["status"]), (Some(0), &Value::from("passed")), "{finished:#?}");
}

#[test]
fn the_tests_drafted_from_the_served_spec_pass() {
    let demo = Demo::start();
    let project = demo.project("", &[]);
    let spec = format!("http://127.0.0.1:{}/openapi/users.yaml", demo.port);
    let imported =
        project.vc(&["import", "openapi", &spec, "--id", "users", "--tests", "--negative"]).output().unwrap();
    assert_eq!(imported.status.code(), Some(0), "{}", crate::said(&imported));
    let (code, finished) = run(&project);
    assert_eq!(code, Some(0), "{finished:#?}");
    assert!(finished.len() > 6, "a test an operation, and the negative ones: {finished:#?}");
    assert!(finished.iter().all(|test| test["status"] == "passed"), "{finished:#?}");
}
