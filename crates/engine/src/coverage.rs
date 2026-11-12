//! The api coverage the Insights screen shows (D300, D301): each api with the tests that call it
//! and the statuses it documents, then each operation a spec snapshot has and no api links to.
//! Which statuses were exercised the web UI reads from the stored exchanges, over its window.

use indexmap::IndexMap;
use serde_json::Value;
use vc_interfaces::{SourceError, SpecFetcher};
use vc_model::{ActionKind, Api, ApiGroup, CheckKind, Expect, Matcher, Program, Skip, SourcePath, StatusPattern, Test};

use crate::list_detail::calls;
use crate::shown::Shown;
use crate::snapshot_guard::snapshot_path;
use crate::{Engine, EngineError};

/// Every api and spec operation, with what covers it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Coverage {
    /// The apis in project order, then the operations only a snapshot has.
    pub apis: Vec<CoveredApi>,
    /// A line for each snapshot that is missing or cannot be read; its operations are left out.
    pub notes: Vec<String>,
}

/// One api, or one operation only a spec snapshot has.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct CoveredApi {
    /// Its full ID, `users#get`; for an operation only a snapshot has, its `operationId`.
    pub api: String,
    /// `GET`, `POST` and so on.
    pub method: String,
    /// Its path as written: `/users/${var.userId}`; the spec's for an operation.
    pub path: String,
    /// The api file it is in, or the snapshot for an operation only that has.
    pub file: String,
    /// The tests that call it, skipped ones included (D301).
    pub tests: u32,
    /// How many of them are skipped.
    pub skipped: u32,
    /// The statuses its `expect`s and its snapshot's responses name, each once: `200`, `2xx`.
    pub documented: Vec<String>,
    /// Whether only a snapshot has it.
    pub spec_only: bool,
}

impl Engine {
    /// Every api with the tests that call it and the statuses it documents, then the operations
    /// of each `openapi/<fileId>.yaml` snapshot that no api links to by `operationId`. A snapshot
    /// is read from the project only, never the network; one that cannot be read, or that is
    /// missing for an api file with a `source`, adds a note and no rows.
    ///
    /// # Errors
    ///
    /// [`EngineError::Invalid`] when the project does not compile.
    pub fn coverage(&self) -> Result<Coverage, EngineError> {
        let (program, _) = self.program()?;
        let mut tally = Tally::new(&program, &self.clock.today().to_string());
        for group in program.api_groups.values() {
            let file_id = program.str(group.id.sym()).unwrap_or_default();
            let path = snapshot_path(file_id);
            match self.spec_snapshot(&path) {
                Snapshot::Read(doc) => tally.spec(group, &path, &doc),
                Snapshot::Missing if group.source.is_some() => tally.notes.push(format!(
                    "`{path}` is missing, so the statuses its spec documents are left out; `vc sync {file_id}` writes it"
                )),
                Snapshot::Missing => {}
                Snapshot::Unreadable => {
                    tally.notes.push(format!("`{path}` cannot be read as a spec, so its operations are left out"));
                }
            }
        }
        Ok(tally.done())
    }

    /// The spec snapshot at `path`, read from the project's sources and decoded as sync decodes
    /// it, through a fetcher with no transport.
    fn spec_snapshot(&self, path: &str) -> Snapshot {
        let Ok(source) = SourcePath::new(path) else { return Snapshot::Unreadable };
        match self.sources.read_bytes(&source) {
            Ok(bytes) => self.pasted_fetcher().decode(path, &bytes).map_or(Snapshot::Unreadable, Snapshot::Read),
            Err(SourceError::Missing { .. }) => Snapshot::Missing,
            Err(_) => Snapshot::Unreadable,
        }
    }
}

/// What a spec snapshot's file holds.
enum Snapshot {
    /// There is none.
    Missing,
    /// It cannot be read or is not YAML or JSON.
    Unreadable,
    /// The spec.
    Read(Value),
}

/// The rows so far, with each one's statuses still as patterns.
struct Tally<'p> {
    program: &'p Program,
    apis: IndexMap<String, (CoveredApi, Vec<StatusPattern>)>,
    spec_only: Vec<(CoveredApi, Vec<StatusPattern>)>,
    notes: Vec<String>,
}

impl<'p> Tally<'p> {
    /// Each api of `program`, with the tests that call it on `today` and its `expect`s' statuses.
    fn new(program: &'p Program, today: &str) -> Self {
        let apis = program.apis.values().map(|api| (text(program, api), (row(program, api), statuses(&api.expect))));
        let mut tally = Self { program, apis: apis.collect(), spec_only: Vec::new(), notes: Vec::new() };
        for test in program.tests.values() {
            tally.test(test, skipped(program, test, today));
        }
        for action in &program.actions {
            let ActionKind::Call(id) = &action.kind else { continue };
            let found = program.apis.get(id).map(|api| text(program, api)).and_then(|id| tally.apis.get_mut(&id));
            if let Some((_, patterns)) = found {
                patterns.extend(statuses(&action.expect));
            }
        }
        tally
    }

    /// Counts `test` once for each api it calls, with the statuses its own `expect` names.
    fn test(&mut self, test: &Test, skipped: bool) {
        let mut ids: Vec<String> = calls(self.program, test).into_iter().map(|call| call.api).collect();
        ids.sort_unstable();
        ids.dedup();
        for id in ids {
            let Some((row, patterns)) = self.apis.get_mut(&id) else { continue };
            row.tests = row.tests.saturating_add(1);
            row.skipped = row.skipped.saturating_add(u32::from(skipped));
            patterns.extend(statuses(&test.expect));
        }
    }

    /// The responses of each operation in `doc`, `group`'s snapshot at `path`: added to the api that
    /// links to it, or a row of its own.
    fn spec(&mut self, group: &ApiGroup, path: &str, doc: &Value) {
        for operation in operations(doc) {
            let linked = operation.id.as_deref().and_then(|id| self.linked(group, id));
            match linked.and_then(|api| self.apis.get_mut(&api)) {
                Some((_, patterns)) => patterns.extend(operation.statuses),
                None => self.spec_only.push(operation.row(path)),
            }
        }
    }

    /// The full ID of the api in `group` whose `operationId` is `id`.
    fn linked(&self, group: &ApiGroup, id: &str) -> Option<String> {
        let apis = group.apis.iter().filter_map(|api| self.program.apis.get(api));
        apis.into_iter().find(|api| api.operation_id.as_deref() == Some(id)).map(|api| text(self.program, api))
    }

    fn done(self) -> Coverage {
        let rows = self.apis.into_values().chain(self.spec_only);
        let apis = rows.map(|(row, patterns)| CoveredApi { documented: documented(patterns), ..row }).collect();
        Coverage { apis, notes: self.notes }
    }
}

/// `api`'s row, before any test or snapshot adds to it.
fn row(program: &Program, api: &Api) -> CoveredApi {
    let file = program.files.get(api.span.file).map(|file| file.path.as_str().to_owned()).unwrap_or_default();
    CoveredApi {
        api: text(program, api),
        method: api.method.name().to_owned(),
        path: Shown { program, vars: &[] }.template(&api.path),
        file,
        ..CoveredApi::default()
    }
}

fn text(program: &Program, api: &Api) -> String {
    program.str(api.id.sym()).unwrap_or_default().to_owned()
}

/// Whether `test` is skipped on `today`: its own `skip` or its file's, until that has passed.
fn skipped(program: &Program, test: &Test, today: &str) -> bool {
    let file = program.test_files.get(&test.file).and_then(|file| file.skip.as_ref());
    let holds = |skip: &Skip| skip.until.as_deref().is_none_or(|until| until >= today);
    file.into_iter().chain(&test.skip).any(holds)
}

/// The statuses `expect`'s written `status` checks name.
fn statuses(expect: &Expect) -> Vec<StatusPattern> {
    let checks = expect.checks.iter().filter(|check| check.kind == CheckKind::Status);
    checks
        .filter_map(|check| match &check.matcher {
            Matcher::Status(patterns) => Some(patterns.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// `patterns` as text, each once: classes before the codes in them, lowest first.
fn documented(mut patterns: Vec<StatusPattern>) -> Vec<String> {
    let key = |pattern: &StatusPattern| match *pattern {
        StatusPattern::Code(code) => (code, 1),
        StatusPattern::Class(class) => (u16::from(class).saturating_mul(100), 0),
    };
    patterns.sort_by_key(key);
    patterns.dedup();
    patterns
        .into_iter()
        .map(|pattern| match pattern {
            StatusPattern::Code(code) => code.to_string(),
            StatusPattern::Class(class) => format!("{class}xx"),
        })
        .collect()
}

/// The methods an `OpenAPI` path item can hold.
const METHODS: [&str; 8] = ["get", "put", "post", "delete", "options", "head", "patch", "trace"];

/// One operation of a spec.
struct Operation {
    method: String,
    path: String,
    id: Option<String>,
    statuses: Vec<StatusPattern>,
}

impl Operation {
    fn row(self, file: &str) -> (CoveredApi, Vec<StatusPattern>) {
        let api = self.id.unwrap_or_default();
        let row = CoveredApi {
            api,
            method: self.method,
            path: self.path,
            file: file.to_owned(),
            spec_only: true,
            ..CoveredApi::default()
        };
        (row, self.statuses)
    }
}

/// Each operation under `doc`'s `paths`, in the spec's order.
fn operations(doc: &Value) -> Vec<Operation> {
    let paths = doc.get("paths").and_then(Value::as_object).into_iter().flatten();
    paths
        .flat_map(|(path, item)| {
            METHODS.iter().filter_map(move |method| {
                let operation = item.get(*method)?;
                Some(Operation {
                    method: method.to_ascii_uppercase(),
                    path: path.clone(),
                    id: operation.get("operationId").and_then(Value::as_str).map(str::to_owned),
                    statuses: responses(operation),
                })
            })
        })
        .collect()
}

/// The statuses `operation`'s `responses` are keyed by; `default` names none.
fn responses(operation: &Value) -> Vec<StatusPattern> {
    let keys = operation.get("responses").and_then(Value::as_object).into_iter().flat_map(|found| found.keys());
    keys.filter_map(|key| StatusPattern::parse(&key.to_ascii_lowercase())).collect()
}

#[cfg(test)]
#[path = "coverage_tests.rs"]
mod tests;
