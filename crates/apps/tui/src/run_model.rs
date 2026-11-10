//! A watched run folded into what the Run tab shows: its phase and counts, the finished tests
//! grouped by file with their checks, and the latest requests.

use std::collections::{HashMap, VecDeque};

use vc_engine::{CheckResult, Event, Exchange, Names, OwnerKey, RunFeed, RunReport, TestResult, TestStatus, Totals};

use crate::theme::{clean, first_line};

/// The requests kept for the Requests pane, newest last.
const REQUESTS: usize = 200;

/// Where the run is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    /// Nothing has run yet.
    #[default]
    Idle,
    /// Asked for; the feed has not started.
    Starting,
    /// The feed has started.
    Running,
    /// The run ended and its reporters have finished.
    Done(RunReport),
    /// The run could not start or ended in an error.
    Failed(String),
}

/// One check of a finished test.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckLine {
    /// Whether it held.
    pub passed: bool,
    /// Its `expect` key: `status`, `body.match.items[*].price`.
    pub label: String,
    /// The line it is written on, when the program has its file.
    pub line: Option<u32>,
    /// Why it failed: the expected and actual values, or the error it hit.
    pub detail: Option<String>,
}

/// One finished test instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finished {
    /// The test's full ID, which reruns it: `users.crud#createsUser`.
    pub test: String,
    /// The instance within its file: `createsUser [repeat 2]`.
    pub label: String,
    /// How it ended.
    pub status: TestStatus,
    /// How many times it ran.
    pub attempts: u32,
    /// Where it is written: `services/users/crud.test.yaml:4`.
    pub location: Option<String>,
    /// Why it failed, was blocked or was skipped.
    pub reason: Option<String>,
    /// Its checks, in the order they ran.
    pub checks: Vec<CheckLine>,
}

/// The finished tests of one file, in the order they finished.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileGroup {
    /// The file ID: `users.crud`.
    pub file: String,
    /// Its finished instances.
    pub tests: Vec<Finished>,
}

/// One request a run sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestLine {
    /// `GET /users?page=2`.
    pub request: String,
    /// The response's status code.
    pub status: u16,
    /// The whole exchange's time.
    pub millis: u128,
    /// The action that sent it: `call users#list`.
    pub by: String,
}

/// The run as the Run tab shows it.
#[derive(Debug, Default)]
pub struct RunModel {
    names: Option<Names>,
    /// Where the run is.
    pub phase: Phase,
    /// The instances the plan holds.
    pub planned: usize,
    /// The finished instances by status.
    pub totals: Totals,
    /// The finished tests by file, the files in the order they first finished a test.
    pub files: Vec<FileGroup>,
    /// The latest requests, newest last.
    pub requests: VecDeque<RequestLine>,
    /// Events the feed dropped because the screen fell behind.
    pub missed: u64,
    checks: HashMap<OwnerKey, Vec<CheckLine>>,
}

impl RunModel {
    /// A run just asked for: everything from the last one is cleared.
    pub fn starting() -> Self {
        Self { phase: Phase::Starting, ..Self::default() }
    }

    /// Whether a run is asked for or in progress.
    pub const fn busy(&self) -> bool {
        matches!(self.phase, Phase::Starting | Phase::Running)
    }

    /// How many of the planned instances have finished; a skipped test is not planned.
    pub const fn finished(&self) -> usize {
        let totals = &self.totals;
        totals.passed + totals.failed + totals.blocked + totals.flaky
    }

    /// Folds the next item of the feed in.
    pub fn feed(&mut self, item: &RunFeed) {
        match item {
            RunFeed::Started(names) => {
                *self = Self { names: Some(names.clone()), phase: Phase::Running, ..Self::default() };
            }
            RunFeed::Event(sequenced) => self.event(&sequenced.event),
            RunFeed::Missed { count } => self.missed = self.missed.saturating_add(*count),
            RunFeed::Done(report) => self.phase = Phase::Done(report.clone()),
            _ => {}
        }
    }

    /// The run could not start or ended in an error.
    pub fn failed(&mut self, reason: &str) {
        self.phase = Phase::Failed(first_line(reason));
    }

    fn event(&mut self, event: &Event) {
        let Some(names) = self.names.as_ref() else { return };
        match event {
            Event::RunStarted { instances, .. } => self.planned = *instances,
            Event::CheckEvaluated { owner, check, .. } => {
                let line = check_line(names, check);
                self.checks.entry(*owner).or_default().push(line);
            }
            Event::Exchanged { action, exchange, .. } => {
                let line = request_line(exchange, &names.action(*action));
                self.requests.push_back(line);
                if self.requests.len() > REQUESTS {
                    self.requests.pop_front();
                }
            }
            Event::TestFinished { result } => self.finish(result),
            _ => {}
        }
    }

    fn finish(&mut self, result: &TestResult) {
        let Some(names) = self.names.as_ref() else { return };
        let checks = result.instance.and_then(|id| self.checks.remove(&OwnerKey::Instance(id))).unwrap_or_default();
        let file = names.file_of(result.test);
        let finished = finished(names, result, checks);
        self.totals.add(result.status);
        if let Some(group) = self.files.iter_mut().find(|group| group.file == file) {
            group.tests.push(finished);
        } else {
            self.files.push(FileGroup { file, tests: vec![finished] });
        }
    }
}

fn finished(names: &Names, result: &TestResult, checks: Vec<CheckLine>) -> Finished {
    let test = names.test(result.test).to_owned();
    let instance = names.instance(result);
    let label = instance.split_once('#').map_or(instance.as_str(), |(_, label)| label).to_owned();
    let location = names.test_location(result.test).map(|at| format!("{}:{}", at.path, at.line));
    let failure = result.failure.as_ref().map(|failure| failure.message.clone());
    let reason = failure.or_else(|| result.skip_reason.clone()).map(|reason| clean(&reason));
    Finished { test, label, status: result.status, attempts: result.attempts, location, reason, checks }
}

fn check_line(names: &Names, check: &CheckResult) -> CheckLine {
    let line = names.location(check.span).map(|at| at.line);
    let detail = (!check.passed).then(|| clean(&check_detail(check)));
    CheckLine { passed: check.passed, label: clean(&vc_engine::check_label(check)), line, detail }
}

/// `expected 201, got 200`, or the error the check hit.
fn check_detail(check: &CheckResult) -> String {
    if let Some(error) = &check.error {
        return error.clone();
    }
    let actual = check.actual.as_ref().map_or_else(|| "nothing".to_owned(), ToString::to_string);
    format!("expected {}, got {actual}", check.expected)
}

fn request_line(exchange: &Exchange, by: &str) -> RequestLine {
    let uri = &exchange.request.uri;
    let target = uri.path_and_query().map_or_else(|| uri.path().to_owned(), ToString::to_string);
    RequestLine {
        request: clean(&format!("{} {target}", exchange.request.method)),
        status: exchange.response.status.as_u16(),
        millis: exchange.timing.total.as_millis(),
        by: clean(by),
    }
}
