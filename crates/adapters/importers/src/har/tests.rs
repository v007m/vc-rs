use indexmap::IndexMap;
use serde_json::{Value, json};
use vc_interfaces::{
    DraftApi, DraftFlow, DraftStep, ImportDraft, ImportError, ImportNote, ImportOptions, ImportSource, Importer,
    RecordingFilter, SecretFinding,
};
use vc_model::Method;
use vc_testkit::importer_suite;

use super::HarImporter;

const SAMPLE: &str = include_str!("../../fixtures/shop.har");

/// The values the sample recorded that must never be written: a bearer token, a session
/// cookie, an api key, a nonce, a card token, a password and a token a response gave back.
const RECORDED: [&str; 7] = [
    "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ1XzdmM2sifQ.Q2xvc2VkU2Vzc2lvbjQ",
    "s3ss10nC00k1e",
    "pk_9Rt2Lm8Qx4Vz7Nb3Kc6Wd1Hy",
    "Xk9pQ2mZ7vR4tL8wB3nY6cH1",
    "tok_visa",
    "hunter2",
    "Zq8Wv3Lp5Rt7Ny2Kb4Hd6Fj9",
];

fn source(text: &str) -> ImportSource {
    ImportSource { location: "shop.har".to_owned(), bytes: text.as_bytes().to_vec() }
}

async fn read(text: &str) -> Result<ImportDraft, ImportError> {
    let opts = ImportOptions { file_id: "recorded".to_owned(), ..ImportOptions::default() };
    HarImporter.read(&source(text), &opts).await
}

async fn read_flow(id: &str) -> Option<DraftFlow> {
    let opts = ImportOptions { flow: Some(id.to_owned()), ..ImportOptions::default() };
    HarImporter.read(&source(SAMPLE), &opts).await.ok().and_then(|draft| draft.flow)
}

fn step(call: &str, vars: &[(&str, Value)], save: &[(&str, &str)]) -> DraftStep {
    let save = save.iter().map(|(name, template)| ((*name).to_owned(), (*template).to_owned())).collect();
    DraftStep { call: call.to_owned(), vars: map(vars), save }
}

fn map(pairs: &[(&str, Value)]) -> IndexMap<String, Value> {
    pairs.iter().map(|(name, value)| ((*name).to_owned(), value.clone())).collect()
}

fn query(pairs: &[(&str, &str)]) -> IndexMap<String, String> {
    pairs.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())).collect()
}

fn api(id: &str, method: Method, path: &str) -> DraftApi {
    DraftApi { id: id.to_owned(), method, path: path.to_owned(), ..DraftApi::default() }
}

fn json_api(id: &str, path: &str, vars: &[(&str, Value)], body: Value) -> DraftApi {
    DraftApi {
        vars: map(vars),
        body: Some(body),
        content_type: Some("application/json".to_owned()),
        ..api(id, Method::Post, path)
    }
}

fn shop_apis() -> Vec<DraftApi> {
    let cart = ("cartId", json!("crt_8Kq2Tz"));
    let create =
        json_api("createCart", "/v1/carts", &[("currency", json!("EUR"))], json!({"currency": "${var.currency}"}));
    let add = json_api(
        "addItem",
        "/v1/carts/${var.cartId}/items",
        &[cart.clone(), ("sku", json!("sku_red42")), ("quantity", json!(1))],
        json!({"sku": "${var.sku}", "quantity": "${var.quantity}"}),
    );
    let get = DraftApi { vars: map(&[cart]), ..api("getCart", Method::Get, "/v1/carts/${var.cartId}") };
    let cancel = DraftApi {
        vars: map(&[("orderId", json!("ord_5Nw3Rt"))]),
        body: Some(json!({"reason": "changed mind", "password": "${secret.SHOP_PASSWORD}"})),
        content_type: Some("application/x-www-form-urlencoded".to_owned()),
        ..api("cancelOrder", Method::Post, "/v1/orders/${var.orderId}/cancel")
    };
    let list = DraftApi { query: query(&[("status", "open")]), ..api("listOrders", Method::Get, "/v1/orders") };
    vec![create, add, get, cancel, list]
}

fn pay_apis() -> Vec<DraftApi> {
    let card = json!({"number": "4242424242424242", "token": "${secret.PAY_TOKEN}"});
    let vars = [("cartId", json!("crt_8Kq2Tz")), ("nonce", json!("${secret.PAY_NONCE}")), ("card", card)];
    let body = json!({"cartId": "${var.cartId}", "nonce": "${var.nonce}", "card": "${var.card}"});
    let pay = DraftApi {
        query: query(&[("api_key", "${secret.PAY_API_KEY}"), ("currency", "EUR")]),
        ..json_api("createPayment", "/v1/payments", &vars, body)
    };
    vec![pay]
}

#[tokio::test]
async fn behaves_as_an_importer() {
    importer_suite(&HarImporter, &source(SAMPLE)).await;
}

#[tokio::test]
async fn each_origin_the_recording_calls_drafts_one_api_file() {
    let draft = read(SAMPLE).await.unwrap_or_default();
    let files: Vec<_> = draft.groups.iter().map(|group| (group.file_id.as_str(), group.base_url.as_deref())).collect();
    assert_eq!(files, [("shop", Some("${var.shopUrl}")), ("pay", Some("${var.payUrl}"))]);
    let vars: Vec<_> = draft.groups.iter().map(|group| group.config_vars.clone()).collect();
    let origins = [("shopUrl", "https://api.shop.test"), ("payUrl", "https://pay.shop.test")];
    assert_eq!(vars, origins.map(|(var, origin)| map(&[(var, json!(origin))])));
    let apis: Vec<_> = draft.groups.iter().map(|group| group.apis.clone()).collect();
    assert_eq!(apis, [shop_apis(), pay_apis()]);
}

#[tokio::test]
async fn secrets_become_references_named_after_their_file_and_field() {
    let secrets = read(SAMPLE).await.unwrap_or_default().secrets;
    let found = [
        ("PAY_TOKEN", "POST /v1/payments: body field token"),
        ("PAY_NONCE", "POST /v1/payments: body field nonce"),
        ("PAY_API_KEY", "POST /v1/payments: query api_key"),
        ("SHOP_PASSWORD", "POST /v1/orders/${var.orderId}/cancel: body field password"),
    ];
    let expected = found.map(|(name, at)| SecretFinding { name: name.to_owned(), found_at: at.to_owned() });
    assert_eq!(secrets, expected);
}

#[tokio::test]
async fn what_is_filtered_merged_or_left_out_is_noted() {
    let notes = read(SAMPLE).await.unwrap_or_default().notes;
    let notes: Vec<_> = notes.iter().map(|ImportNote { at, message }| format!("{at}: {message}")).collect();
    let headers = |at: &str| {
        ["authorization", "cookie"]
            .map(|name| format!("{at}: header {name}: never written; it maps to the file's auth"))
    };
    let mut expected =
        vec!["the recording: read 12 entries · filtered 3 static, 1 third-party, 1 OPTIONS → 7 requests".to_owned()];
    for at in ["POST /v1/carts", "POST /v1/carts/${var.cartId}/items", "GET /v1/carts/${var.cartId}", "GET /v1/orders"]
    {
        expected.extend(headers(at));
    }
    expected.push("POST /v1/carts/${var.cartId}/items: recorded 2 times, merged into one api".to_owned());
    assert_eq!(notes, expected);
}

async fn summary(opts: ImportOptions) -> Option<String> {
    let draft = HarImporter.read(&source(SAMPLE), &opts).await.ok()?;
    draft.notes.first().map(|note| note.message.clone())
}

#[tokio::test]
async fn the_project_hosts_include_hosts_and_keep_static_decide_what_is_noise() {
    let filtered = |counts: &str| Some(format!("read 12 entries · filtered {counts} requests"));
    let hosts = |hosts: &[&str]| hosts.iter().map(|host| (*host).to_owned()).collect::<Vec<_>>();
    let with = |recording: RecordingFilter| ImportOptions { recording, ..ImportOptions::default() };
    let own = with(RecordingFilter { hosts: hosts(&["API.shop.test"]), ..RecordingFilter::default() });
    assert_eq!(summary(own).await, filtered("3 static, 1 third-party, 1 OPTIONS → 7"));
    let other = with(RecordingFilter { hosts: hosts(&["www.google-analytics.com"]), ..RecordingFilter::default() });
    assert_eq!(summary(other).await, filtered("3 static, 7 third-party, 1 OPTIONS → 1"));
    let include =
        with(RecordingFilter { include_hosts: hosts(&["WWW.google-analytics.com"]), ..RecordingFilter::default() });
    assert_eq!(summary(include).await, filtered("3 static, 0 third-party, 1 OPTIONS → 8"));
    let assets = with(RecordingFilter { keep_static: true, ..RecordingFilter::default() });
    assert_eq!(summary(assets).await, filtered("0 static, 1 third-party, 1 OPTIONS → 10"));
}

#[tokio::test]
async fn the_flow_replays_the_recording_with_values_chained_from_responses() {
    let cart = || ("cartId", json!("${var.cartId}"));
    let steps = vec![
        step("shop#createCart", &[], &[("cartId", "${response.body.id}")]),
        step("shop#addItem", &[cart()], &[]),
        step("shop#addItem", &[cart(), ("sku", json!("sku_blu17")), ("quantity", json!(2))], &[]),
        step("shop#getCart", &[cart()], &[]),
        step("pay#createPayment", &[cart()], &[("orderId", "${response.body.order.id}")]),
        step("shop#cancelOrder", &[("orderId", json!("${var.orderId}"))], &[]),
        step("shop#listOrders", &[], &[]),
    ];
    let expected = DraftFlow { file_id: "shop.checkout".to_owned(), test_id: "replaysCheckout".to_owned(), steps };
    assert_eq!(read_flow("shop.checkout#replaysCheckout").await, Some(expected));
    let bare = read_flow("replaysCheckout").await.map(|flow| (flow.file_id, flow.test_id));
    assert_eq!(bare, Some(("shop.recorded".to_owned(), "replaysCheckout".to_owned())));
    assert_eq!(read(SAMPLE).await.ok().and_then(|draft| draft.flow), None);
}

#[tokio::test]
async fn a_recorded_secret_never_reaches_the_draft_or_its_notes() {
    let opts = ImportOptions { flow: Some("shop.checkout#replaysCheckout".to_owned()), ..ImportOptions::default() };
    let draft = HarImporter.read(&source(SAMPLE), &opts).await;
    let notes = draft.as_ref().map(|draft| draft.notes.clone()).unwrap_or_default();
    let notes: Vec<_> = notes.iter().map(|note| format!("{} {}", note.at, note.message)).collect();
    let output = format!("{draft:?}\n{}", notes.join("\n"));
    let leaked: Vec<_> = RECORDED.iter().filter(|value| output.contains(*value)).collect();
    assert!(leaked.is_empty(), "{leaked:?} in {output}");
}

#[tokio::test]
async fn a_har_1_3_file_reads_as_its_1_2_twin() {
    let newer = SAMPLE.replacen("\"version\": \"1.2\"", "\"version\": \"1.3\"", 1);
    assert_ne!(newer, SAMPLE);
    assert_eq!(read(&newer).await, read(SAMPLE).await);
}

async fn reason(text: &str) -> String {
    match HarImporter.read(&source(text), &ImportOptions::default()).await {
        Err(ImportError::Invalid { location, reason }) if location == "shop.har" => reason,
        other => format!("not invalid: {other:?}"),
    }
}

#[tokio::test]
async fn a_file_that_is_not_a_har_or_calls_no_api_is_invalid() {
    let unsupported = SAMPLE.replacen("\"version\": \"1.2\"", "\"version\": \"0.9\"", 1);
    assert!(reason(&unsupported).await.starts_with("it is not a HAR 1.2 or 1.3 file: "));
    assert!(reason("{\"log\": 1}").await.starts_with("it is not a HAR 1.2 or 1.3 file: "));
    let only_assets: String = SAMPLE.lines().take(5).chain(SAMPLE.lines().skip(16)).collect::<Vec<_>>().join("\n");
    let only_assets = only_assets.replacen("}},\n  ]", "}}\n  ]", 1);
    let expected =
        "it records no call to an api: read 1 entries · filtered 1 static, 0 third-party, 0 OPTIONS → 0 requests";
    assert_eq!(reason(&only_assets).await, expected);
}
