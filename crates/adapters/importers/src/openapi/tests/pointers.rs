//! The `#/pointer` a test's schema names: escaped for a URI, and found again in the snapshot.

use percent_encoding::percent_decode_str;
use serde_json::{Value, json};
use vc_interfaces::{ImportDraft, ImportError, ImportOptions};

use super::{drafted, json_response, path_param, read_json};

/// An item's schema inline under `/items/{id}`, read again through a percent-encoded `$ref`,
/// and a path with brackets.
fn catalog() -> Value {
    let item = json!({ "type": "object", "properties": { "sku": { "type": "string" } } });
    let found = json!({ "type": "array", "items": { "type": "string" } });
    let reused = json!({ "$ref": "#/paths/~1items~1%7Bid%7D/get/responses/200" });
    json!({
        "openapi": "3.1.0",
        "info": { "title": "Catalog", "version": "1" },
        "paths": {
            "/items/{id}": {
                "parameters": [path_param("id")],
                "get": { "operationId": "getItem", "responses": { "200": json_response(&item) } },
            },
            "/items/{id}/copy": {
                "parameters": [path_param("id")],
                "get": { "operationId": "copyItem", "responses": { "200": reused } },
            },
            "/search[all]": { "get": { "operationId": "search", "responses": { "200": json_response(&found) } } },
        },
    })
}

async fn catalog_draft() -> Result<ImportDraft, ImportError> {
    let opts = ImportOptions { file_id: "catalog".to_owned(), tests: true, ..ImportOptions::default() };
    read_json(&catalog(), &opts).await
}

#[tokio::test]
async fn a_schema_pointer_is_escaped_for_a_uri_and_finds_the_schema_in_the_snapshot() -> Result<(), ImportError> {
    let draft = catalog_draft().await?;
    let group = draft.groups.first().cloned().unwrap_or_default();
    let snapshot = draft.snapshot.unwrap_or_default();
    let item = "/paths/~1items~1%7Bid%7D/get/responses/200/content/application~1json/schema";
    let search = "/paths/~1search%5Ball%5D/get/responses/200/content/application~1json/schema";
    let cases = [("getsItem", item, "object"), ("copiesItem", item, "object"), ("searches", search, "array")];
    for (id, expected, ty) in cases {
        let pointer = drafted(&group, id).schema.unwrap_or_default();
        assert_eq!(pointer, expected, "{id}");
        let decoded = percent_decode_str(&pointer).decode_utf8_lossy();
        let schema = snapshot.pointer(&decoded).and_then(|schema| schema.get("type"));
        assert_eq!(schema, Some(&json!(ty)), "{id}");
    }
    Ok(())
}
