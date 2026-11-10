use std::sync::Arc;

use indexmap::IndexMap;
use serde_json::{Value, json};
use vc_interfaces::{DraftApi, DraftGroup, DraftTest, ImportDraft, ImportError, ImportOptions, ImportSource, Importer};
use vc_model::Method;

use super::OpenApiImporter;
use super::fixture::MapFetcher;

const PETSTORE: [(&str, &str); 3] = [
    ("petstore-2.0.json", include_str!("../../fixtures/petstore-2.0.json")),
    ("petstore-3.0.json", include_str!("../../fixtures/petstore-3.0.json")),
    ("petstore-3.1.json", include_str!("../../fixtures/petstore-3.1.json")),
];

fn importer() -> OpenApiImporter {
    OpenApiImporter::new(Arc::new(MapFetcher::default()))
}

fn opts() -> ImportOptions {
    ImportOptions {
        file_id: "petstore".to_owned(),
        tests: true,
        negative: true,
        auth: true,
        ..ImportOptions::default()
    }
}

async fn read(location: &str, text: &str, opts: &ImportOptions) -> Result<ImportDraft, ImportError> {
    importer().read(&ImportSource { location: location.to_owned(), bytes: text.as_bytes().to_vec() }, opts).await
}

async fn read_json(doc: &Value, opts: &ImportOptions) -> Result<ImportDraft, ImportError> {
    read("spec.json", &doc.to_string(), opts).await
}

fn api(id: &str, name: Option<&str>, method: Method, path: &str) -> DraftApi {
    let operation_id = Some(id.to_owned()).filter(|id| !id.starts_with("delete"));
    DraftApi {
        id: id.to_owned(),
        name: name.map(str::to_owned),
        method,
        path: path.to_owned(),
        operation_id,
        ..DraftApi::default()
    }
}

fn test(id: &str, name: &str, call: &str, status: u16) -> DraftTest {
    DraftTest {
        id: id.to_owned(),
        name: name.to_owned(),
        call: format!("petstore#{call}"),
        status,
        ..DraftTest::default()
    }
}

fn expected_apis() -> Vec<DraftApi> {
    let mut list = api("listPets", Some("List pets"), Method::Get, "/pets");
    list.query = IndexMap::from([("limit".to_owned(), "${var.limit}".to_owned())]);
    list.vars = IndexMap::from([("limit".to_owned(), json!(20))]);
    let mut create = api("createPet", Some("Create a pet"), Method::Post, "/pets");
    create.operation_id = Some("create_pet".to_owned());
    create.body = Some(json!({ "name": "${var.name}" }));
    create.vars = IndexMap::from([("name".to_owned(), json!("${fake.person.fullName}"))]);
    let mut get = api("getPet", Some("Get a pet"), Method::Get, "/pets/${var.petId}");
    get.vars = IndexMap::from([("petId".to_owned(), json!("${fake.number.int(1, 100)}"))]);
    let mut delete = api("deletePetsPetId", None, Method::Delete, "/pets/${var.petId}");
    delete.vars = get.vars.clone();
    vec![api("health", None, Method::Get, "/health"), list, create, get, delete]
}

fn expected_tests() -> Vec<DraftTest> {
    let mut negative = test("rejectsCreatePetWithoutName", "Create a pet without name returns 400", "createPet", 400);
    negative.vars = IndexMap::from([("name".to_owned(), Value::Null)]);
    let no_auth = |id: &str, name: &str, call: &str| DraftTest { no_auth: true, ..test(id, name, call, 401) };
    vec![
        test("healths", "health returns 200", "health", 200),
        test("listsPets", "List pets returns 200", "listPets", 200),
        no_auth("rejectsListPetsWithoutAuth", "List pets without auth returns 401", "listPets"),
        test("createsPet", "Create a pet returns 201", "createPet", 201),
        negative,
        no_auth("rejectsCreatePetWithoutAuth", "Create a pet without auth returns 401", "createPet"),
        test("getsPet", "Get a pet returns 200", "getPet", 200),
        no_auth("rejectsGetPetWithoutAuth", "Get a pet without auth returns 401", "getPet"),
        test("deletesPetsPetId", "deletePetsPetId returns 204", "deletePetsPetId", 204),
    ]
}

#[tokio::test]
async fn every_version_of_the_petstore_gives_the_same_draft() -> Result<(), ImportError> {
    let expected = DraftGroup {
        file_id: "petstore".to_owned(),
        name: Some("Petstore".to_owned()),
        base_url: Some("${var.petstoreUrl}/v1".to_owned()),
        config_vars: IndexMap::from([("petstoreUrl".to_owned(), json!("https://petstore.test"))]),
        apis: expected_apis(),
        tests: Vec::new(),
    };
    for (location, text) in PETSTORE {
        let draft = read(location, text, &opts()).await?;
        let group = draft.groups.first().cloned().unwrap_or_default();
        assert_eq!(DraftGroup { tests: Vec::new(), ..group.clone() }, expected, "{location}");
        assert_eq!(group.tests, expected_tests(), "{location}");
        assert!(draft.notes.is_empty(), "{location}: {:?}", draft.notes);
        assert_eq!(draft.snapshot.as_ref().and_then(|doc| doc.get("openapi")), Some(&json!("3.1.0")), "{location}");
    }
    Ok(())
}

#[tokio::test]
async fn the_importer_meets_the_contract() {
    let (location, text) = PETSTORE[2];
    let sample = ImportSource { location: location.to_owned(), bytes: text.as_bytes().to_vec() };
    vc_testkit::importer_suite(&importer(), &sample).await;
}

#[tokio::test]
async fn documents_that_are_not_a_read_version_say_why() {
    let cases = [
        (json!([1, 2]), "it is not an OpenAPI document"),
        (json!({ "info": {} }), "it names no `openapi` or `swagger` version"),
        (json!({ "openapi": "4.0.0" }), "OpenAPI 4.0.0 is not read; 2.0, 3.0 and 3.1 are"),
        (json!({ "swagger": 1.2 }), "OpenAPI 1.2 is not read; 2.0, 3.0 and 3.1 are"),
    ];
    for (doc, reason) in cases {
        let expected = ImportError::Invalid { location: "spec.json".to_owned(), reason: reason.to_owned() };
        assert_eq!(read_json(&doc, &opts()).await, Err(expected));
    }
}

#[tokio::test]
async fn a_swagger_version_written_as_a_number_is_read() -> Result<(), ImportError> {
    let doc = json!({ "swagger": 2.0, "info": { "title": "T", "version": "1" }, "paths": {} });
    let draft = read_json(&doc, &opts()).await?;
    assert_eq!(draft.groups.first().map(|group| group.apis.len()), Some(0));
    Ok(())
}

#[tokio::test]
async fn include_keeps_tags_and_path_prefixes_and_only_keeps_named_operations() -> Result<(), ImportError> {
    let (location, text) = PETSTORE[2];
    let ids = |draft: &ImportDraft| -> Vec<String> {
        draft.groups.iter().flat_map(|group| group.apis.iter().map(|api| api.id.clone())).collect()
    };
    let include =
        |include: &[&str]| ImportOptions { include: include.iter().map(|&i| i.to_owned()).collect(), ..opts() };
    assert_eq!(ids(&read(location, text, &include(&["ops"])).await?), ["health"]);
    assert_eq!(ids(&read(location, text, &include(&["/pets/"])).await?), ["getPet", "deletePetsPetId"]);
    let only = ImportOptions { only: vec!["create_pet".to_owned(), "deletePetsPetId".to_owned()], ..opts() };
    assert_eq!(ids(&read(location, text, &only).await?), ["createPet", "deletePetsPetId"]);
    Ok(())
}

#[tokio::test]
async fn an_operation_no_api_method_fits_is_noted_and_skipped() -> Result<(), ImportError> {
    let doc = json!({
        "openapi": "3.1.0",
        "info": { "title": "T", "version": "1" },
        "paths": { "/echo": { "trace": { "responses": {} }, "get": { "operationId": "echo", "responses": {} } } },
    });
    let draft = read_json(&doc, &ImportOptions { file_id: "echo".to_owned(), ..ImportOptions::default() }).await?;
    let group = draft.groups.first().cloned().unwrap_or_default();
    assert_eq!(group.apis.iter().map(|api| api.id.as_str()).collect::<Vec<_>>(), ["echo"]);
    assert_eq!(group.tests, []);
    assert_eq!(draft.notes.len(), 1);
    assert_eq!(draft.notes.first().map(|note| note.at.as_str()), Some("TRACE /echo"));
    Ok(())
}

#[tokio::test]
async fn bytes_that_do_not_decode_are_invalid_at_their_location() {
    let result = read("broken.yaml", "{", &opts()).await;
    assert!(matches!(result, Err(ImportError::Invalid { location, .. }) if location == "broken.yaml"));
}
