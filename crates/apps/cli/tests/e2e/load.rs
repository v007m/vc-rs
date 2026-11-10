//! `vc load` against the mock: a `p90` threshold on the scenario and on one mix entry decides the
//! exit code, 3 when breached, and the JSON report names the breached metric and test.

use serde_json::Value;
use vc_mock_server::MockServer;

use crate::{CONFIG, Project, SCRIPTED, said};

/// `scripted.sleep#sleeps` sleeps 80 ms a call.
const SLEEPS: (&str, &str) =
    ("services/scripted/sleep.test.yaml", "tests:\n  - id: sleeps\n    call: scripted#slow\n    vars: {ms: 80}\n");

/// A two-second scenario of [`SLEEPS`] whose thresholds are `thresholds` and whose mix entry's
/// are `mix`.
fn scenario(thresholds: &str, mix: &str) -> String {
    format!(
        "name: Sleeping\nmix:\n  - test: scripted.sleep#sleeps\n    weight: 1\n    thresholds: {{{mix}}}\n\
         stages:\n  - {{duration: 2s, users: 2}}\nthresholds: {{{thresholds}}}\n"
    )
}

/// `vc load scripted.peak --json` with `scenario`: its exit code and JSON report.
async fn load(scenario: &str) -> (Option<i32>, Value) {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG, SCRIPTED, SLEEPS, ("load/scripted/peak.load.yaml", scenario)]);
    let base = format!("vars.baseUrl={}", server.base_url());
    let output = project.vc(&["load", "scripted.peak", "--set", &base, "--json"]).output().unwrap();
    let report = serde_json::from_slice(&output.stdout).unwrap_or_else(|_| panic!("{}", said(&output)));
    assert!(server.stats().await.unwrap().count("GET /slow") > 0, "the scenario ran");
    (output.status.code(), report)
}

/// The `(metric, test)` of each breach in `report`.
fn breaches(report: &Value) -> Vec<(String, Value)> {
    let found = report["breaches"].as_array().cloned().unwrap_or_default();
    found
        .iter()
        .map(|breach| (breach["metric"].as_str().unwrap_or_default().to_owned(), breach["test"].clone()))
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_p90_within_its_threshold_passes() {
    let (code, report) = load(&scenario("p90: {lt: 10s}", "p90: {lt: 10s}")).await;
    assert_eq!(code, Some(0), "{report:#}");
    assert!(breaches(&report).is_empty(), "{report:#}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_p90_over_the_scenarios_threshold_exits_3_and_names_the_metric() {
    let (code, report) = load(&scenario("p90: {lt: 20ms}", "p90: {lt: 10s}")).await;
    assert_eq!(code, Some(3), "every call sleeps 80 ms: {report:#}");
    assert_eq!(breaches(&report), [("p90".to_owned(), Value::Null)], "{report:#}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_p90_over_a_mix_entrys_threshold_names_its_test() {
    let (code, report) = load(&scenario("p90: {lt: 10s}", "p90: {lt: 20ms}")).await;
    assert_eq!(code, Some(3), "{report:#}");
    let breached = breaches(&report);
    assert_eq!(breached.len(), 1, "{report:#}");
    assert_eq!(breached[0].0, "p90", "{report:#}");
    assert!(breached[0].1.as_str().is_some_and(|test| test.contains("sleeps")), "{report:#}");
}
