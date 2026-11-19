use std::collections::HashSet;

use vc_diagnostics::{Code, Diagnostic};
use vc_model::{
    AllowedKeys, ContentHash, FileKind, PROTOCOL_KEYS, Protocol, ProtocolKeys, SourceFile, SourceFileId, SourcePath,
    StringTable,
};

use super::Table;
use crate::link::project::{Project, Source};
use crate::parts::Tools;
use crate::{CompileInput, Compiled, shapes, stage};

const CONFIG: &str =
    "syntaxVersion: 2\nproject: shop\nvars: {baseUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n";

const APIS: &str = "\
baseUrl: ${var.baseUrl}
apis:
  - id: get
    path: /users/1
  - id: make
    method: POST
    path: /users
    expect:
      status: 201
      size: {lt: 10kb}
";

const TESTS: &str = "\
tests:
  - id: makesUser
    call: users#make
    poll:
      until: ${response.status == 201}
  - id: getsUser
    steps:
      - call: users#get
        expect:
          size: {lt: 1kb}
";

/// A shared action whose `poll.until` and nested `cache.check` call set keys RESTRICTED takes away.
const NESTED: &str = "\
beforeAll:
  - id: seed
    call: users#make
    poll:
      until:
        size: {lt: 1kb}
    cache:
      for: 1h
      check:
        call: users#get
        timeout: 5s
tests:
  - id: getsUser
    call: users#get
";

/// http with `method`, `size` and `poll` taken away, as a type that restricts keys would.
const RESTRICTED: [(Protocol, ProtocolKeys); 1] = [(
    Protocol::Http,
    ProtocolKeys::Only(AllowedKeys {
        api: &["id", "name", "type", "path", "expect"],
        expect: &["status", "headers", "body"],
        call: &["vars", "expect", "save"],
    }),
)];

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) }
}

fn compile(apis: &str, tests: &str) -> Compiled {
    compile_with(CONFIG, apis, tests)
}

fn compile_with(config: &str, apis: &str, tests: &str) -> Compiled {
    let files = vec![
        file("vibranium.config.yaml", config),
        file("services/users/users.api.yaml", apis),
        file("services/users/crud.test.yaml", tests),
    ];
    crate::compile(CompileInput { files, ..CompileInput::default() })
}

/// V2213 over the users apis and `tests`, checked against `table`.
fn checked(table: &Table, tests: &str) -> Vec<Diagnostic> {
    let files = [file("services/users/users.api.yaml", APIS), file("services/users/crud.test.yaml", tests)];
    let (tools, mut strings) = (Tools::new(), StringTable::new());
    let numbered = files.iter().zip(0..).map(|(source, at)| (SourceFileId::new(at), source));
    let units: Vec<_> =
        numbered.map(|(id, source)| (id, source, stage::unit(id, source, (&tools, &mut strings)))).collect();
    let sources = units.iter().filter_map(|(id, source, unit)| Source::new(*id, source, unit)).collect();
    let project = Project::new(sources, &strings, HashSet::new());
    super::check(&project, table).iter().cloned().collect()
}

fn api_protocols(compiled: &Compiled) -> Vec<Protocol> {
    let program = compiled.program.as_ref().unwrap_or_else(|| panic!("{:?}", compiled.diagnostics));
    program.apis.values().map(|api| api.protocol).collect()
}

#[test]
fn an_explicit_http_type_compiles_to_the_same_program_as_none() {
    let tests = "tests:\n  - id: getsUser\n    call: users#get\n";
    let plain = compile(APIS, tests);
    let typed = compile(&format!("{APIS}type: http\n"), tests);
    assert_eq!(typed.diagnostics.iter().count(), 0, "{:?}", typed.diagnostics);
    let json = |compiled: &Compiled| serde_json::to_value(compiled.program.as_deref().unwrap()).unwrap();
    assert_eq!(json(&typed), json(&plain));
}

#[test]
fn an_api_takes_its_own_type_else_its_files_else_http() {
    let tests = "tests:\n  - id: getsUser\n    call: users#get\n";
    let inherited = "type: http\napis:\n  - id: get\n    path: http://x/users/1\n";
    let own = "apis:\n  - id: get\n    type: http\n    path: http://x/users/1\n";
    let both = "type: http\napis:\n  - id: get\n    type: http\n    path: http://x/users/1\n";
    for apis in [APIS, inherited, own, both] {
        assert!(api_protocols(&compile(apis, tests)).iter().all(|protocol| *protocol == Protocol::Http));
    }
}

#[test]
fn a_misspelt_type_gets_the_close_value_as_its_fix() {
    let apis = "apis:\n  - id: get\n    type: htp\n    path: http://x/users/1\n";
    let compiled = compile(apis, "tests:\n  - id: getsUser\n    call: users#get\n");
    let wrong = compiled.diagnostics.iter().find(|found| found.code == Code::V2302).unwrap();
    assert_eq!(wrong.message, "expected one of http, graphql, found `htp`");
    assert_eq!(wrong.fixes[0].edits[0].new_text, "http");
}

#[test]
fn http_allows_every_key_so_nothing_is_reported() {
    assert_eq!(checked(&PROTOCOL_KEYS, TESTS), []);
    assert_eq!(checked(&PROTOCOL_KEYS, NESTED), []);
    assert!(compile(APIS, TESTS).diagnostics.iter().all(|found| found.code != Code::V2213));
}

#[test]
fn errors_on_an_http_api_or_a_call_that_reaches_one_is_reported_and_a_graphql_api_takes_it() {
    let apis = "apis:\n  - id: get\n    path: http://x/users/1\n    expect:\n      errors: none\n";
    let tests = "tests:\n  - id: getsUser\n    call: users#get\n    expect:\n      errors: [{code: FORBIDDEN}]\n";
    let compiled = compile(apis, tests);
    let stray: Vec<_> = compiled.diagnostics.iter().filter(|found| found.code == Code::V2213).collect();
    let found: Vec<_> = stray.iter().map(|found| (found.message.as_str(), found.primary.span.start.line)).collect();
    let expected = [("`errors` does not apply to a `http` api", 5), ("`errors` does not apply to a `http` api", 5)];
    assert_eq!(found, expected, "the test file's first, then the api file's");
    assert_eq!(&tests[stray[0].fixes[0].edits[0].range.clone()], "      errors: [{code: FORBIDDEN}]\n");
    let graphql =
        format!("type: graphql\n{}", apis.replace("users/1\n", "graphql\n    body: {query: '{ me { id } }'}\n"));
    assert!(compile(&graphql, tests).diagnostics.iter().all(|found| found.code != Code::V2213));
}

#[test]
fn a_test_level_errors_check_that_reaches_an_http_step_is_reported_once_at_the_test() {
    let apis = "apis:\n  - id: get\n    path: http://x/users/1\n";
    let steps = "    steps:\n      - call: users#get\n      - call: users#get\n";
    let tests = format!("tests:\n  - id: getsUser\n    expect:\n      errors: any\n{steps}");
    let compiled = compile(apis, &tests);
    let stray: Vec<_> = compiled.diagnostics.iter().filter(|found| found.code == Code::V2213).collect();
    let [found] = stray.as_slice() else { panic!("{stray:?}") };
    assert_eq!((found.message.as_str(), found.primary.span.start.line), ("`errors` does not apply to a `http` api", 4));
    assert_eq!(found.primary.text, "a test-level default");
    assert!(found.help.iter().any(|help| help.contains("into the `expect` of the steps")), "{found:?}");
    assert_eq!(found.fixes, []);
    let own = tests.replace("call: users#get\n", "call: users#get\n        expect: {errors: none}\n");
    let lines: Vec<_> = compile(apis, &own)
        .diagnostics
        .iter()
        .filter(|found| found.code == Code::V2213)
        .map(|found| found.primary.span.start.line)
        .collect();
    assert_eq!(lines, [7, 9], "each step's own, and not the default no step inherits");
    let graphql = "type: graphql\napis:\n  - id: get\n    path: http://x/graphql\n    body: {query: '{ me { id } }'}\n";
    let typo = tests.replace("      errors: any\n", "      errors: any\n      statsu: 200\n");
    let codes: Vec<_> = compile(graphql, &typo).diagnostics.iter().map(|found| found.code).collect();
    assert_eq!(codes, [Code::V1004], "a key no type knows is V1004 alone");
}

#[test]
fn a_step_whose_expect_replaces_inherits_no_errors_so_it_is_not_reported() {
    let apis = "type: graphql\napis:\n  - id: get\n    path: http://x/graphql\n    body: {query: 'query Me { me { id } }'}\n  \
                - id: health\n    type: http\n    path: http://x/health\n";
    let steps = "    steps:\n      - call: users#get\n      - call: users#health\n        expect: {replace: true, status: 200}\n";
    let tests = format!("tests:\n  - id: getsUser\n    expect:\n      errors: any\n{steps}");
    let codes = |tests: &str| compile(apis, tests).diagnostics.iter().map(|found| found.code).collect::<Vec<_>>();
    assert_eq!(codes(&tests), []);
    assert_eq!(codes(&tests.replace("replace: true", "replace: false")), [Code::V2213]);
}

#[test]
fn errors_on_a_flow_auth_step_is_reported_with_its_removal_since_the_step_is_http() {
    let step = "      - id: token\n        method: POST\n        url: http://x/token\n        expect:\n          errors: none\n";
    let auth = format!("auth:\n  - id: login\n    type: flow\n    steps:\n{step}    apply: {{headers: {{a: b}}}}\n");
    let config = format!("{CONFIG}{auth}");
    let compiled = compile_with(&config, APIS, "tests:\n  - id: getsUser\n    call: users#get\n");
    let stray: Vec<_> = compiled.diagnostics.iter().filter(|found| found.code == Code::V2213).collect();
    let [found] = stray.as_slice() else { panic!("{:?}", compiled.diagnostics) };
    assert_eq!(found.message, "`errors` does not apply to a `http` api");
    assert_eq!(&config[found.fixes[0].edits[0].range.clone()], "          errors: none\n");
}

#[test]
fn a_key_the_type_does_not_allow_is_reported_with_a_fix_that_removes_it() {
    let found = checked(&RESTRICTED, TESTS);
    let keys: Vec<_> = found.iter().map(|found| (found.code, found.primary.span.start.line)).collect();
    assert_eq!(keys, [(Code::V2213, 6), (Code::V2213, 10), (Code::V2213, 4), (Code::V2213, 10)]);
    let method = &found[0];
    assert_eq!(method.message, "`method` does not apply to a `http` api");
    assert_eq!(method.fixes[0].title, "Remove `method`");
    let edit = &method.fixes[0].edits[0];
    assert_eq!((&APIS[edit.range.clone()], edit.new_text.as_str()), ("    method: POST\n", ""));
    assert_eq!(found[2].message, "`poll` does not apply to a `http` api");
    assert_eq!(&TESTS[found[2].fixes[0].edits[0].range.clone()], "    poll:\n      until: ${response.status == 201}\n");
}

#[test]
fn a_poll_until_and_a_nested_cache_check_call_are_checked_too() {
    let found = checked(&RESTRICTED, NESTED);
    let in_tests = found.iter().filter(|found| found.primary.span.file == SourceFileId::new(1));
    let keys: Vec<_> = in_tests.map(|found| (found.message.as_str(), found.primary.span.start.line)).collect();
    let stray = |key: &str| format!("`{key}` does not apply to a `http` api");
    assert_eq!(keys, [(stray("poll").as_str(), 4), (stray("size").as_str(), 6), (stray("timeout").as_str(), 11)]);
}

#[test]
fn the_type_key_offers_every_protocol() {
    let keys: Vec<_> = Protocol::ALL.into_iter().map(Protocol::key).collect();
    for shape in [shapes::root(FileKind::Api), shapes::api()] {
        let ty = shape.all_keys().find(|key| key.name == "type").map(|key| &key.ty);
        assert!(matches!(ty, Some(shapes::Ty::Enum(words)) if *words == keys.as_slice()), "{}", shape.title);
    }
}

#[test]
fn a_graphql_api_reports_a_method_and_a_call_that_sets_a_body() {
    let apis = "type: graphql\napis:\n  - id: get\n    method: GET\n    path: http://x/graphql\n    body: {query: '{ me { id } }'}\n";
    let tests = "tests:\n  - id: getsMe\n    call: users#get\n    body: {query: '{ other }'}\n";
    let compiled = compile(apis, tests);
    let stray: Vec<_> = compiled.diagnostics.iter().filter(|found| found.code == Code::V2213).collect();
    let found: Vec<_> = stray.iter().map(|found| (found.message.as_str(), found.primary.span.start.line)).collect();
    let expected = [("`body` does not apply to a `graphql` api", 4), ("`method` does not apply to a `graphql` api", 4)];
    assert_eq!(found, expected, "the test file's first, then the api file's");
    assert_eq!(&tests[stray[0].fixes[0].edits[0].range.clone()], "    body: {query: '{ other }'}\n");
    assert_eq!(&apis[stray[1].fixes[0].edits[0].range.clone()], "    method: GET\n");
    assert_eq!(api_protocols(&compile(&apis.replace("    method: GET\n", ""), "tests: []\n")), [Protocol::Graphql]);
}

#[test]
fn every_key_a_type_lists_is_one_its_shape_knows() {
    for (protocol, keys) in PROTOCOL_KEYS {
        let (ProtocolKeys::Only(keys) | ProtocolKeys::Except(keys)) = keys;
        let places = [(shapes::api(), keys.api), (shapes::expect(), keys.expect), (shapes::request(), keys.call)];
        for (shape, allowed) in places {
            let unknown: Vec<_> = allowed.iter().filter(|key| shape.find(key).is_none()).collect();
            assert_eq!(unknown, [&""; 0], "{protocol:?} lists keys `{}` does not have", shape.title);
        }
    }
}

#[test]
fn a_test_level_body_that_reaches_a_graphql_step_is_reported_without_a_fix() {
    let apis = "type: graphql\nbaseUrl: http://x\napis:\n  - id: me\n    body: {query: '{ me { id } }'}\n  - id: make\n    type: http\n    method: POST\n    path: /users\n";
    let steps = "    steps:\n      - call: users#make\n      - test: users.crud#other\n      - call: users#me\n";
    let tests = format!(
        "tests:\n  - id: makesUser\n    headers: {{x-a: '1'}}\n    body: {{name: a}}\n{steps}  - id: other\n    call: users#make\n"
    );
    let compiled = compile(apis, &tests);
    let found: Vec<_> = compiled.diagnostics.iter().filter(|found| found.code == Code::V2213).collect();
    let [body] = found.as_slice() else { panic!("{:?}", compiled.diagnostics) };
    assert_eq!((body.message.as_str(), body.primary.span.start.line), ("`body` does not apply to a `graphql` api", 4));
    assert_eq!(body.secondary[0].span.start.line, 8);
    assert_eq!(body.secondary[0].text, "a `graphql` call without its own `body`");
    assert_eq!(body.help, ["move `body` onto the steps that send one; a `graphql` call does not take it"]);
    assert!(body.fixes.is_empty(), "the http step may send it");
    assert!(compiled.program.is_none());
    let http_only = tests.replace("      - call: users#me\n", "");
    assert!(compile(apis, &http_only).diagnostics.iter().all(|found| found.code != Code::V2213));
    let own = tests.replace("      - call: users#me\n", "      - call: users#me\n        vars: {}\n        body: x\n");
    let lines: Vec<_> = compile(apis, &own)
        .diagnostics
        .iter()
        .filter(|found| found.code == Code::V2213)
        .map(|found| found.primary.span.start.line)
        .collect();
    assert_eq!(lines, [10], "a step with its own body reports that body, not the test's");
}

#[test]
fn a_call_takes_a_key_unless_the_type_it_reaches_leaves_it_out() {
    let apis = "baseUrl: http://x\napis:\n  - id: me\n    type: graphql\n    body: {query: '{ me { id } }'}\n  - id: get\n    path: /users\n";
    let tests =
        "tests:\n  - id: a\n    call: users#me\n  - id: b\n    call: users#get\n  - id: c\n    call: users#nope\n";
    let files = [file("services/users/users.api.yaml", apis), file("services/users/crud.test.yaml", tests)];
    let (tools, mut strings) = (Tools::new(), StringTable::new());
    let numbered = files.iter().zip(0..).map(|(source, at)| (SourceFileId::new(at), source));
    let units: Vec<_> =
        numbered.map(|(id, source)| (id, source, stage::unit(id, source, (&tools, &mut strings)))).collect();
    let sources = units.iter().filter_map(|(id, source, unit)| Source::new(*id, source, unit)).collect();
    let project = Project::new(sources, &strings, HashSet::new());
    let source = &project.sources[1];
    let takes = |key: &str| -> Vec<bool> {
        crate::link::list(source.root, "tests")
            .iter()
            .map(|test| super::call_takes(&project, (source, test), (&PROTOCOL_KEYS, key)))
            .collect()
    };
    assert_eq!(takes("body"), [false, true, true]);
    assert_eq!(takes("headers"), [true, true, true]);
}

#[test]
fn a_graphql_block_on_a_file_of_another_type_is_v2213_with_its_removal_as_the_fix() {
    let apis = "baseUrl: http://x\ngraphql:\n  path: /api/graphql\napis:\n  - id: me\n    type: graphql\n    body: {query: 'query Me { me { id } }'}\n";
    let tests = "tests:\n  - id: getsMe\n    call: users#me\n";
    let compiled = compile(apis, tests);
    let found: Vec<_> = compiled.diagnostics.iter().collect();
    let [block] = found.as_slice() else { panic!("{:?}", compiled.diagnostics) };
    assert_eq!((block.code, block.message.as_str()), (Code::V2213, "`graphql` does not apply to a `http` file"));
    assert_eq!(block.primary.span.start.line, 2);
    assert_eq!(block.help, ["remove `graphql`, or set the file's `type: graphql`; only a `graphql` file reads it"]);
    let [fix] = block.fixes.as_slice() else { panic!("{:?}", block.fixes) };
    assert_eq!((fix.title.as_str(), fix.certain), ("Remove `graphql`", true));
    assert_eq!(&apis[fix.edits[0].range.clone()], "graphql:\n  path: /api/graphql\n");
    let typed = apis.replace("baseUrl: http://x\n", "baseUrl: http://x\ntype: graphql\n");
    assert!(compile(&typed, tests).diagnostics.is_empty());
}
