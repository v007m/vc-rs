//! `vc completion <shell>` writes a script that completes commands and flags and asks
//! `vc __complete` for the project's IDs, profiles and tags (D180).

use std::process::Stdio;

use assert_cmd::cargo::cargo_bin;

use crate::{CONFIG, Project, USERS, said};

const CRUD: (&str, &str) =
    ("services/users/crud.test.yaml", "tests:\n  - id: listsUsers\n    tags: [smoke]\n    call: users#list\n");

#[test]
fn each_shell_gets_a_script_that_asks_for_the_ids() {
    let project = Project::new(&[CONFIG]);
    for shell in ["bash", "zsh", "fish", "powershell"] {
        let output = project.vc(&["completion", shell]).output().unwrap();
        assert_eq!(output.status.code(), Some(0), "{shell}: {}", said(&output));
        let script = String::from_utf8_lossy(&output.stdout);
        assert!(script.contains("vc __complete") && script.contains("explain"), "{shell}:\n{script}");
    }
}

#[test]
fn an_unknown_shell_is_refused() {
    let output = Project::new(&[CONFIG]).vc(&["completion", "tcsh"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
}

#[test]
fn complete_offers_the_project_ids_profiles_and_tags() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    project.vc(&["__complete", "run", "users.crud#"]).assert().code(0).stdout("users.crud#listsUsers\n");
    project.vc(&["__complete", "call", "users#l"]).assert().code(0).stdout("users#list\n");
    project.vc(&["__complete", "--profile", ""]).assert().code(0).stdout("dev\n");
    project.vc(&["__complete", "-t", "sm"]).assert().code(0).stdout("smoke\n");
}

#[test]
fn complete_offers_nothing_outside_a_project() {
    let folder = tempfile::tempdir().unwrap();
    let mut vc = assert_cmd::cargo::cargo_bin_cmd!("vc");
    vc.current_dir(folder.path()).env("HOME", folder.path()).args(["__complete", "run", ""]);
    vc.assert().code(0).stdout("");
}

#[test]
fn complete_is_hidden_from_help() {
    let output = Project::new(&[CONFIG]).vc(&["--help"]).output().unwrap();
    assert!(!String::from_utf8_lossy(&output.stdout).contains("__complete"), "{}", said(&output));
}

#[test]
fn a_reader_that_has_gone_ends_the_output_quietly() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    for args in [&["completion", "bash"][..], &["list"], &["__complete", "run", ""]] {
        let (reader, writer) = std::io::pipe().unwrap();
        drop(reader);
        let mut vc = std::process::Command::new(cargo_bin!("vc"));
        vc.current_dir(project.path()).env("HOME", project.home()).args(args);
        let output = vc.stdout(Stdio::from(writer)).stderr(Stdio::piped()).output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(0), "{args:?}: {stderr}");
        assert!(stderr.is_empty(), "{args:?}: {stderr}");
    }
}
