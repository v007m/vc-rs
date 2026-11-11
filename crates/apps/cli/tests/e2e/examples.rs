//! `examples: ${…}`: the rows a template gives are read once before the run, from the vars the
//! profiles and `--set` give and the datasets, and only for the tests the run selects; `--seed`
//! repeats a shuffle of them whatever else the run selects, rows that are not a list of maps are
//! V5111, a var a profile defines from a root read later is V2105 under that profile, and a test
//! given no rows is skipped with a warning.

use serde_json::Value;
use vc_mock_server::MockServer;

use crate::{Project, SCRIPTED, events, of_kind, said};

/// A `dev` profile taking two of the cities, and an `all` profile taking every one.
const CONFIG: (&str, &str) = (
    "vibranium.config.yaml",
    "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n  - id: all\n    vars: {rows: '${dataset.cities}'}\n\
     vars: {baseUrl: 'http://x', rows: '${dataset.cities | take(2)}'}\n",
);

const CITIES: (&str, &str) = (
    "datasets/cities.json",
    r#"[{"name": "Oslo"}, {"name": "Lima"}, {"name": "Pune"}, {"name": "Kyiv"}, {"name": "Doha"}]"#,
);

/// A test sending one request per row `examples` gives, which passes only for Oslo.
fn project(examples: &str) -> Project {
    let test = format!(
        "tests:\n  - id: visitsEachCity\n    examples: {examples}\n    call: scripted#echo\n\
         \x20   body: {{city: '${{example.name}}'}}\n    expect: {{body: {{match: {{body.city: Oslo}}}}}}\n"
    );
    Project::new(&[CONFIG, CITIES, SCRIPTED, ("services/scripted/cities.test.yaml", &test)])
}

/// The label and status of each finished test of a `--reporter json` run, sorted by label.
fn finished(stdout: &[u8]) -> Vec<(Value, Value)> {
    let events = events(stdout);
    let mut finished: Vec<(Value, Value)> = of_kind(&events, "testFinished")
        .iter()
        .map(|event| (event["label"].clone(), event["status"].clone()))
        .collect();
    finished.sort_by_key(|(label, _)| label.to_string());
    finished
}

#[tokio::test(flavor = "multi_thread")]
async fn the_profiles_choose_the_rows_before_the_run() {
    let server = MockServer::start().await.unwrap();
    let project = project("${var.rows}");
    let base = server.base_url();
    for (profile, rows) in [("dev", 2_usize), ("all", 5)] {
        server.reset().await.unwrap();
        let run = project.run(&base, &["--profile", profile, "--reporter", "json"]).output().unwrap();
        assert_eq!(finished(&run.stdout).len(), rows, "{profile}: {}", said(&run));
        assert_eq!(server.stats().await.unwrap().count("POST /echo"), u64::try_from(rows).unwrap(), "{profile}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_seed_repeats_the_shuffled_rows() {
    let server = MockServer::start().await.unwrap();
    let project = project("${dataset.cities | shuffle | take(3)}");
    let run = |seed: &str| {
        let output = project.run(&server.base_url(), &["--seed", seed, "--reporter", "json"]).output().unwrap();
        finished(&output.stdout)
    };
    let first = run("7");
    assert_eq!(first.len(), 3, "{first:?}");
    assert_eq!(run("7"), first, "the same seed draws the same rows in the same order");
    let seeds = ["1", "2", "3", "4", "5", "6"].map(run);
    assert!(seeds.iter().any(|other| *other != first), "another seed draws other rows: {seeds:?}");
}

/// The label, status and failure of each finished instance of `test` in a `--reporter json` run,
/// sorted by label: which row each instance had, since only Oslo passes and a failure names the city.
fn instances(stdout: &[u8], test: &str) -> Vec<(Value, Value, Value)> {
    let events = events(stdout);
    let of_test = of_kind(&events, "testFinished").into_iter().filter(|event| event["test"] == test);
    let mut found: Vec<_> =
        of_test.map(|event| (event["label"].clone(), event["status"].clone(), event["failure"].clone())).collect();
    found.sort_by_key(|(label, ..)| label.to_string());
    found
}

#[tokio::test(flavor = "multi_thread")]
async fn a_seed_gives_a_test_the_same_rows_whatever_else_the_run_selects() {
    let server = MockServer::start().await.unwrap();
    let shuffled = "examples: ${dataset.cities | shuffle | take(3)}\n    call: scripted#echo\n\
                    \x20   body: {city: '${example.name}'}\n    expect: {body: {match: {body.city: Oslo}}}\n";
    let tests = format!("tests:\n  - id: visitsEachCity\n    {shuffled}  - id: revisitsEachCity\n    {shuffled}");
    let project = Project::new(&[CONFIG, CITIES, SCRIPTED, ("services/scripted/cities.test.yaml", &tests)]);
    let second = "scripted.cities#revisitsEachCity";
    let run = |args: &[&str]| {
        let mut all = vec!["--seed", "7", "--reporter", "json"];
        all.extend(args);
        project.run(&server.base_url(), &all).output().unwrap()
    };
    let together = run(&[]);
    let rows = instances(&together.stdout, second);
    assert_eq!(rows.len(), 3, "{}", said(&together));
    let alone = run(&[second]);
    assert_eq!(instances(&alone.stdout, second), rows, "selected alone, it draws the same rows: {}", said(&alone));
    let rerun = run(&["--failed"]);
    let again = instances(&rerun.stdout, second);
    assert!(!again.is_empty(), "one of five cities passes, so some rows failed: {}", said(&rerun));
    assert!(again.iter().all(|row| rows.contains(row)), "a rerun row is the row that failed: {again:?} in {rows:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_profile_whose_rows_read_a_secret_refuses_the_run_only_under_that_profile() {
    let server = MockServer::start().await.unwrap();
    let config = (
        "vibranium.config.yaml",
        "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n  - id: staging\n\
         \x20   vars: {rows: '${secret.ROWS_TOKEN}'}\nvars: {baseUrl: 'http://x', rows: '${dataset.cities | take(2)}'}\n",
    );
    let tests = "tests:\n  - id: visitsEachCity\n    examples: ${var.rows}\n    call: scripted#echo\n\
                 \x20   body: {city: '${example.name}'}\n";
    let schema = (".env.schema", "# a token the rows must not read\nROWS_TOKEN=\n");
    let token = (".env", "ROWS_TOKEN=not-before-the-run\n");
    let project =
        Project::new(&[config, CITIES, SCRIPTED, ("services/scripted/cities.test.yaml", tests), schema, token]);
    let check = project.vc(&["check"]).output().unwrap();
    assert_eq!(check.status.code(), Some(0), "only a profile reads the secret, so it compiles: {}", said(&check));
    let staging = project.run(&server.base_url(), &["--profile", "staging"]).output().unwrap();
    assert_ne!(staging.status.code(), Some(0), "{}", said(&staging));
    assert!(said(&staging).contains("V2105"), "{}", said(&staging));
    assert!(said(&staging).contains("`var.rows` needs `secret`"), "{}", said(&staging));
    assert_eq!(server.stats().await.unwrap().count("POST /echo"), 0, "nothing ran");
    let dev = project.run(&server.base_url(), &["--profile", "dev"]).output().unwrap();
    assert_eq!(dev.status.code(), Some(0), "the other profile's rows render: {}", said(&dev));
    assert_eq!(server.stats().await.unwrap().count("POST /echo"), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn rows_a_set_var_makes_text_are_v5111() {
    let project = project("${var.rows}");
    let run = project.run("http://127.0.0.1:9", &["--set", "vars.rows=oops"]).output().unwrap();
    assert_ne!(run.status.code(), Some(0), "{}", said(&run));
    assert!(said(&run).contains("V5111") && said(&run).contains("list of maps"), "{}", said(&run));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_test_given_no_rows_is_skipped_with_a_warning() {
    let server = MockServer::start().await.unwrap();
    let project = project("${dataset.cities | take(0)}");
    let run = project.run(&server.base_url(), &["--reporter", "json"]).output().unwrap();
    let statuses: Vec<Value> = finished(&run.stdout).into_iter().map(|(_, status)| status).collect();
    assert_eq!(statuses, ["skipped"], "{}", said(&run));
    assert_eq!(server.stats().await.unwrap().count("POST /echo"), 0);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains("warning: `scripted.cities#visitsEachCity`: examples gave no rows"), "{stderr}");
    assert_eq!(run.status.code(), Some(0), "a skipped test exits 0, as any skip does: {}", said(&run));
}

#[tokio::test(flavor = "multi_thread")]
async fn only_the_selected_tests_rows_are_rendered() {
    let server = MockServer::start().await.unwrap();
    let tests = "tests:\n  - id: visitsEachCity\n    examples: ${dataset.cities | take(2)}\n    call: scripted#echo\n\
                 \x20   body: {city: '${example.name}'}\n  - id: readsTheRows\n    examples: ${var.rows}\n\
                 \x20   call: scripted#echo\n";
    let project = Project::new(&[CONFIG, CITIES, SCRIPTED, ("services/scripted/cities.test.yaml", tests)]);
    let selected = ["--set", "vars.rows=oops", "scripted.cities#visitsEachCity"];
    let run = project.run(&server.base_url(), &selected).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "the unselected test's rows are never rendered: {}", said(&run));
    assert_eq!(server.stats().await.unwrap().count("POST /echo"), 2);
    let all = project.run(&server.base_url(), &["--set", "vars.rows=oops"]).output().unwrap();
    assert!(said(&all).contains("V5111"), "selected, its rows are rendered and refused: {}", said(&all));
}
