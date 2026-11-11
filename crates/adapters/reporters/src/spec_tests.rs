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
    assert_eq!(profiles(&[]), None);
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
