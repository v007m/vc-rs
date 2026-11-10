//! The CTRF and `JUnit` files of a run with a pass, a failure, a skip and a flaky pass.
//!
//! The CTRF schema is not vendored (`jsonschema` serves `body.schema` checks, D201) and no
//! pure-Rust XSD validator exists, so both files are checked structurally: CTRF against the
//! required fields and enums of its schema, `JUnit` for well-nested tags and the attributes CI
//! tools read.

use std::fs;

use serde_json::Value;
use vc_mock_server::MockServer;

use crate::{CONFIG, Project, SCRIPTED, USERS};

const MIXED: (&str, &str) = (
    "services/users/mixed.test.yaml",
    "tests:\n  - id: listsUsers\n    call: users#list\n\
     \x20 - id: wantsAnError\n    call: users#list\n    expect: {status: 500}\n\
     \x20 - id: isParked\n    call: users#list\n    skip: {reason: later}\n\
     \x20 - id: passesSecondTime\n    call: scripted#flaky\n    expect: {status: 200}\n\
     \x20   retry: {attempts: 2, delay: 10ms}\n",
);

const STATUSES: &[&str] = &["passed", "failed", "skipped", "pending", "other"];

async fn reported() -> Project {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG, USERS, SCRIPTED, MIXED]);
    project.run(&server.base_url(), &["--report", "ctrf,junit", "--out", "out"]).assert().code(1);
    project
}

fn has(value: &Value, keys: &[&str]) {
    for key in keys {
        assert!(value.get(key).is_some(), "`{key}` is required: {value:#}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn ctrf_has_every_field_its_schema_requires() {
    let project = reported().await;
    let ctrf: Value = serde_json::from_slice(&fs::read(project.path().join("out/ctrf.json")).unwrap()).unwrap();
    has(&ctrf, &["results", "reportFormat", "specVersion"]);
    assert_eq!(ctrf["reportFormat"], "CTRF");
    let version = ctrf["specVersion"].as_str().unwrap();
    assert!(version.split('.').count() == 3 && version.split('.').all(|part| part.parse::<u32>().is_ok()), "{version}");
    let results = &ctrf["results"];
    has(results, &["tool", "summary", "tests"]);
    has(&results["tool"], &["name"]);
    let summary = &results["summary"];
    for key in ["tests", "passed", "failed", "skipped", "pending", "other", "start", "stop"] {
        assert!(summary[key].is_u64(), "summary.{key} is an integer: {summary:#}");
    }
    assert_eq!((&summary["tests"], &summary["failed"], &summary["skipped"]), (&4.into(), &1.into(), &1.into()));
    let tests = results["tests"].as_array().unwrap();
    for test in tests {
        has(test, &["name", "status", "duration"]);
        assert!(STATUSES.contains(&test["status"].as_str().unwrap()), "{test:#}");
        assert!(test["duration"].is_u64() && test["flaky"].is_boolean(), "{test:#}");
    }
    let flaky: Vec<&Value> = tests.iter().filter(|test| test["flaky"] == true).collect();
    assert_eq!((flaky.len(), &flaky[0]["status"]), (1, &"passed".into()), "{tests:#?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn junit_is_well_nested_and_carries_the_counts() {
    let project = reported().await;
    let junit = fs::read_to_string(project.path().join("out/junit.xml")).unwrap();
    assert!(junit.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"), "{junit}");
    let tags = tags(&junit);
    let root = tags.iter().find(|tag| tag.starts_with("testsuites")).unwrap();
    let counts = ["tests", "failures", "errors", "skipped"].map(|name| attr(root, name));
    assert_eq!(counts, ["4", "1", "0", "1"].map(|count| Some(count.to_owned())), "{root}");
    let cases: Vec<&&str> = tags.iter().filter(|tag| tag.starts_with("testcase")).collect();
    assert_eq!(cases.len(), 4, "{junit}");
    assert!(cases.iter().all(|case| attr(case, "name").is_some() && attr(case, "classname").is_some()), "{junit}");
    for child in ["failure", "skipped", "system-out"] {
        assert_eq!(tags.iter().filter(|tag| tag.starts_with(child)).count(), 1, "one <{child}>: {junit}");
    }
}

/// The opening tags, in order, after checking that every tag closes in order.
fn tags(xml: &str) -> Vec<&str> {
    let mut open: Vec<&str> = Vec::new();
    let mut seen = Vec::new();
    for piece in xml.split('<').skip(1) {
        let (tag, _) = piece.split_once('>').expect("every tag closes with `>`");
        let name = |tag: &str| tag.split_whitespace().next().unwrap_or_default().trim_end_matches('/').to_owned();
        if tag.starts_with('?') {
            continue;
        }
        if let Some(closing) = tag.strip_prefix('/') {
            assert_eq!(open.pop().map(name).as_deref(), Some(closing), "mis-nested </{closing}>");
            continue;
        }
        seen.push(tag);
        if !tag.ends_with('/') {
            open.push(tag);
        }
    }
    assert!(open.is_empty(), "never closed: {open:?}");
    seen
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let start = tag.find(&format!(" {name}=\""))? + name.len() + 3;
    tag.get(start..)?.split_once('"').map(|(value, _)| value.to_owned())
}
