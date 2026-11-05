use indexmap::IndexMap;

use crate::{
    ActionId, AuthId, Duration, FileId, HistoryConfig, Matcher, Method, ProfileId, RedactRule, SecretsConfig,
    StatusPattern, Sym, Template, TemplateValue, TestId, VarDefs,
};

/// How long before expiry credentials are fetched again when `refreshBefore` is not written.
const REFRESH_BEFORE: std::time::Duration = std::time::Duration::from_secs(60);

/// `vibranium.config.yaml`: settings that are not tables of their own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Config {
    /// `syntaxVersion`.
    pub syntax_version: u32,
    /// `project`.
    pub project: Option<String>,
    /// `defaults`.
    pub defaults: Defaults,
    /// `http.retry` (D143).
    pub http_retry: HttpRetry,
    /// `vars` and `profiles.<id>.vars` at project level.
    pub vars: VarDefs,
    /// `reports.dir`.
    pub reports_dir: Option<String>,
    /// `reports.formats`.
    pub report_formats: Vec<String>,
    /// `redact.headers`.
    pub redact_headers: Vec<String>,
    /// `redact.fields`.
    pub redact_fields: Vec<String>,
    /// `redact.rules` (D171).
    pub redact_rules: Vec<RedactRule>,
    /// `secrets`, with the names `.env.schema` declares (D163, D165).
    pub secrets: SecretsConfig,
    /// `history` (D170).
    pub history: HistoryConfig,
}

/// `defaults`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Defaults {
    /// `timeout` per request.
    pub timeout: Option<Duration>,
    /// `concurrency`: test instances at once.
    pub concurrency: Option<u32>,
    /// `perHost`: requests at once to one host.
    pub per_host: Option<u32>,
    /// `headers` on every request.
    pub headers: IndexMap<String, Template>,
}

/// `http.retry`: retrying one request on a transport failure (D143); off by default, and the
/// default of every other key is D143's.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct HttpRetry {
    /// Further attempts; 0 turns it off.
    pub attempts: u32,
    /// Statuses that are retried.
    pub on_status: Vec<StatusPattern>,
    /// `connect` and `timeout` in `on`.
    pub on_transport: Vec<String>,
    /// Methods that are retried.
    pub methods: Vec<Method>,
    /// `backoff.base`.
    pub base: Duration,
    /// `backoff.max`.
    pub max: Duration,
    /// `backoff.jitter`.
    pub jitter: bool,
}

impl Default for HttpRetry {
    /// `{ attempts: 0, on: [connect, timeout, 502, 503, 504], methods: [GET, HEAD, OPTIONS, PUT,
    /// DELETE], backoff: { base: 100ms, max: 2s, jitter: true } }` (D143).
    fn default() -> Self {
        Self {
            attempts: 0,
            on_status: [502, 503, 504].map(StatusPattern::Code).to_vec(),
            on_transport: vec!["connect".to_owned(), "timeout".to_owned()],
            methods: vec![Method::Get, Method::Head, Method::Options, Method::Put, Method::Delete],
            base: Duration::new(std::time::Duration::from_millis(100)),
            max: Duration::new(std::time::Duration::from_secs(2)),
            jitter: true,
        }
    }
}

/// One entry of `profiles`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Profile {
    /// `id`, lowercase.
    pub id: ProfileId,
    /// `name`.
    pub name: Option<String>,
    /// `default: true`.
    pub default: bool,
    /// `vars`.
    pub vars: IndexMap<crate::VarName, TemplateValue>,
}

/// One entry of `auth`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct AuthDef {
    /// `id`.
    pub id: AuthId,
    /// `name`.
    pub name: Option<String>,
    /// `type`, as registered (`basic`, `bearer`, `apiKey`, `oauth2ClientCredentials`, `flow`, …).
    pub kind: String,
    /// The type's own keys, templates compiled: everything but the keys below.
    pub settings: IndexMap<String, TemplateValue>,
    /// `vars`.
    pub vars: VarDefs,
    /// `steps` of a `flow`, in order; each is an action owned by the run.
    pub steps: Vec<ActionId>,
    /// `expiresIn`: when the credentials go stale; `None` keeps them for the run unless the
    /// token response says otherwise.
    pub expires_in: Option<Template>,
    /// `refreshBefore`: fetch again this long before expiry.
    pub refresh_before: Duration,
    /// `refreshOn`: statuses that fetch the credentials again and retry the request once.
    pub refresh_on: Vec<u16>,
    /// `cache`: keep the credentials across runs, encrypted (not in M3).
    pub cache: bool,
}

impl AuthDef {
    /// A definition of `kind` with no settings, vars or steps, and the default refresh rules.
    #[must_use]
    pub fn new(id: AuthId, kind: impl Into<String>) -> Self {
        Self {
            id,
            name: None,
            kind: kind.into(),
            settings: IndexMap::new(),
            vars: VarDefs::default(),
            steps: Vec::new(),
            expires_in: None,
            refresh_before: Duration::new(REFRESH_BEFORE),
            refresh_on: vec![401],
            cache: false,
        }
    }
}

/// One load file.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct LoadScenario {
    /// The file ID, from the path (D139).
    pub id: FileId,
    /// `name`.
    pub name: Option<String>,
    /// `mix`.
    pub mix: Vec<MixEntry>,
    /// `stages`.
    pub stages: Vec<LoadStage>,
    /// `thinkTime`: fixed, or `min` and `max`.
    pub think_time: (Duration, Duration),
    /// `checks: full`.
    pub full_checks: bool,
    /// `thresholds`: metric to matcher.
    pub thresholds: IndexMap<String, Matcher>,
    /// `abortOn`.
    pub abort_on: Vec<String>,
}

/// One entry of a load `mix`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct MixEntry {
    /// `test`.
    pub test: TestId,
    /// `weight`.
    pub weight: u32,
    /// `thresholds` for that test.
    pub thresholds: IndexMap<String, Matcher>,
}

/// One entry of load `stages`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct LoadStage {
    /// `duration`.
    pub duration: Duration,
    /// `users` at the end of the stage.
    pub users: u32,
}

/// One entry of `datasets`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Dataset {
    /// `id`.
    pub id: crate::DatasetId,
    /// `file`.
    pub file: String,
}

/// One entry of `generators`: the generator kind and its settings.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct GeneratorDef {
    /// `id`.
    pub id: Sym,
    /// The kind key, such as `fromRegExp` or `oneOf`.
    pub kind: String,
    /// Its value.
    pub settings: TemplateValue,
}

#[cfg(test)]
mod tests {
    use super::{Config, HttpRetry};

    #[test]
    fn http_retry_is_off_by_default() {
        let retry = HttpRetry::default();
        assert_eq!(
            (retry.attempts, retry.on_status.len(), retry.on_transport.len(), retry.methods.len()),
            (0, 3, 2, 5)
        );
        assert_eq!((retry.base.get().as_millis(), retry.max.get().as_millis(), retry.jitter), (100, 2000, true));
        assert_eq!(Config::default().syntax_version, 0);
    }
}
