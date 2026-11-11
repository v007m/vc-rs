use std::sync::Arc;

use serde_json::json;
use vc_interfaces::{
    DraftApi, DraftBefore, DraftCleanup, DraftExample, DraftGroup, DraftTest, ImportDraft, ImportNote,
};
use vc_model::{FileKind, Method};
use vc_testkit::{FakeVcs, MemorySources};

use super::drafted;
use crate::create::header;
use crate::sample::{builder, engine};
use crate::{EngineError, ImportRequest};

const API: &str = "id: pets
name: Pets
baseUrl: ${var.petsUrl}/v1
source:
  openapi: https://pets.test/openapi.json
  include:
    - pets

apis:
  - id: list
    name: List
    path: /pets
    query:
      limit: ${var.limit}
  - id: create
    name: Create a pet
    method: POST
    path: /pets
    vars:
      name: ${fake.person.fullName}
    headers:
      content-type: application/x-www-form-urlencoded
    body:
      name: ${var.name}
    save:
      petId: ${response.body.id}
    cleanup:
      call: remove
      vars:
        petId: ${response.body.id}
    operationId: createPet
";

const TESTS: &str = "id: pets.openapi

tests:
  - id: listsPets
    name: List
    before:
      - call: pets#create
        keep: true
    call: pets#list
    expect:
      status: 200
      body:
        schema:
          $file: ../../openapi/pets.yaml#/components/schemas/Pet
  - id: rejectsInvalidCreate
    name: \"Create a pet: rejects an invalid body\"
    call: pets#create
    examples:
      - name: name missing
        body: {}
      - name: name is not a string
        body:
          name: 123
    body: ${example.body}
    expect:
      status: 400
  - id: rejectsCreateWithoutAuth
    name: \"Create a pet: needs a signed-in caller\"
    call: pets#create
    skip:
      reason: \"needs an existing pet: set vars.petId to one\"
    vars:
      name: null
    auth: none
    expect:
      status: 401
";

fn group(tests: bool) -> DraftGroup {
    let list = DraftApi {
        id: "list".to_owned(),
        path: "/pets".to_owned(),
        query: std::iter::once(("limit".to_owned(), "${var.limit}".to_owned())).collect(),
        ..DraftApi::default()
    };
    let create = DraftApi {
        id: "create".to_owned(),
        name: Some("Create a pet".to_owned()),
        method: Method::Post,
        path: "/pets".to_owned(),
        vars: std::iter::once(("name".to_owned(), json!("${fake.person.fullName}"))).collect(),
        body: Some(json!({ "name": "${var.name}" })),
        content_type: Some("application/x-www-form-urlencoded".to_owned()),
        operation_id: Some("createPet".to_owned()),
        save: std::iter::once(("petId".to_owned(), "${response.body.id}".to_owned())).collect(),
        cleanup: Some(DraftCleanup {
            call: "remove".to_owned(),
            vars: std::iter::once(("petId".to_owned(), "${response.body.id}".to_owned())).collect(),
        }),
        ..DraftApi::default()
    };
    DraftGroup {
        file_id: "pets".to_owned(),
        name: Some("Pets".to_owned()),
        base_url: Some("${var.draftUrl}/v1".to_owned()),
        config_vars: std::iter::once(("draftUrl".to_owned(), json!("https://pets.test"))).collect(),
        apis: vec![list, create],
        tests: if tests { drafted_tests() } else { Vec::new() },
    }
}

fn drafted_tests() -> Vec<DraftTest> {
    let ok = DraftTest {
        id: "listsPets".to_owned(),
        name: "List".to_owned(),
        before: vec![DraftBefore { call: "pets#create".to_owned(), keep: true }],
        call: "pets#list".to_owned(),
        status: 200,
        schema: Some("/components/schemas/Pet".to_owned()),
        ..DraftTest::default()
    };
    let row = |name: &str, body| DraftExample { name: name.to_owned(), body };
    let negative = DraftTest {
        id: "rejectsInvalidCreate".to_owned(),
        name: "Create a pet: rejects an invalid body".to_owned(),
        call: "pets#create".to_owned(),
        body: Some(json!("${example.body}")),
        examples: vec![row("name missing", json!({})), row("name is not a string", json!({ "name": 123 }))],
        status: 400,
        ..DraftTest::default()
    };
    let no_auth = DraftTest {
        id: "rejectsCreateWithoutAuth".to_owned(),
        name: "Create a pet: needs a signed-in caller".to_owned(),
        skip: Some("needs an existing pet: set vars.petId to one".to_owned()),
        call: "pets#create".to_owned(),
        vars: std::iter::once(("name".to_owned(), json!(null))).collect(),
        no_auth: true,
        status: 401,
        ..DraftTest::default()
    };
    vec![ok, negative, no_auth]
}

fn draft(tests: bool) -> ImportDraft {
    ImportDraft {
        groups: vec![group(tests)],
        notes: vec![ImportNote { at: "TRACE /echo".to_owned(), message: "no api method fits".to_owned() }],
        snapshot: Some(json!({ "openapi": "3.1.0", "paths": {} })),
        ..ImportDraft::default()
    }
}

fn request() -> ImportRequest {
    ImportRequest {
        kind: "openapi".to_owned(),
        source: "https://pets.test/openapi.json".to_owned(),
        include: vec!["pets".to_owned()],
        ..ImportRequest::default()
    }
}

#[test]
fn a_draft_becomes_an_api_file_a_test_file_the_base_var_and_a_snapshot() {
    let project = engine(FakeVcs::default()).authoring().unwrap();
    let (changes, snapshots) = drafted(&project, &draft(true), &request()).unwrap();
    let paths: Vec<_> = changes.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(paths, ["services/pets/openapi.test.yaml", "services/pets/pets.api.yaml", "vibranium.config.yaml"]);
    let after = |at: usize| changes.files.get(at).map(|file| file.after.clone()).unwrap_or_default();
    assert_eq!(after(0), format!("{}{TESTS}", header(FileKind::Test, "pets.openapi")));
    assert_eq!(after(1), format!("{}{API}", header(FileKind::Api, "pets")));
    assert!(after(2).contains("petsUrl: https://pets.test"), "{}", after(2));
    assert_eq!(
        changes.titles,
        ["Import 2 apis from https://pets.test/openapi.json", "TRACE /echo: no api method fits"]
    );
    let snapshot = snapshots.first().unwrap();
    assert_eq!((snapshot.path.as_str(), snapshots.len()), ("openapi/pets.yaml", 1));
    assert!(snapshot.after.starts_with("# Snapshot of https://pets.test/openapi.json as OpenAPI 3.1"));
    assert!(
        snapshot
            .after
            .ends_with("`vc sync pets` merges against it: commit it, and do not edit it.\nopenapi: 3.1.0\npaths: {}\n")
    );
}

#[test]
fn a_config_var_that_holds_the_origin_is_reused_and_files_that_exist_are_refused() {
    let sources = MemorySources::new(&[
        ("vibranium.config.yaml", b"syntaxVersion: 2\nvars: {petsUrl: 'https://pets.test'}\n"),
        ("services/pets/openapi.test.yaml", b"tests:\n  - id: keepsMine\n    call: users#list\n"),
        ("services/users/users.api.yaml", b"baseUrl: ${var.petsUrl}\napis:\n  - id: list\n    path: /users\n"),
    ]);
    let project = builder(FakeVcs::default()).sources(Arc::new(sources)).build().unwrap().authoring().unwrap();
    let (changes, _) = drafted(&project, &draft(false), &request()).unwrap();
    let paths: Vec<_> = changes.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(paths, ["services/pets/pets.api.yaml"]);
    let refused = drafted(&project, &draft(true), &request()).err();
    assert!(
        matches!(&refused, Some(EngineError::Edit { reason, .. }) if reason == "`pets.openapi` exists"),
        "{refused:?}"
    );
    let users =
        ImportDraft { groups: vec![DraftGroup { file_id: "users".to_owned(), ..group(false) }], ..draft(false) };
    let refused = drafted(&project, &users, &request()).err();
    let help = refused.as_ref().and_then(EngineError::help);
    assert_eq!(help.as_deref(), Some("`vc sync users` merges the spec into it"), "{refused:?}");
}

#[test]
fn the_operations_an_import_took_by_id_are_kept_in_its_source_for_sync() {
    let project = engine(FakeVcs::default()).authoring().unwrap();
    let only = ImportRequest { only: vec!["createPet".to_owned()], ..request() };
    let (changes, _) = drafted(&project, &draft(false), &only).unwrap();
    let api = changes.files.iter().find(|file| file.path == "services/pets/pets.api.yaml");
    let source = "  include:\n    - pets\n  only:\n    - createPet\n\napis:";
    assert!(api.is_some_and(|api| api.after.contains(source)), "{api:?}");
}
