use indexmap::IndexMap;

use crate::{
    ActionId, ApiId, AuthRef, Duration, Expect, FileId, HttpRetry, Matcher, Method, Path, RequestHooks, SaveSpec,
    SourceSpan, Template, TemplateValue, VarDefs,
};

/// One api file: what its apis share.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct ApiGroup {
    /// The file ID, from the path (D139).
    pub id: FileId,
    /// `name`.
    pub name: Option<String>,
    /// `baseUrl`.
    pub base_url: Option<Template>,
    /// `auth` for every api in the file.
    pub auth: AuthRef,
    /// `headers` for every api in the file.
    pub headers: IndexMap<String, Template>,
    /// `vars` and `profiles.<id>.vars`.
    pub vars: VarDefs,
    /// `beforeRequest` and `afterResponse` for every api in the file.
    pub hooks: RequestHooks,
    /// `source`: the API spec the file syncs from.
    pub source: Option<ApiSource>,
    /// The apis, in file order.
    pub apis: Vec<ApiId>,
}

/// An api file's `source`: the API spec `vc sync` reads.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct ApiSource {
    /// `openapi`: a URL or a project-relative file, JSON or YAML, as written.
    pub openapi: String,
    /// `headers` sent when fetching a protected URL.
    pub headers: IndexMap<String, Template>,
    /// `include`: only operations with one of these tags; every operation when empty.
    pub include: Vec<String>,
}

/// One api: a request and the checks that always hold for it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Api {
    /// `<fileId>#<id>` (D138).
    pub id: ApiId,
    /// `name`.
    pub name: Option<String>,
    /// Its api file: base URL, auth and headers come from there.
    pub group: FileId,
    /// `method`.
    pub method: Method,
    /// `path`.
    pub path: Template,
    /// The api's default vars.
    pub vars: VarDefs,
    /// `query`.
    pub query: IndexMap<String, Template>,
    /// `headers`.
    pub headers: IndexMap<String, Template>,
    /// `body`.
    pub body: BodySpec,
    /// `auth`.
    pub auth: AuthRef,
    /// `timeout`.
    pub timeout: Option<Duration>,
    /// `http.retry`, its keys over the config's; `None` when the api writes none (D143).
    pub http_retry: Option<HttpRetry>,
    /// `expect`; `status: 2xx` when none is written.
    pub expect: Expect,
    /// `save`.
    pub save: SaveSpec,
    /// `saveOn`: the statuses on which `save` runs; the api's `expect.status`, else `2xx`, when
    /// none is written (D191).
    pub save_on: Matcher,
    /// `cleanup`: a call action with vars templated over the response.
    pub cleanup: Option<ActionId>,
    /// `beforeRequest` and `afterResponse`.
    pub hooks: RequestHooks,
    /// `deprecated`.
    pub deprecated: bool,
    /// `operationId`: the spec operation the api was imported from, kept by sync.
    pub operation_id: Option<String>,
    /// The whole item.
    pub span: SourceSpan,
}

/// A request body.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum BodySpec {
    /// No body.
    #[default]
    None,
    /// Nested YAML or JSON with templates inside.
    Json(TemplateValue),
    /// A text body.
    Text(Template),
    /// `$file`: the file's bytes.
    File(Template),
    /// `$base64`: decoded bytes.
    Base64(Template),
    /// `$extend` another payload, with changes and removals.
    Extend {
        /// The payload extended.
        base: Template,
        /// Values changed or added.
        changes: TemplateValue,
        /// `$remove` paths.
        remove: Vec<Path>,
    },
    /// Form fields, chosen by `content-type`.
    Form(IndexMap<String, TemplateValue>),
    /// Multipart parts, chosen by `content-type`.
    Multipart(IndexMap<String, PartSpec>),
}

/// One part of a multipart body.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum PartSpec {
    /// A field value.
    Value(TemplateValue),
    /// A file part.
    File(Template),
}

#[cfg(test)]
mod tests {
    use super::BodySpec;

    #[test]
    fn a_body_defaults_to_none() {
        assert_eq!(BodySpec::default(), BodySpec::None);
    }
}
