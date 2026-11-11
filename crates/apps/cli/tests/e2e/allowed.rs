//! `allowFailure` (D248): a failure it covers is reported apart and keeps the run passing, a
//! lapsed one fails the run and `vc check` warns V4008, and `--strict` fails every one.

use std::fs;

use serde_json::Value;
use vc_mock_server::MockServer;

use crate::{CONFIG, Project, USERS, events, of_kind, said};

/// The file's own `allowFailure` covers each of its tests.
const SANDBOX: (&str, &str) = (
    "services/users/sandbox.test.yaml",
    concat!(
        "allowFailure: {reason: upstream sandbox, until: 2099-12-31}\n",
        "tests:\n",
        "  - id: wantsAnError\n    call: users#list\n    expect: {status: 500}\n",
        "  - id: listsUsers\n    call: users#list\n    expect: {status: 200}\n",
    ),
);

/// A test's own `allowFailure`, as `true` and as an object with only a reason.
const OWN: (&str, &str) = (
    "services/users/own.test.yaml",
    concat!(
        "tests:\n",
        "  - id: toleratesAnError\n    call: users#list\n    expect: {status: 500}\n    allowFailure: true\n",
        "  - id: tracksAnError\n    call: users#list\n    expect: {status: 404}\n",
        "    allowFailure: {reason: 'see #212'}\n",
    ),
);

/// An `allowFailure` whose `until` has passed.
const LAPSED: (&str, &str) = (
    "services/users/lapsed.test.yaml",
    concat!(
        "tests:\n",
        "  - id: outlivesItsTicket\n    call: users#list\n    expect: {status: 500}\n",
        "    allowFailure: {reason: old ticket, until: 2020-01-01}\n",
    ),
);

/// Each finished test's id and status.
fn statuses(stdout: &[u8]) -> Vec<(String, String)> {
    let events = events(stdout);
    let pair = |event: &&Value| (event["test"].as_str().unwrap_or("").to_owned(), event["status"].to_string());
    let mut pairs: Vec<_> = of_kind(&events, "testFinished").iter().map(pair).collect();
    pairs.sort();
    pairs
}

#[tokio::test(flavor = "multi_thread")]
async fn allowed_failures_are_reported_apart_and_keep_the_run_passing() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG, USERS, SANDBOX, OWN]);
    let args = ["--json", "--report", "ctrf,junit", "--out", "out"];
    let output = project.run(&server.base_url(), &args).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let allowed = "\"allowedFailure\"".to_owned();
    let expected = [
        ("users.own#toleratesAnError", allowed.clone()),
        ("users.own#tracksAnError", allowed.clone()),
        ("users.sandbox#listsUsers", "\"passed\"".to_owned()),
        ("users.sandbox#wantsAnError", allowed),
    ];
    assert_eq!(statuses(&output.stdout), expected.map(|(id, status)| (id.to_owned(), status)));
    let ctrf: Value = serde_json::from_slice(&fs::read(project.path().join("out/ctrf.json")).unwrap()).unwrap();
    let summary = &ctrf["results"]["summary"];
    assert_eq!((&summary["passed"], &summary["failed"], &summary["other"]), (&1.into(), &0.into(), &3.into()));
    assert_eq!(ctrf["results"]["extra"]["totals"]["allowedFailures"], 3, "{ctrf:#}");
    let junit = fs::read_to_string(project.path().join("out/junit.xml")).unwrap();
    assert!(junit.contains(r#"<testsuites name="vibranium" tests="4" failures="0" errors="0" skipped="3""#), "{junit}");
    assert!(
        junit.contains(r#"<skipped message="allowed to fail (upstream sandbox): status: expected [500], got 200">"#),
        "{junit}"
    );
    assert!(
        junit.contains(r#"<skipped message="allowed to fail (see #212): status: expected [404], got 200">"#),
        "{junit}"
    );
    assert!(junit.contains(r#"<skipped message="allowed to fail: status: expected [500], got 200">"#), "{junit}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_console_marks_an_allowed_failure_and_counts_it_apart() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG, USERS, SANDBOX]);
    let output = project.run(&server.base_url(), &[]).output().unwrap();
    let said = said(&output);
    assert_eq!(output.status.code(), Some(0), "{said}");
    assert!(said.contains("⚠ users.sandbox#wantsAnError allowed to fail: upstream sandbox"), "{said}");
    assert!(said.contains("1 passed · 1 allowed to fail"), "{said}");
}

#[tokio::test(flavor = "multi_thread")]
async fn strict_fails_the_run_on_every_allowed_failure() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG, USERS, SANDBOX, OWN]);
    let output = project.run(&server.base_url(), &["--strict", "--json"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", said(&output));
    let failed = statuses(&output.stdout).into_iter().filter(|(_, status)| status == "\"failed\"").count();
    assert_eq!(failed, 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lapsed_allow_failure_fails_the_run_and_warns_v4008() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG, USERS, LAPSED]);
    let output = project.run(&server.base_url(), &["--json"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", said(&output));
    assert_eq!(statuses(&output.stdout), [("users.lapsed#outlivesItsTicket".to_owned(), "\"failed\"".to_owned())]);
    let check = project.vc(&["check"]).output().unwrap();
    let warned = said(&check);
    assert_eq!(check.status.code(), Some(0), "a warning, not an error: {warned}");
    assert!(warned.contains("warning[V4008]: this allowFailure expired after 2020-01-01"), "{warned}");
    assert!(warned.contains("fix: Remove the allowFailure (certain)"), "{warned}");
}
