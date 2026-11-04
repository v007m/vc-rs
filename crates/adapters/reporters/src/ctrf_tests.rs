use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;
use vc_interfaces::{Artifact, ReportError};
use vc_testkit::reporter_suite;

use super::*;
use crate::testing::{self, Broken, Capture};

/// The document the scripted run writes.
async fn written() -> Value {
    let (capture, captured) = Capture::new();
    let context = testing::context(capture);
    let mut reporter = CtrfReporter::new(&context);
    let events = testing::script(&context.program(), context.plan());
    let artifacts = testing::run(&mut reporter, &events, &testing::summary()).await;
    assert_eq!(artifacts, [Artifact::new("ctrf", PathBuf::from("out/ctrf.json"))]);
    let (printed, saved) = captured.take();
    assert!(printed.is_empty(), "the file reporter prints nothing");
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].0, "ctrf.json");
    serde_json::from_str(&saved[0].1).unwrap()
}

#[tokio::test]
async fn passes_the_reporter_suite() {
    let (capture, _captured) = Capture::new();
    reporter_suite(&mut || Box::new(CtrfReporter::new(&testing::context(capture.clone())))).await;
}

#[tokio::test]
async fn writes_the_scripted_run() {
    let document = written().await;
    insta::assert_snapshot!(serde_json::to_string_pretty(&document).unwrap());
}

/// The fields the CTRF schema (ctrf-io/ctrf, `schema/ctrf.schema.json`, spec 0.2.0) requires.
#[tokio::test]
async fn has_every_field_ctrf_requires() {
    let document = written().await;
    assert_eq!(document["reportFormat"], "CTRF");
    assert_eq!(document["specVersion"], "0.2.0", "the ctrf-io/ctrf spec and schema version");
    let results = &document["results"];
    assert!(results["tool"]["name"].is_string());
    let summary = &results["summary"];
    for key in ["tests", "passed", "failed", "pending", "skipped", "other", "start", "stop"] {
        assert!(summary[key].is_u64(), "summary.{key} is a count: {summary}");
    }
    let tests = results["tests"].as_array().unwrap();
    assert_eq!(summary["tests"].as_u64(), Some(6));
    assert_eq!(tests.len(), 6);
    for test in tests {
        assert!(test["name"].is_string(), "{test}");
        let status = test["status"].as_str().unwrap();
        assert!(["passed", "failed", "skipped", "pending", "other"].contains(&status), "{test}");
        assert!(test["duration"].is_u64(), "{test}");
        assert!(test["extra"].is_object(), "{test}");
    }
    let counted = |status: &str| tests.iter().filter(|t| t["status"] == status).count();
    assert_eq!(summary["passed"].as_u64(), Some(u64::try_from(counted("passed")).unwrap()));
    assert_eq!(summary["failed"].as_u64(), Some(u64::try_from(counted("failed")).unwrap()));
    assert_eq!(summary["skipped"].as_u64(), Some(u64::try_from(counted("skipped")).unwrap()));
}

#[tokio::test]
async fn keeps_vibranium_statuses_and_the_blocking_action() {
    let document = written().await;
    let tests = document["results"]["tests"].as_array().unwrap();
    let raw: Vec<&str> = tests.iter().map(|t| t["rawStatus"].as_str().unwrap()).collect();
    assert_eq!(raw, ["passed", "failed", "blocked", "flaky", "passed", "skipped"]);
    assert_eq!(tests[2]["extra"]["blockedBy"], "seed");
    assert_eq!(tests[1]["extra"]["blockedBy"], Value::Null);
    assert_eq!(tests[3]["flaky"], true);
    assert_eq!(tests[3]["retries"], 1);
    assert_eq!(tests[5]["message"], "rewrite pending");
    assert!(tests[0].get("message").is_none());
    assert_eq!(tests[2]["start"], 0, "a test that never ran has no start");
}

#[tokio::test]
async fn reports_a_file_it_cannot_save() {
    let mut reporter = CtrfReporter::new(&testing::context(Arc::new(Broken)));
    let refused = ReportError::Write { location: "ctrf.json".into(), reason: "read-only".into() };
    assert_eq!(reporter.finish(&testing::summary()).await, Err(refused));
}
