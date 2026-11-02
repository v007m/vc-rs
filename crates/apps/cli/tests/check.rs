use std::fs;
use std::path::Path;

use assert_cmd::{Command, cargo::cargo_bin_cmd};
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

const CONFIG: (&str, &[u8]) = ("vibranium.config.yaml", b"syntaxVersion: 2\n");
const USERS: (&str, &[u8]) = ("services/users/users.api.yaml", b"apis: []\n");
const BAD: (&str, &[u8]) = ("services/orders/orders.api.yaml", &[0xff, 0xfe]);

fn project(files: &[(&str, &[u8])]) -> std::io::Result<TempDir> {
    let dir = tempfile::tempdir()?;
    for (path, bytes) in files {
        let file = dir.path().join(path);
        if let Some(folder) = file.parent() {
            fs::create_dir_all(folder)?;
        }
        fs::write(file, bytes)?;
    }
    Ok(dir)
}

fn check(dir: &Path) -> Command {
    let mut vc = cargo_bin_cmd!("vc");
    vc.current_dir(dir).arg("check").env_remove("NO_COLOR");
    vc
}

#[test]
fn passes_a_clean_project_and_saves_the_compile_cache() {
    let dir = project(&[CONFIG, USERS]).unwrap();
    check(dir.path()).assert().success().stdout("0 errors · 0 warnings · 2 files checked\n").stderr("");
    assert!(dir.path().join(".vibranium/cache/compile.bin").is_file());
}

#[test]
fn finds_the_project_from_a_subfolder() {
    let dir = project(&[CONFIG, USERS]).unwrap();
    check(&dir.path().join("services/users")).assert().success().stdout(contains("2 files checked"));
}

#[test]
fn reports_an_unreadable_file_and_exits_2() {
    let dir = project(&[CONFIG, USERS, BAD]).unwrap();
    let expected = "error[V1001]: services/orders/orders.api.yaml: is not UTF-8 text\n \
                    --> services/orders/orders.api.yaml:1:1\n";
    check(dir.path())
        .assert()
        .code(2)
        .stdout(contains(expected).and(contains("= help: save the file as UTF-8")))
        .stdout(contains("1 error · 0 warnings · 3 files checked\n").and(contains("\u{1b}[").not()));
}

#[test]
fn prints_json_with_the_flag() {
    let dir = project(&[CONFIG, BAD]).unwrap();
    let output = check(dir.path()).arg("--json").assert().code(2).get_output().stdout.clone();
    let text = String::from_utf8(output).unwrap();
    assert!(text.starts_with('{') && text.ends_with("}\n"), "{text}");
    assert!(text.contains("\"code\": \"V1001\"") && text.contains("\"errors\": 1"), "{text}");
}

#[test]
fn reports_only_on_the_paths_given() {
    let dir = project(&[CONFIG, USERS, BAD]).unwrap();
    check(dir.path()).arg("services/users").assert().success().stdout("0 errors · 0 warnings · 1 file checked\n");
    check(&dir.path().join("services")).arg("orders").assert().code(2).stdout(contains("1 error"));
}

#[test]
fn refuses_a_path_with_no_definition_files() {
    let dir = project(&[CONFIG, USERS, ("docs/readme.md", b"")]).unwrap();
    check(dir.path()).arg("docs").assert().code(2).stderr(contains("error: docs: no definition file"));
    check(dir.path()).arg("missing").assert().code(2).stderr(contains("error: missing: not a file or folder"));
}

#[test]
fn refuses_to_run_outside_a_project() {
    let dir = project(&[USERS]).unwrap();
    check(dir.path()).assert().code(2).stdout("").stderr(contains("error: no vibranium.config.yaml in"));
}
