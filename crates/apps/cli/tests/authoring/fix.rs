//! The V4 naming lints are warnings (D180), and `vc check --fix` previews the certain fixes as
//! a diff and asks before applying them (D88): a convention rename with its references, a
//! typo with one close match, an expired skip removed. Declining changes nothing.

use crate::{CONFIG, CRUD, Project, USERS, checks_clean, said};

/// One file that breaks a naming convention, and the code `vc check` warns with.
const LINTS: [(&str, &str, &str); 5] = [
    ("services/users/crud.test.yaml", "tests:\n  - id: Lists_users\n    call: users#list\n", "V4001"),
    ("services/more/more.api.yaml", "apis:\n  - id: ListAll\n    path: /users\n", "V4002"),
    ("services/users/CRUD.test.yaml", "tests:\n  - id: listsUsers\n    call: users#list\n", "V4003"),
    (
        "services/users/tags.test.yaml",
        "tests:\n  - id: listsUsers\n    tags: [SlowNetwork]\n    call: users#list\n",
        "V4004",
    ),
    (
        "vibranium.config.yaml",
        "syntaxVersion: 2\nprofiles:\n  - id: Dev\n    default: true\nvars: {baseUrl: 'http://x'}\n",
        "V4004",
    ),
];

/// A test whose ID breaks the convention but whose name gives the right one.
const BAD_ID: (&str, &str) = (
    "services/users/crud.test.yaml",
    "tests:\n  - id: Lists_users  # keep me\n    name: Lists users\n    call: users#list\n",
);

/// `users.flows`, reusing the badly named test.
const FLOWS: (&str, &str) = (
    "services/users/flows.test.yaml",
    "tests:\n  - id: listsTwice\n    steps:\n      - test: users.crud#Lists_users\n",
);

#[test]
fn each_naming_convention_is_a_warning_and_check_still_passes() {
    for (path, text, code) in LINTS {
        let project = Project::new(&[CONFIG, USERS, (path, text)]);
        let output = project.vc(&["check"]).output().unwrap();
        let shown = said(&output);
        assert_eq!(output.status.code(), Some(0), "{code}: warnings never fail a check\n{shown}");
        assert!(shown.contains(&format!("warning[{code}]")), "{code}:\n{shown}");
        assert!(shown.contains("0 errors · 1 warning"), "{code}:\n{shown}");
    }
}

#[test]
fn the_v4001_warning_carries_the_rename_as_its_fix() {
    let project = Project::new(&[CONFIG, USERS, BAD_ID]);
    let output = project.vc(&["check"]).output().unwrap();
    let shown = said(&output);
    assert!(shown.contains("fix:") && shown.contains("listsUsers"), "{shown}");
}

#[test]
fn fix_previews_a_diff_then_renames_with_every_reference_when_confirmed() {
    let project = Project::new(&[CONFIG, USERS, BAD_ID, FLOWS]);
    let output = project.vc(&["check", "--fix"]).write_stdin("y\n").output().unwrap();
    let shown = said(&output);
    assert_eq!(output.status.code(), Some(0), "{shown}");
    assert!(shown.contains("-  - id: Lists_users") && shown.contains("+  - id: listsUsers"), "a diff:\n{shown}");
    assert_eq!(project.read(BAD_ID.0), BAD_ID.1.replace("Lists_users", "listsUsers"));
    assert_eq!(project.read(FLOWS.0), FLOWS.1.replace("Lists_users", "listsUsers"));
    checks_clean(&project);
}

#[test]
fn fix_changes_nothing_when_declined() {
    let project = Project::new(&[CONFIG, USERS, BAD_ID, FLOWS]);
    project.vc(&["check", "--fix"]).write_stdin("n\n").output().unwrap();
    assert_eq!((project.read(BAD_ID.0), project.read(FLOWS.0)), (BAD_ID.1.to_owned(), FLOWS.1.to_owned()));
}

#[test]
fn fix_removes_an_expired_skip() {
    let expired =
        CRUD.1.replace("    call: users#list\n", "    call: users#list\n    skip: {reason: old, until: 2020-01-01}\n");
    let project = Project::new(&[CONFIG, USERS, (CRUD.0, &expired)]);
    assert!(said(&project.vc(&["check"]).output().unwrap()).contains("warning[V3104]"));
    project.vc(&["check", "--fix"]).write_stdin("y\n").assert().code(0);
    assert_eq!(project.read(CRUD.0), CRUD.1);
}

#[test]
fn fix_corrects_a_typo_with_one_close_match() {
    let typo = CRUD.1.replace("call: users#list\n", "call: users#lst\n");
    let project = Project::new(&[CONFIG, USERS, (CRUD.0, &typo)]);
    project.vc(&["check"]).assert().code(2);
    project.vc(&["check", "--fix"]).write_stdin("y\n").assert().code(0);
    assert_eq!(project.read(CRUD.0), CRUD.1);
}

#[test]
fn fix_leaves_a_problem_with_two_answers_as_help() {
    let apis = ("services/things/things.api.yaml", "apis:\n  - id: getAb\n    path: /a\n  - id: getAc\n    path: /b\n");
    let test = ("services/things/it.test.yaml", "tests:\n  - id: getsIt\n    call: things#getA\n");
    let project = Project::new(&[CONFIG, apis, test]);
    project.vc(&["check", "--fix"]).write_stdin("y\n").assert().code(2);
    assert_eq!(project.read(test.0), test.1, "no certain fix, so nothing written");
}
