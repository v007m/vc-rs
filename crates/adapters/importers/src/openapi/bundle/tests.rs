use serde_json::{Value, json};
use vc_interfaces::{FetchError, ImportError};

use super::bundle;
use crate::openapi::fixture::MapFetcher;

async fn bundled(fetcher: &MapFetcher, root: Value) -> Result<Value, ImportError> {
    bundle(fetcher, "specs/api.json", root).await
}

#[tokio::test]
async fn local_references_stay() -> Result<(), ImportError> {
    let root = json!({ "a": { "$ref": "#/components/schemas/User" } });
    assert_eq!(bundled(&MapFetcher::default(), root.clone()).await?, root);
    Ok(())
}

#[tokio::test]
async fn external_references_are_inlined_with_their_own_references() -> Result<(), ImportError> {
    let common = json!({
        "User": { "type": "object", "properties": { "role": { "$ref": "#/Role" }, "team": { "$ref": "teams/team.json" } } },
        "Role": { "type": "string" },
    });
    let team =
        json!({ "type": "object", "properties": { "lead": { "$ref": "../api.json#/components/schemas/Lead" } } });
    let fetcher = MapFetcher::with(&[("specs/common.json", common), ("specs/teams/team.json", team)]);
    let root = json!({ "user": { "$ref": "common.json#/User" } });
    let expected = json!({ "user": { "type": "object", "properties": {
        "role": { "type": "string" },
        "team": { "type": "object", "properties": { "lead": { "$ref": "#/components/schemas/Lead" } } },
    }}});
    assert_eq!(bundled(&fetcher, root).await?, expected);
    Ok(())
}

#[tokio::test]
async fn a_reference_into_another_document_is_percent_decoded_before_it_is_looked_up() -> Result<(), ImportError> {
    let items =
        json!({ "paths": { "/items/{id}": { "get": { "responses": { "200": { "description": "an item" } } } } } });
    let fetcher = MapFetcher::with(&[("specs/items.json", items)]);
    let root = json!({ "item": { "$ref": "items.json#/paths/~1items~1%7Bid%7D/get/responses/200" } });
    assert_eq!(bundled(&fetcher, root).await?, json!({ "item": { "description": "an item" } }));
    Ok(())
}

#[tokio::test]
async fn a_cycle_is_an_error_naming_the_chain() {
    let node = json!({ "Node": { "properties": { "next": { "$ref": "#/Node" } } } });
    let fetcher = MapFetcher::with(&[("specs/node.json", node)]);
    let result = bundled(&fetcher, json!({ "n": { "$ref": "node.json#/Node" } })).await;
    let chain = vec!["specs/node.json#/Node".to_owned(), "specs/node.json#/Node".to_owned()];
    assert_eq!(result, Err(ImportError::Cycle { chain }));
}

#[tokio::test]
async fn documents_that_refer_to_each_other_are_fetched_once_each() -> Result<(), ImportError> {
    let a = json!({ "A": { "type": "string" }, "toB": { "$ref": "b.json#/B" } });
    let b = json!({ "B": { "type": "integer" }, "toA": { "$ref": "a.json#/A" } });
    let fetcher = MapFetcher::with(&[("specs/a.json", a), ("specs/b.json", b)]);
    let root = json!({ "x": { "$ref": "a.json#/toB" }, "y": { "$ref": "b.json#/toA" } });
    assert_eq!(bundled(&fetcher, root).await?, json!({ "x": { "type": "integer" }, "y": { "type": "string" } }));
    Ok(())
}

#[tokio::test]
async fn references_that_name_nothing_or_leave_the_project_are_refused() {
    let fetcher = MapFetcher::with(&[("specs/a.json", json!({}))]);
    let missing = bundled(&fetcher, json!({ "x": { "$ref": "a.json#/Nope" } })).await;
    assert!(matches!(missing, Err(ImportError::Invalid { reason, .. }) if reason.contains("a.json#/Nope")));
    let outside = bundled(&fetcher, json!({ "x": { "$ref": "../../etc/passwd" } })).await;
    let refused = FetchError::NotAllowed { location: "../../etc/passwd".to_owned() };
    assert_eq!(outside, Err(ImportError::Fetch(refused)));
    let absent = bundled(&fetcher, json!({ "x": { "$ref": "gone.json" } })).await;
    assert!(
        matches!(absent, Err(ImportError::Fetch(FetchError::Failed { location, .. })) if location == "specs/gone.json")
    );
}

#[tokio::test]
async fn a_spec_of_too_many_documents_is_refused() {
    let documents: Vec<(String, Value)> = (0..70)
        .map(|n| (format!("specs/d{n}.json"), json!({ "next": { "$ref": format!("d{}.json", n + 1) } })))
        .collect();
    let documents: Vec<(&str, Value)> = documents.iter().map(|(at, doc)| (at.as_str(), doc.clone())).collect();
    let fetcher = MapFetcher::with(&documents);
    let result = bundled(&fetcher, json!({ "x": { "$ref": "d0.json" } })).await;
    assert!(matches!(result, Err(ImportError::Invalid { reason, .. }) if reason.contains("more than 64")));
}
