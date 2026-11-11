use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use vc_interfaces::SourceProvider;
use vc_testkit::{FakeTransport, FakeVcs, MemoryEnvironment, MemorySources, status};

use super::{ImportRequest, options};
use crate::sample::builder;
use crate::{Changes, Engine, EngineBuilder, EngineError, FileChange};

const PETSTORE: &str = include_str!("../../adapters/importers/fixtures/petstore-3.1.json");
const SHOP: &str = include_str!("../../adapters/importers/fixtures/shop.curl");

fn engine() -> Engine {
    let served = FakeTransport::new(|request, _| {
        let served = [("/petstore.json", PETSTORE), ("/shop.curl", SHOP)];
        let found = served.iter().find(|(path, _)| request.uri.path() == *path).map(|(_, body)| *body);
        let mut response = status(if found.is_some() { 200 } else { 404 });
        response.body = found.unwrap_or_default().into();
        Ok(response)
    });
    builder(FakeVcs::default()).transport(Arc::new(served)).build().unwrap()
}

fn request(kind: &str, source: &str) -> ImportRequest {
    ImportRequest {
        kind: kind.to_owned(),
        source: source.to_owned(),
        tests: true,
        negative: true,
        auth: true,
        ..ImportRequest::default()
    }
}

#[tokio::test]
async fn an_imported_spec_passes_the_compile_guard_with_its_snapshot() {
    let changes = engine().import(&request("openapi", "https://specs.test/petstore.json")).await.unwrap();
    let paths: Vec<_> = changes.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "services/petstore/openapi.test.yaml",
            "services/petstore/petstore.api.yaml",
            "vibranium.config.yaml",
            "openapi/petstore.yaml"
        ]
    );
    assert_eq!(changes.titles, ["Import 5 apis from https://specs.test/petstore.json"]);
    let api = changes.files.get(1).map(|file| file.after.as_str()).unwrap_or_default();
    assert!(
        api.contains("baseUrl: ${var.petstoreUrl}/v1\nsource:\n  openapi: https://specs.test/petstore.json\n"),
        "{api}"
    );
}

#[tokio::test]
async fn a_spec_given_as_text_is_read_without_fetching_and_named_by_its_source() {
    let given = ImportRequest { text: Some(PETSTORE.to_owned()), ..request("openapi", "pasted/petstore.json") };
    let changes = engine().import(&given).await.unwrap();
    let paths: Vec<_> = changes.files.iter().map(|file| file.path.as_str()).collect();
    assert!(paths.contains(&"services/petstore/petstore.api.yaml"), "{paths:?}");
    assert_eq!(changes.titles, ["Import 5 apis from pasted/petstore.json"]);
}

#[tokio::test]
async fn a_pasted_spec_may_not_fetch_a_remote_ref() {
    let fetched = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&fetched);
    let served = FakeTransport::new(move |_, _| {
        counted.fetch_add(1, Ordering::SeqCst);
        let mut response = status(200);
        response.body = r#"{"type": "object"}"#.into();
        Ok(response)
    });
    let engine = builder(FakeVcs::default()).transport(Arc::new(served)).build().unwrap();
    let schema = r#"{"$ref": "http://internal.test/schemas.json#/User"}"#;
    let spec = format!(
        r#"{{"openapi": "3.1.0", "info": {{"title": "Users", "version": "1"}}, "paths": {{"/users": {{"get": {{
        "operationId": "listUsers", "responses": {{"200": {{"description": "ok",
        "content": {{"application/json": {{"schema": {schema}}}}}}}}}}}}}}}}}"#
    );
    let given = ImportRequest { text: Some(spec), ..request("openapi", "pasted") };
    let refused = engine.import(&given).await;
    assert!(
        matches!(&refused, Err(EngineError::Edit { reason, .. }) if reason.contains("may not fetch URLs")),
        "{refused:?}"
    );
    assert_eq!(fetched.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn imported_curl_commands_compile_and_write_no_credential() {
    let changes = engine().import(&request("curl", "https://specs.test/shop.curl")).await.unwrap();
    let paths: Vec<_> = changes.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(paths, ["services/shop/shop.api.yaml", "vibranium.config.yaml"]);
    let api = changes.files.first().map(|file| file.after.as_str()).unwrap_or_default();
    assert!(api.contains("baseUrl: ${var.shopUrl}\n"), "{api}");
    assert!(!api.contains("source:"), "a curl file has no spec to sync: {api}");
    assert!(api.contains("path: /v1/orders/${var.orderId}/items\n"), "{api}");
    let written: String = changes.files.iter().map(|file| file.after.as_str()).collect();
    let credentials = ["eyJhbGciOi", "abc123", "s3cret", "t0k3n", "hunter2"];
    assert!(credentials.iter().all(|value| !written.contains(value)), "{written}");
}

#[test]
fn the_importers_are_listed_by_name() {
    assert_eq!(Engine::importers(), ["bruno", "curl", "har", "http", "openapi", "postman"]);
}

#[tokio::test]
async fn what_cannot_be_imported_is_refused_with_help() {
    let help = |result: Result<_, EngineError>| result.err().and_then(|error| error.help());
    let missing = engine().import(&request("openapi", "https://specs.test/gone.json")).await;
    assert_eq!(help(missing).as_deref(), Some("give a URL, or a path inside the project"));
    let unknown = engine().import(&request("wsdl", "https://specs.test/petstore.json")).await;
    assert_eq!(help(unknown).as_deref(), Some("the importers are bruno, curl, har, http, openapi, postman"));
    let folder = engine().import(&request("bruno", "https://specs.test/shop")).await;
    let export = "give the Bruno collection's folder (the one with its bruno.json) or the collection exported as JSON";
    assert_eq!(help(folder).as_deref(), Some(export));
    let users =
        ImportRequest { file_id: Some("users".to_owned()), ..request("openapi", "https://specs.test/petstore.json") };
    assert_eq!(help(engine().import(&users).await).as_deref(), Some("`vc sync users` merges the spec into it"));
}

#[tokio::test]
async fn a_spec_kept_at_its_snapshot_path_is_refused_so_it_is_never_overwritten() {
    let files: [(&str, &[u8]); 2] = [
        ("vibranium.config.yaml", b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n"),
        ("openapi/coll.yaml", PETSTORE.as_bytes()),
    ];
    let sources: Arc<dyn SourceProvider> = Arc::new(MemorySources::new(&files));
    let engine = builder(FakeVcs::default()).sources(sources).build().unwrap();
    let kept = ImportRequest { file_id: Some("coll".to_owned()), ..request("openapi", "openapi/coll.yaml") };
    let Err(EngineError::Edit { reason, help }) = engine.import(&kept).await else {
        panic!("importing the snapshot path over itself is refused")
    };
    assert_eq!(reason, "openapi/coll.yaml is where `vc import` keeps the snapshot `vc sync coll` merges against");
    let moved = "move the spec out of `openapi/` (to `specs/coll.yaml`, say) and import it from there";
    assert_eq!(help.as_deref(), Some(moved));
    let named = engine.import(&request("openapi", "./openapi/coll.yaml")).await;
    assert!(matches!(&named, Err(EngineError::Edit { reason, .. }) if reason.contains("snapshot")), "{named:?}");
    let other = ImportRequest { file_id: Some("shop".to_owned()), ..request("openapi", "openapi/coll.yaml") };
    let changes = engine.import(&other).await.unwrap();
    assert!(
        changes.files.iter().any(|file| file.path == "openapi/shop.yaml"),
        "a different ID writes its own snapshot"
    );
}

#[tokio::test]
async fn a_bruno_collection_folder_in_the_project_is_listed_and_read_file_by_file() {
    let ping = b"meta {\n  name: Ping\n  seq: 1\n}\n\nget {\n  url: https://api.shop.test/v1/ping\n  body: none\n}\n";
    let files: [(&str, &[u8]); 4] = [
        ("vibranium.config.yaml", b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n"),
        ("bruno/shop/bruno.json", b"{\"version\": \"1\", \"name\": \"Shop\", \"type\": \"collection\"}"),
        ("bruno/shop/ping.bru", ping),
        ("bruno/loose/ping.bru", ping),
    ];
    let sources: Arc<dyn SourceProvider> = Arc::new(MemorySources::new(&files));
    let engine = builder(FakeVcs::default()).sources(sources).build().unwrap();
    let changes = engine.import(&request("bruno", "bruno/shop")).await.unwrap();
    let written: String = changes.files.iter().map(|file| file.after.as_str()).collect();
    assert!(written.contains("path: /v1/ping\n") || written.contains("path: /ping\n"), "{written}");
    let loose = engine.import(&request("bruno", "bruno/loose")).await.err().map(|error| error.to_string());
    assert!(loose.as_deref().unwrap_or_default().contains("the folder has no bruno.json"), "{loose:?}");
}

#[test]
fn the_file_id_defaults_to_the_spec_name_up_to_its_first_dot() {
    let id = |source: &str| options(&ImportRequest { source: source.to_owned(), ..ImportRequest::default() }).file_id;
    assert_eq!(id("specs/Users.v2.yaml"), "users");
    assert_eq!(id("https://a.test/api/order-items.json?x=1"), "order-items");
    assert_eq!(id("https://a.test/api/orders?format=json"), "orders");
    let given = ImportRequest {
        file_id: Some("pay.refunds".to_owned()),
        only: vec!["a".to_owned()],
        auth: true,
        ..ImportRequest::default()
    };
    let opts = options(&given);
    assert_eq!(
        (opts.file_id.as_str(), opts.only, opts.auth, opts.tests),
        ("pay.refunds", vec!["a".to_owned()], true, false)
    );
}

const SHOP_HAR: &str = include_str!("../../adapters/importers/fixtures/shop.har");

/// What the recording holds that no output may: a bearer token, a session cookie, an api key, a
/// nonce, a card token, a password and a token a response gave back.
const RECORDED: [&str; 7] = [
    "eyJhbGciOiJIUzI1NiJ9",
    "s3ss10nC00k1e",
    "pk_9Rt2Lm8Qx4Vz7Nb3Kc6Wd1Hy",
    "Xk9pQ2mZ7vR4tL8wB3nY6cH1",
    "tok_visa",
    "hunter2",
    "Zq8Wv3Lp5Rt7Ny2Kb4Hd6Fj9",
];

/// A shop project whose `shop` file points at the recording's api host, with an api the
/// recording calls under another var name, and a schema that declares one secret.
fn shop_engine() -> Engine {
    shop_engine_with(b"SHOP_TOKEN=\n")
}

/// [`shop_engine`] with `schema` as its `.env.schema`.
fn shop_engine_with(schema: &'static [u8]) -> Engine {
    let files: [(&str, &[u8]); 3] = [
        (
            "vibranium.config.yaml",
            b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {shopUrl: 'https://api.shop.test/v1'}\nauth:\n  - id: shopToken\n    type: bearer\n    token: ${secret.SHOP_TOKEN}\n",
        ),
        (
            "services/shop/shop.api.yaml",
            b"baseUrl: ${var.shopUrl}\nauth: shopToken\napis:\n  - id: getCart\n    path: /carts/${var.id}\n",
        ),
        (".env.schema", schema),
    ];
    let served = FakeTransport::new(|_, _| {
        let mut response = status(200);
        response.body = SHOP_HAR.into();
        Ok(response)
    });
    let sources: Arc<dyn SourceProvider> = Arc::new(MemorySources::new(&files));
    builder(FakeVcs::default()).sources(sources).transport(Arc::new(served)).build().unwrap()
}

async fn shop_import() -> Changes {
    let request = ImportRequest {
        flow: Some("shop.checkout#replaysCheckout".to_owned()),
        ..request("har", "https://recordings.test/shop.har")
    };
    shop_engine().import(&request).await.unwrap()
}

fn after<'c>(changes: &'c Changes, path: &str) -> &'c str {
    changes.files.iter().find(|file| file.path == path).map(|file| file.after.as_str()).unwrap_or_default()
}

#[tokio::test]
async fn a_recording_merges_into_the_file_its_host_renders_to_and_declares_its_secrets() {
    let changes = shop_import().await;
    let paths: Vec<_> = changes.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            ".env.schema",
            "services/pay/pay.api.yaml",
            "services/shop/checkout.test.yaml",
            "services/shop/shop.api.yaml",
            "vibranium.config.yaml"
        ]
    );
    let shop = after(&changes, "services/shop/shop.api.yaml");
    assert_eq!(shop.matches("id: getCart").count(), 1, "{shop}");
    assert!(shop.contains("  - id: addItem\n") && shop.contains("    path: /carts/${var.cartId}/items\n"), "{shop}");
    assert!(!shop.contains("/v1/"), "the base URL's path is taken off: {shop}");
    assert!(after(&changes, "vibranium.config.yaml").contains("payUrl: https://pay.shop.test"));
    let schema = after(&changes, ".env.schema");
    assert_eq!(schema, "SHOP_TOKEN=\nPAY_TOKEN=\nPAY_NONCE=\nPAY_API_KEY=\nSHOP_PASSWORD=\n");
    let flow = after(&changes, "services/shop/checkout.test.yaml");
    assert!(flow.contains("      - call: shop#getCart\n        vars:\n          id: ${var.cartId}\n"), "{flow}");
    assert!(flow.contains("      - call: pay#createPayment\n"), "{flow}");
    assert_eq!(
        changes.titles,
        [
            "Import 6 apis from https://recordings.test/shop.har",
            "read 12 entries · filtered 3 static, 1 third-party, 1 OPTIONS → 7 requests",
            "  api.shop.test → shop (existing)  1 matched, 4 new · auth shopToken",
            "  pay.shop.test → pay (new)  1 new · no auth yet: `vc new auth payAuth --type bearer`",
            "secrets  header authorization, cookie dropped → each file's auth · body field token → \
             ${secret.PAY_TOKEN} · body field nonce → ${secret.PAY_NONCE} · query api_key → ${secret.PAY_API_KEY} \
             · body field password → ${secret.SHOP_PASSWORD}",
            ".env.schema  declares PAY_TOKEN, PAY_NONCE, PAY_API_KEY, SHOP_PASSWORD",
            "flow shop.checkout#replaysCheckout (7 steps)",
            "POST /v1/carts/${var.cartId}/items: recorded 2 times, merged into one api",
        ]
    );
}

#[tokio::test]
async fn a_recorded_secret_never_reaches_a_file_a_title_or_a_diff() {
    let changes = shop_import().await;
    let diffs: Vec<_> = changes.files.iter().map(FileChange::diff).collect();
    let output = format!("{}\n{}\n{changes:?}", changes.titles.join("\n"), diffs.join("\n"));
    let leaked: Vec<_> = RECORDED.iter().filter(|value| output.contains(*value)).collect();
    assert!(leaked.is_empty(), "{leaked:?} in {output}");
}

#[test]
fn a_typed_spec_path_is_read_from_the_project_and_a_url_as_given() {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::write(root.join("vibranium.config.yaml"), "syntaxVersion: 2\n").unwrap();
    fs::create_dir_all(root.join("specs")).unwrap();
    fs::write(root.join("specs/shop.yaml"), "openapi: 3.1.0\n").unwrap();
    let engine = EngineBuilder::new(root.clone())
        .vcs(Arc::new(FakeVcs::default()))
        .environment(Arc::new(MemoryEnvironment::new(&[])))
        .build()
        .unwrap();
    assert_eq!(engine.spec_location(&root, "specs/shop.yaml").unwrap(), "specs/shop.yaml");
    assert_eq!(engine.spec_location(&root.join("specs"), "./shop.yaml").unwrap(), "specs/shop.yaml");
    assert_eq!(engine.spec_location(&root, "https://specs.test/shop.yaml").unwrap(), "https://specs.test/shop.yaml");
    let Err(EngineError::Edit { reason, help }) = engine.spec_location(&root, "../shop.yaml") else {
        panic!("a path outside the project is refused")
    };
    assert!(reason.starts_with("../shop.yaml: not a file or folder in the project at"), "{reason}");
    assert_eq!(help.as_deref(), Some("give a URL, or a path inside the project"));
}

#[tokio::test]
async fn a_recording_stops_on_a_secrets_schema_it_cannot_read() {
    let request = ImportRequest {
        flow: Some("shop.checkout#replaysCheckout".to_owned()),
        ..request("har", "https://recordings.test/shop.har")
    };
    let error = shop_engine_with(b"SHOP_TOKEN=\xff\n").import(&request).await.unwrap_err();
    let EngineError::Invalid(report) = error else { panic!("{error:?}") };
    let codes: Vec<_> = report.diagnostics.iter().map(|found| found.code.to_string()).collect();
    assert_eq!(codes, ["V1012"]);
    assert_eq!(report.files.len(), 1, "the one file is the schema the diagnostic points into");
}
