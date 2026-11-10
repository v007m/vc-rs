use serde_json::json;
use vc_model::{CheckKind, ContentHash, Expect, Matcher, Part, Program, SourceFile, SourcePath, TemplateValue};

use crate::{CompileInput, Compiled};

const CONFIG: &str = "syntaxVersion: 2\nvars: {baseUrl: 'http://x'}\nprofiles:\n  - {id: dev, default: true}\n";

const APIS: &str = "baseUrl: ${var.baseUrl}\napis:\n  - id: get\n    path: /users/1\n    expect:\n      body:\n        schema: {$file: ../../schemas/user.json#/definitions/user}\n  - id: list\n    path: /users\n";

const TESTS: &str = "tests:\n  - id: getsUser\n    call: users#get\n  - id: listsUsers\n    call: users#list\n    expect:\n      body:\n        schema:\n          type: array\n          minItems: 1\n  - id: waitsForUsers\n    call: users#list\n    poll:\n      until: {body: {schema: ../../schemas/list.json}}\n      every: 1s\n      timeout: 5s\n";

const USER: &str = "{\"$schema\": \"http://json-schema.org/draft-06/schema#\", \"definitions\": {\"user\": {\"type\": \"object\", \"required\": [\"id\"]}}}\n";

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) }
}

/// The users apis and tests, with the schema files when `given`.
fn compile(given: bool) -> Compiled {
    let schemas = [file("schemas/user.json", USER), file("schemas/list.json", "{\"type\": \"array\"}\n")];
    let assets = Some(schemas.iter().map(|file| file.path.clone()).collect());
    let mut files = vec![
        file("vibranium.config.yaml", CONFIG),
        file("services/users/users.api.yaml", APIS),
        file("services/users/crud.test.yaml", TESTS),
    ];
    files.extend(schemas.into_iter().filter(|_| given));
    crate::compile(CompileInput { files, assets, ..CompileInput::default() })
}

/// The schema key of each `body.schema` check in `expect`.
fn keys(expect: &Expect) -> Vec<String> {
    let schemas = expect.checks.iter().filter(|check| check.kind == CheckKind::Schema);
    let key = |matcher: &Matcher| match matcher {
        Matcher::Equals(TemplateValue::String(template)) => match &template.parts[..] {
            [Part::Literal(key)] => key.clone(),
            other => format!("{other:?}"),
        },
        other => format!("{other:?}"),
    };
    schemas.map(|check| key(&check.matcher)).collect()
}

/// Every schema key the program's calls and polls check, sorted.
fn checked(program: &Program) -> Vec<String> {
    let actions = program.actions.iter();
    let mut found: Vec<String> = actions.flat_map(|action| keys(&action.expect)).collect();
    found.extend(program.actions.iter().filter_map(|action| action.poll.as_ref()).flat_map(|poll| keys(&poll.until)));
    found.sort();
    found.dedup();
    found
}

#[test]
fn schema_files_not_given_are_wanted_and_their_checks_still_built() {
    let compiled = compile(false);
    let paths: Vec<&str> = compiled.data_wanted.iter().map(|data| data.path.as_str()).collect();
    assert_eq!(paths, ["schemas/list.json", "schemas/user.json"]);
    let program = compiled.program.unwrap();
    let inline = "services/users/crud.test.yaml:9:11";
    assert_eq!(checked(&program), ["schemas/list.json", "schemas/user.json#/definitions/user", inline]);
    assert_eq!(program.schemas.keys().collect::<Vec<_>>(), [inline]);
    assert_eq!(program.schemas[inline], json!({"type": "array", "minItems": 1}));
}

#[test]
fn given_schema_files_are_held_whole_by_their_path() {
    let compiled = compile(true);
    assert_eq!(
        (compiled.data_wanted.len(), compiled.diagnostics.has_errors()),
        (0, false),
        "{:?}",
        compiled.diagnostics
    );
    let program = compiled.program.unwrap();
    let mut held: Vec<&str> = program.schemas.keys().map(String::as_str).collect();
    held.sort_unstable();
    assert_eq!(held, ["schemas/list.json", "schemas/user.json", "services/users/crud.test.yaml:9:11"]);
    assert_eq!(program.schemas["schemas/user.json"]["definitions"]["user"]["required"], json!(["id"]));
    assert_eq!(program.schemas["schemas/list.json"], json!({"type": "array"}));
}
