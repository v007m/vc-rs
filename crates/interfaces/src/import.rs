//! What an importer reads (an `OpenAPI` spec, a HAR file, ...) and the draft it hands the engine.

use async_trait::async_trait;
use indexmap::IndexMap;
use serde_json::Value;
use vc_model::Method;

use crate::FetchError;

/// Reads one kind of foreign file into an [`ImportDraft`].
///
/// Every kind (`openapi`, `har`, ...) is an entry of the importers registry in
/// `vc-adapter-importers`. The engine turns the draft into definition files through
/// `parser::edit` and its compile guard (D107), so an importer writes no YAML itself. Every
/// implementation runs `importer_suite` (D112).
#[async_trait]
pub trait Importer: Send + Sync {
    /// The draft for `source`.
    ///
    /// # Errors
    ///
    /// An [`ImportError`] when the file cannot be read as this kind, or a file it refers to
    /// cannot be fetched.
    async fn read(&self, source: &ImportSource, opts: &ImportOptions) -> Result<ImportDraft, ImportError>;

    /// What to do when the source cannot be fetched or read as one file (a folder, say), when
    /// this kind has more to say than the engine's generic help.
    fn source_help(&self) -> Option<&'static str> {
        None
    }

    /// Whether this kind reads a folder (a Bruno collection) as well as a file. The engine then
    /// hands it a folder as an [`ImportSource`] with no bytes, and the importer reads the
    /// folder's files through its [`crate::SpecFetcher`].
    fn reads_folders(&self) -> bool {
        false
    }
}

/// The file being imported: where it came from and its bytes, already fetched or read.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct ImportSource {
    /// A URL or a project-relative path, as written; relative references resolve against it
    /// ([`crate::resolve_reference`]).
    pub location: String,
    /// The file's bytes.
    pub bytes: Vec<u8>,
}

/// What `vc import` was asked for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct ImportOptions {
    /// `--id`: the file ID of the api file the draft fills.
    pub file_id: String,
    /// `--tests`: a test per api, expecting its success status.
    pub tests: bool,
    /// `--negative`: tests that send what the schema rejects.
    pub negative: bool,
    /// `--auth`: a test per secured api that calls it with no auth and expects 401.
    pub auth: bool,
    /// `--include`: only operations with one of these tags or path prefixes; all when empty.
    pub include: Vec<String>,
    /// `--only`: only these operation IDs; all when empty.
    pub only: Vec<String>,
    /// HAR `--flow`: the test replaying the recording, `<fileId>#<testId>`; none when not asked.
    pub flow: Option<String>,
    /// HAR: which recorded calls are the project's.
    pub recording: RecordingFilter,
}

/// Which calls in a recording are the project's; the rest are noise.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct RecordingFilter {
    /// The hosts the active profiles' base URLs render to, whose sites are the project's; the
    /// site of the recording's first call when empty.
    pub hosts: Vec<String>,
    /// `--include-host`: hosts kept whatever their site.
    pub include_hosts: Vec<String>,
    /// `--keep-static`: static files are kept.
    pub keep_static: bool,
}

/// An importer's result: the api files to write, and what the user should know.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct ImportDraft {
    /// One per target api file, existing or new.
    pub groups: Vec<DraftGroup>,
    /// HAR: the recorded order as a test's steps, with chained saves.
    pub flow: Option<DraftFlow>,
    /// Values taken out of the draft and the secrets that replace them.
    pub secrets: Vec<SecretFinding>,
    /// Things the user should know: skipped entries, guesses.
    pub notes: Vec<ImportNote>,
    /// HAR: what was read and filtered, one line (`read 46 entries · filtered 21 static, …`).
    pub summary: Option<String>,
    /// `OpenAPI`: the bundled 3.1 document, saved as the sync snapshot `openapi/<fileId>.yaml`.
    pub snapshot: Option<Value>,
}

/// One api file of a draft.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct DraftGroup {
    /// The file ID (`users`).
    pub file_id: String,
    /// `name`.
    pub name: Option<String>,
    /// `baseUrl`, as template text (`${var.usersUrl}`).
    pub base_url: Option<String>,
    /// Vars the file needs in the config, such as a base URL var, with their values.
    pub config_vars: IndexMap<String, Value>,
    /// The apis, in spec or recording order.
    pub apis: Vec<DraftApi>,
    /// The tests `--tests`, `--negative` and `--auth` asked for, in file `<fileId>.crud`.
    pub tests: Vec<DraftTest>,
}

/// One api of a draft.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct DraftApi {
    /// The camelCase ID within its file.
    pub id: String,
    /// `name`.
    pub name: Option<String>,
    /// `method`.
    pub method: Method,
    /// `path`, as template text (`/users/${var.userId}`).
    pub path: String,
    /// The api's vars and their defaults; a value may be template text (`${fake.internet.email}`).
    pub vars: IndexMap<String, Value>,
    /// `query`, as template text by name.
    pub query: IndexMap<String, String>,
    /// `body`, with template text inside.
    pub body: Option<Value>,
    /// The request content type, when it is not JSON.
    pub content_type: Option<String>,
    /// `operationId` in the spec.
    pub operation_id: Option<String>,
    /// The full ID of an existing api with the same method and path, which is reused.
    pub matches: Option<String>,
    /// The spec's optional query parameters the api does not send, by name; sync reports their
    /// changes.
    pub optional_query: Vec<String>,
    /// `save`: names and the templates over `response` they read.
    pub save: IndexMap<String, String>,
    /// `cleanup`: the api in the same file that undoes a call, and its vars.
    pub cleanup: Option<DraftCleanup>,
}

/// The `cleanup` of a [`DraftApi`]: an api in the same file and the vars it is called with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct DraftCleanup {
    /// The api's own ID in the same file.
    pub call: String,
    /// Its vars, as template text over `response`.
    pub vars: IndexMap<String, String>,
}

/// One test of a draft: a call and the status it expects.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct DraftTest {
    /// The camelCase ID, a third-person verb (`getsUser`).
    pub id: String,
    /// `name`.
    pub name: String,
    /// The api it calls, `<fileId>#<id>`.
    pub call: String,
    /// `vars` on the call.
    pub vars: IndexMap<String, Value>,
    /// `auth: none` on the call.
    pub no_auth: bool,
    /// `expect.status`.
    pub status: u16,
}

/// HAR: the recorded session as one test's steps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct DraftFlow {
    /// The test file's ID.
    pub file_id: String,
    /// The test's ID.
    pub test_id: String,
    /// The steps, in recorded order.
    pub steps: Vec<DraftStep>,
}

/// One step of a [`DraftFlow`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct DraftStep {
    /// The api it calls, `<fileId>#<id>`.
    pub call: String,
    /// `vars`, template text inside.
    pub vars: IndexMap<String, Value>,
    /// `save`: a name to its template text over the response.
    pub save: IndexMap<String, String>,
}

/// A value taken out of the draft and the secret that replaces it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct SecretFinding {
    /// The secret's name in `.env.schema` (`SHOP_API_KEY`).
    pub name: String,
    /// Where the value was found, such as `POST /orders: header authorization`; never the value.
    pub found_at: String,
}

/// Something the user should know about an import.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct ImportNote {
    /// What it is about, such as `GET /users/{id}` or a JSON pointer into the spec.
    pub at: String,
    /// What happened.
    pub message: String,
}

/// Why a file could not be imported.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ImportError {
    /// No importer is registered for the kind.
    #[error("unknown import kind {kind:?}; known: {}", known.join(", "))]
    UnknownKind {
        /// The kind asked for.
        kind: String,
        /// The registered kinds.
        known: Vec<String>,
    },
    /// The file is not valid for this kind, or of a version the importer does not read.
    #[error("cannot read {location}: {reason}")]
    Invalid {
        /// The file.
        location: String,
        /// What is wrong, with the place in the file where possible.
        reason: String,
    },
    /// References lead back to a document already being read.
    #[error("the references form a cycle: {}", chain.join(" -> "))]
    Cycle {
        /// The documents, from the first to the one that repeats.
        chain: Vec<String>,
    },
    /// A referenced file could not be fetched.
    #[error(transparent)]
    Fetch(#[from] FetchError),
}
