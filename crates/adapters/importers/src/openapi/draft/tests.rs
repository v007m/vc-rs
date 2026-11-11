use serde_json::{Value, json};
use vc_interfaces::{DraftGroup, ImportNote, ImportOptions};

use super::{Reader, templated};

fn spec(paths: &Value, servers: &Value) -> Value {
    json!({ "openapi": "3.1.0", "info": { "title": "", "version": "1" }, "servers": servers, "paths": paths })
}

fn group(doc: &Value, source: &str) -> Result<(DraftGroup, Vec<ImportNote>), serde_json::Error> {
    let spec: oas3::Spec = serde_json::from_value(doc.clone())?;
    let opts = ImportOptions { file_id: "users.admin".to_owned(), ..ImportOptions::default() };
    let reader = Reader { spec: &spec, doc, opts: &opts };
    let (routes, _) = reader.routes();
    Ok(reader.group(source, &routes))
}

/// The IDs of the apis drafted from `doc` with these `--include` and `--only` values.
fn drafted(doc: &Value, (include, only): (&[&str], &[&str])) -> Result<Vec<String>, serde_json::Error> {
    let spec: oas3::Spec = serde_json::from_value(doc.clone())?;
    let owned = |ids: &[&str]| -> Vec<String> { ids.iter().map(|&id| id.to_owned()).collect() };
    let opts = ImportOptions { include: owned(include), only: owned(only), ..ImportOptions::default() };
    let reader = Reader { spec: &spec, doc, opts: &opts };
    let (routes, _) = reader.routes();
    Ok(reader.group("spec.yaml", &routes).0.apis.into_iter().map(|api| api.id).collect())
}

#[test]
fn include_keeps_a_path_and_what_is_under_it_by_whole_segments() -> Result<(), serde_json::Error> {
    let get = |id: &str| json!({ "get": { "operationId": id, "responses": {} } });
    let doc = spec(&json!({ "/user": get("me"), "/user/{id}": get("one"), "/users": get("all") }), &json!([]));
    assert_eq!(drafted(&doc, (&["/user"], &[]))?, ["me", "one"]);
    assert_eq!(drafted(&doc, (&["/user/"], &[]))?, ["one"]);
    assert_eq!(drafted(&doc, (&["/users"], &[]))?, ["all"]);
    Ok(())
}

#[test]
fn clashing_ids_are_numbered_on_and_only_picks_a_renamed_one() -> Result<(), serde_json::Error> {
    let get = json!({ "get": { "operationId": "fetch", "responses": {} } });
    let doc = spec(&json!({ "/a": get, "/b": get, "/c": get }), &json!([]));
    assert_eq!(drafted(&doc, (&[], &[]))?, ["fetch", "fetch2", "fetch3"]);
    assert_eq!(drafted(&doc, (&[], &["fetch3"]))?, ["fetch3"]);
    assert_eq!(drafted(&doc, (&["/b"], &[]))?, ["fetch2"]);
    assert_eq!(drafted(&doc, (&[], &["fetch"]))?, ["fetch", "fetch2", "fetch3"]);
    Ok(())
}

#[test]
fn path_templates_read_camel_case_vars() {
    assert_eq!(templated("/users/{user_id}/roles/{role}"), "/users/${var.userId}/roles/${var.role}");
    assert_eq!(templated("/broken/{open"), "/broken/{open");
}

#[test]
fn a_server_that_is_only_a_path_takes_the_origin_the_spec_came_from() -> Result<(), serde_json::Error> {
    let doc = spec(&json!({}), &json!([{ "url": "/api/" }]));
    let (fetched, _) = group(&doc, "https://specs.test/users.yaml")?;
    assert_eq!(fetched.base_url.as_deref(), Some("${var.usersAdminUrl}/api"));
    assert_eq!(fetched.config_vars.get("usersAdminUrl"), Some(&json!("https://specs.test")));
    let (local, _) = group(&spec(&json!({}), &json!([])), "specs/users.yaml")?;
    assert_eq!(local.base_url.as_deref(), Some("${var.usersAdminUrl}"));
    assert_eq!(local.config_vars.get("usersAdminUrl"), Some(&json!("http://localhost")));
    assert_eq!(local.name, None);
    Ok(())
}

#[test]
fn ids_that_clash_are_told_apart_and_operation_parameters_win() -> Result<(), serde_json::Error> {
    let shared = json!([{ "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }]);
    let own = json!([
        { "name": "id", "in": "path", "required": true, "schema": { "type": "string", "format": "uuid" } },
        { "name": "page_size", "in": "query", "required": true, "schema": { "type": "integer", "minimum": 5, "maximum": 50 } },
        { "name": "debug", "in": "header", "schema": { "type": "boolean", "default": true } },
        { "$ref": "#/components/parameters/Missing" },
    ]);
    let paths = json!({
        "/a/{id}": { "parameters": shared, "get": { "operationId": "fetch", "parameters": own, "responses": {} } },
        "/b": { "get": { "operationId": "fetch", "responses": {} } },
    });
    let (drafted, notes) = group(&spec(&paths, &json!([{ "url": "http://x.test" }])), "spec.yaml")?;
    let ids: Vec<_> = drafted.apis.iter().map(|api| api.id.as_str()).collect();
    assert_eq!(ids, ["fetch", "fetch2"]);
    let first = drafted.apis.first().cloned().unwrap_or_default();
    assert_eq!(first.vars.get("id"), Some(&json!("${fake.string.uuid}")));
    assert_eq!(first.vars.get("pageSize"), Some(&json!("${fake.number.int(5, 50)}")));
    assert_eq!(first.query.get("page_size").map(String::as_str), Some("${var.pageSize}"));
    assert!(!first.vars.contains_key("debug"));
    assert_eq!(notes.iter().map(|note| note.at.as_str()).collect::<Vec<_>>(), ["GET /a/{id}"]);
    Ok(())
}

#[test]
fn a_body_shows_its_example_and_names_a_content_type_that_is_not_json() -> Result<(), serde_json::Error> {
    let form = json!({ "application/x-www-form-urlencoded": {
        "examples": { "first": { "value": { "q": "cats" } } },
    }});
    let clash = json!({ "application/json": {
        "schema": { "type": "object", "required": ["id"], "properties": { "id": { "type": "integer" } } },
    }});
    let id_param = json!([{ "name": "id", "in": "path", "required": true, "example": 7 }]);
    let paths = json!({
        "/search": { "post": { "requestBody": { "content": form }, "responses": {} } },
        "/things/{id}": { "put": { "operationId": "putThing", "parameters": id_param, "requestBody": { "content": clash }, "responses": {} } },
    });
    let (drafted, notes) = group(&spec(&paths, &json!([])), "spec.yaml")?;
    let search = drafted.apis.first().cloned().unwrap_or_default();
    assert_eq!(search.id, "postSearch");
    assert_eq!(search.body, Some(json!({ "q": "cats" })));
    assert_eq!(search.content_type.as_deref(), Some("application/x-www-form-urlencoded"));
    let put = drafted.apis.get(1).cloned().unwrap_or_default();
    assert_eq!(put.body, Some(json!({ "id": "${var.id}" })));
    assert_eq!(put.vars.get("id"), Some(&json!(7)));
    assert_eq!(put.content_type, None);
    let messages: Vec<_> = notes.iter().map(|note| note.message.as_str()).collect();
    assert_eq!(messages, ["body field id shares a name with a parameter"]);
    Ok(())
}

#[test]
fn a_query_param_with_no_value_to_send_is_listed_as_optional() -> Result<(), serde_json::Error> {
    let params = json!([
        { "name": "limit", "in": "query", "schema": { "type": "integer" } },
        { "name": "page", "in": "query", "schema": { "type": "integer", "default": 1 } },
    ]);
    let paths = json!({ "/users": { "get": { "operationId": "listUsers", "parameters": params, "responses": {} } } });
    let (drafted, _) = group(&spec(&paths, &json!([{ "url": "http://x.test" }])), "spec.yaml")?;
    let api = drafted.apis.first().cloned().unwrap_or_default();
    assert_eq!(api.optional_query, ["limit"]);
    assert_eq!(api.query.keys().map(String::as_str).collect::<Vec<_>>(), ["page"]);
    Ok(())
}
