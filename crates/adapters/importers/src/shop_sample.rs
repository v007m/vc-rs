//! What the Postman and Bruno Shop samples draft: the two hold the same requests, so they
//! import into the same api files and the same `--flow` test.

use indexmap::IndexMap;
use serde_json::{Value, json};
use vc_interfaces::{DraftApi, DraftFlow, DraftStep, ImportDraft, SecretFinding};
use vc_model::Method;

pub fn map(pairs: &[(&str, Value)]) -> IndexMap<String, Value> {
    pairs.iter().map(|(name, value)| ((*name).to_owned(), value.clone())).collect()
}

pub fn query(pairs: &[(&str, &str)]) -> IndexMap<String, String> {
    pairs.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())).collect()
}

/// The request's name the samples give an api with this ID: `createCart` is `Create cart`.
fn sentence(id: &str) -> String {
    let words =
        id.chars().flat_map(|c| if c.is_ascii_uppercase() { vec![' ', c.to_ascii_lowercase()] } else { vec![c] });
    let text: String = words.collect();
    let mut chars = text.chars();
    chars.next().map(|first| first.to_ascii_uppercase().to_string() + chars.as_str()).unwrap_or_default()
}

fn api(id: &str, method: Method, path: &str) -> DraftApi {
    DraftApi { id: id.to_owned(), name: Some(sentence(id)), method, path: path.to_owned(), ..DraftApi::default() }
}

fn json_api(api: DraftApi, vars: &[(&str, Value)], body: Value) -> DraftApi {
    DraftApi { vars: map(vars), body: Some(body), content_type: Some("application/json".to_owned()), ..api }
}

fn shop_apis() -> Vec<DraftApi> {
    let cart = ("cartId", json!(""));
    let create = json_api(
        api("createCart", Method::Post, "/v1/carts"),
        &[("currency", json!("EUR"))],
        json!({"currency": "${var.currency}"}),
    );
    let add = json_api(
        api("addItem", Method::Post, "/v1/carts/${var.cartId}/items"),
        &[cart.clone(), ("quantity", json!("2")), ("sku", json!("sku_red42"))],
        json!({"sku": "${var.sku}", "quantity": "${var.quantity}"}),
    );
    let get = DraftApi { vars: map(&[cart]), ..api("getCart", Method::Get, "/v1/carts/${var.cartId}") };
    let health = api("health", Method::Get, "/v1/health");
    let search = DraftApi {
        query: query(&[("q", "red"), ("access_token", "${secret.SHOP_ACCESS_TOKEN}")]),
        ..api("search", Method::Get, "/v1/products")
    };
    let login = json_api(
        api("login", Method::Post, "/v1/login"),
        &[("username", json!("ann")), ("password", json!("${secret.SHOP_PASSWORD}"))],
        json!({"username": "${var.username}", "password": "${var.password}"}),
    );
    vec![create, add, get, health, search, login]
}

/// The payment apis, `pay` sending the form `form`.
fn pay_apis(form: Value) -> Vec<DraftApi> {
    let key = ("api_key", "${secret.PAY_API_KEY}");
    let pay = DraftApi {
        vars: map(&[("currency", json!("EUR")), ("cartId", json!(""))]),
        query: query(&[("currency", "${var.currency}"), key]),
        body: Some(form),
        content_type: Some("application/x-www-form-urlencoded".to_owned()),
        ..api("pay", Method::Post, "/payments")
    };
    let upload = DraftApi {
        vars: map(&[("paymentId", json!("pay_3Hs8Lw"))]),
        query: query(&[key]),
        ..api("uploadReceipt", Method::Post, "/payments/${var.paymentId}/receipt")
    };
    vec![pay, upload]
}

/// Asserts that `draft` holds the shop and payment api files, `pay` sending the form `form`.
pub fn assert_files(draft: &ImportDraft, form: Value) {
    let files: Vec<_> = draft.groups.iter().map(|group| (group.file_id.as_str(), group.base_url.as_deref())).collect();
    assert_eq!(files, [("shop", Some("${var.shopUrl}")), ("pay", Some("${var.payUrl}"))]);
    let vars: Vec<_> = draft.groups.iter().map(|group| group.config_vars.clone()).collect();
    let origins = [("shopUrl", "https://api.shop.test"), ("payUrl", "https://pay.shop.test")];
    assert_eq!(vars, origins.map(|(var, origin)| map(&[(var, json!(origin))])));
    let apis: Vec<_> = draft.groups.iter().map(|group| group.apis.clone()).collect();
    assert_eq!(apis, [shop_apis(), pay_apis(form)]);
    assert_eq!(draft.summary.as_deref(), Some("collection Shop · 8 requests in 2 folders"));
}

fn step(call: &str, vars: &[(&str, Value)], save: &[(&str, &str)]) -> DraftStep {
    let save = save.iter().map(|(name, template)| ((*name).to_owned(), (*template).to_owned())).collect();
    DraftStep { call: call.to_owned(), vars: map(vars), save }
}

/// `shop.checkout#buysACart`, calling each request in order and sending on the cart and order.
pub fn checkout() -> DraftFlow {
    let cart = || ("cartId", json!("${var.cartId}"));
    let steps = vec![
        step("shop#createCart", &[], &[("cartId", "${response.body.id}")]),
        step("shop#addItem", &[cart()], &[]),
        step("shop#getCart", &[cart()], &[]),
        step("pay#pay", &[cart()], &[("orderId", "${response.body.order.id}")]),
        step("pay#uploadReceipt", &[], &[]),
        step("shop#health", &[], &[]),
        step("shop#search", &[], &[]),
        step("shop#login", &[], &[]),
    ];
    DraftFlow { file_id: "shop.checkout".to_owned(), test_id: "buysACart".to_owned(), steps }
}

/// The secrets both samples hold, and `more`, by name as a draft lists them.
pub fn secrets(more: &[(&str, &str)]) -> Vec<SecretFinding> {
    let shared = [
        ("PAY_API_KEY", "POST /payments: query api_key"),
        ("PAY_NONCE", "POST /payments: body field nonce"),
        ("SHOP_ACCESS_TOKEN", "GET /v1/products: query access_token"),
        ("SHOP_PASSWORD", "POST /v1/login: body field password"),
    ];
    let found = shared
        .iter()
        .chain(more)
        .map(|(name, at)| SecretFinding { name: (*name).to_owned(), found_at: (*at).to_owned() });
    let mut found: Vec<_> = found.collect();
    found.sort_by(|one, other| one.name.cmp(&other.name));
    found
}
