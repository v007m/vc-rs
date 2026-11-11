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
        ("powershell", "-CommandName 'vc', 'vibranium'"),
    ];
    for (shell, line) in registers {
        assert!(script(shell).contains(line), "{shell} lacks `{line}`");
    }
    assert!(script("zsh").starts_with("#compdef vc vibranium\n"), "compinit reads both names from the first line");
    assert!(!script("bash").contains("$(vc __complete"), "the hook asks the name that was typed");
    let tilde = "    [[ $typed == \"~/\"* ]] && typed=\"$HOME/${typed#\\~/}\"\n";
    assert!(script("bash").contains(tilde), "bash expands a typed `~/`");
}

/// fish completes a name only by its own lines; `vibranium`'s ask `vibranium`, never the `vc`
/// first on `PATH`.
#[test]
fn the_fish_script_gives_each_name_its_own_lines() {
    let output = Project::new(&[CONFIG]).vc(&["completion", "fish"]).output().unwrap();
    let script = String::from_utf8_lossy(&output.stdout).into_owned();
    let hooks = [
        "complete -c vc -l profile -x -a '(command (commandline -opc)[1] __complete --profile (commandline -ct))'",
        "complete -c vc -s t -l tag -x -a '(command (commandline -opc)[1] __complete --tag (commandline -ct))'",
        "complete -c vibranium -l profile -x -a '(command vibranium __complete --profile (commandline -ct))'",
        "complete -c vibranium -s t -l tag -x -a '(command vibranium __complete --tag (commandline -ct))'",
        concat!(
            "complete -c vibranium -f -n '__fish_seen_subcommand_from run watch plan list explain call skip unskip ",
            "rename' -a '(command vibranium __complete (commandline -opc)[2] (commandline -ct))'"
        ),
    ];
    for hook in hooks {
        assert!(script.lines().any(|line| line == hook), "no `{hook}`:\n{script}");
    }
    assert!(!script.contains(" -w vc"), "no wrap, which would ask `vc`:\n{script}");
    let count = |name: &str| script.lines().filter(|line| line.starts_with(&format!("complete -c {name} "))).count();
    assert_eq!(count("vibranium"), count("vc"), "the same commands and flags for both names");
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

/// Even with a `vc` on `PATH`, `vibranium` runs only the one next to it (D39).
#[test]
fn vibranium_without_vc_says_how_to_install_both() {
    let (folder, vibranium) = alone("vibranium");
    let on_path = std::path::Path::new(env!("CARGO_BIN_EXE_vc")).parent().unwrap();
    let mut command = assert_cmd::Command::new(vibranium);
    command.env("PATH", on_path).arg("--version");
    let output = command.output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
    let text = said(&output);
    let folder = std::fs::canonicalize(folder.path()).unwrap();
    let missing = format!("there is no `vc{}` in {}", std::env::consts::EXE_SUFFIX, folder.display());
    assert!(text.contains("error: `vibranium` runs the `vc` next to it, and cannot: "), "{text}");
    assert!(text.contains(&missing), "{text}");
    assert!(
        text.contains(
            "help: install both with `cargo install --git https://github.com/v007m/vc-rs vc-cli vc-demo --locked`"
        ),
        "{text}"
    );
}

/// A link to `vibranium` runs the `vc` beside the file it points to, never a `vc` beside the link.
#[cfg(unix)]
#[test]
fn a_link_to_vibranium_runs_the_vc_beside_its_real_file() {
    use std::os::unix::fs::PermissionsExt;
    let folder = tempfile::tempdir().unwrap();
    let decoy = folder.path().join("vc");
    std::fs::write(&decoy, "#!/bin/sh\necho decoy\n").unwrap();
    std::fs::set_permissions(&decoy, std::fs::Permissions::from_mode(0o755)).unwrap();
    let link = folder.path().join("vibranium");
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_vibranium"), &link).unwrap();
    let version = assert_cmd::Command::new(env!("CARGO_BIN_EXE_vc")).arg("--version").output().unwrap().stdout;
    let output = assert_cmd::Command::new(&link).arg("--version").output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert_eq!(String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&version), "{}", said(&output));
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
