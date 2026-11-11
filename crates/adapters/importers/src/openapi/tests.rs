use std::sync::Arc;

use indexmap::IndexMap;
use serde_json::{Value, json};
use vc_interfaces::{
    DraftApi, DraftBefore, DraftCleanup, DraftExample, DraftGroup, DraftTest, ImportDraft, ImportError, ImportOptions,
    ImportSource, Importer,
};
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
    list.optional_query = vec!["tag".to_owned()];
    let mut create = api("createPet", Some("Create a pet"), Method::Post, "/pets");
    create.operation_id = Some("create_pet".to_owned());
    create.body = Some(json!({ "name": "${var.name}" }));
    create.vars = IndexMap::from([("name".to_owned(), json!("${fake.person.fullName}"))]);
    let read_id = || IndexMap::from([("petId".to_owned(), "${response.body.id}".to_owned())]);
    create.save = read_id();
    create.cleanup = Some(DraftCleanup { call: "deletePetsPetId".to_owned(), vars: read_id() });
    let mut get = api("getPet", Some("Get a pet"), Method::Get, "/pets/${var.petId}");
    get.vars = IndexMap::from([("petId".to_owned(), json!("${fake.number.int(1, 100)}"))]);
    let mut delete = api("deletePetsPetId", None, Method::Delete, "/pets/${var.petId}");
    delete.vars = get.vars.clone();
    vec![api("health", None, Method::Get, "/health"), list, create, get, delete]
}

fn expected_tests() -> Vec<DraftTest> {
    let pet = || Some("/components/schemas/Pet".to_owned());
    let pets = Some("/paths/~1pets/get/responses/200/content/application~1json/schema".to_owned());
    let create_first = |keep| vec![DraftBefore { call: "petstore#createPet".to_owned(), keep }];
    let no_auth = |id: &str, label: &str, call: &str| DraftTest {
        no_auth: true,
        ..test(id, &format!("{label}: needs a signed-in caller"), call, 401)
    };
    let row = |name: &str, body| DraftExample { name: name.to_owned(), body };
    let negative = DraftTest {
        body: Some(json!("${example.body}")),
        examples: vec![row("name missing", json!({})), row("name is not a string", json!({ "name": 123 }))],
        ..test("rejectsInvalidCreatePet", "Create a pet: rejects an invalid body", "createPet", 400)
    };
    vec![
        test("healths", "Health", "health", 200),
        DraftTest { schema: pets, ..test("listsPets", "List pets", "listPets", 200) },
        no_auth("rejectsListPetsWithoutAuth", "List pets", "listPets"),
        DraftTest { schema: pet(), ..test("createsPet", "Create a pet", "createPet", 201) },
        negative,
        no_auth("rejectsCreatePetWithoutAuth", "Create a pet", "createPet"),
        DraftTest { before: create_first(false), schema: pet(), ..test("getsPet", "Get a pet", "getPet", 200) },
        no_auth("rejectsGetPetWithoutAuth", "Get a pet", "getPet"),
        DraftTest {
            before: create_first(true),
            ..test("deletesPetsPetId", "Delete pets pet id", "deletePetsPetId", 204)
        },
    ]
}

/// A JSON response with `schema`.
fn json_response(schema: &Value) -> Value {
    json!({ "description": "ok", "content": { "application/json": { "schema": schema } } })
}

fn path_param(name: &str) -> Value {
    json!({ "name": name, "in": "path", "required": true, "schema": { "type": "string" } })
}

/// Users own orders; nothing creates a cart; signing in is one word.
fn shop() -> Value {
    let user = json!({ "type": "object", "required": ["age", "role"], "properties": {
        "age": { "type": "integer" },
        "role": { "type": "string", "enum": ["admin", "member"] },
    } });
    let create_user = json!({
        "operationId": "createUser",
        "requestBody": { "content": { "application/json": { "schema": user } } },
        "responses": {
            "201": json_response(&json!({ "properties": { "userId": { "type": "string" } } })),
            "422": { "description": "invalid" },
        },
    });
    let create_order = json!({ "operationId": "createOrder", "responses": {
        "201": json_response(&json!({ "properties": { "id": { "type": "integer" } } })),
    } });
    let order =
        json!({ "operationId": "getOrder", "responses": { "200": json_response(&json!({ "type": "object" })) } });
    json!({
        "openapi": "3.1.0",
        "info": { "title": "Shop", "version": "1" },
        "servers": [{ "url": "https://shop.test" }],
        "paths": {
            "/users": { "post": create_user },
            "/users/{userId}/orders": { "parameters": [path_param("userId")], "post": create_order },
            "/users/{userId}/orders/{orderId}": {
                "parameters": [path_param("userId"), path_param("orderId")],
                "get": order,
            },
            "/carts/{cartId}": {
                "parameters": [path_param("cartId")],
                "get": { "operationId": "getCart", "responses": { "200": { "description": "the cart" } } },
            },
            "/sessions": { "post": { "operationId": "login", "responses": { "204": { "description": "in" } } } },
        },
    })
}

async fn shop_draft() -> Result<DraftGroup, ImportError> {
    let opts = ImportOptions { file_id: "shop".to_owned(), tests: true, negative: true, ..ImportOptions::default() };
    let draft = read_json(&shop(), &opts).await?;
    Ok(draft.groups.first().cloned().unwrap_or_default())
}

fn drafted(group: &DraftGroup, id: &str) -> DraftTest {
    group.tests.iter().find(|test| test.id == id).cloned().unwrap_or_default()
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
async fn a_test_creates_each_item_its_path_names_first_and_skips_when_nothing_creates_one() -> Result<(), ImportError> {
    let group = shop_draft().await?;
    let calls = |id: &str| drafted(&group, id).before.into_iter().map(|action| action.call).collect::<Vec<_>>();
    assert_eq!(calls("getsOrder"), ["shop#createUser", "shop#createOrder"]);
    assert_eq!(calls("createsOrder"), ["shop#createUser"]);
    let cart = drafted(&group, "getsCart");
    assert_eq!(cart.skip.as_deref(), Some("needs an existing cart: set vars.cartId to one"));
    assert_eq!(cart.before, []);
    let saves: Vec<_> = group.apis.iter().map(|api| (api.id.as_str(), api.save.clone(), api.cleanup.clone())).collect();
    let save = |var: &str, read: &str| IndexMap::from([(var.to_owned(), read.to_owned())]);
    assert!(saves.contains(&("createUser", save("userId", "${response.body.userId}"), None)), "{saves:?}");
    assert!(saves.contains(&("createOrder", save("orderId", "${response.body.id}"), None)), "{saves:?}");
    Ok(())
}

#[tokio::test]
async fn a_test_expects_the_first_2xx_and_the_schema_it_documents() -> Result<(), ImportError> {
    let group = shop_draft().await?;
    let created = drafted(&group, "createsUser");
    let at = "/paths/~1users/post/responses/201/content/application~1json/schema";
    assert_eq!((created.name.as_str(), created.status, created.schema.as_deref()), ("Create user", 201, Some(at)));
    let order = drafted(&group, "getsOrder").schema;
    let at = "/paths/~1users~1%7BuserId%7D~1orders~1%7BorderId%7D/get/responses/200/content/application~1json/schema";
    assert_eq!(order.as_deref(), Some(at));
    let signed_in = drafted(&group, "logsIn");
    assert_eq!((signed_in.name.as_str(), signed_in.status, signed_in.schema), ("Login", 204, None));
    Ok(())
}

#[tokio::test]
async fn a_negative_test_has_a_row_per_missing_mistyped_and_unlisted_field() -> Result<(), ImportError> {
    let rejects = drafted(&shop_draft().await?, "rejectsInvalidCreateUser");
    let names: Vec<_> = rejects.examples.iter().map(|row| row.name.as_str()).collect();
    let expected =
        ["age missing", "age is not an integer", "role missing", "role is not a string", "role outside its enum"];
    assert_eq!(names, expected);
    let body = |at: usize| rejects.examples.get(at).map(|row| row.body.clone()).unwrap_or_default();
    assert_eq!(body(0), json!({ "role": "admin" }));
    assert_eq!(body(1), json!({ "age": "not a number", "role": "admin" }));
    assert_eq!(body(4).get("role"), Some(&json!("not-a-listed-value")));
    assert_eq!((rejects.name.as_str(), rejects.status), ("Create user: rejects an invalid body", 422));
    assert_eq!(rejects.body, Some(json!("${example.body}")));
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
