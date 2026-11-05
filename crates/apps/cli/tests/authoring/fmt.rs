//! `vc fmt` rewrites files in the house style: block YAML with a 2-space indent, keys in the
//! order of the shape definitions (D180), templates unquoted unless they need quotes. It keeps
//! comments, and formatting a formatted file changes nothing.

use crate::{CONFIG, CRUD, Project, USERS, checks_clean};

const MESSY: (&str, &str) = (
    "services/users/users.api.yaml",
    concat!(
        "# the users service\n",
        "baseUrl: ${var.baseUrl}\n",
        "apis:\n",
        "  # one user\n",
        "  - path: '/users/${var.userId}'   # by id\n",
        "    vars: {userId: 0}\n",
        "    id: get\n",
        "    name: 'Gets a user'\n",
        "    expect: {status: 200}\n",
    ),
);

const TIDY: &str = concat!(
    "# the users service\n",
    "baseUrl: ${var.baseUrl}\n",
    "apis:\n",
    "  # one user\n",
    "  - id: get\n",
    "    name: Gets a user\n",
    "    path: /users/${var.userId}   # by id\n",
    "    vars:\n",
    "      userId: 0\n",
    "    expect:\n",
    "      status: 200\n",
);

#[test]
fn rewrites_a_file_in_the_house_style_and_keeps_its_comments() {
    let project = Project::new(&[CONFIG, MESSY]);
    project.vc(&["fmt"]).assert().code(0);
    assert_eq!(project.read(MESSY.0), TIDY);
    checks_clean(&project);
}

#[test]
fn formatting_a_formatted_file_changes_nothing() {
    let project = Project::new(&[CONFIG, (MESSY.0, TIDY)]);
    project.vc(&["fmt"]).assert().code(0);
    assert_eq!(project.read(MESSY.0), TIDY);
    project.vc(&["fmt"]).assert().code(0);
    assert_eq!(project.read(MESSY.0), TIDY);
}

#[test]
fn formats_only_the_paths_given() {
    let swapped = ("    name: Lists users\n    call: users#list\n", "    call: users#list\n    name: Lists users\n");
    let messy_test = CRUD.1.replace(swapped.0, swapped.1);
    let project = Project::new(&[CONFIG, MESSY, (CRUD.0, &messy_test)]);
    project.vc(&["fmt", "services/users/crud.test.yaml"]).assert().code(0);
    assert_eq!(project.read(CRUD.0), CRUD.1, "name before call, as in the test shape");
    assert_eq!(project.read(MESSY.0), MESSY.1, "a file not named is untouched");
}

#[test]
fn strip_comments_removes_the_generated_ones_and_keeps_yours() {
    let project = Project::new(&[CONFIG, USERS]);
    let new = ["new", "api", "users#deactivate", "--method", "POST", "--path", "/users/{id}/deactivate"];
    project.vc(&new).assert().code(0);
    project.vc(&["fmt", "--strip-comments"]).assert().code(0);
    let text = project.read(USERS.0);
    assert!(!text.contains("# users#deactivate ·"), "the generated comment is gone:\n{text}");
    assert!(text.contains("# the users service\n") && text.contains("# makes one\n"), "yours stay:\n{text}");
}

#[test]
fn refuses_a_file_that_does_not_parse_and_leaves_it_alone() {
    let broken = "apis:\n  - id: list\n   path: /users\n";
    let project = Project::new(&[CONFIG, (USERS.0, broken)]);
    let output = project.vc(&["fmt"]).output().unwrap();
    assert!(!output.status.success());
    assert_eq!(project.read(USERS.0), broken);
}
