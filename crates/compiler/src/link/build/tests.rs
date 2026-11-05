use std::sync::Arc;

use vc_model::{
    Action, ActionKind, AuthRef, Bodies, BodySpec, CheckKind, ContentHash, Duration, Examples, Keep, Loop, Method,
    Origin, Owner, Program, Segment, SourceFile, SourcePath, StatusPattern, TemplateValue, Test, TestBody,
};

use crate::CompileInput;

const CONFIG: &str = "syntaxVersion: 2\nproject: shop\nvars: {baseUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n    vars: {region: eu}\nauth:\n  - id: admin\n    type: bearer\n    token: t\nbeforeRun:\n  - id: warm\n    call: users#list\nreports: {dir: out, formats: [ctrf], html: {layout: single}}\nredact: {headers: [x-key], fields: [password]}\nhistory: {bodies: all, maxBodySize: 1kb, keep: 7}\n";

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
    let compiled = crate::compile(CompileInput { files, ..CompileInput::default() });
    let lint = |found: &vc_diagnostics::Diagnostic| found.code.category() == vc_diagnostics::Category::Lifecycle;
    assert!(compiled.diagnostics.iter().all(lint), "{:?}", compiled.diagnostics);
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
    let [warm] = program.run_blocks.before_run[..] else { panic!("one beforeRun action") };
    assert_eq!(action(&program, warm).owner, Owner::Run);
    assert_eq!(action(&program, warm).id.map(|id| name(&program, id)), Some("warm"));
}

#[test]
fn builds_the_report_and_history_settings() {
    let program = program();
    assert!(program.config.html_single_file);
    let history = program.config.history;
    assert_eq!((history.bodies, history.max_body_size.bytes(), history.keep), (Bodies::All, 1024, Keep::Runs(7)));
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
    let compiled = crate::compile(CompileInput { files, ..CompileInput::default() });
    let lint = |found: &vc_diagnostics::Diagnostic| found.code.category() == vc_diagnostics::Category::Lifecycle;
    assert!(compiled.diagnostics.iter().all(lint), "{:?}", compiled.diagnostics);
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
    assert_eq!(program.run_blocks.after_run.len(), 1);
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

const ROWS: &str = "vars: {rows: [{page: 3}]}\ntests:\n  - id: listsEachPage\n    call: users#list\n    examples:\n      - {page: 1}\n      - {page: '${var.rows}'}\n  - id: listsFilePages\n    call: users#list\n    standalone: false\n    examples: {$file: data/pages.csv}\n  - id: listsVarPages\n    call: users#list\n    standalone: true\n    examples: ${var.rows}\n";

/// The config alone, with `extra` appended to a minimal one.
fn config_with(extra: &str) -> Arc<Program> {
    let text = format!("syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n{extra}");
    let files = vec![file("vibranium.config.yaml", &text)];
    let compiled = crate::compile(CompileInput { files, ..CompileInput::default() });
    let lint = |found: &vc_diagnostics::Diagnostic| found.code.category() == vc_diagnostics::Category::Lifecycle;
    assert!(compiled.diagnostics.iter().all(lint), "{:?}", compiled.diagnostics);
    compiled.program.unwrap()
}

#[test]
fn history_keeps_every_run_or_falls_back_to_the_defaults() {
    let all = config_with("history: {keep: all}\nreports: {html: {layout: folder}}\n");
    assert_eq!(all.config.history.keep, Keep::All);
    assert_eq!(all.config.history.bodies, Bodies::Failures, "unwritten keys keep their defaults");
    assert!(!all.config.html_single_file);
    let none = config_with("");
    assert_eq!((none.config.history.max_body_size.bytes(), none.config.history.keep), (64 * 1024, Keep::Runs(50)));
}

fn rows() -> Arc<Program> {
    with_tests("services/users/rows.test.yaml", ROWS)
}

/// The config and the users apis, with one test file.
fn with_tests(path: &str, text: &str) -> Arc<Program> {
    let files =
        vec![file("vibranium.config.yaml", CONFIG), file("services/users/users.api.yaml", APIS), file(path, text)];
    let compiled = crate::compile(CompileInput { files, ..CompileInput::default() });
    let lint = |found: &vc_diagnostics::Diagnostic| found.code.category() == vc_diagnostics::Category::Lifecycle;
    assert!(compiled.diagnostics.iter().all(lint), "{:?}", compiled.diagnostics);
    compiled.program.unwrap()
}

#[test]
fn a_test_is_standalone_unless_it_says_otherwise() {
    let program = program();
    assert!(test(&program, "users.crud#listsUsers").standalone, "the shape's default is true");
    let program = rows();
    let standalone = |id: &str| test(&program, id).standalone;
    let found = ["listsEachPage", "listsFilePages", "listsVarPages"].map(|id| standalone(&format!("users.rows#{id}")));
    assert_eq!(found, [true, false, true]);
}

#[test]
fn builds_examples_written_inline_from_a_file_or_from_a_template() {
    assert_eq!(test(&program(), "users.crud#listsUsers").examples, None);
    let built = rows();
    let examples = |id: &str| test(&built, id).examples.clone();
    let Some(Examples::Rows(rows)) = examples("users.rows#listsEachPage") else { panic!("inline rows") };
    assert_eq!(rows.len(), 2);
    assert!(matches!(examples("users.rows#listsFilePages"), Some(Examples::File(_))));
    let Some(Examples::Template(template)) = examples("users.rows#listsVarPages") else { panic!("a template") };
    assert!(template.single_ref().is_some());
}

const LOOPS: &str = "beforeEach:\n  - call: users#create\n    forEach: [a, b]\n    parallel: 2\n    vars: {name: '${item}'}\ntests:\n  - id: listsTwice\n    call: users#list\n    repeat: 2\n  - id: createsMany\n    steps:\n      - call: users#create\n        repeat: 3\n        vars: {n: '${iteration.index}'}\n      - call: users#list\n";

#[test]
fn lowers_repeat_and_for_each_on_actions_but_not_a_tests_repeat() {
    let program = with_tests("services/users/loops.test.yaml", LOOPS);
    let file = program.test_files.values().next().unwrap();
    let seed = action(&program, file.lifecycle.before_each[0]);
    let Some(Loop::ForEach { items: TemplateValue::List(items), parallel: 2 }) = &seed.repeat else {
        panic!("a forEach over a literal list, two at a time: {:?}", seed.repeat)
    };
    assert_eq!(items.len(), 2);
    let twice = test(&program, "users.loops#listsTwice");
    let TestBody::Call(call) = twice.body else { panic!("expected a call") };
    assert_eq!((twice.repeat, &action(&program, call).repeat), (2, &None), "a test's repeat stays on the test");
    let [many, list] = steps(&program, test(&program, "users.loops#createsMany"))[..] else { panic!("two steps") };
    assert_eq!(many.repeat, Some(Loop::Repeat { times: 3, parallel: 1 }));
    assert_eq!(list.repeat, None);
}

const DATA: &str = "tests:\n  - id: listsEachCity\n    call: users#list\n    examples: ${dataset.cities}\n  - id: listsFileRows\n    call: users#list\n    examples: {$file: rows.yaml}\n  - id: createsEachCity\n    steps:\n      - call: users#create\n        forEach: ${dataset.cities}\n        vars: {name: '${item.city}'}\n";

/// The users apis and [`DATA`], with the data files when `given`.
fn data(given: bool) -> crate::Compiled {
    let data =
        [file("datasets/cities.csv", "city,n\nOslo,1\nKöln,2\n"), file("services/users/rows.yaml", "- {page: 1}\n")];
    let assets = Some(data.iter().map(|file| file.path.clone()).collect());
    let mut files = vec![file("vibranium.config.yaml", CONFIG), file("services/users/users.api.yaml", APIS)];
    files.push(file("services/users/data.test.yaml", DATA));
    files.extend(data.into_iter().filter(|_| given));
    crate::compile(CompileInput { files, assets, ..CompileInput::default() })
}

#[test]
fn rows_from_a_given_dataset_or_file_are_built_as_literal_rows() {
    let wanted = data(false);
    let paths: Vec<&str> = wanted.data_wanted.iter().map(|data| data.path.as_str()).collect();
    assert_eq!(paths, ["datasets/cities.csv", "services/users/rows.yaml"]);
    let program = wanted.program.unwrap();
    assert!(matches!(test(&program, "users.data#listsEachCity").examples, Some(Examples::Template(_))));
    let compiled = data(true);
    assert_eq!((compiled.data_wanted.len(), compiled.diagnostics.has_errors()), (0, false));
    let program = compiled.program.unwrap();
    let examples = |id: &str| test(&program, id).examples.clone();
    let rows = |found| if let Some(Examples::Rows(rows)) = found { rows.len() } else { 0 };
    assert_eq!((rows(examples("users.data#listsEachCity")), rows(examples("users.data#listsFileRows"))), (2, 1));
    let [each] = steps(&program, test(&program, "users.data#createsEachCity"))[..] else { panic!("one step") };
    let Some(Loop::ForEach { items: TemplateValue::List(items), .. }) = &each.repeat else { panic!("{each:?}") };
    let cities = program.datasets.values().next().unwrap();
    assert_eq!((items.len(), cities.file.as_str(), cities.rows.len()), (2, "datasets/cities.csv", 2));
    assert_eq!(cities.rows[1]["city"], "Köln");
}

const RETRY_CONFIG: &str = "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nhttp:\n  retry:\n    attempts: 2\n    on: [connect, 503]\n    backoff: {base: 50ms, jitter: false}\n";

const RETRY_APIS: &str = "apis:\n  - id: list\n    path: /users\n  - id: put\n    method: PUT\n    path: /users\n    http:\n      retry: {methods: [PUT, PATCH], backoff: {max: 1s}}\n";

#[test]
fn http_retry_takes_the_configs_keys_and_an_apis_keys_over_them() {
    let files = vec![file("vibranium.config.yaml", RETRY_CONFIG), file("services/users/users.api.yaml", RETRY_APIS)];
    let compiled = crate::compile(CompileInput { files, ..CompileInput::default() });
    let program = compiled.program.unwrap();
    let retry = &program.config.http_retry;
    assert_eq!(
        (retry.attempts, &retry.on_status[..], &retry.on_transport[..]),
        (2, &[StatusPattern::Code(503)][..], &["connect".to_owned()][..])
    );
    let millis = |ms| Duration::new(std::time::Duration::from_millis(ms));
    assert_eq!((retry.base, retry.max, retry.jitter, retry.methods.len()), (millis(50), seconds(2), false, 5));
    let api = |id: &str| program.apis.values().find(|api| name(&program, api.id.sym()) == id).unwrap();
    assert_eq!(api("users#list").http_retry, None, "an api without `http` uses the config's");
    let put = api("users#put").http_retry.clone().unwrap();
    assert_eq!(
        (put.attempts, &put.methods[..], put.base, put.max),
        (2, &[Method::Put, Method::Patch][..], millis(50), seconds(1))
    );
    assert!(!put.jitter);
}

#[test]
fn for_each_wins_over_repeat_when_both_are_written() {
    let text = "tests:\n  - id: createsEach\n    steps:\n      - call: users#create\n        forEach: ['${var.baseUrl}']\n        repeat: 4\n";
    let program = with_tests("services/users/both.test.yaml", text);
    let [each] = steps(&program, test(&program, "users.both#createsEach"))[..] else { panic!("one step") };
    assert!(matches!(each.repeat, Some(Loop::ForEach { parallel: 1, .. })), "{:?}", each.repeat);
}

#[test]
fn lowers_cache_with_its_check_built_before_the_action() {
    let text = "beforeAll:\n  - id: admin\n    call: users#create\n    cache: {for: 12h, check: {call: users#list}}\ntests:\n  - id: listsUsers\n    call: users#list\n";
    let program = with_tests("services/users/cached.test.yaml", text);
    let admin = program.test_files.values().next().unwrap().lifecycle.before_all[0];
    let Some(cache) = action(&program, admin).cache else { panic!("a cache") };
    let check = cache.check.unwrap();
    assert_eq!((cache.ttl, check < admin), (seconds(12 * 3600), true));
    assert!(matches!(action(&program, check).kind, ActionKind::Call(_)));
}
