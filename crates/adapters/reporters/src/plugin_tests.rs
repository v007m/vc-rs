use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;

use serde_json::{Value, json};
use vc_interfaces::{Artifact, PluginHost, PluginKind, PluginManifest, Secrets};
use vc_testkit::{FakePlugin, reporter_suite, suite_plugin_manifest};

use super::*;
use crate::testing::{self, Capture};

fn manifest() -> PluginManifest {
    PluginManifest { id: "acmeReport".to_owned(), kind: PluginKind::Reporter, ..suite_plugin_manifest() }
}

/// A plugin that takes every event and answers `reporter/finish` with `files`.
fn plugin(files: Value) -> FakePlugin {
    FakePlugin::new(manifest(), &Secrets::new())
        .on("reporter/onEvent", |_, _| Ok(Value::Null))
        .on("reporter/finish", move |_, _| Ok(json!({"files": files.clone()})))
}

#[tokio::test]
async fn passes_the_reporter_suite() {
    let (capture, _captured) = Capture::new();
    let host: Arc<dyn PluginHost> = Arc::new(plugin(json!([{"name": "report.txt", "content": "ok"}])));
    reporter_suite(&mut || Box::new(PluginReporter::new(Arc::clone(&host), &testing::context(capture.clone())))).await;
}

#[tokio::test]
async fn sends_each_event_then_saves_the_files_the_plugin_returns() {
    let (sender, received) = mpsc::channel();
    let host = FakePlugin::new(manifest(), &Secrets::new())
        .on("reporter/onEvent", move |params, _| {
            sender.send(params["event"]["event"].clone()).unwrap();
            Ok(Value::Null)
        })
        .on("reporter/finish", |params, _| {
            let content = format!("status {}", params["summary"]["status"]);
            Ok(json!({"files": [{"name": "acme/report.txt", "content": content}]}))
        });
    let (capture, captured) = Capture::new();
    let context = testing::context(capture);
    let mut reporter = PluginReporter::new(Arc::new(host), &context);
    let events = testing::script(&context.program(), context.plan());
    let artifacts = testing::run(&mut reporter, &events, &testing::summary()).await;
    assert_eq!(artifacts, [Artifact::new("acmeReport", PathBuf::from("out/acme/report.txt"))]);
    assert_eq!(received.try_iter().count(), events.len());
    let (printed, saved) = captured.take();
    assert_eq!(printed, "");
    assert_eq!(saved.len(), 1);
    assert!(saved[0].1.starts_with("status "), "{saved:?}");
}

#[tokio::test]
async fn refuses_a_file_name_outside_the_report_folder() {
    for name in ["../x.txt", "/etc/x", "", "a/../../x"] {
        let (capture, captured) = Capture::new();
        let host = Arc::new(plugin(json!([{"name": name, "content": "x"}])));
        let mut reporter = PluginReporter::new(host, &testing::context(capture));
        let failed = reporter.finish(&testing::summary()).await.unwrap_err().to_string();
        assert!(
            failed.starts_with("cannot write the `acmeReport` report: ") && failed.contains("inside the report folder"),
            "{failed}"
        );
        assert_eq!(captured.take().1, []);
    }
}

#[tokio::test]
async fn a_failing_plugin_fails_only_its_report() {
    let (capture, _captured) = Capture::new();
    let host = Arc::new(FakePlugin::new(manifest(), &Secrets::new()));
    let context = testing::context(capture);
    let mut reporter = PluginReporter::new(host, &context);
    let events = testing::script(&context.program(), context.plan());
    reporter.on_event(&events[0]).await.unwrap();
    let failed = reporter.finish(&testing::summary()).await.unwrap_err().to_string();
    assert!(
        failed.starts_with("cannot write the `acmeReport` report: ") && failed.contains("reporter/onEvent"),
        "{failed}"
    );
    assert!(format!("{reporter:?}").contains("acmeReport"));
}

#[tokio::test]
async fn a_plugin_that_failed_an_event_gets_no_more() {
    let (capture, _captured) = Capture::new();
    let context = testing::context(capture);
    let mut reporter = PluginReporter::new(Arc::new(FakePlugin::new(manifest(), &Secrets::new())), &context);
    let events = testing::script(&context.program(), context.plan());
    let mut failed = None;
    for event in events.iter().cycle().take(QUEUE + 2) {
        if let Err(error) = reporter.on_event(event).await {
            failed = Some(error.to_string());
            break;
        }
    }
    let failed = failed.expect("a full queue behind a failed plugin fails the next event");
    assert!(failed.contains("reporter/onEvent"), "{failed}");
    assert_eq!(reporter.on_event(&events[0]).await.unwrap_err().to_string(), failed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_slow_plugin_does_not_hold_up_the_events_and_finish_waits_for_them() {
    let (sender, received) = mpsc::channel();
    let host = plugin(json!([])).on("reporter/onEvent", move |params, _| {
        std::thread::sleep(std::time::Duration::from_millis(50));
        sender.send(params["event"]["event"].clone()).unwrap();
        Ok(Value::Null)
    });
    let (capture, _captured) = Capture::new();
    let context = testing::context(capture);
    let mut reporter = PluginReporter::new(Arc::new(host), &context);
    let events = testing::script(&context.program(), context.plan());
    let started = std::time::Instant::now();
    for event in &events {
        reporter.on_event(event).await.unwrap();
    }
    assert!(started.elapsed() < std::time::Duration::from_millis(50) * u32::try_from(events.len()).unwrap());
    reporter.finish(&testing::summary()).await.unwrap();
    assert_eq!(received.try_iter().count(), events.len());
}

#[tokio::test]
async fn a_plugin_a_whole_queue_behind_is_dropped_at_once_with_one_reason() {
    let (capture, _captured) = Capture::new();
    let context = testing::context(capture);
    let host = plugin(json!([])).stalling("reporter/onEvent");
    let mut reporter = PluginReporter::new(Arc::new(host), &context);
    let events = testing::script(&context.program(), context.plan());
    let started = std::time::Instant::now();
    let mut taken = 0;
    let failed = loop {
        match reporter.on_event(&events[taken % events.len()]).await {
            Ok(()) => taken += 1,
            Err(error) => break error.to_string(),
        }
    };
    assert!(started.elapsed() < std::time::Duration::from_secs(5), "the run never waits on the plugin");
    assert_eq!(taken, QUEUE + 1, "the queue, plus the event the plugin holds");
    assert_eq!(failed, "cannot write the `acmeReport` report: it fell 1024 events behind the run");
    assert_eq!(reporter.on_event(&events[0]).await.unwrap_err().to_string(), failed);
    assert_eq!(reporter.finish(&testing::summary()).await.unwrap_err().to_string(), failed);
}
