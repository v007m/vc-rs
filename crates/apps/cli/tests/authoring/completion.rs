//! `vc completion <shell>` prints a completion script from `clap_complete` that asks the hidden
//! `vc __complete` for the project's own IDs (D180).

use crate::{CONFIG, CRUD, Project, USERS, said};

#[test]
fn prints_a_script_for_each_shell_that_completes_through_vc() {
    let project = Project::empty();
    for shell in ["bash", "zsh", "fish", "powershell"] {
        let output = project.vc(&["completion", shell]).output().unwrap();
        let script = String::from_utf8_lossy(&output.stdout);
        assert_eq!(output.status.code(), Some(0), "{shell}: {}", said(&output));
        assert!(script.contains("__complete"), "{shell} asks vc for IDs:\n{script}");
    }
    let zsh = project.vc(&["completion", "zsh"]).output().unwrap();
    assert!(String::from_utf8_lossy(&zsh.stdout).starts_with("#compdef vc"));
}

/// The lines of each shell's hook, the only ones that name `__complete`: they ask `vc` for IDs.
const HOOK_CALLS: [&str; 4] = ["__complete \"$kind\"", "__complete $kind", "__complete (commandline", "__complete --"];

#[test]
fn never_offers_the_hidden_command_on_tab() {
    let project = Project::empty();
    for shell in ["bash", "zsh", "fish", "powershell"] {
        let script = String::from_utf8_lossy(&project.vc(&["completion", shell]).output().unwrap().stdout).into_owned();
        let offered = script.lines().filter(|line| line.contains("__complete"));
        let listed: Vec<&str> = offered.filter(|line| !HOOK_CALLS.iter().any(|call| line.contains(call))).collect();
        assert!(listed.is_empty(), "{shell} offers __complete: {listed:?}");
        assert!(script.contains("completion"), "{shell} still offers the commands");
    }
}

#[test]
fn refuses_an_unknown_shell() {
    let output = Project::empty().vc(&["completion", "tcsh"]).output().unwrap();
    assert!(!output.status.success());
}

#[test]
fn completes_file_and_test_ids_from_the_project() {
    let orders = ("services/orders/orders.api.yaml", "apis:\n  - id: list\n    path: /orders\n");
    let project = Project::new(&[CONFIG, USERS, CRUD, orders]);
    let files =
        String::from_utf8_lossy(&project.vc(&["__complete", "run", "users.c"]).output().unwrap().stdout).into_owned();
    assert!(files.lines().any(|line| line.starts_with("users.crud")), "{files}");
    assert!(!files.contains("orders"), "{files}");
    let tests = project.vc(&["__complete", "run", "users.crud#"]).output().unwrap();
    let tests = String::from_utf8_lossy(&tests.stdout);
    assert!(tests.contains("users.crud#listsUsers") && tests.contains("users.crud#createsUser"), "{tests}");
}

#[test]
fn completes_api_ids_for_call() {
    let project = Project::new(&[CONFIG, USERS]);
    let apis = project.vc(&["__complete", "call", "users#"]).output().unwrap();
    let apis = String::from_utf8_lossy(&apis.stdout);
    for id in ["users#list", "users#create", "users#remove"] {
        assert!(apis.contains(id), "{id}: {apis}");
    }
}

#[test]
fn the_hidden_command_is_not_in_the_help() {
    let help = Project::empty().vc(&["--help"]).output().unwrap();
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(help.contains("completion") && !help.contains("__complete"), "{help}");
}
