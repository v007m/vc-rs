use serde_json::{Value, json};

use super::to_openapi3;

fn converted(doc: Value) -> Value {
    let Value::Object(doc) = doc else { return Value::Null };
    Value::Object(to_openapi3(doc))
}

#[test]
fn host_base_path_and_scheme_become_a_server() {
    let doc = converted(json!({ "swagger": "2.0", "host": "api.test", "basePath": "/v1", "schemes": ["http"] }));
    assert_eq!(doc["servers"], json!([{ "url": "http://api.test/v1" }]));
    assert_eq!(doc["openapi"], "3.0.3");
    assert!(doc.get("swagger").is_none());
    assert_eq!(converted(json!({ "basePath": "/v1" }))["servers"], json!([{ "url": "/v1" }]));
    assert_eq!(converted(json!({}))["servers"], json!([{ "url": "/" }]));
}

#[test]
fn sections_move_into_components_and_refs_follow() {
    let doc = converted(json!({
        "definitions": { "Pet": { "type": "object" } },
        "parameters": {
            "Limit": { "name": "limit", "in": "query", "type": "integer", "default": 20 },
            "NewPet": { "name": "pet", "in": "body", "schema": { "$ref": "#/definitions/Pet" } },
        },
        "securityDefinitions": { "basic": { "type": "basic" } },
        "paths": { "/pets": { "get": {
            "parameters": [{ "$ref": "#/parameters/Limit" }],
            "responses": { "200": { "description": "ok", "schema": { "$ref": "#/definitions/Pet" } } },
        }}},
    }));
    let components = &doc["components"];
    assert_eq!(components["schemas"]["Pet"], json!({ "type": "object" }));
    let limit = json!({ "name": "limit", "in": "query", "schema": { "type": "integer", "default": 20 } });
    assert_eq!(components["parameters"], json!({ "Limit": limit }));
    assert_eq!(components["securitySchemes"]["basic"], json!({ "type": "http", "scheme": "basic" }));
    let get = &doc["paths"]["/pets"]["get"];
    assert_eq!(get["parameters"], json!([{ "$ref": "#/components/parameters/Limit" }]));
    let content = json!({ "application/json": { "schema": { "$ref": "#/components/schemas/Pet" } } });
    assert_eq!(get["responses"]["200"], json!({ "description": "ok", "content": content }));
}

#[test]
fn a_body_parameter_becomes_the_request_body() {
    let doc = converted(json!({
        "consumes": ["application/json", "application/xml"],
        "parameters": { "NewPet": { "name": "pet", "in": "body", "required": true, "schema": { "type": "object" } } },
        "paths": { "/pets": { "post": {
            "parameters": [{ "$ref": "#/parameters/NewPet" }, { "name": "x-trace", "in": "header", "type": "string" }],
            "responses": {},
        }}},
    }));
    let post = &doc["paths"]["/pets"]["post"];
    let media = json!({ "schema": { "type": "object" } });
    let body = json!({ "required": true, "content": { "application/json": media, "application/xml": media } });
    assert_eq!(post["requestBody"], body);
    assert_eq!(post["parameters"], json!([{ "name": "x-trace", "in": "header", "schema": { "type": "string" } }]));
}

#[test]
fn form_parameters_become_a_form_body() {
    let doc = converted(json!({ "paths": { "/pets/{id}/photo": { "post": {
        "consumes": ["multipart/form-data"],
        "parameters": [
            { "name": "file", "in": "formData", "type": "file", "required": true },
            { "name": "note", "in": "formData", "type": "string" },
        ],
    }}}}));
    let body = &doc["paths"]["/pets/{id}/photo"]["post"]["requestBody"]["content"]["multipart/form-data"];
    let properties = json!({ "file": { "type": "string", "format": "binary" }, "note": { "type": "string" } });
    assert_eq!(body["schema"], json!({ "type": "object", "properties": properties, "required": ["file"] }));
}

#[test]
fn response_examples_and_headers_are_kept() {
    let doc =
        converted(json!({ "produces": ["application/json"], "paths": { "/pets": { "get": { "responses": { "200": {
            "description": "ok",
            "schema": { "type": "array" },
            "examples": { "application/json": [] },
            "headers": { "x-next": { "type": "string" } },
        }}}}}}));
    let ok = &doc["paths"]["/pets"]["get"]["responses"]["200"];
    assert_eq!(ok["content"]["application/json"], json!({ "schema": { "type": "array" }, "example": [] }));
    assert_eq!(ok["headers"]["x-next"], json!({ "schema": { "type": "string" } }));
}

#[test]
fn oauth2_flows_are_named_the_3_0_way() {
    let doc = converted(json!({ "securityDefinitions": { "auth": {
        "type": "oauth2", "flow": "application", "tokenUrl": "https://auth.test/token", "scopes": {},
    }}}));
    let flows = json!({ "clientCredentials": { "tokenUrl": "https://auth.test/token", "scopes": {} } });
    assert_eq!(doc["components"]["securitySchemes"]["auth"], json!({ "type": "oauth2", "flows": flows }));
}

#[test]
fn collection_formats_become_explode() {
    let doc = converted(json!({ "paths": { "/pets": { "parameters": [
        { "name": "tags", "in": "query", "type": "array", "items": { "type": "string" }, "collectionFormat": "multi" },
    ]}}}));
    let tags = &doc["paths"]["/pets"]["parameters"][0];
    assert_eq!(tags["explode"], true);
    assert_eq!(tags["schema"], json!({ "type": "array", "items": { "type": "string" } }));
}
