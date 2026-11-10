use std::sync::Arc;

use serde_json::json;
use vc_interfaces::HistoryReader;
use vc_model::AlertTrigger;
use vc_testkit::{FakeNotifiers, FakeTransport, FakeVcs, MemoryEnvironment, MemoryHistory, MemorySources, status};

use crate::sample::builder;
use crate::{AlertSent, Engine, ExplainRequest};

const CONFIG: &str = "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n  - id: prod\n";
const SCHEMA: (&str, &[u8]) = (".env.schema", b"# @sensitive\nHOOK=\n");
const HOOK: &str = "https://hooks.example/s3cr3t";
const ALERTS: &str = concat!(
    "alerts:\n",
    "  - on: [recovery]\n    tags: [smoke]\n    notify:\n      - webhook: {url: '${secret.HOOK}'}\n",
    "  - profiles: [prod]\n    notify:\n      - slack: {webhook: x}\n      - teams: {webhook: x}\n",
);

/// The project with `alerts`, sending to `notifiers`; its history records every run.
fn project(alerts: &str, notifiers: FakeNotifiers) -> (Engine, Arc<MemoryHistory>) {
    let config = format!("{CONFIG}{alerts}");
    let sources = MemorySources::new(&[("vibranium.config.yaml", config.as_bytes()), SCHEMA]);
    let history = Arc::new(MemoryHistory::new());
    let engine = builder(FakeVcs::default())
        .sources(Arc::new(sources))
        .environment(Arc::new(MemoryEnvironment::new(&[("HOOK", HOOK)])))
        .history(history.clone(), history.clone())
        .notifiers(Arc::new(notifiers))
        .transport(Arc::new(FakeTransport::new(|_, _| Ok(status(200)))))
        .build()
        .unwrap();
    (engine, history)
}

fn profiles(profiles: &[&str]) -> ExplainRequest {
    ExplainRequest { profiles: profiles.iter().map(|profile| (*profile).to_owned()).collect(), sets: Vec::new() }
}

fn shown(sent: &[AlertSent]) -> Vec<(usize, &str, bool)> {
    sent.iter().map(|sent| (sent.rule, sent.channel.as_str(), sent.error.is_none())).collect()
}

#[tokio::test]
async fn the_sample_goes_through_every_channel_of_each_rule_the_profiles_match_ignoring_on_and_tags() {
    let (notifiers, log) = FakeNotifiers::new(&["webhook", "slack", "teams"]);
    let (engine, history) = project(ALERTS, notifiers.failing("teams"));
    let sent = engine.test_alerts(&profiles(&["prod"])).await.unwrap();
    assert_eq!(shown(&sent), [(1, "webhook", true), (2, "slack", true), (2, "teams", false)]);
    let error = sent[2].error.as_deref().unwrap_or_default();
    assert!(error.contains("teams") && !error.contains("s3cr3t"), "{error}");
    let notices = log.take();
    assert_eq!(notices[0].settings, json!({"url": HOOK}), "secrets resolve as in a run");
    let notice = &notices[0].notice;
    assert!(notice.test);
    assert_eq!(
        (notice.trigger, notice.summary.totals.passed, notice.summary.totals.failed),
        (AlertTrigger::Failure, 1, 1)
    );
    assert_eq!(notice.failures[0].test, "project#sampleTest");
    assert_eq!(notice.failures[0].message, "this is a test alert");
    assert_eq!(notice.summary.profiles, ["prod"]);
    assert!(history.runs(10).unwrap().is_empty(), "nothing is recorded");
}

#[tokio::test]
async fn a_rule_for_other_profiles_is_left_out_and_no_match_sends_nothing() {
    let (notifiers, log) = FakeNotifiers::new(&["webhook", "slack", "teams"]);
    let (engine, _) = project(ALERTS, notifiers.clone());
    let sent = engine.test_alerts(&profiles(&[])).await.unwrap();
    assert_eq!(shown(&sent), [(1, "webhook", true)], "the default profile is dev");
    let prod_only = ALERTS.replace("  - on: [recovery]\n    tags: [smoke]\n", "  - profiles: [prod]\n");
    let (engine, _) = project(&prod_only, notifiers);
    assert_eq!(engine.test_alerts(&profiles(&["dev"])).await.unwrap(), []);
    assert_eq!(log.take().len(), 1, "only the first sample was sent");
}
