use vc_model::{BodySpec, ContentHash, Program, SourceFile, SourcePath, TemplateValue};

use super::body_files;
use crate::CompileInput;

const CONFIG: &str = "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n";

const APIS: &str = r"baseUrl: http://u
vars: {name: u}
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
";

const TESTS: &str = "tests:\n  - id: sendsAnother\n    call: users#upload\n    body: {$file: ../../other.bin}\n";

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

fn literal(value: &TemplateValue) -> Option<&str> {
    match value {
        TemplateValue::String(template) => template.as_literal(),
        _ => None,
    }
}

#[test]
fn a_file_body_path_is_resolved_from_the_folder_of_its_file() {
    let program = program();
    let upload = program.apis.values().next().unwrap();
    let BodySpec::File(path) = &upload.body else { panic!("a `$file` body: {:?}", upload.body) };
    assert_eq!(path.as_literal(), Some("services/users/fixtures/u.csv"));
}

#[test]
fn a_file_part_in_a_map_or_list_is_resolved_too() {
    let program = program();
    let parts = program.apis.values().nth(1).unwrap();
    let BodySpec::Json(TemplateValue::Map(fields)) = &parts.body else { panic!("a map body: {:?}", parts.body) };
    let file = |value: &TemplateValue| match value {
        TemplateValue::Map(map) => map.get("$file").and_then(literal).map(str::to_owned),
        TemplateValue::List(items) => items.first().and_then(|item| match item {
            TemplateValue::Map(map) => map.get("$file").and_then(literal).map(str::to_owned),
            _ => None,
        }),
        _ => None,
    };
    assert_eq!(fields.get("doc").and_then(file).as_deref(), Some("services/shared/doc.pdf"));
    assert_eq!(fields.get("scans").and_then(file).as_deref(), Some("services/users/a.png"));
    assert_eq!(fields.get("caption").and_then(literal), Some("x"), "other fields stay as written");
}

#[test]
fn the_engine_is_told_every_literal_file_once_in_order() {
    let paths: Vec<String> = body_files(&program()).iter().map(ToString::to_string).collect();
    let expected = ["other.bin", "services/shared/doc.pdf", "services/users/a.png", "services/users/fixtures/u.csv"];
    assert_eq!(paths, expected, "a templated path is not read");
}
