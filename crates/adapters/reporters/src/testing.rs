//! A compiled sample project, a scripted run of it, and destinations and a stopwatch that tests
//! can read back.
//!
//! `users.crud` has `getsUser` (passes), `rejectsBadEmail` with two example rows (the first
//! fails two checks, the second is blocked by `seed`), `listsUsers` repeated twice (the first
//! repeat is flaky) and `parksLegacy`, skipped.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use http::{HeaderValue, Method, Uri};
use serde_json::{Value, json};
use vc_compiler::CompileInput;
use vc_interfaces::{Artifact, ReportError, Reporter, Row, RunSummary};
use vc_model::{
    ActionId, ActionOutcome, CheckKind, CheckResult, CleanupRecord, ConnectionInfo, ContentHash, Event, Exchange,
    Failure, InstanceId, Origin, OwnerKey, Path, Plan, Program, RenderedRequest, RunStatus, Segment, SourceFile,
    SourceFileId, SourcePath, SourceSpan, TestBody, TestId, TestResult, TestStatus, Timing, Totals,
};
use vc_planner::{PlanOptions, Selection};

use crate::{Destination, Recorder, ReporterContext, RunFacts, RunSnapshot, Stopwatch};

const CONFIG: &str = "syntaxVersion: 2
profiles:
  - id: dev
    default: true
vars: {baseUrl: 'http://x'}
";

const USERS: &str = "name: Users
baseUrl: ${var.baseUrl}
apis:
  - id: get
    path: /users/1
  - id: create
    method: POST
    path: /users
";

const CRUD: &str = "name: Crud
tags: [smoke]
beforeAll:
  - id: seed
    call: users#create
tests:
  - id: getsUser
    call: users#get
    tags: [read, smoke]
    expect: {status: 200}
  - id: rejectsBadEmail
    examples:
      - email: no-at-sign
      - email: x
    call: users#create
    vars: {email: '${example.email}'}
    expect:
      status: 400
      body:
        match:
          errors[0].field: email
  - id: listsUsers
    call: users#get
    repeat: 2
  - id: parksLegacy
    call: users#get
    skip: {reason: rewrite pending}
";

const CRUD_PATH: &str = "services/users/crud.test.yaml";

/// The sample project and its plan, with every test.
pub fn project() -> (Arc<Program>, Arc<Plan>) {
    let files = [("vibranium.config.yaml", CONFIG), ("services/users/users.api.yaml", USERS), (CRUD_PATH, CRUD)];
    let files = files.iter().map(|(path, text)| source(path, text)).collect();
    let compiled = vc_compiler::compile(CompileInput { files, ..CompileInput::default() });
    let Some(program) = compiled.program else { panic!("{:?}", compiled.diagnostics) };
    let mut program = Arc::unwrap_or_clone(program);
    for test in program.tests.values_mut() {
        test.standalone = true;
    }
    let options = PlanOptions { today: Some("2026-11-03".into()), ..PlanOptions::default() };
    let plan = vc_planner::plan(&program, &Selection::default(), &options).unwrap();
    (Arc::new(program), Arc::new(plan))
}

fn source(path: &str, text: &str) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) }
}

/// A context over the sample project, writing to `destination`, with a [`Ticks`] stopwatch.
pub fn context(destination: Arc<dyn Destination>) -> ReporterContext {
    let (program, plan) = project();
    ReporterContext::new(program, plan, destination).stopwatch(Arc::new(Ticks::default()))
}

/// Feeds `events` to `reporter`, then finishes it with `summary`.
pub async fn run(reporter: &mut dyn Reporter, events: &[Event], summary: &RunSummary) -> Vec<Artifact> {
    for event in events {
        reporter.on_event(event).await.unwrap();
    }
    reporter.finish(summary).await.unwrap()
}

/// Records `events` and returns the history rows and the snapshot of run 7, ended as [`summary`].
pub fn record(context: &ReporterContext, events: &[Event]) -> (Vec<Row>, RunSnapshot) {
    let mut recorder = Recorder::new(context);
    let rows = events.iter().flat_map(|event| recorder.take(event)).collect();
    (rows, recorder.finish(RunFacts::of(7, &summary())))
}

/// The snapshot of the scripted run.
pub fn snapshot() -> RunSnapshot {
    let (capture, _captured) = Capture::new();
    let context = context(capture);
    record(&context, &script(&context.program(), context.plan())).1
}

/// The summary the script ends with: one of each status, two passes, on profile `dev`.
pub fn summary() -> RunSummary {
    RunSummary::new(RunStatus::Failed, totals())
        .timed(1_793_750_400_000, Duration::from_millis(2_500))
        .profiles(["dev"])
}

fn totals() -> Totals {
    let mut totals = Totals::default();
    let statuses = [TestStatus::Passed, TestStatus::Failed, TestStatus::Blocked, TestStatus::Flaky];
    statuses.into_iter().chain([TestStatus::Passed, TestStatus::Skipped]).for_each(|s| totals.add(s));
    totals
}

/// A `POST` with `request` as its body, answered 201 with `response`, a non-UTF-8 header and
/// 1.5ms of DNS in 12ms.
pub fn exchange(request: &str, response: &str) -> Exchange {
    let mut sent = RenderedRequest::new(Method::POST, Uri::from_static("http://localhost/users"));
    sent.headers.insert("authorization", HeaderValue::from_static("Bearer ***"));
    sent.body = request.to_owned().into();
    let mut answer = vc_testkit::status(201);
    answer.headers.insert("x-raw", HeaderValue::from_bytes(b"caf\xe9").unwrap());
    answer.body = response.to_owned().into();
    let timing =
        Timing { dns: Some(Duration::from_micros(1_500)), total: Duration::from_millis(12), ..Timing::default() };
    Exchange { request: sent, response: answer, timing, connection: ConnectionInfo::default() }
}

/// A stopwatch that moves on 150ms every time it is read.
#[derive(Debug, Default)]
pub struct Ticks(AtomicU64);

impl Stopwatch for Ticks {
    fn elapsed(&self) -> Duration {
        Duration::from_millis(150 * self.0.fetch_add(1, Ordering::Relaxed))
    }
}

/// Something a [`Capture`] was given.
#[derive(Debug)]
enum Output {
    Printed(String),
    Saved(String, String),
}

/// A destination that keeps what it is given, read back through [`Captured`].
#[derive(Debug)]
pub struct Capture(Sender<Output>);

/// The receiving end of a [`Capture`].
#[derive(Debug)]
pub struct Captured(Receiver<Output>);

impl Capture {
    pub fn new() -> (Arc<Self>, Captured) {
        let (sender, receiver) = mpsc::channel();
        (Arc::new(Self(sender)), Captured(receiver))
    }
}

impl Destination for Capture {
    fn print(&self, text: &str) -> Result<(), ReportError> {
        self.0.send(Output::Printed(text.to_owned())).unwrap();
        Ok(())
    }

    fn save(&self, name: &str, contents: &[u8]) -> Result<PathBuf, ReportError> {
        let text = String::from_utf8(contents.to_vec()).unwrap();
        self.0.send(Output::Saved(name.to_owned(), text)).unwrap();
        Ok(PathBuf::from("out").join(name))
    }
}

/// Every file saved, as (name, text).
pub type Saved = Vec<(String, String)>;

impl Captured {
    /// Everything printed, then every file saved as (name, text), since the last call.
    pub fn take(&self) -> (String, Saved) {
        let mut printed = String::new();
        let mut saved = Vec::new();
        for output in self.0.try_iter() {
            match output {
                Output::Printed(text) => printed.push_str(&text),
                Output::Saved(name, text) => saved.push((name, text)),
            }
        }
        (printed, saved)
    }
}

/// A destination that refuses every write.
#[derive(Debug)]
pub struct Broken;

impl Destination for Broken {
    fn print(&self, _: &str) -> Result<(), ReportError> {
        Err(ReportError::Write { location: "console".into(), reason: "closed".into() })
    }

    fn save(&self, name: &str, _: &[u8]) -> Result<PathBuf, ReportError> {
        Err(ReportError::Write { location: name.into(), reason: "read-only".into() })
    }
}

/// The IDs the script names.
struct Ids {
    file: vc_model::FileId,
    seed: ActionId,
    gets: (TestId, ActionId),
    rejects: (TestId, ActionId),
    lists: (TestId, ActionId),
    parks: TestId,
}

fn ids(program: &Program, plan: &Plan) -> Ids {
    let test = |short: &str| {
        let full = format!("users.crud#{short}");
        let found = program.tests.values().find(|t| program.str(t.id.sym()) == Some(full.as_str())).unwrap();
        let TestBody::Call(call) = &found.body else { panic!("{short} calls one api") };
        (found.id, *call)
    };
    let file = &plan.files[0];
    Ids {
        file: file.id,
        seed: file.before_all[0].action,
        gets: test("getsUser"),
        rejects: test("rejectsBadEmail"),
        lists: test("listsUsers"),
        parks: test("parksLegacy").0,
    }
}

/// The span of the first `needle` in the test file.
fn span_of(program: &Program, needle: &str) -> SourceSpan {
    let count = u32::try_from(program.files.len()).unwrap();
    let id = (0..count).map(SourceFileId::new).find(|id| program.files.get(*id).unwrap().path.as_str() == CRUD_PATH);
    let id = id.unwrap();
    let start = program.files.get(id).unwrap().text.find(needle).unwrap();
    let at = |offset| program.files.position(id, offset).unwrap();
    SourceSpan::new(id, at(start), at(start + needle.len()))
}

/// A passing status check written in `test`, at the start of the first file.
pub fn sample_check(test: TestId) -> CheckResult {
    let at = vc_model::Pos::new(1, 1, 0);
    check(test, CheckKind::Status, SourceSpan::new(SourceFileId::new(0), at, at), (json!(200), json!(200)))
}

/// A check written in `test`'s `expect`; it passes when `actual` is `expected`.
fn check(test: TestId, kind: CheckKind, span: SourceSpan, (expected, actual): (Value, Value)) -> CheckResult {
    let passed = expected == actual;
    let code = (!passed).then(|| "V5101".to_owned());
    let origin = Origin::Test(test);
    CheckResult {
        kind,
        target: None,
        origin,
        span,
        passed,
        expected,
        actual: Some(actual),
        diff: None,
        error: None,
        code,
    }
}

fn instance(index: u32) -> OwnerKey {
    OwnerKey::Instance(InstanceId::new(index))
}

fn finished(test: TestId, instance: Option<u32>, status: TestStatus, failure: Option<Failure>) -> Event {
    let attempts = match status {
        TestStatus::Flaky => 2,
        TestStatus::Skipped | TestStatus::Blocked => 0,
        TestStatus::Passed | TestStatus::Failed => 1,
    };
    let skip_reason = (status == TestStatus::Skipped).then(|| "rewrite pending".to_owned());
    let instance = instance.map(InstanceId::new);
    let result = TestResult { test, instance, status, attempts, failure, skip_reason, duration: Duration::ZERO };
    Event::TestFinished { result }
}

/// One call by `owner`, with its checks; it fails when one does.
fn call(action: ActionId, owner: OwnerKey, checks: Vec<CheckResult>) -> Vec<Event> {
    let failed = checks.iter().find(|c| !c.passed).map(|c| {
        let actual = c.actual.clone().unwrap_or_default();
        Failure::new(format!("expected {}, got {actual}", c.expected)).with_code("V5101").at(action)
    });
    let outcome = failed.map_or(ActionOutcome::Passed, ActionOutcome::Failed);
    let mut events = vec![Event::ActionStarted { action, owner }];
    events.extend(checks.into_iter().map(|check| Event::CheckEvaluated { action, owner, check: Box::new(check) }));
    events.push(Event::ActionFinished { action, owner, outcome });
    events
}

/// The scripted run, in event order.
pub fn script(program: &Program, plan: &Plan) -> Vec<Event> {
    let ids = ids(program, plan);
    let status =
        |test, actual| check(test, CheckKind::Status, span_of(program, "status: 200"), (json!(200), json!(actual)));
    let seed = OwnerKey::File(ids.file);
    let mut events = vec![Event::RunStarted { files: 1, instances: 5 }, Event::FileStarted { file: ids.file }];
    events.extend(call(ids.seed, seed, Vec::new()));
    events.extend(call(ids.gets.1, instance(0), vec![status(ids.gets.0, 200)]));
    events.push(finished(ids.gets.0, Some(0), TestStatus::Passed, None));
    events.extend(call(ids.rejects.1, instance(1), rejected(program, ids.rejects.0)));
    let failure = Failure::new("expected 400, got 201").with_code("V5101").at(ids.rejects.1);
    events.push(finished(ids.rejects.0, Some(1), TestStatus::Failed, Some(failure)));
    let blocked = Failure::new("needs seed, which failed").with_code("V5106").at(ids.seed);
    events.push(finished(ids.rejects.0, Some(2), TestStatus::Blocked, Some(blocked)));
    events.extend(call(ids.lists.1, instance(3), vec![status(ids.lists.0, 503)]));
    events.extend(call(ids.lists.1, instance(3), vec![status(ids.lists.0, 200)]));
    let retried = Failure::new("expected 200, got 503").with_code("V5101").at(ids.lists.1);
    events.push(finished(ids.lists.0, Some(3), TestStatus::Flaky, Some(retried)));
    events.extend(call(ids.lists.1, instance(4), Vec::new()));
    events.push(finished(ids.lists.0, Some(4), TestStatus::Passed, None));
    events.push(finished(ids.parks, None, TestStatus::Skipped, None));
    let gone = ActionOutcome::Failed(Failure::new("410 gone").with_code("V5107"));
    let cleanup = CleanupRecord { action: ids.seed, owner: seed, outcome: gone };
    events.extend([Event::CleanupFinished { cleanup }, Event::FileFinished { file: ids.file, totals: totals() }]);
    events.push(Event::RunFinished { status: RunStatus::Failed, totals: totals() });
    events
}

/// The failing row's checks: the status, and a body match with a diff and control characters.
fn rejected(program: &Program, test: TestId) -> Vec<CheckResult> {
    let status = check(test, CheckKind::Status, span_of(program, "status: 400"), (json!(400), json!(201)));
    let span = span_of(program, "errors[0].field: email");
    let mut matched = check(test, CheckKind::Match, span, (json!("email"), json!("e\u{1b}[2Jmail")));
    let segments = vec![Segment::Key("errors".into()), Segment::Index(0), Segment::Key("field".into())];
    matched.target = Some(Path { segments });
    matched.diff = Some("- email\n+ e\u{1b}[2Jmail".into());
    vec![status, matched]
}
