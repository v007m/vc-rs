//! Engine behaviour against in-memory fake ports (no network).
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use ir::{
    Assertion, Extract, FieldName, FileId, FileName, HeaderName, HookRef, Method, Program, ProgramBuilder, RequestSpec,
    Role, Scope, SetupRef, StatusCode, Template, TestDraft, TestName, VarName, Vars,
};
use ports::{
    AuthProvider, HookRunner, HttpRequest, HttpResponse, PortError, PortErrorKind, Reporter, ResultStore, RunSummary,
    TestResult, Transport,
};
use serde_json::json;

use crate::{Engine, EngineConfig, Ports};

#[derive(Default)]
struct FakeServer {
    setup_calls: AtomicUsize,
    fail_setup: bool,
}

fn has_header(req: &HttpRequest, name: &str) -> bool {
    req.headers.iter().any(|(k, _)| k.as_str() == name)
}

#[async_trait]
impl Transport for FakeServer {
    async fn send(&self, req: HttpRequest) -> Result<HttpResponse, PortError> {
        let (status, body) = match (req.method, req.path.as_str()) {
            (_, _) if !has_header(&req, "authorization") => (401, json!({})),
            (Method::Post, "/sessions") => {
                self.setup_calls.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(20)).await;
                if self.fail_setup { (500, json!({})) } else { (201, json!({"id": "s-1"})) }
            }
            (Method::Get, "/sessions/s-1") => (200, json!({"id": "s-1"})),
            (Method::Get, "/hooked") if has_header(&req, "x-hooked") => (200, json!({})),
            _ => (404, json!({})),
        };
        Ok(HttpResponse { status: StatusCode::new(status), headers: vec![], body })
    }
}

struct FakeAuth;

#[async_trait]
impl AuthProvider for FakeAuth {
    async fn authorize(&self, mut req: HttpRequest) -> Result<HttpRequest, PortError> {
        req.headers.push((HeaderName::new("authorization"), "Bearer t".into()));
        Ok(req)
    }
}

struct FakeHooks;

#[async_trait]
impl HookRunner for FakeHooks {
    async fn before_request(&self, hook: &HookRef, mut req: HttpRequest) -> Result<HttpRequest, PortError> {
        if hook.export.as_str() != "addHeader" {
            return Err(PortError::new(PortErrorKind::Hook, "unknown hook"));
        }
        req.headers.push((HeaderName::new("x-hooked"), "1".into()));
        Ok(req)
    }
}

#[derive(Default)]
struct Counters {
    stored: AtomicUsize,
    reported: AtomicUsize,
    runs_finished: AtomicUsize,
}

#[async_trait]
impl ResultStore for Counters {
    async fn record(&self, _result: TestResult) -> Result<(), PortError> {
        self.stored.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

impl Reporter for Counters {
    fn test_finished(&self, _result: &TestResult) {
        self.reported.fetch_add(1, Ordering::SeqCst);
    }
    fn run_finished(&self, _summary: &RunSummary) {
        self.runs_finished.fetch_add(1, Ordering::SeqCst);
    }
}

fn request(method: Method, path: &str) -> RequestSpec {
    RequestSpec { method, path: Template::new(path), headers: vec![], body: None }
}

fn draft(file: FileId, req: RequestSpec, expect: u16) -> TestDraft {
    TestDraft {
        name: TestName::new(format!("{:?}", req.path)),
        file,
        role: Role::Test,
        request: req,
        setup: None,
        extract: vec![],
        assertions: vec![Assertion::Status(StatusCode::new(expect))],
        before_request: None,
    }
}

/// `files` files, each with `per_file` tests depending on one shared setup test.
fn shared_setup_program(files: usize, per_file: usize, scope: Scope) -> Program {
    let mut b = ProgramBuilder::default();
    let first = b.add_file(FileName::new("f0")).unwrap();
    let mut setup = draft(first, request(Method::Post, "/sessions"), 201);
    setup.role = Role::SetupOnly;
    setup.extract = vec![Extract { var: VarName::new("sid"), field: FieldName::new("id") }];
    let setup_id = b.add_test(setup).unwrap();
    for f in 0..files {
        let file = if f == 0 { first } else { b.add_file(FileName::new(format!("f{f}"))).unwrap() };
        for _ in 0..per_file {
            let mut t = draft(file, request(Method::Get, "/sessions/{{sid}}"), 200);
            t.setup = Some(SetupRef { test: setup_id, scope });
            t.assertions.push(Assertion::BodyField { field: FieldName::new("id"), expected: Template::new("{{sid}}") });
            b.add_test(t).unwrap();
        }
    }
    b.build().unwrap()
}

struct BrokenStore;

#[async_trait]
impl ResultStore for BrokenStore {
    async fn record(&self, _result: TestResult) -> Result<(), PortError> {
        Err(PortError::new(PortErrorKind::Store, "disk full"))
    }
}

fn ports(server: Arc<FakeServer>, counters: Arc<Counters>) -> Ports {
    Ports {
        transport: server,
        auth: Arc::new(FakeAuth),
        hooks: Arc::new(FakeHooks),
        store: Arc::clone(&counters) as Arc<dyn ResultStore>,
        reporter: counters,
    }
}

async fn run_with(program: Program, ports: Ports, concurrency: usize) -> RunSummary {
    Engine::new(Arc::new(program), ports, EngineConfig { concurrency, vars: Vars::default() }).run().await
}

async fn run(program: Program, server: Arc<FakeServer>, counters: Arc<Counters>) -> RunSummary {
    run_with(program, ports(server, counters), 16).await
}

#[tokio::test]
async fn fifty_tests_share_one_file_scoped_setup() {
    let server = Arc::new(FakeServer::default());
    let summary = run(shared_setup_program(1, 50, Scope::File), Arc::clone(&server), Arc::default()).await;
    assert_eq!(server.setup_calls.load(Ordering::SeqCst), 1);
    assert_eq!(summary, RunSummary { passed: 50, ..RunSummary::default() });
}

#[tokio::test]
async fn file_scope_runs_setup_once_per_file_and_run_scope_once() {
    let per_file = Arc::new(FakeServer::default());
    run(shared_setup_program(3, 4, Scope::File), Arc::clone(&per_file), Arc::default()).await;
    assert_eq!(per_file.setup_calls.load(Ordering::SeqCst), 3);
    let per_run = Arc::new(FakeServer::default());
    run(shared_setup_program(3, 4, Scope::Run), Arc::clone(&per_run), Arc::default()).await;
    assert_eq!(per_run.setup_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn failed_setup_is_memoized_and_errors_dependents() {
    let server = Arc::new(FakeServer { fail_setup: true, ..FakeServer::default() });
    let summary = run(shared_setup_program(1, 10, Scope::File), Arc::clone(&server), Arc::default()).await;
    assert_eq!(server.setup_calls.load(Ordering::SeqCst), 1);
    assert_eq!(summary, RunSummary { errored: 10, ..RunSummary::default() });
}

#[tokio::test]
async fn assertions_hooks_and_auth_are_applied() {
    let mut b = ProgramBuilder::default();
    let f = b.add_file(FileName::new("f")).unwrap();
    b.add_test(draft(f, request(Method::Get, "/missing"), 200)).unwrap();
    b.add_test(draft(f, request(Method::Get, "/missing"), 404)).unwrap();
    let mut hooked = draft(f, request(Method::Get, "/hooked"), 200);
    hooked.before_request = Some(HookRef { module: FileName::new("h.ts"), export: VarName::new("addHeader") });
    b.add_test(hooked).unwrap();
    let mut bad_hook = draft(f, request(Method::Get, "/hooked"), 200);
    bad_hook.before_request = Some(HookRef { module: FileName::new("h.ts"), export: VarName::new("nope") });
    b.add_test(bad_hook).unwrap();
    let summary = run(b.build().unwrap(), Arc::default(), Arc::default()).await;
    assert_eq!(summary, RunSummary { passed: 2, failed: 1, errored: 1, store_errors: 0 });
}

#[tokio::test]
async fn every_result_reaches_store_and_reporter() {
    let counters = Arc::new(Counters::default());
    run(shared_setup_program(2, 5, Scope::File), Arc::default(), Arc::clone(&counters)).await;
    assert_eq!(counters.stored.load(Ordering::SeqCst), 10);
    assert_eq!(counters.reported.load(Ordering::SeqCst), 10);
    assert_eq!(counters.runs_finished.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn store_failures_are_counted_not_fatal() {
    let mut broken = ports(Arc::default(), Arc::default());
    broken.store = Arc::new(BrokenStore);
    let summary = run_with(shared_setup_program(1, 3, Scope::File), broken, 16).await;
    assert_eq!(summary, RunSummary { passed: 3, store_errors: 3, ..RunSummary::default() });
}

#[tokio::test]
async fn runs_with_concurrency_one_and_zero() {
    for concurrency in [0, 1] {
        let summary =
            run_with(shared_setup_program(1, 3, Scope::File), ports(Arc::default(), Arc::default()), concurrency).await;
        assert_eq!(summary.passed, 3);
    }
}
