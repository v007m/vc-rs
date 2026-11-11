use std::path::PathBuf;

use vc_interfaces::{Artifact, ReportError};

use super::*;
use crate::testing::{self, Broken, Capture};

/// The XML `snapshot` renders to.
fn rendered(snapshot: &RunSnapshot) -> String {
    let (capture, captured) = Capture::new();
    let artifacts = JunitFormat.render(snapshot, capture.as_ref()).unwrap();
    assert_eq!(artifacts, [Artifact::new("junit", PathBuf::from("out/junit.xml"))]);
    let (printed, mut saved) = captured.take();
    assert!(printed.is_empty(), "the file reporter prints nothing");
    assert_eq!(saved.len(), 1);
    let (name, text) = saved.remove(0);
    assert_eq!(name, "junit.xml");
    text
}

/// The XML the scripted run writes.
fn written() -> String {
    rendered(&testing::snapshot())
}

#[test]
fn writes_the_scripted_run() {
    let xml = written();
    assert!(!xml.contains('\u{1b}'), "the response's escape character is replaced");
    insta::assert_snapshot!(xml);
}

#[test]
fn counts_blocked_tests_as_errors_and_writes_each_outcome() {
    let xml = written();
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

#[test]
fn puts_tests_of_files_that_never_started_in_their_own_suite() {
    let (capture, _captured) = Capture::new();
    let context = testing::context(capture);
    let events = testing::script(&context.program(), context.plan());
    let finished: Vec<_> = events.into_iter().filter(|e| matches!(e, vc_model::Event::TestFinished { .. })).collect();
    let xml = rendered(&testing::record(&context, &finished).1);
    assert_eq!(xml.matches("<testsuite ").count(), 1, "{xml}");
    assert_eq!(xml.matches("<testcase ").count(), 6, "{xml}");
}

#[test]
fn notes_a_flaky_test_without_a_failure_plainly() {
    let mut view = testing::snapshot().tests.remove(3);
    view.attempts = 3;
    assert_eq!(flaky(&view, ""), "flaky: passed after 3 attempts");
}

#[test]
fn reports_a_file_it_cannot_save() {
    let refused = ReportError::Write { location: "junit.xml".into(), reason: "read-only".into() };
    assert_eq!(JunitFormat.render(&testing::snapshot(), &Broken), Err(refused));
}

#[test]
fn names_the_suites_after_the_project() {
    let mut snapshot = testing::snapshot();
    snapshot.project = Some("shop".to_owned());
    let xml = rendered(&snapshot);
    assert!(xml.contains(r#"<testsuites name="shop" tests="6""#), "{xml}");
}

#[test]
fn gives_each_suite_the_seed_and_files_name_and_each_case_its_tests_name() {
    let mut snapshot = testing::snapshot();
    snapshot.run.seed = Some(42);
    let xml = rendered(&snapshot);
    let suite = "time=\"0.750\">\n    <properties>\n      <property name=\"seed\" value=\"42\"/>\n      \
                 <property name=\"name\" value=\"Crud\"/>\n    </properties>\n    <testcase ";
    assert!(xml.contains(suite), "{xml}");
    let case = "<testcase name=\"users.crud#listsUsers [repeat 2]\" classname=\"users.crud\" time=\"0.150\" \
                file=\"services/users/crud.test.yaml\" line=\"22\">\n      <properties>\n        \
                <property name=\"name\" value=\"Lists every user\"/>\n      </properties>\n    </testcase>";
    assert!(xml.contains(case), "{xml}");
    let unnamed = "<testcase name=\"users.crud#getsUser\" classname=\"users.crud\" time=\"0.150\" \
                   file=\"services/users/crud.test.yaml\" line=\"7\"/>";
    assert!(xml.contains(unnamed), "a test with no `name` has no properties: {xml}");
}
