//! `vc explain <code|id>` prints a code's page anywhere and a test or api resolved for the
//! active profile; `vc config explain <key>` shows a value and the layer that set it (D180).

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;

use crate::{CONFIG, Project, USERS, said};

/// `vc <args>` in an empty folder: no project above it.
fn outside(args: &[&str]) -> (Command, tempfile::TempDir) {
    let folder = tempfile::tempdir().unwrap();
    let mut vc = cargo_bin_cmd!("vc");
    vc.current_dir(folder.path()).args(args).env("NO_COLOR", "1").env("HOME", folder.path());
    (vc, folder)
}

#[test]
fn a_code_is_explained_outside_any_project() {
    let (mut vc, _folder) = outside(&["explain", "V2003"]);
    let output = vc.output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("V2003: Unknown api or test\n"), "{stdout}");
    assert!(stdout.contains("\nWrong\n") && stdout.contains("\nRight\n") && !stdout.contains("```"), "{stdout}");
}

#[test]
fn a_lowercase_code_is_the_same_code() {
    let (mut vc, _folder) = outside(&["explain", "v2101"]);
    let output = vc.output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("V2101: "), "{}", said(&output));
}

#[test]
fn an_unknown_code_exits_2() {
    let (mut vc, _folder) = outside(&["explain", "V9999"]);
    let output = vc.output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
    assert!(said(&output).contains("V9999"), "{}", said(&output));
}

#[test]
fn an_api_is_resolved_for_the_active_profile() {
    let project = Project::new(&[CONFIG, USERS]);
    let output = project.vc(&["explain", "users#list", "--set", "vars.baseUrl=http://api.test"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("profile  dev\n") && stdout.contains("GET http://api.test/users\n"), "{stdout}");
}

#[test]
fn an_unknown_id_names_the_closest() {
    let project = Project::new(&[CONFIG, USERS]);
    let output = project.vc(&["explain", "users#lst"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
    assert!(said(&output).contains("users#list"), "{}", said(&output));
}

#[test]
fn config_explain_names_the_layer_that_won() {
    let project = Project::new(&[CONFIG, USERS]);
    let output = project.vc(&["config", "explain", "vars.baseUrl", "--set", "vars.baseUrl=http://y"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("vars.baseUrl = http://y  (--set)\n"), "{stdout}");
    assert!(stdout.contains("over http://x  (vibranium.config.yaml)"), "{stdout}");
    project
        .vc(&["config", "explain", "defaults.timeout"])
        .assert()
        .code(0)
        .stdout("defaults.timeout = 30s  (built-in)\n");
}

#[test]
fn config_explain_refuses_an_unknown_key() {
    let project = Project::new(&[CONFIG]);
    let output = project.vc(&["config", "explain", "defaults.timout"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
    assert!(said(&output).contains("defaults.timeout"), "{}", said(&output));
}
