//! `vc completion <shell>` writes a script that completes commands and flags and asks
//! `vc __complete` for the project's IDs, profiles and tags (D180). `vibranium` is `vc` under
//! its long name (D39), and each script completes both.

use assert_cmd::cargo::cargo_bin_cmd;

use crate::{CONFIG, Project, USERS, alone, said};

const CRUD: (&str, &str) =
    ("services/users/crud.test.yaml", "tests:\n  - id: listsUsers\n    tags: [smoke]\n    call: users#list\n");

#[test]
fn each_shell_gets_a_script_that_asks_for_the_ids() {
    let project = Project::new(&[CONFIG]);
    for shell in ["bash", "zsh", "fish", "powershell"] {
        let output = project.vc(&["completion", shell]).output().unwrap();
        assert_eq!(output.status.code(), Some(0), "{shell}: {}", said(&output));
        let script = String::from_utf8_lossy(&output.stdout);
        assert!(script.contains("__complete") && script.contains("explain"), "{shell}:\n{script}");
        assert!(script.contains("vibranium"), "{shell} completes the long name too:\n{script}");
    }
}

#[test]
fn each_script_registers_both_names() {
    let project = Project::new(&[CONFIG]);
    let script = |shell: &str| {
        String::from_utf8_lossy(&project.vc(&["completion", shell]).output().unwrap().stdout).into_owned()
    };
    let registers = [
        ("bash", "complete -F _vc_ids -o bashdefault -o default vc vibranium\n"),
        ("zsh", "compdef _vc_ids vc vibranium\n"),
        ("fish", "complete -c vibranium -w vc\n"),
        ("powershell", "-CommandName 'vc', 'vibranium'"),
    ];
    for (shell, line) in registers {
        assert!(script(shell).contains(line), "{shell} lacks `{line}`");
    }
    assert!(script("zsh").starts_with("#compdef vc vibranium\n"), "compinit reads both names from the first line");
    assert!(!script("bash").contains("$(vc __complete"), "the hook asks the name that was typed");
}

#[test]
fn vibranium_is_vc_under_its_long_name() {
    let project = Project::new(&[CONFIG, USERS]);
    let vibranium = |args: &[&str]| {
        let mut vibranium = cargo_bin_cmd!("vibranium");
        vibranium.current_dir(project.path()).env("HOME", project.home()).env("NO_COLOR", "1").args(args);
        vibranium
    };
    let version = project.vc(&["--version"]).output().unwrap().stdout;
    vibranium(&["--version"]).assert().code(0).stdout(String::from_utf8(version).unwrap());
    vibranium(&["__complete", "call", "users#l"]).assert().code(0).stdout("users#list\n");
    vibranium(&["completion", "tcsh"]).assert().code(2);
}

#[test]
fn vibranium_without_vc_says_how_to_install_both() {
    let (folder, vibranium) = alone("vibranium");
    let mut command = assert_cmd::Command::new(vibranium);
    command.env("PATH", folder.path()).arg("--version");
    let output = command.output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
    let text = said(&output);
    assert!(text.contains("error: `vibranium` runs `vc`, which is not next to it or on PATH"), "{text}");
    assert!(
        text.contains(
            "help: install both with `cargo install --git https://github.com/v007m/vc-rs vc-cli vc-demo --locked`"
        ),
        "{text}"
    );
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
