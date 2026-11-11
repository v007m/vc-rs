//! What a generated test creates first, and what the creators save and clean up.

use serde_json::{Value, json};
use vc_interfaces::{DraftBefore, DraftGroup, ImportError, ImportOptions};

use super::{drafted, json_response, path_param, read_json};

/// Users with orders: a `DELETE` on a user's orders and one on the user.
fn accounts() -> Value {
    let user = json_response(&json!({ "properties": { "id": { "type": "integer" } } }));
    let deleted = |id: &str| json!({ "operationId": id, "responses": { "204": { "description": "gone" } } });
    json!({
        "openapi": "3.1.0",
        "info": { "title": "Accounts", "version": "1" },
        "paths": {
            "/users": { "post": { "operationId": "createUser", "responses": { "201": user } } },
            "/users/{userId}": { "parameters": [path_param("userId")], "delete": deleted("deleteUser") },
            "/users/{userId}/orders": { "parameters": [path_param("userId")], "delete": deleted("clearOrders") },
        },
    })
}

async fn accounts_draft() -> Result<DraftGroup, ImportError> {
    let opts = ImportOptions { file_id: "accounts".to_owned(), tests: true, ..ImportOptions::default() };
    let draft = read_json(&accounts(), &opts).await?;
    Ok(draft.groups.first().cloned().unwrap_or_default())
}

#[tokio::test]
async fn only_a_delete_of_the_created_item_keeps_it() -> Result<(), ImportError> {
    let group = accounts_draft().await?;
    let before = |keep| vec![DraftBefore { call: "accounts#createUser".to_owned(), keep }];
    assert_eq!(drafted(&group, "deletesUser").before, before(true));
    assert_eq!(drafted(&group, "clearsOrders").before, before(false));
    Ok(())
}
