use serde_json::{Value, json};
use vc_interfaces::{DraftFlow, ImportDraft, ImportError, ImportNote, ImportOptions, ImportSource, Importer};
use vc_testkit::importer_suite;

use super::PostmanImporter;
use crate::shop_sample::{assert_files, checkout, map, secrets as shop_secrets};

const SAMPLE: &str = include_str!("../../fixtures/shop.postman.json");

/// The values the sample holds that must never be written: a bearer token and an api key in
/// variables, a nonce, a basic credential and a password.
const RECORDED: [&str; 5] =
    ["Rt7Ny2Kb4Hd6Fj9Zq8Wv3Lp5", "Lp5Rt7Ny2Kb4Hd6Fj9Zq8Wv3", "Xk9pQ2mZ7vR4tL8wB3nY6cH1", "YW5uOmh1bnRlcjI=", "hunter2"];

fn source(text: &str) -> ImportSource {
    ImportSource { location: "shop.postman.json".to_owned(), bytes: text.as_bytes().to_vec() }
}

async fn read(text: &str) -> Result<ImportDraft, ImportError> {
    let opts = ImportOptions { file_id: "shop".to_owned(), ..ImportOptions::default() };
    PostmanImporter.read(&source(text), &opts).await
}

async fn read_flow(id: &str) -> Option<DraftFlow> {
    let opts = ImportOptions { flow: Some(id.to_owned()), ..ImportOptions::default() };
    PostmanImporter.read(&source(SAMPLE), &opts).await.ok().and_then(|draft| draft.flow)
}

#[tokio::test]
async fn behaves_as_an_importer() {
    importer_suite(&PostmanImporter, &source(SAMPLE)).await;
}

#[tokio::test]
async fn each_origin_the_collection_calls_drafts_one_api_file() {
    let form = json!({"cartId": "${var.cartId}", "nonce": "${secret.PAY_NONCE}", "email": "${fake.internet.email}"});
    assert_files(&read(SAMPLE).await.unwrap_or_default(), form);
}

#[tokio::test]
async fn credentials_and_random_values_become_references_named_after_their_file_and_field() {
    let secrets = read(SAMPLE).await.unwrap_or_default().secrets;
    assert_eq!(secrets, shop_secrets(&[]));
}

#[tokio::test]
async fn auth_headers_and_scripts_that_are_left_out_are_noted() {
    let notes = read(SAMPLE).await.unwrap_or_default().notes;
    let notes: Vec<_> = notes.iter().map(|ImportNote { at, message }| format!("{at}: {message}")).collect();
    let auth = "header authorization: never written; it maps to the file's auth";
    let saves = "the test script is not imported, apart from the values it saves";
    let expected = [
        format!("POST /v1/carts: {auth}"),
        "POST /v1/carts: header accept is not imported".to_owned(),
        format!("POST /v1/carts: {saves}"),
        format!("POST /v1/carts/${{var.cartId}}/items: {auth}"),
        format!("GET /v1/carts/${{var.cartId}}: {auth}"),
        "GET /v1/carts/${var.cartId}: the pre-request script is not imported".to_owned(),
        format!("POST /payments: {saves}"),
        "POST /payments/${var.paymentId}/receipt: a multipart body is not imported, so it is left out".to_owned(),
        format!("GET /v1/health: {auth}"),
        format!("POST /v1/login: {auth}"),
        "POST /v1/login: header x-request-id is not imported".to_owned(),
        "collection: the collection's scripts are not imported".to_owned(),
    ];
    assert_eq!(notes, expected);
}

#[tokio::test]
async fn the_flow_calls_each_request_in_order_sending_on_what_scripts_save() {
    assert_eq!(read_flow("shop.checkout#buysACart").await, Some(checkout()));
    let bare = read_flow("buysACart").await.map(|flow| (flow.file_id, flow.test_id));
    assert_eq!(bare, Some(("shop.recorded".to_owned(), "buysACart".to_owned())));
    assert_eq!(read(SAMPLE).await.ok().and_then(|draft| draft.flow), None);
}

#[tokio::test]
async fn a_secret_in_the_collection_never_reaches_the_draft_or_its_notes() {
    let opts = ImportOptions { flow: Some("shop.checkout#buysACart".to_owned()), ..ImportOptions::default() };
    let draft = PostmanImporter.read(&source(SAMPLE), &opts).await;
    let notes = draft.as_ref().map(|draft| draft.notes.clone()).unwrap_or_default();
    let notes: Vec<_> = notes.iter().map(|note| format!("{} {}", note.at, note.message)).collect();
    let output = format!("{draft:?}\n{}", notes.join("\n"));
    let leaked: Vec<_> = RECORDED.iter().filter(|value| output.contains(*value)).collect();
    assert!(leaked.is_empty(), "{leaked:?} in {output}");
}

const SCHEMA: &str = "https://schema.getpostman.com/json/collection/v2.1.0/collection.json";

fn collection(item: &Value) -> String {
    json!({"info": {"name": "Ping", "schema": SCHEMA}, "item": item}).to_string()
}

#[tokio::test]
async fn a_base_url_without_a_value_is_the_local_host_until_it_is_set() {
    let item = json!([{"name": "Ping", "request": "{{host}}/ping/{{$randomColor}}"}]);
    let draft = read(&collection(&item)).await.unwrap_or_default();
    let files: Vec<_> = draft.groups.iter().map(|group| (group.file_id.clone(), group.config_vars.clone())).collect();
    assert_eq!(files, [("shop".to_owned(), map(&[("shopUrl", json!("http://localhost"))]))]);
    let paths: Vec<_> = draft.groups.iter().flat_map(|group| &group.apis).map(|api| api.path.as_str()).collect();
    assert_eq!(paths, ["/ping/{{$randomColor}}"]);
    let notes: Vec<_> = draft.notes.iter().map(|ImportNote { at, message }| format!("{at}: {message}")).collect();
    let expected = [
        "collection: its base URL has no value, so `shopUrl` is http://localhost: set it in the config",
        "GET /ping/{{$randomColor}}: `{{$randomColor}}` has no generator, so it is left as written",
    ];
    assert_eq!(notes, expected);
}

async fn reason(text: &str) -> String {
    match read(text).await {
        Err(ImportError::Invalid { location, reason }) if location == "shop.postman.json" => reason,
        other => format!("not invalid: {other:?}"),
    }
}

#[tokio::test]
async fn a_file_that_is_not_a_v2_1_collection_or_calls_no_api_is_invalid() {
    let not = "it is not a Postman collection v2.1: export it from Postman as Collection v2.1";
    assert_eq!(reason(&SAMPLE.replacen("/collection/v2.1.0/", "/collection/v2.0.0/", 1)).await, not);
    assert_eq!(reason("{\"item\": []}").await, not);
    let empty = json!({"info": {"name": "Empty", "schema": SCHEMA}, "item": [{"name": "Later", "item": []}]});
    assert_eq!(
        reason(&empty.to_string()).await,
        "it holds no request to an api: collection Empty · 0 requests in 1 folder"
    );
}
