//! What a generated test creates first, and what the creators save and clean up.

use indexmap::IndexMap;
use serde_json::{Value, json};
use vc_interfaces::{DraftApi, DraftBefore, DraftCleanup, DraftGroup, ImportError, ImportOptions};

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

/// Users and admin users, each item under a generic `{id}`, as in the mock users spec.
fn directory() -> Value {
    let user = || json_response(&json!({ "properties": { "id": { "type": "integer" } } }));
    let op = |id: &str, status: &str, response: Value| json!({ "operationId": id, "responses": { status: response } });
    let gone = || json!({ "description": "gone" });
    json!({
        "openapi": "3.1.0",
        "info": { "title": "Directory", "version": "1" },
        "paths": {
            "/users": { "post": op("createUser", "201", user()) },
            "/users/{id}": {
                "parameters": [path_param("id")],
                "get": op("getUser", "200", user()),
                "delete": op("deleteUser", "204", gone()),
            },
            "/admin/users": { "post": op("createAdmin", "201", user()) },
            "/admin/users/{id}": { "parameters": [path_param("id")], "get": op("getAdmin", "200", user()) },
        },
    })
}

/// [`directory`], with or without its admin users.
async fn directory_draft(admins: bool) -> Result<DraftGroup, ImportError> {
    let mut doc = directory();
    if !admins && let Some(paths) = doc.get_mut("paths").and_then(Value::as_object_mut) {
        paths.retain(|path, _| !path.starts_with("/admin"));
    }
    let opts = ImportOptions { file_id: "directory".to_owned(), tests: true, auth: true, ..ImportOptions::default() };
    let draft = read_json(&doc, &opts).await?;
    Ok(draft.groups.first().cloned().unwrap_or_default())
}

fn api<'g>(group: &'g DraftGroup, id: &str) -> Option<&'g DraftApi> {
    group.apis.iter().find(|api| api.id == id)
}

#[tokio::test]
async fn a_creator_saves_its_resource_s_id_and_the_test_passes_it_to_the_parameter() -> Result<(), ImportError> {
    let group = directory_draft(false).await?;
    let create = api(&group, "createUser");
    let read = || IndexMap::from([("id".to_owned(), "${response.body.id}".to_owned())]);
    let saves = IndexMap::from([("userId".to_owned(), "${response.body.id}".to_owned())]);
    assert_eq!(create.map(|api| &api.save), Some(&saves));
    let cleanup = Some(DraftCleanup { call: "deleteUser".to_owned(), vars: read() });
    assert_eq!(create.map(|api| api.cleanup.clone()), Some(cleanup));
    let passed = IndexMap::from([("id".to_owned(), json!("${var.userId}"))]);
    assert_eq!(drafted(&group, "getsUser").vars, passed);
    assert_eq!(drafted(&group, "deletesUser").vars, passed);
    assert!(drafted(&group, "createsUser").vars.is_empty());
    assert!(drafted(&group, "rejectsGetUserWithoutAuth").vars.is_empty());
    Ok(())
}

#[tokio::test]
async fn creators_whose_saves_clash_are_numbered_in_route_order() -> Result<(), ImportError> {
    let group = directory_draft(true).await?;
    let order: Vec<&str> = group.apis.iter().map(|api| api.id.as_str()).collect();
    assert_eq!(order, ["createAdmin", "getAdmin", "createUser", "getUser", "deleteUser"]);
    let saved = |id: &str| api(&group, id).map(|api| api.save.keys().cloned().collect::<Vec<_>>());
    assert_eq!(saved("createAdmin"), Some(vec!["userId".to_owned()]));
    assert_eq!(saved("createUser"), Some(vec!["userId2".to_owned()]));
    let passed = |var: &str| IndexMap::from([("id".to_owned(), json!(format!("${{var.{var}}}")))]);
    assert_eq!(drafted(&group, "getsAdmin").vars, passed("userId"));
    assert_eq!(drafted(&group, "getsUser").vars, passed("userId2"));
    Ok(())
}

/// Users with orders, the user under a generic `{id}` and the order under `{orderId}`.
fn nested() -> Value {
    let item = || json_response(&json!({ "properties": { "id": { "type": "integer" } } }));
    let op = |id: &str, status: &str| json!({ "operationId": id, "responses": { status: item() } });
    json!({
        "openapi": "3.1.0",
        "info": { "title": "Nested", "version": "1" },
        "paths": {
            "/users": { "post": op("createUser", "201") },
            "/users/{id}/orders": { "parameters": [path_param("id")], "post": op("createOrder", "201") },
            "/users/{id}/orders/{orderId}": {
                "parameters": [path_param("id"), path_param("orderId")],
                "get": op("getOrder", "200"),
            },
        },
    })
}

#[tokio::test]
async fn a_nested_creator_reads_the_outer_item_through_the_var_the_test_passes() -> Result<(), ImportError> {
    let opts = ImportOptions { file_id: "nested".to_owned(), tests: true, ..ImportOptions::default() };
    let draft = read_json(&nested(), &opts).await?;
    let group = draft.groups.first().cloned().unwrap_or_default();
    let order = drafted(&group, "getsOrder");
    let calls: Vec<&str> = order.before.iter().map(|before| before.call.as_str()).collect();
    assert_eq!(calls, ["nested#createUser", "nested#createOrder"]);
    assert_eq!(order.vars, IndexMap::from([("id".to_owned(), json!("${var.userId}"))]));
    assert_eq!(drafted(&group, "createsOrder").vars, order.vars, "createOrder's path reads `${{var.id}}` too");
    let saved = |id: &str, var: &str| {
        let save = IndexMap::from([(var.to_owned(), "${response.body.id}".to_owned())]);
        api(&group, id).map(|api| &api.save) == Some(&save)
    };
    assert!(saved("createUser", "userId") && saved("createOrder", "orderId"), "{:?}", group.apis);
    Ok(())
}
