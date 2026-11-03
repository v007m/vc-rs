use indexmap::IndexMap;

use crate::{Duration, FileId, Matcher, ProfileId, StatusPattern, Sym, Template, TemplateValue, TestId, VarDefs};

/// `vibranium.config.yaml`: settings that are not tables of their own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
}

/// `defaults`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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

/// `http.retry`: retrying one request on a transport failure (D143); off by default.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HttpRetry {
    /// Further attempts; 0 turns it off.
    pub attempts: u32,
    /// Statuses that are retried.
    pub on_status: Vec<StatusPattern>,
    /// `connect` and `timeout` in `on`.
    pub on_transport: Vec<String>,
    /// Methods that are retried.
    pub methods: Vec<crate::Method>,
    /// `backoff.base`.
    pub base: Duration,
    /// `backoff.max`.
    pub max: Duration,
    /// `backoff.jitter`.
    pub jitter: bool,
}

/// One entry of `profiles`.
#[derive(Clone, Debug, PartialEq, Eq)]
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
pub struct AuthDef {
    /// `id`.
    pub id: crate::AuthId,
    /// `name`.
    pub name: Option<String>,
    /// `type`, as registered (`basic`, `bearer`, `apiKey`, `oauth2ClientCredentials`, `flow`, …).
    pub kind: String,
    /// The type's own keys, templates compiled.
    pub settings: IndexMap<String, TemplateValue>,
    /// `vars`.
    pub vars: VarDefs,
}

/// One load file.
#[derive(Clone, Debug, PartialEq, Eq)]
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
pub struct LoadStage {
    /// `duration`.
    pub duration: Duration,
    /// `users` at the end of the stage.
    pub users: u32,
}

/// One entry of `datasets`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dataset {
    /// `id`.
    pub id: crate::DatasetId,
    /// `file`.
    pub file: String,
}

/// One entry of `generators`: the generator kind and its settings.
#[derive(Clone, Debug, PartialEq, Eq)]
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
        assert_eq!(HttpRetry::default().attempts, 0);
        assert_eq!(Config::default().syntax_version, 0);
    }
}
