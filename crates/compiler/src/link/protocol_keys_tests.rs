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

/// V2213 over the users apis and tests, checked against `table`.
fn checked(table: &Table) -> Vec<Diagnostic> {
    let files = [file("services/users/users.api.yaml", APIS), file("services/users/crud.test.yaml", TESTS)];
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
    assert_eq!(wrong.message, "expected one of http, found `htp`");
    assert_eq!(wrong.fixes[0].edits[0].new_text, "http");
}

#[test]
fn http_allows_every_key_so_nothing_is_reported() {
    assert_eq!(checked(&PROTOCOL_KEYS), []);
    assert!(compile(APIS, TESTS).diagnostics.iter().all(|found| found.code != Code::V2213));
}

#[test]
fn a_key_the_type_does_not_allow_is_reported_with_a_fix_that_removes_it() {
    let found = checked(&RESTRICTED);
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
fn the_type_key_offers_every_protocol() {
    let keys: Vec<_> = Protocol::ALL.into_iter().map(Protocol::key).collect();
    for shape in [shapes::root(FileKind::Api), shapes::api()] {
        let ty = shape.all_keys().find(|key| key.name == "type").map(|key| &key.ty);
        assert!(matches!(ty, Some(shapes::Ty::Enum(words)) if *words == keys.as_slice()), "{}", shape.title);
    }
}
