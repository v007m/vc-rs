use std::path::PathBuf;
use std::sync::Arc;

use vc_interfaces::{Artifact, ReportError};
use vc_testkit::reporter_suite;

use super::*;
use crate::testing::{self, Broken, Capture};

/// The XML the scripted run writes.
async fn written() -> String {
    let (capture, captured) = Capture::new();
    let context = testing::context(capture);
    let mut reporter = JunitReporter::new(&context);
    let events = testing::script(&context.program(), context.plan());
    let artifacts = testing::run(&mut reporter, &events, &testing::summary()).await;
    assert_eq!(artifacts, [Artifact::new("junit", PathBuf::from("out/junit.xml"))]);
    let (printed, mut saved) = captured.take();
    assert!(printed.is_empty(), "the file reporter prints nothing");
    assert_eq!(saved.len(), 1);
    let (name, text) = saved.remove(0);
    assert_eq!(name, "junit.xml");
    text
}

#[tokio::test]
async fn passes_the_reporter_suite() {
    let (capture, _captured) = Capture::new();
    reporter_suite(&mut || Box::new(JunitReporter::new(&testing::context(capture.clone())))).await;
}

#[tokio::test]
async fn writes_the_scripted_run() {
    let xml = written().await;
    assert!(!xml.contains('\u{1b}'), "the response's escape character is replaced");
    insta::assert_snapshot!(xml);
}

#[tokio::test]
async fn counts_blocked_tests_as_errors_and_writes_each_outcome() {
    let xml = written().await;
    let suites = r#"<testsuites name="vibranium" tests="6" failures="1" errors="1" skipped="1" time="2.500">"#;
    assert!(xml.contains(suites), "{xml}");
    assert!(xml.contains(r#"<testsuite name="users.crud" tests="6" failures="1" errors="1" skipped="1""#), "{xml}");
    assert!(xml.contains(r#"<failure message="expected 400, got 201" type="V5101">"#), "{xml}");
    assert!(xml.contains(r#"<error message="needs seed, which failed" type="V5106">"#), "{xml}");
    assert!(xml.contains(r#"<skipped message="rewrite pending"/>"#), "{xml}");
    assert!(
        xml.contains(
            "<system-out>flaky: passed after 2 attempts; the failed attempt: expected 200, got 503</system-out>"
        ),
        "{xml}"
    );
    assert_eq!(xml.matches("<testcase ").count(), 6);
}

#[tokio::test]
async fn puts_tests_of_files_that_never_started_in_their_own_suite() {
    let (capture, captured) = Capture::new();
    let context = testing::context(capture);
    let mut reporter = JunitReporter::new(&context);
    let events = testing::script(&context.program(), context.plan());
    let finished = events.into_iter().filter(|e| matches!(e, vc_model::Event::TestFinished { .. }));
    testing::run(&mut reporter, &finished.collect::<Vec<_>>(), &testing::summary()).await;
    let xml = captured.take().1.remove(0).1;
    assert_eq!(xml.matches("<testsuite ").count(), 1, "{xml}");
    assert_eq!(xml.matches("<testcase ").count(), 6, "{xml}");
}

#[test]
fn notes_a_flaky_test_without_a_failure_plainly() {
    let (capture, _captured) = Capture::new();
    let context = testing::context(capture);
    let reporter = JunitReporter::new(&context);
    let test = *context.program().tests.keys().next().unwrap();
    let result = vc_model::TestResult {
        test,
        instance: None,
        status: vc_model::TestStatus::Flaky,
        attempts: 3,
        failure: None,
        skip_reason: None,
    };
    let record = TestRecord { result, steps: Vec::new(), start: None, stop: std::time::Duration::ZERO };
    assert_eq!(reporter.outcome(&record), "      <system-out>flaky: passed after 3 attempts</system-out>\n");
}

#[tokio::test]
async fn reports_a_file_it_cannot_save() {
    let mut reporter = JunitReporter::new(&testing::context(Arc::new(Broken)));
    let refused = ReportError::Write { location: "junit.xml".into(), reason: "read-only".into() };
    assert_eq!(reporter.finish(&testing::summary()).await, Err(refused));
}
