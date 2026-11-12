//! `*.api.yaml`.

use super::common;
use super::common::{AUTH, HOOKS, METHODS, PROFILES, TEMPLATES, TEXTS, VARS};
use super::types;
use super::types::{Scope, Shape, Ty};

shape! {
    api_file = Shape::new("api", "An api file, `services/<service>/<service>.api.yaml`", &[
        types::key("id", Ty::Text).default("path-derived").doc("File ID; must match the path").example("users"),
        types::key("name", Ty::Text).doc("Shown in listings"),
        types::key("baseUrl", Ty::Template).doc("Prefixed to every `path`").example("${var.baseUrl}"),
        types::key("auth", AUTH).doc("Default auth for the file"),
        types::key("headers", TEMPLATES).doc("Default headers"),
        types::key("vars", VARS).doc("Defaults for every api in the file"),
        types::key("profiles", PROFILES).doc("Per-profile overrides of the file's vars"),
        types::key("source", Ty::Obj(source)).doc("The OpenAPI spec this file syncs from"),
        types::key("beforeRequest", HOOKS).doc("Functions run before every request in the file"),
        types::key("afterResponse", HOOKS).doc("Functions run after every response in the file"),
        types::key("apis", Ty::List(&Ty::Obj(api))).doc("The apis"),
    ])
}

shape! {
    source = Shape::new("source", "The OpenAPI spec an api file syncs from", &[
        types::key("openapi", Ty::Text).required().doc("A URL or a file, JSON or YAML").example("specs/users.yaml"),
        types::key("headers", TEMPLATES).doc("Headers for a protected URL"),
        types::key("include", TEXTS).doc("Take only operations with these tags or path prefixes"),
        types::key("only", TEXTS).doc("Take only these operations, by operation ID"),
    ])
}

shape! {
    api = Shape::new("apiEntry", "One api: a request and the checks that always hold for it", &[
        types::key("id", Ty::Id).required().doc("camelCase; `create` is referenced as `users#create`").example("create"),
        types::key("name", Ty::Text).doc("Shown in listings"),
        types::key("method", Ty::Enum(METHODS)).default("GET").doc("HTTP method"),
        types::key("path", Ty::Template).required().doc("Appended to `baseUrl`").example("/users/${var.userId}"),
        types::key("vars", VARS).doc("Defaults for this api; callers override them"),
        types::key("query", TEMPLATES).doc("Query string"),
        types::key("headers", TEMPLATES).doc("Merged over the file's headers"),
        types::key("body", Ty::Value).doc("Request body: a map, text, `$file`, `$base64` or `$extend`"),
        types::key("auth", AUTH).default("the file's").doc("Overrides the file's auth"),
        types::key("timeout", Ty::Duration).default("defaults.timeout").doc("Per request attempt"),
        types::key("http", Ty::Obj(common::http)).doc("Transport retry for this api; keys replace the config's"),
        types::key("expect", Ty::Obj(common::expect)).default("status: 2xx").scope(Scope::CALL).doc("Checks that hold on every call"),
        types::key("save", VARS).scope(Scope::CALL).doc("Values every caller gets as vars"),
        types::key("saveOn", Ty::Status).default("expect.status, else 2xx").scope(Scope::CALL).doc("The statuses on which `save` runs").example("201"),
        types::key("cleanup", Ty::Obj(common::cleanup)).scope(Scope::CALL).doc("How to undo a successful call"),
        types::key("operationId", Ty::Text).doc("Link to the OpenAPI operation, kept by sync"),
        types::key("beforeRequest", HOOKS).doc("Functions run before this api's requests, after the file's"),
        types::key("afterResponse", HOOKS).doc("Functions run after this api's responses, after the file's"),
        types::key("deprecated", Ty::Bool).default("false").doc("Set by sync when the operation leaves the spec"),
    ])
}
