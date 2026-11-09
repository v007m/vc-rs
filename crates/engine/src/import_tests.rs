use std::sync::Arc;

use vc_testkit::{FakeTransport, FakeVcs, status};

use super::{ImportRequest, options};
use crate::sample::builder;
use crate::{Engine, EngineError};

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

#[tokio::test]
async fn what_cannot_be_imported_is_refused_with_help() {
    let help = |result: Result<_, EngineError>| result.err().and_then(|error| error.help());
    let missing = engine().import(&request("openapi", "https://specs.test/gone.json")).await;
    assert_eq!(help(missing).as_deref(), Some("give a URL, or a path inside the project"));
    let unknown = engine().import(&request("wsdl", "https://specs.test/petstore.json")).await;
    assert_eq!(help(unknown).as_deref(), Some("the importers are curl, har, openapi"));
    let users =
        ImportRequest { file_id: Some("users".to_owned()), ..request("openapi", "https://specs.test/petstore.json") };
    assert_eq!(help(engine().import(&users).await).as_deref(), Some("`vc sync users` merges the spec into it"));
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
