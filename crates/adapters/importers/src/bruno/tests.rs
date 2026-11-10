use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use vc_interfaces::{
    FetchError, ImportDraft, ImportError, ImportNote, ImportOptions, ImportSource, Importer, SpecFetcher,
};
use vc_testkit::importer_suite;

use super::BrunoImporter;
use crate::shop_sample::{assert_files, checkout, secrets as shop_secrets};

const SAMPLE: &str = include_str!("../../fixtures/shop.bruno.json");

/// The same collection as its folder of `.bru` files, with a folder `bruno.json` leaves out.
const FOLDER: [(&str, &str); 16] = [
    ("Carts/add-item.bru", include_str!("../../fixtures/shop.bruno/Carts/add-item.bru")),
    ("Carts/create-cart.bru", include_str!("../../fixtures/shop.bruno/Carts/create-cart.bru")),
    ("Carts/folder.bru", include_str!("../../fixtures/shop.bruno/Carts/folder.bru")),
    ("Carts/get-cart.bru", include_str!("../../fixtures/shop.bruno/Carts/get-cart.bru")),
    ("Checkout/folder.bru", include_str!("../../fixtures/shop.bruno/Checkout/folder.bru")),
    ("Checkout/pay.bru", include_str!("../../fixtures/shop.bruno/Checkout/pay.bru")),
    ("Checkout/upload-receipt.bru", include_str!("../../fixtures/shop.bruno/Checkout/upload-receipt.bru")),
    ("bruno.json", include_str!("../../fixtures/shop.bruno/bruno.json")),
    ("collection.bru", include_str!("../../fixtures/shop.bruno/collection.bru")),
    ("drafts/later.bru", include_str!("../../fixtures/shop.bruno/drafts/later.bru")),
    ("environments/Local.bru", include_str!("../../fixtures/shop.bruno/environments/Local.bru")),
    ("environments/Staging.bru", include_str!("../../fixtures/shop.bruno/environments/Staging.bru")),
    ("health.bru", include_str!("../../fixtures/shop.bruno/health.bru")),
    ("login.bru", include_str!("../../fixtures/shop.bruno/login.bru")),
    ("search.bru", include_str!("../../fixtures/shop.bruno/search.bru")),
    ("stream-prices.bru", include_str!("../../fixtures/shop.bruno/stream-prices.bru")),
];

/// Serves the files of one folder, `bruno/shop`, which also lists a receipt it cannot fetch:
/// only `bruno.json` and the `.bru` files are read.
struct Folder(Vec<(&'static str, &'static str)>);

#[async_trait]
impl SpecFetcher for Folder {
    async fn fetch(&self, location: &str) -> Result<Vec<u8>, FetchError> {
        let path = location.strip_prefix("bruno/shop/");
        let found = self.0.iter().find(|(file, _)| path == Some(*file));
        let missing = || FetchError::Failed { location: location.to_owned(), reason: "not found".to_owned() };
        found.map(|(_, text)| text.as_bytes().to_vec()).ok_or_else(missing)
    }

    async fn list(&self, folder: &str) -> Result<Vec<String>, FetchError> {
        let files = self.0.iter().map(|(file, _)| (*file).to_owned()).chain(["receipt.pdf".to_owned()]);
        Ok(if folder == "bruno/shop" { files.collect() } else { Vec::new() })
    }

    fn decode(&self, location: &str, _bytes: &[u8]) -> Result<Value, FetchError> {
        Err(FetchError::Undecodable { location: location.to_owned(), reason: "never".to_owned() })
    }
}

fn importer(files: &[(&'static str, &'static str)]) -> BrunoImporter {
    BrunoImporter::new(Arc::new(Folder(files.to_vec())))
}

/// The values the sample holds that must never be written: a token in a secret environment
/// variable, a nonce, a basic credential and a password.
const RECORDED: [&str; 4] = ["Rt7Ny2Kb4Hd6Fj9Zq8Wv3Lp5", "Xk9pQ2mZ7vR4tL8wB3nY6cH1", "YW5uOmh1bnRlcjI=", "hunter2"];

fn source(text: &str) -> ImportSource {
    ImportSource { location: "shop.bruno.json".to_owned(), bytes: text.as_bytes().to_vec() }
}

async fn read(text: &str) -> Result<ImportDraft, ImportError> {
    let opts = ImportOptions { file_id: "shop".to_owned(), ..ImportOptions::default() };
    importer(&FOLDER).read(&source(text), &opts).await
}

fn folder(location: &str) -> ImportSource {
    ImportSource { location: location.to_owned(), bytes: Vec::new() }
}

#[tokio::test]
async fn behaves_as_an_importer() {
    importer_suite(&importer(&FOLDER), &source(SAMPLE)).await;
    importer_suite(&importer(&FOLDER), &folder("bruno/shop")).await;
}

#[tokio::test]
async fn a_collection_folder_drafts_what_its_json_export_drafts() {
    let opts = ImportOptions {
        file_id: "shop".to_owned(),
        flow: Some("shop.checkout#buysACart".to_owned()),
        ..ImportOptions::default()
    };
    let exported = importer(&[]).read(&source(SAMPLE), &opts).await;
    let read = importer(&FOLDER).read(&folder("bruno/shop"), &opts).await;
    assert!(read.is_ok(), "{read:?}");
    assert_eq!(read, exported);
}

#[tokio::test]
async fn a_folder_without_bruno_json_or_with_a_broken_file_is_invalid() {
    let opts = ImportOptions { file_id: "shop".to_owned(), ..ImportOptions::default() };
    let refusal =
        async |files: &[(&'static str, &'static str)]| match importer(files).read(&folder("bruno/shop"), &opts).await {
            Err(ImportError::Invalid { location, reason }) if location == "bruno/shop" => reason,
            other => format!("not invalid: {other:?}"),
        };
    let no_config = refusal(&FOLDER[..7]).await;
    assert_eq!(no_config, "it is not a Bruno collection: the folder has no bruno.json");
    let broken = [("bruno.json", "{\"name\": \"Shop\"}"), ("health.bru", "get {\n  url: http://x/health\n")];
    assert_eq!(refusal(&broken).await, "health.bru: its get block has no closing }");
    let none = [("bruno.json", "{\"name\": \"Empty\"}"), ("Later/folder.bru", "meta {\n  name: Later\n}\n")];
    assert_eq!(refusal(&none).await, "it holds no request to an api: collection Empty · 0 requests in 1 folder");
}

#[tokio::test]
async fn each_origin_the_collection_calls_drafts_one_api_file_from_its_first_environment() {
    let form = json!({
        "cartId": "${var.cartId}",
        "nonce": "${secret.PAY_NONCE}",
        "merchant": "${secret.PAY_MERCHANT_ID}",
        "email": "${fake.internet.email}",
    });
    assert_files(&read(SAMPLE).await.unwrap_or_default(), form);
}

#[tokio::test]
async fn secret_and_environment_variables_become_references_named_after_their_file_and_field() {
    let secrets = read(SAMPLE).await.unwrap_or_default().secrets;
    assert_eq!(secrets, shop_secrets(&[("PAY_MERCHANT_ID", "POST /payments: var MERCHANT_ID")]));
}

#[tokio::test]
async fn auth_headers_scripts_tests_and_other_requests_that_are_left_out_are_noted() {
    let notes = read(SAMPLE).await.unwrap_or_default().notes;
    let notes: Vec<_> = notes.iter().map(|ImportNote { at, message }| format!("{at}: {message}")).collect();
    let auth = "header authorization: never written; it maps to the file's auth";
    let client = "header x-client is not imported";
    let expected = [
        format!("POST /v1/carts: {auth}"),
        format!("POST /v1/carts: {client}"),
        "POST /v1/carts: the tests are not imported".to_owned(),
        format!("POST /v1/carts/${{var.cartId}}/items: {auth}"),
        format!("POST /v1/carts/${{var.cartId}}/items: {client}"),
        "POST /v1/carts/${var.cartId}/items: the assertions are not imported".to_owned(),
        format!("GET /v1/carts/${{var.cartId}}: {auth}"),
        format!("GET /v1/carts/${{var.cartId}}: {client}"),
        "GET /v1/carts/${var.cartId}: the pre-request script is not imported".to_owned(),
        format!("POST /payments: {client}"),
        format!("POST /payments/${{var.paymentId}}/receipt: {client}"),
        "POST /payments/${var.paymentId}/receipt: a multipart body is not imported, so it is left out".to_owned(),
        format!("GET /v1/health: {auth}"),
        format!("GET /v1/health: {client}"),
        format!("GET /v1/products: {client}"),
        format!("POST /v1/login: {client}"),
        format!("POST /v1/login: {auth}"),
        "POST /v1/login: header x-request-id is not imported".to_owned(),
        "collection: the collection's scripts are not imported".to_owned(),
        "collection: the vars default to environment Local; Staging is not imported".to_owned(),
        "collection: Stream prices is a grpc request, which is not imported".to_owned(),
    ];
    assert_eq!(notes, expected);
}

#[tokio::test]
async fn the_flow_calls_each_request_in_order_sending_on_what_post_response_vars_save() {
    let opts = ImportOptions { flow: Some("shop.checkout#buysACart".to_owned()), ..ImportOptions::default() };
    let flow = importer(&[]).read(&source(SAMPLE), &opts).await.ok().and_then(|draft| draft.flow);
    assert_eq!(flow, Some(checkout()));
}

#[tokio::test]
async fn a_secret_in_the_collection_never_reaches_the_draft_or_its_notes() {
    let opts = ImportOptions { flow: Some("shop.checkout#buysACart".to_owned()), ..ImportOptions::default() };
    for source in [source(SAMPLE), folder("bruno/shop")] {
        let draft = importer(&FOLDER).read(&source, &opts).await;
        let notes = draft.as_ref().map(|draft| draft.notes.clone()).unwrap_or_default();
        let notes: Vec<_> = notes.iter().map(|note| format!("{} {}", note.at, note.message)).collect();
        let output = format!("{draft:?}\n{}", notes.join("\n"));
        let leaked: Vec<_> = RECORDED.iter().filter(|value| output.contains(*value)).collect();
        assert!(leaked.is_empty(), "{leaked:?} in {output}");
    }
}

async fn reason(text: &str) -> String {
    match read(text).await {
        Err(ImportError::Invalid { location, reason }) if location == "shop.bruno.json" => reason,
        other => format!("not invalid: {other:?}"),
    }
}

#[tokio::test]
async fn a_file_that_is_not_a_bruno_export_or_calls_no_api_is_invalid() {
    let not = "it is not a Bruno collection: export the collection as JSON from Bruno";
    assert_eq!(reason("meta {\n  name: Ping\n  type: http\n}").await, not);
    assert_eq!(reason("{\"info\": {}, \"item\": []}").await, not);
    let empty = json!({"name": "Empty", "items": [{"type": "folder", "name": "Later", "items": []}]});
    assert_eq!(
        reason(&empty.to_string()).await,
        "it holds no request to an api: collection Empty · 0 requests in 1 folder"
    );
}
