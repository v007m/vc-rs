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
    }
}
