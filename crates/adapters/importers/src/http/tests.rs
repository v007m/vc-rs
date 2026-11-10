use indexmap::IndexMap;
use serde_json::{Value, json};
use vc_interfaces::{DraftApi, ImportDraft, ImportError, ImportNote, ImportOptions, ImportSource, Importer};
use vc_model::Method;
use vc_testkit::importer_suite;

use super::HttpImporter;

const SAMPLE: &str = include_str!("../../fixtures/shop.http");

fn source(text: &str) -> ImportSource {
    ImportSource { location: "shop.http".to_owned(), bytes: text.as_bytes().to_vec() }
}

async fn read(text: &str) -> Result<ImportDraft, ImportError> {
    let opts = ImportOptions { file_id: "shop".to_owned(), ..ImportOptions::default() };
    HttpImporter.read(&source(text), &opts).await
}

fn map(pairs: &[(&str, Value)]) -> IndexMap<String, Value> {
    pairs.iter().map(|(name, value)| ((*name).to_owned(), value.clone())).collect()
}

fn api(id: &str, name: &str, (method, path): (Method, &str)) -> DraftApi {
    let name = Some(name.to_owned());
    DraftApi { id: id.to_owned(), name, method, path: path.to_owned(), ..DraftApi::default() }
}

fn sample_apis() -> Vec<DraftApi> {
    let rename = DraftApi {
        vars: map(&[("userId", json!("u_7f3k")), ("name", json!("Ada")), ("password", json!(""))]),
        body: Some(json!({"name": "${var.name}", "password": "${var.password}"})),
        content_type: Some("application/json".to_owned()),
        ..api("patchV1UsersUserId", "Rename a user", (Method::Patch, "/v1/users/${var.userId}"))
    };
    let orders = DraftApi {
        query: [("status", "open"), ("page", "2")].map(|(name, value)| (name.to_owned(), value.to_owned())).into(),
        ..api("getV1Orders", "List open orders", (Method::Get, "/v1/orders"))
    };
    let create = DraftApi {
        vars: map(&[("sku", json!("")), ("quantity", json!(2)), ("ref", json!("${fake.string.uuid}"))]),
        body: Some(json!({"sku": "${var.sku}", "quantity": "${var.quantity}", "ref": "${var.ref}"})),
        content_type: Some("application/json".to_owned()),
        ..api("postV1Orders", "createOrder", (Method::Post, "/v1/orders"))
    };
    let order = DraftApi {
        vars: map(&[("createOrderId", json!(""))]),
        ..api("getV1OrdersCreateOrderId", "The order just made", (Method::Get, "/v1/orders/${var.createOrderId}"))
    };
    vec![rename, orders, create, order]
}

#[tokio::test]
async fn behaves_as_an_importer() {
    importer_suite(&HttpImporter, &source(SAMPLE)).await;
}

#[tokio::test]
async fn each_request_drafts_an_api_with_the_file_variables_written_in() {
    let draft = read(SAMPLE).await.unwrap_or_default();
    let group = draft.groups.first().cloned().unwrap_or_default();
    assert_eq!((group.file_id.as_str(), group.base_url.as_deref()), ("shop", Some("${var.shopUrl}")));
    assert_eq!(group.config_vars, map(&[("shopUrl", json!("https://api.shop.test"))]));
    assert_eq!(group.apis, sample_apis());
}

#[tokio::test]
async fn what_is_left_out_or_undefined_is_noted_without_its_value() {
    let notes = read(SAMPLE).await.unwrap_or_default().notes;
    let notes: Vec<_> = notes.iter().map(|ImportNote { at, message }| format!("{at}: {message}")).collect();
    let rename = "PATCH /v1/users/${var.userId}";
    let order = "GET /v1/orders/${var.createOrderId}";
    let expected = [
        format!("{rename}: header Authorization: a credential is never written; add it with `vc new auth`"),
        format!("{rename}: headers are not imported: Accept"),
        format!("{rename}: body field password: a credential is never written, so it is empty"),
        "POST /v1/orders: a response handler is not imported".to_owned(),
        format!("{order}: {{{{createOrder.response.body.$.id}}}} reads another request's response; it is the var createOrderId"),
        "request 5: it calls https://status.shop.test, not https://api.shop.test: import it into another file with its own `--id`".to_owned(),
        "POST /v1/orders: sku is not defined in the file; give it a value".to_owned(),
        format!("{order}: createOrderId is not defined in the file; give it a value"),
    ];
    assert_eq!(notes, expected);
}

#[tokio::test]
async fn a_credential_never_reaches_the_draft() {
    let draft = format!("{:?}", read(SAMPLE).await);
    assert!(
        ["not-a-real-token", "not-a-real-password", "Bearer"].iter().all(|value| !draft.contains(value)),
        "{draft}"
    );
}

#[tokio::test]
async fn an_undefined_origin_and_a_bare_path_are_located() {
    let draft = read("GET {{host}}/a\n\n###\nGET /b\nHost: api.test\n").await.unwrap_or_default();
    let group = draft.groups.first().cloned().unwrap_or_default();
    assert_eq!(group.config_vars, map(&[("shopUrl", json!("http://localhost"))]));
    let notes: Vec<_> = draft.notes.iter().map(|note| note.message.as_str()).collect();
    assert!(notes.contains(&"{{host}} is not defined in the file; set the base-URL var to its value"), "{notes:?}");
    assert!(notes.iter().any(|note| note.contains("it calls http://api.test, not http://localhost")), "{notes:?}");
}

#[tokio::test]
async fn a_file_without_a_request_is_invalid() {
    let invalid = |reason: &str| -> Result<ImportDraft, ImportError> {
        Err(ImportError::Invalid { location: "shop.http".to_owned(), reason: reason.to_owned() })
    };
    assert_eq!(read("# nothing\n@a = 1\n").await, invalid("it holds no request"));
    assert_eq!(read("FROB https://a.test").await, invalid("request 1: an api cannot use FROB"));
    let text = "GET https://a.test/ok\n\n###\nnot a request";
    assert_eq!(read(text).await, invalid(r#"request 2: "not a request" is not a URL"#));
}
