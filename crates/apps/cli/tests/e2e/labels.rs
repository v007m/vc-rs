//! How a run labels what it reports: an example row's `name`, every repeat of a repeated test the
//! same however it repeats, and a skipped test reported inside its file and counted in its totals.

use serde_json::Value;
use vc_mock_server::MockServer;

use crate::{CONFIG, Project, USERS, events, of_kind, said};

/// The label of every finished test of a `--reporter json` run, in the order reported.
fn labels(stdout: &[u8]) -> Vec<Value> {
    of_kind(&events(stdout), "testFinished").iter().map(|event| event["label"].clone()).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_example_row_with_a_name_is_labeled_by_it() {
    let server = MockServer::start().await.unwrap();
    let test = "tests:\n  - id: listsEachPage\n    call: users#list\n    query: {page: '${example.page}'}\n\
                \x20   examples:\n      - {name: first page, page: 1}\n      - {page: 2}\n";
    let project = Project::new(&[CONFIG, USERS, ("services/users/pages.test.yaml", test)]);
    let run = project.run(&server.base_url(), &["--reporter", "json"]).output().unwrap();
    let mut labels = labels(&run.stdout);
    labels.sort_by_key(ToString::to_string);
    let expected = ["users.pages#listsEachPage [row 2]", "users.pages#listsEachPage › first page"];
    assert_eq!(labels, expected, "{}", said(&run));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_repeated_test_shows_its_repeat_the_same_way_by_flag_or_key() {
    let server = MockServer::start().await.unwrap();
    let test = "tests:\n  - id: listsUsers\n    call: users#list\n\
                \x20 - id: listsAgain\n    call: users#list\n    repeat: 2\n";
    let project = Project::new(&[CONFIG, USERS, ("services/users/repeat.test.yaml", test)]);
    let by_key = project.run(&server.base_url(), &["--reporter", "json"]).output().unwrap();
    let mut keyed = labels(&by_key.stdout);
    keyed.sort_by_key(ToString::to_string);
    let expected =
        ["users.repeat#listsAgain [repeat 1]", "users.repeat#listsAgain [repeat 2]", "users.repeat#listsUsers"];
    assert_eq!(keyed, expected, "{}", said(&by_key));
    let flagged = ["--repeat", "2", "--reporter", "json", "users.repeat#listsUsers"];
    let by_flag = project.run(&server.base_url(), &flagged).output().unwrap();
    let mut flagged = labels(&by_flag.stdout);
    flagged.sort_by_key(ToString::to_string);
    assert_eq!(
        flagged,
        ["users.repeat#listsUsers [repeat 1]", "users.repeat#listsUsers [repeat 2]"],
        "{}",
        said(&by_flag)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_skipped_test_is_reported_inside_its_file_and_counted_in_its_totals() {
    let server = MockServer::start().await.unwrap();
    let test = "tests:\n  - id: listsUsers\n    call: users#list\n\
                \x20 - id: isParked\n    call: users#list\n    skip: {reason: later}\n";
    let project = Project::new(&[CONFIG, USERS, ("services/users/parked.test.yaml", test)]);
    let run = project.run(&server.base_url(), &["--reporter", "json"]).output().unwrap();
    let events = events(&run.stdout);
    let at = |wanted: &dyn Fn(&Value) -> bool| events.iter().position(wanted);
    let started = at(&|event| event["event"] == "fileStarted");
    let skipped = at(&|event| event["event"] == "testFinished" && event["status"] == "skipped");
    let finished = at(&|event| event["event"] == "fileFinished");
    assert!(started.is_some() && started < skipped && skipped < finished, "{events:#?}");
    let totals = &of_kind(&events, "fileFinished")[0]["totals"];
    assert_eq!((&totals["passed"], &totals["skipped"]), (&1.into(), &1.into()), "{events:#?}");
    assert_eq!(of_kind(&events, "testFinished").len(), 2, "reported once: {events:#?}");
}
