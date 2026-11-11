//! `vc approvals` lists the agent calls waiting for a person and `vc approve <id>` decides one
//! (D195); a request that does not exist exits 2 and says how to list them.

use crate::{CONFIG, Project, said};

#[test]
fn an_empty_queue_says_nothing_is_waiting() {
    let project = Project::new(&[CONFIG]);
    let output = project.vc(&["approvals"]).assert().code(0).get_output().clone();
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "no requests waiting");
    let output = project.vc(&["approvals", "--all"]).assert().code(0).get_output().clone();
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "no requests");
}

#[test]
fn approving_an_unknown_request_exits_2() {
    let project = Project::new(&[CONFIG]);
    for args in [&["approve", "7"][..], &["approve", "7", "--deny"], &["approve", "7", "--always"]] {
        let output = project.vc(args).assert().code(2).get_output().clone();
        let said = said(&output);
        assert!(said.contains("no approval request 7"), "{args:?}: {said}");
        assert!(said.contains("`vc approvals` lists the waiting requests"), "{args:?}: {said}");
        assert!(!said.contains("history:"), "an unknown request is not a history problem: {said}");
    }
}

#[test]
fn a_request_id_is_its_number_with_or_without_the_hash() {
    let project = Project::new(&[CONFIG]);
    let output = project.vc(&["approve", "#7"]).assert().code(2).get_output().clone();
    assert!(said(&output).starts_with("error: no approval request 7\n"), "{}", said(&output));
    let output = project.vc(&["approve", "abc"]).assert().code(2).get_output().clone();
    let said = said(&output);
    assert!(said.starts_with("error: `abc` is not a request number\n"), "{said}");
    assert!(said.contains("help: pass the number `vc approvals` lists"), "{said}");
    assert!(!said.contains("invalid digit"), "no raw parser error: {said}");
}
