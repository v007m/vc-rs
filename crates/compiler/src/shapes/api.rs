//! `*.api.yaml`.

use super::common::{AUTH, HOOKS, METHODS, PROFILES, TEMPLATES, TEXTS, VARS, cleanup, expect, http};
use super::types::{Scope, Shape, Ty, key};

shape! {
    api_file = Shape::new("api", "An api file, `services/<service>/<service>.api.yaml`", &[
        key("id", Ty::Text).default("path-derived").doc("File ID; must match the path (D139)").example("users"),
        key("name", Ty::Text).doc("Shown in listings"),
        key("baseUrl", Ty::Template).doc("Prefixed to every `path`").example("${var.baseUrl}"),
        key("auth", AUTH).doc("Default auth for the file"),
        key("headers", TEMPLATES).doc("Default headers"),
        key("vars", VARS).doc("Defaults for every api in the file"),
        key("profiles", PROFILES).doc("Per-profile overrides of the file's vars"),
        key("source", Ty::Obj(source)).doc("The OpenAPI spec this file syncs from"),
        key("beforeRequest", HOOKS).doc("Functions run before every request in the file"),
        key("afterResponse", HOOKS).doc("Functions run after every response in the file"),
        key("apis", Ty::List(&Ty::Obj(api))).doc("The apis"),
    ])
}

shape! {
    source = Shape::new("source", "The OpenAPI spec an api file syncs from", &[
        key("openapi", Ty::Text).required().doc("A URL or a file, JSON or YAML").example("specs/users.yaml"),
        key("headers", TEMPLATES).doc("Headers for a protected URL"),
        key("include", TEXTS).doc("Take only operations with these tags"),
    ])
}

shape! {
    api = Shape::new("apiEntry", "One api: a request and the checks that always hold for it", &[
        key("id", Ty::Id).required().doc("camelCase; `create` is referenced as `users#create`").example("create"),
        key("name", Ty::Text).doc("Shown in listings"),
        key("method", Ty::Enum(METHODS)).default("GET").doc("HTTP method"),
        key("path", Ty::Template).required().doc("Appended to `baseUrl`").example("/users/${var.userId}"),
        key("vars", VARS).doc("Defaults for this api; callers override them"),
        key("query", TEMPLATES).doc("Query string"),
        key("headers", TEMPLATES).doc("Merged over the file's headers"),
        key("body", Ty::Value).doc("Request body: a map, text, `$file`, `$base64` or `$extend`"),
        key("auth", AUTH).default("the file's").doc("Overrides the file's auth"),
        key("timeout", Ty::Duration).default("defaults.timeout").doc("Per request attempt"),
        key("http", Ty::Obj(http)).doc("Transport retry for this api; keys replace the config's (D143)"),
        key("expect", Ty::Obj(expect)).default("status: 2xx").scope(Scope::CALL).doc("Checks that hold on every call"),
        key("save", VARS).scope(Scope::CALL).doc("Values every caller gets as vars"),
        key("cleanup", Ty::Obj(cleanup)).scope(Scope::CALL).doc("How to undo a successful call"),
        key("operationId", Ty::Text).doc("Link to the OpenAPI operation, kept by sync"),
        key("beforeRequest", HOOKS).doc("Functions run before this api's requests, after the file's"),
        key("afterResponse", HOOKS).doc("Functions run after this api's responses, after the file's"),
        key("deprecated", Ty::Bool).default("false").doc("Set by sync when the operation leaves the spec"),
    ])
}
