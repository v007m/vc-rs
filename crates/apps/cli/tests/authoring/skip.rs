//! `vc skip` and `vc unskip` write and remove a `skip` in the YAML (D77): the long form with
//! `--until`, the short form without, and at the top of a file for a file ID. Unskipping gives
//! back the original bytes.

use vc_mock_server::MockServer;

use crate::{CONFIG, CRUD, Project, USERS, checks_clean, said};

const UNTIL: [&str; 6] = ["skip", "users.crud#listsUsers", "--reason", "upstream down", "--until", "2099-01-01"];

#[test]
fn skip_with_a_date_writes_the_long_form_into_the_test() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    project.vc(&UNTIL).assert().code(0);
    let expected = CRUD.1.replace(
        "    call: users#list\n",
        "    call: users#list\n    skip:\n      reason: upstream down\n      until: 2099-01-01\n",
    );
    assert_eq!(project.read(CRUD.0), expected);
    checks_clean(&project);
}

#[test]
fn skip_with_only_a_reason_writes_the_short_form() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    project.vc(&["skip", "users.crud#listsUsers", "--reason", "upstream down"]).assert().code(0);
    let expected = CRUD.1.replace("    call: users#list\n", "    call: users#list\n    skip: upstream down\n");
    assert_eq!(project.read(CRUD.0), expected);
}

#[test]
fn skipping_a_file_puts_skip_at_its_top() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    project.vc(&["skip", "users.crud", "--reason", "being rewritten"]).assert().code(0);
    let text = project.read(CRUD.0);
    assert!(text.lines().any(|line| line == "skip: being rewritten"), "{text}");
    assert_eq!(text.replace("skip: being rewritten\n", ""), CRUD.1, "nothing else changes");
}

#[test]
fn unskip_gives_back_the_original_bytes() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    project.vc(&UNTIL).assert().code(0);
    project.vc(&["unskip", "users.crud#listsUsers"]).assert().code(0);
    assert_eq!(project.read(CRUD.0), CRUD.1);
    project.vc(&["skip", "users.crud", "--reason", "later"]).assert().code(0);
    project.vc(&["unskip", "users.crud"]).assert().code(0);
    assert_eq!(project.read(CRUD.0), CRUD.1);
}

#[test]
fn unskip_removes_a_skip_written_by_hand_in_flow_form() {
    let by_hand = CRUD.1.replace("    call: users#list\n", "    call: users#list\n    skip: {reason: flaky}  # mine\n");
    let project = Project::new(&[CONFIG, USERS, (CRUD.0, &by_hand)]);
    project.vc(&["unskip", "users.crud#listsUsers"]).assert().code(0);
    assert_eq!(project.read(CRUD.0), CRUD.1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_skipped_test_is_reported_with_its_reason() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    project.serve_from(&server.base_url());
    project.vc(&UNTIL).assert().code(0);
    let output = project.vc(&["run", "users.crud"]).output().unwrap();
    let text = said(&output);
    assert_eq!(output.status.code(), Some(0), "{text}");
    assert!(text.contains("skipped: upstream down") && text.contains("1 passed · 1 skipped"), "{text}");
    assert_eq!(server.stats().await.unwrap().count("GET /users"), 0, "the skipped test sent nothing");
}

#[test]
fn skip_refuses_an_unknown_test_and_writes_nothing() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    let output = project.vc(&["skip", "users.crud#listUsers", "--reason", "x"]).output().unwrap();
    assert!(!output.status.success());
    assert!(said(&output).contains("users.crud#listsUsers"), "suggests the close match: {}", said(&output));
    assert_eq!(project.read(CRUD.0), CRUD.1);
}

#[test]
fn skip_refuses_a_date_that_is_not_one() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    let args = ["skip", "users.crud#listsUsers", "--reason", "x", "--until", "next week"];
    assert!(!project.vc(&args).output().unwrap().status.success());
    assert_eq!(project.read(CRUD.0), CRUD.1);
}
