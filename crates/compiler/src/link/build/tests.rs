use std::sync::Arc;

use vc_model::{
    Action, ActionKind, AuthRef, BodySpec, CheckKind, ContentHash, Duration, Method, Origin, Owner, Program, Segment,
    SourceFile, SourcePath, Test, TestBody,
};

use crate::{CompileInput, compile};

const CONFIG: &str = "syntaxVersion: 2\nproject: shop\nvars: {baseUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n    vars: {region: eu}\nauth:\n  - id: admin\n    type: bearer\n    token: t\nbeforeRun:\n  - id: warm\n    call: users#list\nreports: {dir: out, formats: [ctrf]}\nredact: {headers: [x-key], fields: [password]}\n";

const APIS: &str = "name: Users\nbaseUrl: ${var.baseUrl}\nheaders: {accept: json}\nvars: {page: 1}\nprofiles:\n  - id: dev\n    vars: {page: 2}\napis:\n  - id: list\n    path: /users\n    expect: {status: 200}\n  - id: create\n    method: POST\n    path: /users\n    body: {name: Ada}\n    expect:\n      status: 201\n      headers: {Content-Type: {contains: json}}\n      cel: {named: response.body.name == 'Ada'}\n    save: {userId: '${response.body.id}'}\n    cleanup: {call: remove, vars: {id: '${response.body.id}'}}\n  - id: remove\n    method: DELETE\n    path: /users/${var.id}\n    deprecated: true\n";

const TESTS: &str = "name: Crud\ntags: [smoke]\nbeforeAll:\n  - id: seed\n    call: users#create\nbeforeEach:\n  - function: hooks/a.ts#reset\ntests:\n  - id: listsUsers\n    call: users#list\n    vars: {limit: 2}\n    retry: {attempts: 2}\n    repeat: 3\n    expect: {status: 202}\n  - id: createsTwice\n    auth: admin\n    headers: {x-a: '1', x-b: '2'}\n    body: {name: Bo}\n    expect: {status: 299, size: {lt: 1kb}}\n    save: {last: '${response.status}'}\n    steps:\n      - call: users#create\n        headers: {x-b: '3'}\n        expect: {status: 201, cel: {more: response.body.name == 'Bo'}}\n      - id: again\n        call: users#create\n        auth: none\n        body: {name: Cy}\n        expect: {replace: true, size: {lt: 2kb}}\n        save: {last: '${response.body.id}'}\n      - function: hooks/a.ts#check\n";

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) }
}

fn program() -> Arc<Program> {
    let files = vec![
        file("vibranium.config.yaml", CONFIG),
        file("services/users/users.api.yaml", APIS),
        file("services/users/crud.test.yaml", TESTS),
    ];
    let compiled = compile(CompileInput { files, ..CompileInput::default() });
    assert!(compiled.diagnostics.is_empty(), "{:?}", compiled.diagnostics);
    compiled.program.unwrap()
}

fn name(program: &Program, sym: vc_model::Sym) -> &str {
    program.str(sym).unwrap()
}

fn test<'p>(program: &'p Program, id: &str) -> &'p Test {
    program.tests.values().find(|test| name(program, test.id.sym()) == id).unwrap()
}

fn action(program: &Program, id: vc_model::ActionId) -> &Action {
    &program.actions[usize::try_from(id.index()).unwrap()]
}

fn steps<'p>(program: &'p Program, test: &Test) -> Vec<&'p Action> {
    let TestBody::Steps(ids) = &test.body else { panic!("expected steps") };
    ids.iter().map(|id| action(program, *id)).collect()
}

/// Each check as kind, its line, and whether it came from the api, the test or the step.
fn checks(action: &Action) -> Vec<(CheckKind, u32, char)> {
    let origin = |origin: Origin| match origin {
        Origin::Api(_) => 'a',
        Origin::Test(_) => 't',
        Origin::Action(_) => 's',
    };
    action.expect.checks.iter().map(|check| (check.kind, check.span.start.line, origin(check.origin))).collect()
}

#[test]
fn builds_the_config_profiles_and_run_blocks() {
    let program = program();
    assert_eq!((program.config.syntax_version, program.config.project.as_deref()), (2, Some("shop")));
    assert_eq!(program.config.vars.values.len(), 1);
    assert_eq!(program.config.reports_dir.as_deref(), Some("out"));
    assert_eq!(
        (&program.config.redact_headers[..], &program.config.redact_fields[..]),
        (&["x-key".to_owned()][..], &["password".to_owned()][..])
    );
    assert_eq!(program.config.report_formats, ["ctrf"]);
    let profile = program.profiles.values().next().unwrap();
    assert!(profile.default && profile.vars.len() == 1);
    let [warm] = program.run_blocks.before_all[..] else { panic!("one beforeRun action") };
    assert_eq!(action(&program, warm).owner, Owner::Run);
    assert_eq!(action(&program, warm).id.map(|id| name(&program, id)), Some("warm"));
}

#[test]
fn builds_api_groups_with_their_apis_and_cleanups() {
    let program = program();
    let group = program.api_groups.values().next().unwrap();
    assert_eq!((name(&program, group.id.sym()), group.apis.len()), ("users", 3));
    assert_eq!((group.headers.len(), group.vars.values.len(), group.vars.by_profile.len()), (1, 1, 1));
    let create = &program.apis[&group.apis[1]];
    assert_eq!((create.method, name(&program, create.id.sym())), (Method::Post, "users#create"));
    assert!(matches!(create.body, BodySpec::Json(_)));
    assert_eq!(create.save.values.len(), 1);
    assert!(program.apis[&group.apis[2]].deprecated);
    let header = create.expect.checks.iter().find(|check| check.kind == CheckKind::Header).unwrap();
    assert_eq!(header.target.as_ref().unwrap().segments, [Segment::Key("content-type".into())]);
}

#[test]
fn an_apis_cleanup_calls_another_api_with_its_vars() {
    let program = program();
    let group = program.api_groups.values().next().unwrap();
    let create = &program.apis[&group.apis[1]];
    let cleanup = action(&program, create.cleanup.unwrap());
    let ActionKind::Call(target) = cleanup.kind else { panic!("cleanup calls an api") };
    assert_eq!(name(&program, target.sym()), "users#remove");
    assert_eq!(cleanup.vars.values.len(), 1);
}

#[test]
fn a_call_test_merges_its_checks_over_the_apis_and_keeps_its_own_vars() {
    let program = program();
    let file = program.test_files.values().next().unwrap();
    assert_eq!((file.tests.len(), file.tags.len(), file.lifecycle.before_each.len()), (2, 1, 1));
    assert_eq!(action(&program, file.lifecycle.before_all[0]).owner, Owner::File(file.id));
    let lists = test(&program, "users.crud#listsUsers");
    assert_eq!((lists.repeat, lists.retry.unwrap().attempts, lists.vars.values.len()), (3, 2, 1));
    let TestBody::Call(call) = lists.body else { panic!("expected a call") };
    let call = action(&program, call);
    assert_eq!(checks(call), [(CheckKind::Status, 14, 't')]);
    assert!(call.vars.values.is_empty() && call.retry.is_none() && call.id.is_none());
}

#[test]
fn steps_take_the_tests_keys_as_defaults() {
    let program = program();
    let twice = test(&program, "users.crud#createsTwice");
    let [first, again, check] = steps(&program, twice)[..] else { panic!("three steps") };
    let headers: Vec<_> = first.headers.keys().map(String::as_str).collect();
    assert_eq!(headers, ["x-a", "x-b"]);
    assert_eq!(first.headers["x-b"].parts.len(), 1);
    assert!(matches!(first.auth, Some(AuthRef::Use { .. })));
    assert!(matches!(&first.body, Some(BodySpec::Json(_))));
    assert_eq!(again.auth, Some(AuthRef::None));
    assert!(check.headers.is_empty() && check.auth.is_none() && check.expect.checks.is_empty());
}

#[test]
fn the_tests_save_goes_to_its_last_step_unless_the_step_saves_the_name() {
    let program = program();
    let twice = test(&program, "users.crud#createsTwice");
    let [first, again, check] = steps(&program, twice)[..] else { panic!("three steps") };
    assert_eq!(again.save.values.len(), 1);
    assert_eq!(check.save.values.len(), 1);
    assert!(first.save.values.is_empty());
}

#[test]
fn step_checks_merge_api_then_test_then_step() {
    let program = program();
    let twice = test(&program, "users.crud#createsTwice");
    let [first, again, _] = steps(&program, twice)[..] else { panic!("three steps") };
    let expected = [
        (CheckKind::Status, 24, 's'),
        (CheckKind::Header, 18, 'a'),
        (CheckKind::Cel, 19, 'a'),
        (CheckKind::Size, 19, 't'),
        (CheckKind::Cel, 24, 's'),
    ];
    assert_eq!(checks(first), expected);
    assert_eq!(checks(again), [(CheckKind::Size, 29, 's')]);
    assert_eq!(twice.expect.checks.len(), 2);
}

const MORE_CONFIG: &str = "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\ndefaults: {timeout: 5s, concurrency: 4, perHost: 2, headers: {x-d: '1'}}\nafterRun:\n  - function: hooks/a.ts#report\n";

const ORDERS: &str = "baseUrl: http://o\nbeforeRequest:\n  - function: hooks/a.ts#sign\napis:\n  - id: upload\n    method: POST\n    path: /orders\n    body: {$file: data/o.json}\n    afterResponse:\n      - function: hooks/a.ts#log\n  - id: mixed\n    method: POST\n    path: /orders\n    body: {$file: data/o.json, name: x}\n";

const FLOW: &str = "skip: {reason: later, until: 2026-12-01}\ntests:\n  - id: waitsForOrder\n    call: orders#upload\n    skip: flaky\n    poll:\n      until: {status: 200}\n      every: 2s\n      timeout: 30s\n    expect:\n      body: {equals: {ok: true}}\n      custom:\n        - function: hooks/a.ts#isSorted\n  - id: reusesTheWait\n    headers: {x-t: '1'}\n    expect: {status: 202}\n    steps:\n      - test: waitsForOrder\n        expect: {size: {lt: 1kb}}\n      - function: hooks/a.ts#check\n";

fn more() -> Arc<Program> {
    let files = vec![
        file("vibranium.config.yaml", MORE_CONFIG),
        file("services/orders/orders.api.yaml", ORDERS),
        file("services/orders/flow.test.yaml", FLOW),
    ];
    let compiled = compile(CompileInput { files, ..CompileInput::default() });
    assert!(compiled.diagnostics.is_empty(), "{:?}", compiled.diagnostics);
    compiled.program.unwrap()
}

fn seconds(secs: u64) -> Duration {
    Duration::new(std::time::Duration::from_secs(secs))
}

#[test]
fn builds_the_config_defaults_and_after_run() {
    let program = more();
    let defaults = &program.config.defaults;
    assert_eq!((defaults.timeout, defaults.concurrency, defaults.per_host), (Some(seconds(5)), Some(4), Some(2)));
    assert_eq!(defaults.headers.len(), 1);
    assert_eq!(program.run_blocks.after_all.len(), 1);
}

#[test]
fn reads_request_hooks_and_file_bodies() {
    let program = more();
    let group = program.api_groups.values().next().unwrap();
    let [sign] = &group.hooks.before_request[..] else { panic!("one file hook") };
    assert_eq!((sign.file.as_str(), sign.export.as_str()), ("hooks/a.ts", "sign"));
    let (upload, mixed) = (&program.apis[&group.apis[0]], &program.apis[&group.apis[1]]);
    assert_eq!(upload.hooks.after_response.len(), 1);
    assert!(matches!(upload.body, BodySpec::File(_)));
    assert!(matches!(mixed.body, BodySpec::Json(_)), "a `$file` map with other keys is JSON");
}

#[test]
fn reads_skips_polls_and_body_and_function_checks() {
    let program = more();
    let file = program.test_files.values().next().unwrap();
    let skip = file.skip.as_ref().unwrap();
    assert_eq!((skip.reason.as_str(), skip.until.as_deref()), ("later", Some("2026-12-01")));
    let waits = test(&program, "orders.flow#waitsForOrder");
    assert_eq!(waits.skip.as_ref().map(|skip| skip.reason.as_str()), Some("flaky"));
    let TestBody::Call(call) = waits.body else { panic!("expected a call") };
    let call = action(&program, call);
    let poll = call.poll.as_ref().unwrap();
    assert_eq!((poll.every, poll.timeout, poll.until.checks.len()), (seconds(2), seconds(30), 1));
    let kinds: Vec<_> = call.expect.checks.iter().map(|check| check.kind).collect();
    assert_eq!(kinds, [CheckKind::Equals, CheckKind::Custom]);
}

#[test]
fn test_steps_take_the_tests_defaults_and_function_steps_take_none() {
    let program = more();
    let reuses = test(&program, "orders.flow#reusesTheWait");
    let [reuse, hook] = steps(&program, reuses)[..] else { panic!("two steps") };
    assert!(matches!(reuse.kind, ActionKind::Test(_)));
    assert_eq!(reuse.headers.keys().map(String::as_str).collect::<Vec<_>>(), ["x-t"]);
    assert_eq!(checks(reuse), [(CheckKind::Status, 16, 't'), (CheckKind::Size, 19, 's')]);
    assert!(hook.headers.is_empty() && hook.expect.checks.is_empty());
}
