//! The global `--project <dir>` flag.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use predicates::str;
use tempfile::TempDir;

const CONFIG: (&str, &str) = ("vibranium.config.yaml", "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n");
const USERS: (&str, &str) = ("services/users/users.api.yaml", "apis:\n  - id: list\n    path: /users\n");
const CRUD: (&str, &str) = ("services/users/crud.test.yaml", "tests:\n  - id: listsUsers\n    call: users#list\n");

fn write(root: &Path, files: &[(&str, &str)]) -> std::io::Result<()> {
    for (path, text) in files {
        let file = root.join(path);
        if let Some(folder) = file.parent() {
            fs::create_dir_all(folder)?;
        }
        fs::write(file, text)?;
    }
    Ok(())
}

fn project() -> std::io::Result<TempDir> {
    let dir = tempfile::tempdir()?;
    write(&dir.path().join("app"), &[CONFIG, USERS, CRUD])?;
    Ok(dir)
}

fn vc(cwd: &Path, args: &[&str]) -> Command {
    let mut vc = cargo_bin_cmd!("vc");
    vc.current_dir(cwd).args(args).env("NO_COLOR", "1");
    vc
}

#[test]
fn works_on_a_project_from_another_folder_before_or_after_the_command() {
    let dir = project().unwrap();
    vc(dir.path(), &["--project", "app", "check"]).assert().code(0).stdout(str::contains("3 files checked"));
    vc(dir.path(), &["check", "--project", "app"]).assert().code(0).stdout(str::contains("3 files checked"));
    vc(dir.path(), &["list", "tests", "--project", "app"]).assert().code(0).stdout("users.crud#listsUsers\n");
    vc(dir.path(), &["plan", "--project", "app/services"]).assert().code(0).stdout(str::contains("listsUsers"));
}

#[test]
fn path_selectors_start_at_the_project_root() {
    let dir = project().unwrap();
    vc(dir.path(), &["--project", "app", "list", "tests", "services/users"])
        .assert()
        .code(0)
        .stdout("users.crud#listsUsers\n");
}

#[test]
fn a_missing_folder_exits_2_with_help() {
    let dir = project().unwrap();
    vc(dir.path(), &["--project", "nowhere", "check"])
        .assert()
        .code(2)
        .stderr(str::contains("--project nowhere: no such folder"))
        .stderr(str::contains("help: pass the folder that holds `vibranium.config.yaml`"));
}

#[test]
fn a_folder_without_a_config_exits_2() {
    let dir = project().unwrap();
    fs::create_dir_all(dir.path().join("empty")).unwrap();
    vc(&dir.path().join("empty"), &["--project", ".", "list", "files"]).assert().code(2);
}

#[test]
fn schema_export_writes_under_the_project_root() {
    let dir = project().unwrap();
    vc(dir.path(), &["--project", "app", "schema", "export", "--out", "schemas"]).assert().code(0);
    assert!(dir.path().join("app/schemas").is_dir());
}

/// A project inside a folder the enclosing repository ignores is still discovered in full
/// (D133: discovery respects `.gitignore`, and the walker reads the parents' ignore files).
#[test]
fn finds_every_file_of_a_project_inside_a_git_ignored_folder() {
    let repo = tempfile::tempdir().unwrap();
    let git = std::process::Command::new("git").arg("init").arg("-q").current_dir(repo.path()).status().unwrap();
    assert!(git.success());
    fs::write(repo.path().join(".gitignore"), "build/\n").unwrap();
    let generated = vc_testkit::generate_project(7, 20);
    let root = repo.path().join("build/gen");
    for file in generated.files() {
        let path = root.join(&file.path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, &file.text).unwrap();
    }
    vc(repo.path(), &["--project", "build/gen", "check"]).assert().code(0).stdout(str::contains("20 files checked"));
    let output =
        vc(repo.path(), &["--project", "build/gen", "list", "files", "--json"]).assert().code(0).get_output().clone();
    let files: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(files.as_array().unwrap().len(), 19, "every definition file; the config has no file ID: {files:#}");
}
