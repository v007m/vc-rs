//! `examples: ${…}`: the rows a template gives are read once before the run, from the vars the
//! profiles and `--set` give and the datasets; `--seed` repeats a shuffle of them, and rows that
//! are not a list of maps are V5111.

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
    let mut finished: Vec<(Value, Value)> =
        of_kind(&events, "testFinished").iter().map(|event| (event["label"].clone(), event["status"].clone())).collect();
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

#[tokio::test(flavor = "multi_thread")]
async fn rows_a_set_var_makes_text_are_v5111() {
    let project = project("${var.rows}");
    let run = project.run("http://127.0.0.1:9", &["--set", "vars.rows=oops"]).output().unwrap();
    assert_ne!(run.status.code(), Some(0), "{}", said(&run));
    assert!(said(&run).contains("V5111") && said(&run).contains("list of maps"), "{}", said(&run));
}
