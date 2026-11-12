use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use vc_interfaces::{Artifact, ReportError, RunSummary};
use vc_model::{ActionId, Event, OwnerKey, RunStatus, Totals};
use vc_testkit::reporter_suite;

use super::*;
use crate::testing::{self, Broken, Capture, Captured};

fn spec(color: Color) -> (SpecReporter, Captured, Vec<Event>) {
    let (capture, captured) = Capture::new();
    let context = testing::context(capture).color(color);
    let events = testing::script(&context.program(), context.plan());
    (SpecReporter::new(&context), captured, events)
}

fn artifacts(paths: &[&str]) -> Vec<Artifact> {
    paths.iter().map(|path| Artifact::new("x", PathBuf::from(path))).collect()
}

#[tokio::test]
async fn passes_the_reporter_suite() {
    let (capture, _captured) = Capture::new();
    reporter_suite(&mut || Box::new(SpecReporter::new(&testing::context(capture.clone())))).await;
}

#[tokio::test]
async fn prints_tests_failures_and_the_summary() {
    let (mut reporter, captured, events) = spec(Color::Plain);
    let summary = testing::summary().artifacts(artifacts(&["out/ctrf.json", "out/junit.xml"]));
    assert_eq!(testing::run(&mut reporter, &events, &summary).await, []);
    let (printed, saved) = captured.take();
    assert_eq!(saved, []);
    assert!(!printed.contains('\u{1b}'), "no colour and no escape from the response");
    insta::assert_snapshot!(printed);
}

#[tokio::test]
async fn colours_marks_only_when_asked() {
    let (mut reporter, captured, events) = spec(Color::Ansi);
    testing::run(&mut reporter, &events, &testing::summary()).await;
    let printed = captured.take().0;
    assert!(printed.contains("\u{1b}[32m✓\u{1b}[0m users.crud#getsUser"));
    assert!(printed.contains("\u{1b}[1;31m✕\u{1b}[0m users.crud#rejectsBadEmail [row 1]"));
    assert!(printed.contains("\u{1b}[33m⊘\u{1b}[0m"));
    assert!(printed.contains("\u{1b}[2m-\u{1b}[0m"));
    assert!(!printed.contains("\u{1b}[2J"), "the response's escape is still replaced");
}

/// D248: ⚠ with the reason, the failure block as for a failure, and its own count in the summary.
#[tokio::test]
async fn marks_an_allowed_failure_and_counts_it_apart() {
    let (mut reporter, captured, events) = spec(Color::Plain);
    testing::run(&mut reporter, &testing::allowing(events), &testing::allowed_summary()).await;
    let printed = captured.take().0;
    assert!(printed.contains("⚠ users.crud#rejectsBadEmail [row 1] allowed to fail: upstream sandbox"), "{printed}");
    assert!(printed.contains("expected 400, got 201"), "the failure is still shown: {printed}");
    assert!(!printed.contains('✕'), "{printed}");
    assert!(printed.contains("2 passed · 1 blocked · 1 skipped · 1 flaky · 1 allowed to fail"), "{printed}");
}

#[tokio::test]
async fn heads_the_run_with_the_plan_line_when_given_one() {
    let (capture, captured) = Capture::new();
    let context = testing::context(capture).headline("5 tests · 1 file · profile dev\nignored");
    let mut reporter = SpecReporter::new(&context);
    reporter.on_event(&Event::RunStarted { files: 1, instances: 5 }).await.unwrap();
    assert_eq!(captured.take().0, "5 tests · 1 file · profile dev\n\n");
    let (mut plain, captured, _) = spec(Color::Plain);
    plain.on_event(&Event::RunStarted { files: 1, instances: 1 }).await.unwrap();
    assert_eq!(captured.take().0, "1 test in 1 file\n\n");
}

#[tokio::test]
async fn shows_abandoned_cleanups_and_lifecycle_failures() {
    let (mut reporter, captured, _) = spec(Color::Plain);
    let owner = OwnerKey::Run;
    let cleanups = vec![ActionId::new(0), ActionId::new(99)];
    reporter.on_event(&Event::CleanupsAbandoned { owner, cleanups }).await.unwrap();
    let outcome = vc_model::ActionOutcome::Failed(vc_model::Failure::new("refused").with_code("V5105"));
    reporter.on_event(&Event::ActionFinished { action: ActionId::new(0), owner, outcome }).await.unwrap();
    let passed = vc_model::ActionOutcome::Passed;
    reporter.on_event(&Event::ActionFinished { action: ActionId::new(0), owner, outcome: passed }).await.unwrap();
    insta::assert_snapshot!(captured.take().0, @r"
    ! cleanups abandoned for run: seed, action 99
    ✕ seed (run) failed
      error[V5105]: refused
    ");
}

#[tokio::test]
async fn shows_a_warning_with_its_code_as_it_happens() {
    let (mut reporter, captured, _) = spec(Color::Plain);
    let warning = vc_model::Failure::new("`var.token` was saved\u{1b}[2J undeclared").with_code("V5108");
    let warned = Event::Warned { action: ActionId::new(0), owner: OwnerKey::Run, warning };
    reporter.on_event(&warned).await.unwrap();
    let printed = captured.take().0;
    assert!(!printed.contains('\u{1b}'), "{printed:?}");
    assert!(printed.starts_with("! V5108: `var.token` was saved"), "{printed:?}");
}

#[tokio::test]
async fn sums_up_cancelled_runs_and_any_profiles() {
    let totals = Totals { passed: 3, ..Totals::default() };
    let cancelled = RunSummary::new(RunStatus::Cancelled, totals).timed(0, Duration::from_millis(840));
    let (mut reporter, captured, _) = spec(Color::Plain);
    reporter.finish(&cancelled).await.unwrap();
    assert_eq!(captured.take().0.trim_end(), format!("\n{:<60} 840ms", "3 passed · cancelled"));
    let profiled = cancelled.profiles(["staging", "eu"]).artifacts(artifacts(&["a/ctrf.json"]));
    reporter.finish(&profiled).await.unwrap();
    let printed = captured.take().0;
    assert!(printed.contains("840ms · profiles staging, eu\nreports  a/ctrf.json\n"), "{printed}");
    assert!(!printed.contains("next"));
}

#[test]
fn lists_reports_by_folder_when_they_share_one() {
    assert_eq!(reports(&artifacts(&["out/ctrf.json", "out/junit.xml"])), "out/{ctrf.json,junit.xml}");
    assert_eq!(reports(&artifacts(&["out/ctrf.json", "elsewhere/junit.xml"])), "out/ctrf.json, elsewhere/junit.xml");
    assert_eq!(reports(&artifacts(&["out/ctrf.json"])), "out/ctrf.json");
    assert_eq!(reports(&artifacts(&["/", "out/a"])), "/, out/a");
    let (program, plan) = testing::project();
    let names = Names::new(program, &plan);
    assert_eq!(profiles(&names, &[]), None);
    assert_eq!(profiles(&names, &["dev".into(), "eu".into()]).as_deref(), Some("profiles dev \"Local dev\", eu"));
}

#[tokio::test]
async fn reports_a_console_that_refuses_output() {
    let context = testing::context(Arc::new(Broken));
    let mut reporter = SpecReporter::new(&context);
    let refused = ReportError::Write { location: "console".into(), reason: "closed".into() };
    assert_eq!(reporter.on_event(&Event::RunStarted { files: 0, instances: 0 }).await, Err(refused.clone()));
    assert_eq!(reporter.on_event(&Event::FileStarted { file: first_file(&context) }).await, Ok(()));
    assert_eq!(reporter.finish(&testing::summary()).await, Err(refused));
}

fn first_file(context: &crate::ReporterContext) -> vc_model::FileId {
    context.plan().files[0].id
}

/// One attempt of instance 0's call, whose status check gets `actual`.
fn attempt(context: &crate::ReporterContext, actual: u16) -> Vec<Event> {
    let (action, owner) = (ActionId::new(0), OwnerKey::Instance(vc_model::InstanceId::new(0)));
    let mut check = testing::sample_check(context.plan().files[0].instances[0].test);
    (check.passed, check.actual, check.code) = (false, Some(actual.into()), Some("V5101".into()));
    let failed = vc_model::Failure::new(format!("expected 200, got {actual}")).with_code("V5101").at(action);
    let outcome = vc_model::ActionOutcome::Failed(failed);
    let evaluated = Event::CheckEvaluated { action, owner, check: Box::new(check) };
    vec![Event::ActionStarted { action, owner }, evaluated, Event::ActionFinished { action, owner, outcome }]
}

#[tokio::test]
async fn a_retried_failure_shows_each_different_failure_once_under_its_attempts() {
    let (mut reporter, captured, _) = spec(Color::Plain);
    let context = testing::context(Capture::new().0);
    let mut events: Vec<Event> = [503, 503, 500].into_iter().flat_map(|actual| attempt(&context, actual)).collect();
    let test = context.plan().files[0].instances[0].test;
    let failure = vc_model::Failure::new("expected 200, got 500").with_code("V5101");
    let (instance, status) = (Some(vc_model::InstanceId::new(0)), vc_model::TestStatus::Failed);
    let (attempts, failure, duration) = (3, Some(failure), Duration::ZERO);
    let result = vc_model::TestResult { test, instance, status, attempts, failure, skip_reason: None, duration };
    events.push(Event::TestFinished { result });
    for event in &events {
        reporter.on_event(event).await.unwrap();
    }
    let printed = captured.take().0;
    assert!(printed.starts_with("✕ users.crud#getsUser (3 attempts)"), "{printed}");
    assert!(printed.contains("  attempts 1 and 2 of 3:\n") && printed.contains("  attempt 3 of 3:\n"), "{printed}");
    assert_eq!(printed.matches("got 503").count(), 1, "the same failure once: {printed}");
    assert_eq!(printed.matches("got 500").count(), 1, "{printed}");
    assert!(
        printed.contains("expect.status") && !printed.contains("= at:"),
        "the failed check, not the step: {printed}"
    );
}

#[tokio::test]
async fn follows_a_named_test_and_profile_with_their_names() {
    let (mut reporter, captured, events) = spec(Color::Plain);
    testing::run(&mut reporter, &events, &testing::summary()).await;
    let printed = captured.take().0;
    assert!(printed.contains("✓ users.crud#listsUsers [repeat 2] — Lists every user"), "{printed}");
    assert!(printed.contains("✓ users.crud#getsUser "), "an unnamed test shows its ID alone: {printed}");
    assert!(printed.contains(" · profile dev \"Local dev\""), "{printed}");
}

const TEARDOWN_CONFIG: &str = "syntaxVersion: 2
profiles:
  - id: dev
    default: true
vars: {baseUrl: 'http://x'}
afterRun:
  - id: flush
    call: users#purge
  - id: bye
    call: users#purge
";
const TEARDOWN_USERS: &str = "baseUrl: ${var.baseUrl}
apis:
  - id: get
    path: /users/1
  - id: purge
    method: DELETE
    path: /users
";
const TEARDOWN_CRUD: &str = "beforeAll:
  - id: warm
    call: users#get
afterAll:
  - id: sweep
    call: users#purge
  - id: tidy
    call: users#purge
tests:
  - id: getsUser
    call: users#get
";

/// A failed `afterAll` or `afterRun` action names its block in the summary, each block once,
/// while a failed `beforeAll` and a teardown action that passed are not named (D279).
#[tokio::test]
async fn names_each_failed_teardown_block_once_in_the_summary() {
    let (program, plan) = testing::project_of(&[
        ("vibranium.config.yaml", TEARDOWN_CONFIG),
        ("services/users/users.api.yaml", TEARDOWN_USERS),
        ("services/users/crud.test.yaml", TEARDOWN_CRUD),
    ]);
    let (capture, captured) = Capture::new();
    let mut reporter = SpecReporter::new(&ReporterContext::new(program, Arc::clone(&plan), capture));
    let file = &plan.files[0];
    let failed = vc_model::ActionOutcome::Failed(vc_model::Failure::new("expected 204, got 500").with_code("V5101"));
    let finished = |action, owner, outcome| Event::ActionFinished { action, owner, outcome };
    let events = [
        finished(file.before_all[0].action, OwnerKey::File(file.id), failed.clone()),
        finished(file.after_all[0], OwnerKey::File(file.id), failed.clone()),
        finished(file.after_all[1], OwnerKey::File(file.id), vc_model::ActionOutcome::Passed),
        finished(plan.after_run[0], OwnerKey::Run, failed.clone()),
        finished(plan.after_run[1], OwnerKey::Run, failed),
    ];
    let totals = Totals { passed: 1, ..Totals::default() };
    let summary = RunSummary::new(RunStatus::Failed, totals).timed(0, Duration::from_millis(12));
    testing::run(&mut reporter, &events, &summary).await;
    let printed = captured.take().0;
    let tail = printed.rsplit_once("\n\n").map_or(printed.as_str(), |(_, tail)| tail);
    assert_eq!(tail, format!("{:<60} 12ms\nteardown failed: afterAll in users.crud, afterRun\n", "1 passed"));
    assert!(printed.contains("✕ sweep (file users.crud) failed"), "the action is shown as it fails: {printed}");
}
