//! `command:` actions through `sh -c` (D181): a non-zero exit fails the test with V5110, an
//! interpolated value stays one word, the child sees `.env` and the config vars, `repeat` runs
//! the command once per turn, and a line calling `vc` finds the running one when `PATH` has none.
#![cfg(unix)]

use std::fs;

use crate::{CONFIG, Project};

fn steps(lines: &str) -> (&'static str, String) {
    ("services/tools/shell.test.yaml", format!("tests:\n  - id: runsShell\n    steps:\n{lines}"))
}

#[test]
fn a_non_zero_exit_fails_the_test_with_v5110() {
    let (path, test) = steps("      - command: echo going down >&2; exit 3\n");
    let project = Project::new(&[CONFIG, (path, &test)]);
    let output = project.vc(&["run"]).assert().code(1).get_output().clone();
    let said = crate::said(&output);
    assert!(said.contains("V5110") && said.contains("exited with 3") && said.contains("going down"), "{said}");
}

#[test]
fn an_interpolated_value_is_one_shell_word() {
    let (path, test) = steps("      - command: echo ${var.word} > said.txt\n");
    let project = Project::new(&[CONFIG, (path, &test)]);
    project.vc(&["run", "--set", "vars.word=it's; touch pwned"]).assert().code(0);
    assert_eq!(fs::read_to_string(project.path().join("said.txt")).unwrap(), "it's; touch pwned\n");
    assert!(!project.path().join("pwned").exists(), "the value ran as a command");
}

#[test]
fn the_child_sees_dot_env_and_the_config_vars() {
    let (path, test) = steps("      - command: echo \"$FROM_FILE $VIB_VAR_word\" > env.txt\n");
    let project = Project::new(&[CONFIG, (path, &test)]);
    project.write(".env", "FROM_FILE=file\n");
    project.vc(&["run", "--set", "vars.word=set"]).assert().code(0);
    assert_eq!(fs::read_to_string(project.path().join("env.txt")).unwrap(), "file set\n");
}

#[test]
fn a_repeated_command_runs_once_per_turn() {
    let (path, test) =
        steps("      - command: echo ${iteration.index}/${iteration.count} >> turns.txt\n        repeat: 3\n");
    let project = Project::new(&[CONFIG, (path, &test)]);
    project.vc(&["run"]).assert().code(0);
    assert_eq!(fs::read_to_string(project.path().join("turns.txt")).unwrap(), "1/3\n2/3\n3/3\n");
}

#[test]
fn a_command_finds_the_running_vc_though_it_is_not_on_path() {
    let (path, test) = steps("      - command: vc --version > version.txt\n");
    let project = Project::new(&[CONFIG, (path, &test)]);
    let output = project.vc(&["run"]).env("PATH", "/usr/bin:/bin").output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", crate::said(&output));
    let version = fs::read_to_string(project.path().join("version.txt")).unwrap();
    assert!(version.starts_with("vc "), "{version}");
}

#[test]
fn a_vc_already_on_path_is_still_the_one_a_command_finds() {
    use std::os::unix::fs::PermissionsExt;
    let (path, test) = steps("      - command: vc --version > version.txt\n");
    let project = Project::new(&[CONFIG, (path, &test)]);
    let tools = tempfile::tempdir().unwrap();
    let other = tools.path().join("vc");
    fs::write(&other, "#!/bin/sh\necho the user's own vc\n").unwrap();
    fs::set_permissions(&other, fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:/usr/bin:/bin", tools.path().display());
    project.vc(&["run"]).env("PATH", path).assert().code(0);
    assert_eq!(fs::read_to_string(project.path().join("version.txt")).unwrap(), "the user's own vc\n");
}
