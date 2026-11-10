use indexmap::IndexMap;
use serde_json::{Value, json};
use vc_interfaces::{DraftApi, ImportDraft, ImportError, ImportNote, ImportOptions, ImportSource, Importer};
use vc_model::Method;
use vc_testkit::importer_suite;

use super::CurlImporter;

const SAMPLE: &str = include_str!("../../fixtures/shop.curl");

fn source(text: &str) -> ImportSource {
    ImportSource { location: "shop.curl".to_owned(), bytes: text.as_bytes().to_vec() }
}

async fn read(text: &str) -> Result<ImportDraft, ImportError> {
    let opts = ImportOptions { file_id: "shop".to_owned(), ..ImportOptions::default() };
    CurlImporter.read(&source(text), &opts).await
}

fn map(pairs: &[(&str, Value)]) -> IndexMap<String, Value> {
    pairs.iter().map(|(name, value)| ((*name).to_owned(), value.clone())).collect()
}

fn api(id: &str, method: Method, path: &str) -> DraftApi {
    DraftApi { id: id.to_owned(), method, path: path.to_owned(), ..DraftApi::default() }
}

fn sample_apis() -> Vec<DraftApi> {
    let rename = DraftApi {
        vars: map(&[
            ("userId", json!("u_7f3k")),
            ("name", json!("Ada")),
            ("address", json!({"city": "Paris"})),
            ("password", json!("")),
        ]),
        body: Some(json!({"name": "${var.name}", "address": "${var.address}", "password": "${var.password}"})),
        content_type: Some("application/json".to_owned()),
        ..api("patchV1UsersUserId", Method::Patch, "/v1/users/${var.userId}")
    };
    let orders = DraftApi {
        query: [("status", "open"), ("page", "2")].map(|(name, value)| (name.to_owned(), value.to_owned())).into(),
        ..api("getV1Orders", Method::Get, "/v1/orders")
    };
    let items = DraftApi {
        vars: map(&[("orderId", json!("42"))]),
        body: Some(json!({"note": "gift", "receipt": ""})),
        content_type: Some("multipart/form-data".to_owned()),
        ..api("postV1OrdersOrderIdItems", Method::Post, "/v1/orders/${var.orderId}/items")
    };
    let search = DraftApi {
        query: IndexMap::from([("q".to_owned(), "red shoes".to_owned())]),
        ..api("getV1Search", Method::Get, "/v1/search")
    };
    let login = DraftApi {
        body: Some(json!({"user": "ada", "password": ""})),
        content_type: Some("application/x-www-form-urlencoded".to_owned()),
        ..api("postV1Login", Method::Post, "/v1/login")
    };
    let user = DraftApi {
        vars: map(&[("userId", json!("u_9xq2"))]),
        ..api("getV1UsersUserId", Method::Get, "/v1/users/${var.userId}")
    };
    vec![rename, orders, items, search, login, user]
}

#[tokio::test]
async fn behaves_as_an_importer() {
    importer_suite(&CurlImporter, &source(SAMPLE)).await;
}

#[tokio::test]
async fn each_command_drafts_an_api_against_one_base_url() {
    let draft = read(SAMPLE).await.unwrap_or_default();
    let group = draft.groups.first().cloned().unwrap_or_default();
    assert_eq!((group.file_id.as_str(), group.base_url.as_deref()), ("shop", Some("${var.shopUrl}")));
    assert_eq!(group.config_vars, map(&[("shopUrl", json!("https://api.shop.test"))]));
    assert_eq!(group.apis, sample_apis());
}

#[tokio::test]
async fn what_is_left_out_is_noted_without_its_value() {
    let notes = read(SAMPLE).await.unwrap_or_default().notes;
    let notes: Vec<_> = notes.iter().map(|ImportNote { at, message }| format!("{at}: {message}")).collect();
    let auth = "a credential is never written; add it with `vc new auth`";
    let emptied = "a credential is never written, so it is empty";
    let expected = [
        format!("PATCH /v1/users/${{var.userId}}: -b: {auth}"),
        format!("PATCH /v1/users/${{var.userId}}: header authorization: {auth}"),
        "PATCH /v1/users/${var.userId}: headers are not imported: accept".to_owned(),
        format!("PATCH /v1/users/${{var.userId}}: body field password: {emptied}"),
        format!("GET /v1/orders: -u: {auth}"),
        format!("GET /v1/orders: query access_token: {auth}"),
        "POST /v1/orders/${var.orderId}/items: form field receipt: the file it sends is not read, so it is empty"
            .to_owned(),
        format!("POST /v1/login: body field password: {emptied}"),
        "GET /v1/users/${var.userId}: the same api as `getV1UsersUserId`, which is kept".to_owned(),
        "curl 8: it calls https://cdn.shop.test, not https://api.shop.test: import it into another file with its own `--id`"
            .to_owned(),
    ];
    assert_eq!(notes, expected);
}

#[tokio::test]
async fn a_credential_never_reaches_the_draft() {
    let draft = format!("{:?}", read(SAMPLE).await);
    let values = ["eyJhbGciOi", "Bearer", "abc123", "s3cret", "t0k3n", "hunter2"];
    assert!(values.iter().all(|value| !draft.contains(value)), "{draft}");
}

#[tokio::test]
async fn a_file_without_an_api_is_invalid() {
    let invalid = |reason: &str| -> Result<ImportDraft, ImportError> {
        Err(ImportError::Invalid { location: "shop.curl".to_owned(), reason: reason.to_owned() })
    };
    assert_eq!(read("echo hi").await, invalid("it holds no curl command"));
    assert_eq!(read("curl -v").await, invalid("curl 1: it names no URL"));
    assert_eq!(read("curl 'a").await, invalid("a ' quote is not closed"));
    assert_eq!(read("curl -X FROB a.test").await, invalid("curl 1: an api cannot use FROB"));
    let latin1 = ImportSource { location: "shop.curl".to_owned(), bytes: vec![b'c', 0xff] };
    let opts = ImportOptions::default();
    assert_eq!(CurlImporter.read(&latin1, &opts).await, invalid("it is not UTF-8 text"));
}
