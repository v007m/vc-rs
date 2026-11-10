use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Value, json};
use vc_interfaces::{Artifact, ReportError};
use vc_testkit::reporter_suite;

use super::*;
use crate::testing::{self, Broken, Capture};

#[tokio::test]
async fn passes_the_reporter_suite() {
    let (capture, _captured) = Capture::new();
    reporter_suite(&mut || Box::new(JsonReporter::new(&testing::context(capture.clone())))).await;
}

#[tokio::test]
async fn prints_one_json_object_per_event_then_the_summary() {
    let (capture, captured) = Capture::new();
    let context = testing::context(capture);
    let mut reporter = JsonReporter::new(&context);
    let events = testing::script(&context.program(), context.plan());
    let summary = testing::summary().artifacts(vec![Artifact::new("ctrf", PathBuf::from("out/ctrf.json"))]);
    assert_eq!(testing::run(&mut reporter, &events, &summary).await, []);
    let (printed, saved) = captured.take();
    assert_eq!(saved, []);
    let lines: Vec<Value> = printed.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
    assert_eq!(lines.len(), events.len() + 1);
    let last = lines.last().unwrap();
    assert_eq!(last["artifacts"], json!([{"reporter": "ctrf", "path": "out/ctrf.json"}]));
    insta::assert_snapshot!(printed);
}

#[tokio::test]
async fn reports_a_destination_that_refuses_to_print() {
    let context = testing::context(Arc::new(Broken));
    let mut reporter = JsonReporter::new(&context);
    let refused = ReportError::Write { location: "console".into(), reason: "closed".into() };
    let started = vc_model::Event::RunStarted { files: 0, instances: 0 };
    assert_eq!(reporter.on_event(&started).await, Err(refused.clone()));
    assert_eq!(reporter.finish(&testing::summary()).await, Err(refused));
}

#[tokio::test]
async fn prints_a_warning_with_its_action() {
    let (capture, captured) = Capture::new();
    let mut reporter = JsonReporter::new(&testing::context(capture));
    let warning = vc_model::Failure::new("undeclared").with_code("V5108").at(vc_model::ActionId::new(0));
    let owner = vc_model::OwnerKey::Run;
    reporter.on_event(&vc_model::Event::Warned { action: vc_model::ActionId::new(0), owner, warning }).await.unwrap();
    let line: Value = serde_json::from_str(captured.take().0.trim_end()).unwrap();
    assert_eq!(line["event"], "warned");
    assert_eq!((&line["warning"]["code"], &line["warning"]["message"]), (&json!("V5108"), &json!("undeclared")));
    assert_eq!(line["action"], line["warning"]["action"]);
}

#[test]
fn merges_objects_and_keeps_anything_else() {
    assert_eq!(with(json!({"a": 1}), json!({"b": 2, "a": 3})), json!({"a": 3, "b": 2}));
    assert_eq!(with(json!([1]), json!({"b": 2})), json!([1]));
    assert_eq!(with(json!({"a": 1}), json!(2)), json!({"a": 1}));
}
