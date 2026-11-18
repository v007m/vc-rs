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
    let files = vec![
        file("vibranium.config.yaml", CONFIG),
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
fn every_key_a_type_allows_is_one_its_shape_knows() {
    for (protocol, keys) in PROTOCOL_KEYS {
        let ProtocolKeys::Only(keys) = keys else { continue };
        let places = [(shapes::api(), keys.api), (shapes::expect(), keys.expect), (shapes::request(), keys.call)];
        for (shape, allowed) in places {
            let unknown: Vec<_> = allowed.iter().filter(|key| shape.find(key).is_none()).collect();
            assert_eq!(unknown, [&""; 0], "{protocol:?} allows keys `{}` does not have", shape.title);
        }
    }
}
