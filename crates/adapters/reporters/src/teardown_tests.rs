use std::sync::Arc;

use serde_json::{Value, json};
use vc_interfaces::RunSummary;
use vc_model::{ActionOutcome, Event, Failure, OwnerKey, RunStatus, Totals};

use crate::ctrf::CtrfFormat;
use crate::json::JsonReporter;
use crate::junit::JunitFormat;
use crate::testing::{self, Capture};
use crate::{Format, ReporterContext, RunSnapshot, html_data};

const CONFIG: &str = "syntaxVersion: 2
profiles:
  - id: dev
    default: true
vars: {baseUrl: 'http://x'}
afterRun:
  - id: flush
    call: users#purge
";
const USERS: &str = "baseUrl: ${var.baseUrl}
apis:
  - id: get
    path: /users/1
  - id: purge
    method: DELETE
    path: /users
";
const CRUD: &str = "afterAll:
  - id: sweep
    call: users#purge
  - id: tidy
    call: users#purge
tests:
  - id: getsUser
    call: users#get
";
/// A teardown failure as it reaches the reporters: masked already by the executor's sink.
const MESSAGE: &str = "expected 204, got 500 for token ****";

/// A context over a project with an `afterAll` of two actions and an `afterRun` of one.
fn context() -> ReporterContext {
    context_to(Capture::new().0)
}

fn context_to(destination: Arc<Capture>) -> ReporterContext {
    let (program, plan) = testing::project_of(&[
        ("vibranium.config.yaml", CONFIG),
        ("services/users/users.api.yaml", USERS),
        ("services/users/crud.test.yaml", CRUD),
    ]);
    ReporterContext::new(program, plan, destination)
}

/// The teardown events of a run: when `failing`, both `afterAll` actions and `flush` fail.
fn events(context: &ReporterContext, failing: bool) -> Vec<Event> {
    let (plan, file) = (context.plan(), &context.plan().files[0]);
    let outcome = || {
        if failing { ActionOutcome::Failed(Failure::new(MESSAGE).with_code("V5101")) } else { ActionOutcome::Passed }
    };
    let finished = |action, owner| Event::ActionFinished { action, owner, outcome: outcome() };
    vec![
        finished(file.after_all[0], OwnerKey::File(file.id)),
        finished(file.after_all[1], OwnerKey::File(file.id)),
        finished(plan.after_run[0], OwnerKey::Run),
    ]
}

/// The snapshot of a run with its teardown `failing` or not.
fn snapshot(failing: bool) -> RunSnapshot {
    let context = context();
    testing::record(&context, &events(&context, failing)).1
}

fn expected() -> Value {
    json!([
        {"block": "afterAll", "file": "users.crud", "message": MESSAGE},
        {"block": "afterRun", "file": null, "message": MESSAGE},
    ])
}

/// The one file `format` writes for `snapshot`.
fn written(format: &dyn Format, snapshot: &RunSnapshot) -> String {
    let (capture, captured) = Capture::new();
    format.render(snapshot, capture.as_ref()).unwrap();
    let (_, mut saved) = captured.take();
    saved.remove(0).1
}

#[test]
fn each_failed_block_is_listed_once_with_its_file_and_first_message() {
    let failed = snapshot(true);
    assert_eq!(serde_json::to_value(&failed.teardown_failures).unwrap(), expected());
    assert_eq!(failed.teardown_failed(), ["afterAll in users.crud", "afterRun"]);
    assert!(snapshot(false).teardown_failures.is_empty(), "a passed teardown lists nothing");
}

#[tokio::test]
async fn the_json_summary_lists_the_failed_blocks() {
    let (capture, captured) = Capture::new();
    let context = context_to(capture);
    let mut reporter = JsonReporter::new(&context);
    let summary = RunSummary::new(RunStatus::Failed, Totals { passed: 1, ..Totals::default() });
    testing::run(&mut reporter, &events(&context, true), &summary).await;
    let printed = captured.take().0;
    let last: Value = serde_json::from_str(printed.lines().last().unwrap()).unwrap();
    assert_eq!((&last["event"], &last["teardownFailures"]), (&json!("summary"), &expected()));
    assert_eq!(last["totals"]["passed"], 1, "no count changes");
}

#[test]
fn ctrf_lists_them_under_extra_and_keeps_its_counts() {
    let document = |failing| -> Value { serde_json::from_str(&written(&CtrfFormat, &snapshot(failing))).unwrap() };
    let (failed, passed) = (document(true), document(false));
    assert_eq!(failed["results"]["extra"]["teardownFailures"], expected());
    assert_eq!(passed["results"]["extra"]["teardownFailures"], json!([]));
    assert_eq!(failed["results"]["summary"], passed["results"]["summary"], "the counts are the same");
}

#[test]
fn junit_gives_each_failed_block_one_system_err_and_keeps_its_counts() {
    let (failed, passed) = (written(&JunitFormat, &snapshot(true)), written(&JunitFormat, &snapshot(false)));
    let errs: Vec<&str> = failed.lines().filter(|line| line.contains("<system-err>")).map(str::trim).collect();
    assert_eq!(
        errs,
        [
            format!("<system-err>afterAll in users.crud failed: {MESSAGE}</system-err>"),
            format!("<system-err>afterRun failed: {MESSAGE}</system-err>"),
        ]
    );
    assert!(failed.contains("</system-err>\n</testsuites>\n"), "on <testsuites>, after its suites: {failed}");
    let head = |xml: &str| xml.lines().find(|line| line.starts_with("<testsuites")).map(str::to_owned);
    assert_eq!(head(&failed), head(&passed), "the counts are the same");
    assert!(!passed.contains("<system-err>"));
}

#[test]
fn the_html_summary_lists_them() {
    let entries = html_data::entries(&snapshot(true));
    let summary = entries.iter().find(|(key, _)| key == "summary").map(|(_, value)| value).unwrap();
    assert_eq!(summary["teardownFailures"], expected());
}
