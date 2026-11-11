//! How a run labels what it reports: an example row's `name`, every repeat of a repeated test the
//! same however it repeats, a skipped test reported inside its file and counted in its totals, and
//! the `name` of a test, file and profile in the console, the json events, CTRF and `JUnit`.

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

#[tokio::test(flavor = "multi_thread")]
async fn a_retried_failure_is_shown_once_under_its_attempts() {
    let server = MockServer::start().await.unwrap();
    let test = "tests:\n  - id: wantsAnError\n    call: users#list\n    expect: {status: 500}\n\
                \x20   retry: {attempts: 2, delay: 10ms}\n";
    let project = Project::new(&[CONFIG, USERS, ("services/users/retried.test.yaml", test)]);
    let run = project.run(&server.base_url(), &[]).output().unwrap();
    let printed = String::from_utf8_lossy(&run.stdout);
    assert!(printed.contains("✕ users.retried#wantsAnError (3 attempts)"), "{printed}");
    assert!(printed.contains("attempts 1, 2 and 3 of 3:"), "{printed}");
    assert_eq!(printed.matches("expect.status").count(), 1, "one block, not one per attempt: {printed}");
    assert_eq!(server.stats().await.unwrap().count("GET /users"), 3);
}

/// A config whose default profile, `dev`, is named `Local dev`.
const NAMED_CONFIG: (&str, &str) = (
    "vibranium.config.yaml",
    "syntaxVersion: 2\nprofiles:\n  - id: dev\n    name: Local dev\n    default: true\nvars: {baseUrl: 'http://x'}\n",
);

/// A file named `User list` with one named test and one without a name.
const NAMED: (&str, &str) = (
    "services/users/named.test.yaml",
    "name: User list\ntests:\n  - id: listsUsers\n    name: Lists every user\n    call: users#list\n\
     \x20 - id: listsAgain\n    call: users#list\n",
);

#[tokio::test(flavor = "multi_thread")]
async fn a_named_test_file_and_profile_show_their_names_in_every_report() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[NAMED_CONFIG, USERS, NAMED]);
    let run = project.run(&server.base_url(), &["--report", "ctrf,junit", "--out", "out"]).output().unwrap();
    let printed = String::from_utf8_lossy(&run.stdout);
    assert!(printed.contains("users.named#listsUsers — Lists every user"), "{}", said(&run));
    assert!(printed.contains("profile dev \"Local dev\""), "{}", said(&run));
    let read = |name: &str| std::fs::read_to_string(project.path().join("out").join(name)).unwrap();
    let ctrf: Value = serde_json::from_str(&read("ctrf.json")).unwrap();
    let tests = ctrf["results"]["tests"].as_array().unwrap();
    let named = tests.iter().find(|test| test["name"] == "users.named#listsUsers").unwrap();
    assert_eq!(
        (&named["extra"]["name"], &named["extra"]["fileName"]),
        (&"Lists every user".into(), &"User list".into())
    );
    assert_eq!(ctrf["results"]["extra"]["profileNames"]["dev"], "Local dev");
    let junit = read("junit.xml");
    assert!(junit.contains("<property name=\"name\" value=\"User list\"/>"), "{junit}");
    assert!(junit.contains("<property name=\"name\" value=\"Lists every user\"/>"), "{junit}");
    let json = project.run(&server.base_url(), &["--reporter", "json"]).output().unwrap();
    let events = events(&json.stdout);
    let names: Vec<&Value> = of_kind(&events, "testFinished").iter().map(|event| &event["name"]).collect();
    assert!(names.contains(&&Value::from("Lists every user")) && names.contains(&&Value::Null), "{events:#?}");
    assert_eq!(of_kind(&events, "summary")[0]["profileNames"]["dev"], "Local dev");
}
