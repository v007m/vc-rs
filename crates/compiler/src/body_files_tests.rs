use vc_model::{ActionId, ActionKind, BodySpec, ContentHash, Program, SourceFile, SourcePath, TemplateValue};

use super::body_files;
use crate::CompileInput;

const CONFIG: &str = "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {name: u, base: {a: 1}}\n";

const APIS: &str = r"baseUrl: http://u
apis:
  - id: upload
    method: POST
    path: /u
    body: {$file: fixtures/u.csv}
  - id: parts
    method: POST
    path: /p
    headers: {content-type: multipart/form-data}
    body: {doc: {$file: ../shared/doc.pdf}, scans: [{$file: ./a.png}], caption: x}
  - id: templated
    method: POST
    path: /t
    body: {$file: '${var.name}.csv'}
  - id: asJson
    method: POST
    path: /j
    body: {doc: {$file: fixtures/doc.pdf}}
  - id: outside
    method: POST
    path: /o
    body: {$file: ../../../secret.txt}
";

const TESTS: &str = r"tests:
  - id: sendsAnother
    call: users#upload
    body: {$file: ../../other.bin}
  - id: uploads
    call: users#upload
  - id: sendsParts
    call: users#parts
  - id: sendsTemplated
    call: users#templated
  - id: sendsJson
    call: users#asJson
  - id: sendsOutside
    call: users#outside
  - id: extendsParts
    call: users#asJson
    headers: {Content-Type: multipart/form-data; boundary=b}
    body:
      $extend: ${var.base}
      doc: {$file: ./b.pdf}
      note: {$file: '${var.name}'}
";

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) }
}

fn program() -> std::sync::Arc<Program> {
    let files = vec![
        file("vibranium.config.yaml", CONFIG),
        file("services/users/users.api.yaml", APIS),
        file("services/users/crud.test.yaml", TESTS),
    ];
    let compiled = crate::compile(CompileInput { files, ..CompileInput::default() });
    assert!(!compiled.diagnostics.has_errors(), "{:?}", compiled.diagnostics);
    compiled.program.unwrap()
}

/// Every call action, in order.
fn calls(program: &Program) -> Vec<ActionId> {
    let calls = program.actions.iter().enumerate().filter(|(_, action)| matches!(action.kind, ActionKind::Call(_)));
    calls.map(|(at, _)| ActionId::new(u32::try_from(at).unwrap())).collect()
}

/// The slots `body_files` finds for `actions`, as `pointer=path` text.
fn found(program: &Program, actions: Vec<ActionId>) -> Vec<String> {
    let found = body_files(program, actions);
    found
        .into_iter()
        .map(|((_, pointer), path)| format!("{pointer}={}", path.map_or_else(|e| e, |p| p.to_string())))
        .collect()
}

#[test]
fn the_bodies_stay_as_written() {
    let program = program();
    let upload = program.apis.values().next().unwrap();
    let BodySpec::File(path) = &upload.body else { panic!("a `$file` body: {:?}", upload.body) };
    assert_eq!(path.as_literal(), Some("fixtures/u.csv"));
    let parts = program.apis.values().nth(1).unwrap();
    let BodySpec::Json(TemplateValue::Map(fields)) = &parts.body else { panic!("a map body: {:?}", parts.body) };
    let TemplateValue::Map(doc) = &fields["doc"] else { panic!("a map part: {fields:?}") };
    let TemplateValue::String(written) = &doc["$file"] else { panic!("a text path: {doc:?}") };
    assert_eq!(written.as_literal(), Some("../shared/doc.pdf"));
}

#[test]
fn each_literal_file_is_resolved_from_the_folder_of_its_file_at_its_pointer() {
    let program = program();
    let actions = program.actions.iter().enumerate().map(|(at, _)| ActionId::new(u32::try_from(at).unwrap()));
    let mut all = found(&program, actions.collect());
    assert_eq!(body_files(&program, calls(&program)).len(), all.len(), "only calls and inline requests send bodies");
    all.sort();
    let expected = [
        "/doc=services/users/b.pdf",
        "/doc=services/shared/doc.pdf",
        "/scans/0=services/users/a.png",
        "=other.bin",
        "=services/users/fixtures/u.csv",
        "=it leads out of the project",
    ];
    let mut expected = expected.map(str::to_owned).to_vec();
    expected.sort();
    assert_eq!(all, expected, "a templated path is not read, and a json body's `$file` is sent as written");
}

#[test]
fn only_the_actions_given_are_looked_at() {
    let program = program();
    assert_eq!(found(&program, Vec::new()), Vec::<String>::new());
    let first = calls(&program).into_iter().take(1).collect();
    assert_eq!(found(&program, first), ["=other.bin"], "the test's own body is over its api's");
}
