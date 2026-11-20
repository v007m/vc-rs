use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    ActionId, ApiId, AuthRef, Duration, Expect, FileId, HttpRetry, Matcher, Method, Path, Protocol, RequestHooks,
    SaveSpec, SourceSpan, StatusPattern, Template, TemplateValue, VarDefs,
};

/// One api file: what its apis share.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct ApiSource {
    /// `openapi`: a URL or a project-relative file, JSON or YAML, as written.
    pub openapi: String,
    /// `headers` sent when fetching a protected URL.
    pub headers: IndexMap<String, Template>,
    /// `include`: only operations with one of these tags or path prefixes; every operation when
    /// empty.
    pub include: Vec<String>,
    /// `only`: only the operations with these operation IDs; every operation when empty (D277).
    pub only: Vec<String>,
}

/// One api: a request and the checks that always hold for it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    /// `type`: its own, else its file's, else `http` (D342).
    pub protocol: Protocol,
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
    /// How a streaming api's call opens and keeps its stream; `None` on a type that does not
    /// stream (D376).
    pub stream: Option<StreamSettings>,
    /// `exchange`: what a stream call sends and waits for, in order; empty when none is written,
    /// and then the call reads until the server closes (D377, D381).
    pub exchange: Vec<ExchangeItem>,
    /// `expect`; its status is [`Api::success_status`] when none is written.
    pub expect: Expect,
    /// `save`.
    pub save: SaveSpec,
    /// `saveOn`: the statuses on which `save` runs; the api's `expect.status`, else
    /// [`Api::success_status`], when none is written (D191, D386).
    pub save_on: Matcher,
    /// Whether `saveOn` is written. On a `graphql` api the default one also needs the response's
    /// `errors` absent or empty, and an api's `cleanup` always does (D345).
    pub save_on_written: bool,
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

impl Api {
    /// The status a call of this api succeeds on when its `expect` checks none: its type's
    /// [`Protocol::success_status`].
    #[must_use]
    pub const fn success_status(&self) -> StatusPattern {
        self.protocol.success_status()
    }
}

/// What a streaming api's type decides of its call (D376): whether it opens its stream once more.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct StreamSettings {
    /// `sse.reconnect`, the api's, else its file's: whether an `sse` stream that ends before its
    /// exchange is done is opened once more (D385). Always `false` on a `websocket` api.
    pub reconnect: bool,
}

/// One item of an `exchange` (D377).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct ExchangeItem {
    /// What it sends or waits for.
    pub kind: ExchangeKind,
    /// The whole item, reported when a `receive` is unmet.
    pub span: SourceSpan,
}

/// What one `exchange` item does.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum ExchangeKind {
    /// `send`: a value rendered before the stream opens; a map or list is sent as JSON text.
    Send(TemplateValue),
    /// `receive` with one check: the next message it matches, skipping the others (D378).
    Receive(Matcher),
    /// `receive` with a list: a group, each check paired with its own message in any order.
    Group(Vec<Matcher>),
}

/// A request body.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    /// A `graphql` api's request: sent as the JSON `{query, variables, operationName}` (D343).
    Graphql(GraphqlBody),
}

/// A `graphql` api's `body` (D343).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct GraphqlBody {
    /// `query`: the document as written, never a template (D344).
    pub document: String,
    /// `variables`: templates, `{}` when unset.
    pub variables: TemplateValue,
    /// `operationName`, when set.
    pub operation_name: Option<String>,
}

/// One part of a multipart body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
