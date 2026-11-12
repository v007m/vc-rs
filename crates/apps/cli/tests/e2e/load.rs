//! `vc load` against the mock: a `p90` threshold on the scenario and on one mix entry decides the
//! exit code, 3 when breached, and the JSON report names the breached metric and test, with its
//! unit and limit. An error is counted under the call that failed, and `vc history` lists the run and shows what
//! it found.

use serde_json::{Value, json};
use vc_mock_server::MockServer;

use crate::{CONFIG, Project, SCRIPTED, USERS, said};

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
    let listed = report["breaches"].as_array();
    assert!(listed.is_some_and(Vec::is_empty), "the report lists no breaches: {report:#}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_scenarios_name_is_in_its_summary_and_json_report() {
    let (_, report) = load(&scenario("p90: {lt: 10s}", "p90: {lt: 10s}")).await;
    assert_eq!((&report["scenario"], &report["name"]), (&json!("scripted.peak"), &json!("Sleeping")), "{report:#}");
    let server = MockServer::start().await.unwrap();
    let peak = ("load/scripted/peak.load.yaml", scenario("p90: {lt: 10s}", "p90: {lt: 10s}"));
    let project = Project::new(&[CONFIG, SCRIPTED, SLEEPS, (peak.0, &peak.1)]);
    let base = format!("vars.baseUrl={}", server.base_url());
    let output = project.vc(&["load", "scripted.peak", "--set", &base]).output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("scripted.peak — Sleeping · finished after "), "{}", said(&output));
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

#[tokio::test(flavor = "multi_thread")]
async fn a_breach_shows_its_value_and_limit_in_the_metrics_unit() {
    let (_, report) = load(&scenario("p90: {lt: 20ms}", "p90: {lt: 10s}")).await;
    let breach = &report["breaches"][0];
    assert_eq!((&breach["unit"], &breach["limit"]), (&json!("ms"), &json!({"lt": 20.0})), "{report:#}");
    assert!(breach["actual"].as_f64().is_some_and(|ms| ms >= 80.0), "{report:#}");
    assert!(report["p90Ms"].as_f64().is_some_and(|ms| ms >= 80.0), "{report:#}");
}

/// `users.browse#addsThenLists` creates a user (cleaned up after) and then lists the users,
/// expecting a 500 the mock never sends.
const BROWSE: (&str, &str) = (
    "services/users/browse.test.yaml",
    "tests:\n  - id: addsThenLists\n    steps:\n      - call: users#create\n      - call: users#list\n\
     \x20       expect: {status: 500}\n",
);

/// A two-second scenario of [`BROWSE`] that evaluates every test's `expect`.
const BROWSING: (&str, &str) = (
    "load/users/peak.load.yaml",
    "name: Browsing\nmix:\n  - {test: users.browse#addsThenLists, weight: 1}\nstages:\n  - {duration: 2s, users: 1}\n\
     checks: full\nthresholds: {errorRate: {lt: 0.5}}\n",
);

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_check_on_the_second_call_is_counted_under_that_api_and_the_run_is_in_the_history() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG, USERS, BROWSE, BROWSING]);
    let base = format!("vars.baseUrl={}", server.base_url());
    let output = project.vc(&["load", "users.peak", "--set", &base]).output().unwrap();
    assert_eq!(output.status.code(), Some(3), "{}", said(&output));
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("\nbreached errorRate at 100% (lt 50%)\n"), "{text}");
    let heading = "\ntest users.browse#addsThenLists · ";
    assert!(text.contains(heading) && text.contains("\n  errors users#list 200 V5101 × "), "{text}");
    assert!(!text.contains("users#create"), "{text}");
    assert!(server.stats().await.unwrap().count("DELETE /users/{id}") > 0, "each created user is removed");

    let listed = project.vc(&["history"]).output().unwrap();
    let text = String::from_utf8_lossy(&listed.stdout);
    assert!(text.contains("load · ") && text.contains(" iterations · ") && text.contains("users.peak"), "{text}");
    let runs: Value = serde_json::from_slice(&project.vc(&["history", "--json"]).output().unwrap().stdout).unwrap();
    assert_eq!(runs[0]["kind"], "load", "{runs:#}");
    assert!(runs[0]["totals"]["failed"].as_u64().is_some_and(|failed| failed > 0), "{runs:#}");

    let shown = project.vc(&["history", "show", "1"]).output().unwrap();
    let text = String::from_utf8_lossy(&shown.stdout);
    let found = "\n  breached errorRate at 100% (lt 50%)\n  test users.browse#addsThenLists\n    errors users#list 200 V5101 × ";
    assert!(text.contains(found), "the history shows what the run found: {}", said(&shown));
    let shown = project.vc(&["history", "show", "1", "--json"]).output().unwrap();
    let stored: Value = serde_json::from_slice(&shown.stdout).unwrap_or_else(|_| panic!("{}", said(&shown)));
    let findings = stored["findings"].as_array().cloned().unwrap_or_default();
    let found: Vec<_> =
        findings.iter().map(|found| (found["kind"].clone(), found["metric"].clone(), found["api"].clone())).collect();
    assert_eq!(
        found,
        [(json!("breach"), json!("errorRate"), Value::Null), (json!("error"), Value::Null, json!("users#list"))]
    );
    assert_eq!(
        findings[1]["shown"].as_str().map(|shown| shown.starts_with("errors users#list 200 V5101 × ")),
        Some(true)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_report_of_a_load_run_says_how_to_get_one() {
    let server = MockServer::start().await.unwrap();
    let peak = ("load/scripted/peak.load.yaml", scenario("p90: {lt: 10s}", "p90: {lt: 10s}"));
    let project = Project::new(&[CONFIG, SCRIPTED, SLEEPS, (peak.0, &peak.1)]);
    let base = format!("vars.baseUrl={}", server.base_url());
    let loaded = project.vc(&["load", "scripted.peak", "--set", &base]).output().unwrap();
    assert_eq!(loaded.status.code(), Some(0), "{}", said(&loaded));

    let reported = project.vc(&["report"]).output().unwrap();
    assert_eq!(reported.status.code(), Some(2), "{}", said(&reported));
    let stderr = String::from_utf8_lossy(&reported.stderr);
    assert!(stderr.contains("is a load run, and `vc report` writes the files of test runs only"), "{stderr}");
    let help = "= help: `vc history` lists it; for a load report, add `--json` to the next `vc load scripted.peak`, \
                which sends the load again; to report a test run, pass its number from `vc history`\n";
    assert!(stderr.contains(help), "{stderr}");
    assert!(!stderr.contains("snapshot"), "{stderr}");
    assert!(reported.stdout.is_empty(), "no file is written: {}", said(&reported));
}
