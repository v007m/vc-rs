//! `vc run`, `vc plan` and `vc list` against the in-process mock server.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::PredicateBooleanExt;
use predicates::str;
use tempfile::TempDir;
use vc_mock_server::MockServer;

const CONFIG: (&str, &str) = (
    "vibranium.config.yaml",
    "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\n",
);
const USERS: (&str, &str) = (
    "services/users/users.api.yaml",
    "baseUrl: ${var.baseUrl}\napis:\n  - id: list\n    name: List users\n    path: /users\n  - id: missing\n    path: /users/999\n",
);
const PASSING: (&str, &str) = (
    "services/users/crud.test.yaml",
    "tests:\n  - id: listsUsers\n    name: Lists users\n    call: users#list\n    expect: {status: 200}\n",
);
const FAILING: (&str, &str) = (
    "services/users/broken.test.yaml",
    "tests:\n  - id: readsMissingUser\n    call: users#missing\n    expect: {status: 200}\n",
);
/// A failing test whose vars are generated, so its seed is worth repeating.
const FAKED: (&str, &str) = (
    "services/users/broken.test.yaml",
    "tests:\n  - id: readsMissingUser\n    call: users#missing\n    vars: {email: '${fake.internet.email}'}\n    expect: {status: 200}\n",
);

fn project(files: &[(&str, &str)]) -> std::io::Result<TempDir> {
    let dir = tempfile::tempdir()?;
    for (path, text) in files {
        let file = dir.path().join(path);
        if let Some(folder) = file.parent() {
            fs::create_dir_all(folder)?;
        }
        fs::write(file, text)?;
    }
    Ok(dir)
}

fn vc(dir: &Path, args: &[&str]) -> Command {
    let mut vc = cargo_bin_cmd!("vc");
    vc.current_dir(dir).args(args).env("NO_COLOR", "1");
    vc
}

#[tokio::test(flavor = "multi_thread")]
async fn a_passing_run_exits_0_and_writes_ctrf_and_junit() {
    let server = MockServer::start().await.unwrap();
    let dir = project(&[CONFIG, USERS, PASSING]).unwrap();
    let base = format!("vars.baseUrl={}", server.base_url());
    vc(dir.path(), &["run", "--set", &base, "--report", "ctrf,junit", "--out", "out"])
        .assert()
        .code(0)
        .stdout(str::contains("users.crud#listsUsers"));
    let ctrf: serde_json::Value = serde_json::from_slice(&fs::read(dir.path().join("out/ctrf.json")).unwrap()).unwrap();
    assert_eq!(ctrf["results"]["summary"]["passed"], 1, "{ctrf:#}");
    let junit = fs::read_to_string(dir.path().join("out/junit.xml")).unwrap();
    assert!(junit.contains("listsUsers"), "{junit}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failing_run_that_generates_data_exits_1_and_prints_its_seed() {
    let server = MockServer::start().await.unwrap();
    let dir = project(&[CONFIG, USERS, PASSING, FAKED]).unwrap();
    let base = format!("vars.baseUrl={}", server.base_url());
    vc(dir.path(), &["run", "--set", &base, "--seed", "42", "--out", "out"])
        .assert()
        .code(1)
        .stderr(str::contains("seed 42: pass `--seed 42`"));
    assert!(dir.path().join("out/ctrf.json").is_file(), "ctrf is the default report");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failing_run_with_no_generated_data_prints_no_seed_hint() {
    let server = MockServer::start().await.unwrap();
    let dir = project(&[CONFIG, USERS, PASSING, FAILING]).unwrap();
    let base = format!("vars.baseUrl={}", server.base_url());
    let args = ["run", "--set", &base, "--seed", "42", "--report", "ctrf,junit", "--out", "out"];
    let run = vc(dir.path(), &args).assert().code(1);
    run.stderr(str::contains("--seed").not());
    let ctrf: serde_json::Value = serde_json::from_slice(&fs::read(dir.path().join("out/ctrf.json")).unwrap()).unwrap();
    assert_eq!(ctrf["results"]["extra"]["seed"], 42, "the report keeps the seed (D131): {ctrf:#}");
    let junit = fs::read_to_string(dir.path().join("out/junit.xml")).unwrap();
    assert!(junit.contains("<property name=\"seed\" value=\"42\"/>"), "{junit}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_json_reporter_prints_json_lines() {
    let server = MockServer::start().await.unwrap();
    let dir = project(&[CONFIG, USERS, PASSING]).unwrap();
    let base = format!("vars.baseUrl={}", server.base_url());
    let output = vc(dir.path(), &["run", "--set", &base, "--reporter", "json"]).assert().code(0).get_output().clone();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let first = stdout.lines().next().unwrap();
    assert!(serde_json::from_str::<serde_json::Value>(first).is_ok(), "{stdout}");
}

#[test]
fn a_selector_that_matches_nothing_exits_2() {
    let dir = project(&[CONFIG, USERS, PASSING]).unwrap();
    vc(dir.path(), &["run", "orders.flow"]).assert().code(2);
}

#[test]
fn an_unsupported_reporter_exits_2_with_help() {
    let dir = project(&[CONFIG, USERS, PASSING]).unwrap();
    vc(dir.path(), &["run", "--reporter", "dots"]).assert().code(2).stderr(str::contains("not supported yet"));
}

#[test]
fn plan_json_lists_the_instances() {
    let dir = project(&[CONFIG, USERS, PASSING, FAILING]).unwrap();
    let output = vc(dir.path(), &["plan", "--json", "--repeat", "2"]).assert().code(0).get_output().clone();
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let instances: usize =
        plan["files"].as_array().unwrap().iter().map(|file| file["instances"].as_array().unwrap().len()).sum();
    assert_eq!(instances, 4, "{plan:#}");
}

#[test]
fn plan_prints_the_tree() {
    let dir = project(&[CONFIG, USERS, PASSING]).unwrap();
    vc(dir.path(), &["plan"]).assert().code(0).stdout(str::starts_with("plan  ")).stdout(str::contains("listsUsers"));
}

#[test]
fn list_prints_tests_apis_and_files() {
    let dir = project(&[CONFIG, USERS, PASSING, FAILING]).unwrap();
    vc(dir.path(), &["list", "tests"])
        .assert()
        .code(0)
        .stdout("users.broken#readsMissingUser\nusers.crud#listsUsers  Lists users\n");
    let output = vc(dir.path(), &["list", "apis", "--json"]).assert().code(0).get_output().clone();
    let apis: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let ids: Vec<&str> = apis.as_array().unwrap().iter().map(|api| api["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["users#list", "users#missing"]);
    let output = vc(dir.path(), &["list", "files", "--json"]).assert().code(0).get_output().clone();
    let files: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(files.as_array().unwrap().len(), 3, "{files:#}");
}

#[test]
fn list_tests_takes_a_path_selector() {
    let dir = project(&[CONFIG, USERS, PASSING, FAILING]).unwrap();
    vc(dir.path(), &["list", "tests", "services/users/crud.test.yaml"])
        .assert()
        .code(0)
        .stdout("users.crud#listsUsers  Lists users\n");
}
