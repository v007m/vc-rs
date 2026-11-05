//! `--changed` in a real git repository: a changed test file selects its tests, and a changed
//! api file selects the tests that call its apis.

use std::process::Command;

use predicates::str;

use crate::{CONFIG, ORDERS, Project, USERS};

const USER_TESTS: (&str, &str) =
    ("services/users/crud.test.yaml", "tests:\n  - id: listsUsers\n    call: users#list\n");
const ORDER_TESTS: (&str, &str) =
    ("services/orders/flow.test.yaml", "tests:\n  - id: listsOrders\n    call: orders#list\n");

fn git(project: &Project, args: &[&str]) {
    let mut git = Command::new("git");
    git.current_dir(project.path()).env("HOME", project.home()).env("GIT_CONFIG_NOSYSTEM", "1");
    git.args(["-c", "user.name=vc", "-c", "user.email=vc@example.com", "-c", "commit.gpgsign=false"]).args(args);
    let status = git.status().unwrap();
    assert!(status.success(), "git {args:?}");
}

fn committed() -> Project {
    let project = Project::new(&[CONFIG, USERS, ORDERS, USER_TESTS, ORDER_TESTS]);
    git(&project, &["init", "-q"]);
    git(&project, &["add", "-A"]);
    git(&project, &["commit", "-qm", "start"]);
    project
}

#[test]
fn nothing_changed_selects_nothing() {
    let project = committed();
    project.vc(&["list", "tests", "--changed"]).assert().code(0).stdout("");
    project.vc(&["run", "--changed"]).assert().code(2);
}

#[test]
fn a_changed_test_file_selects_its_tests() {
    let project = committed();
    let (path, text) = ORDER_TESTS;
    project.write(path, &format!("{text}  - id: listsOrdersAgain\n    call: orders#list\n"));
    project
        .vc(&["list", "tests", "--changed"])
        .assert()
        .code(0)
        .stdout("orders.flow#listsOrders\norders.flow#listsOrdersAgain\n");
    project.vc(&["list", "files", "--changed"]).assert().code(0).stdout("orders.flow\n");
}

#[test]
fn a_changed_api_file_selects_the_tests_that_call_it_and_a_base_ref_counts_commits() {
    let project = committed();
    let (path, text) = USERS;
    project.write(path, &text.replace("path: /users\n", "path: /users?page=1\n"));
    project.vc(&["list", "tests", "--changed"]).assert().code(0).stdout("users.crud#listsUsers\n");
    git(&project, &["commit", "-qam", "page"]);
    project.vc(&["list", "tests", "--changed"]).assert().code(0).stdout("");
    project
        .vc(&["list", "tests", "--changed", "HEAD~1"])
        .assert()
        .code(0)
        .stdout(str::contains("users.crud#listsUsers"));
}
