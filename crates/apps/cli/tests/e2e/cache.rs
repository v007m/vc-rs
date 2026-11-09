//! The cross-run cache: a `cache:` action is called once across runs, its cleanup waits for
//! `vc cache clear`, the file holds no value in the clear, and `--no-cache` leaves it alone.

use std::fs;

use assert_cmd::Command;
use serde_json::Value;
use vc_mock_server::MockServer;

use crate::{Project, USERS, said};

const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

const LISTS: (&str, &str) = ("services/users/lists.test.yaml", "tests:\n  - id: listsUsers\n    call: users#list\n");

/// A `cache:` whose check always fails, so every run makes a fresh admin that replaces the entry.
const CHECKED: &str = "{for: 1h, check: {call: users#list, expect: {status: 404}}}";

/// A config whose `beforeRun` makes an admin user once an hour.
fn config(base: &str) -> String {
    format!(
        "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {{baseUrl: '{base}'}}\n\
         beforeRun:\n  - id: admin\n    call: users#create\n    save: {{adminName: '${{response.body.name}}'}}\n\
         \x20   cache: {{for: 1h}}\n"
    )
}

fn keyed(project: &Project, args: &[&str]) -> Command {
    let mut vc = project.vc(args);
    vc.env("VIB_CACHE_KEY", KEY);
    vc
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cached_before_run_is_made_once_and_cleaned_up_on_clear() {
    let server = MockServer::start().await.unwrap();
    let config = config(&server.base_url());
    let project = Project::new(&[("vibranium.config.yaml", &config), USERS, LISTS]);
    for _ in 0..2 {
        let run = keyed(&project, &["run"]).output().unwrap();
        assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    }
    let stats = server.stats().await.unwrap();
    assert_eq!(stats.count("POST /users"), 1, "the second run reuses the cached admin: {stats:?}");
    assert_eq!(stats.count("DELETE /users/{id}"), 0, "a cached action's cleanup waits for the entry to go");

    let sealed = fs::read(project.path().join(".vibranium/cache.db")).unwrap();
    assert!(!String::from_utf8_lossy(&sealed).contains("someone"), "saved values are encrypted");
    let listed = keyed(&project, &["cache", "list", "--json"]).output().unwrap();
    let entries: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!((&entries[0]["label"], &entries[0]["profiles"][0]), (&"users#create".into(), &"dev".into()));

    keyed(&project, &["cache", "clear"]).assert().code(0).stdout("cleared 1 entry\n");
    assert_eq!(server.stats().await.unwrap().count("DELETE /users/{id}"), 1, "clearing runs the cleanup");
    keyed(&project, &["cache", "list"]).assert().code(0).stdout("the cache is empty\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_replaced_entry_is_cleaned_up_after_every_test_and_a_failed_cleanup_warns() {
    let server = MockServer::start().await.unwrap();
    let failing = config(&server.base_url()).replace("{for: 1h}", CHECKED);
    let removes = (
        "services/users/removal.test.yaml",
        "tests:\n  - id: removesTheFirstAdmin\n    call: users#remove\n    vars: {userId: 1}\n",
    );
    let project = Project::new(&[("vibranium.config.yaml", &failing), USERS, LISTS, removes]);
    keyed(&project, &["run", "users.lists"]).assert().code(0);
    let run = keyed(&project, &["run"]).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "the first admin outlives the run's tests: {}", said(&run));
    let stats = server.stats().await.unwrap();
    assert_eq!((stats.count("POST /users"), stats.count("DELETE /users/{id}")), (2, 2), "{stats:?}");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains("warning: cache: the cleanup of users#create failed"), "{stderr}");
}

#[tokio::test(flavor = "multi_thread")]
async fn no_cache_runs_the_action_and_its_cleanup_every_time() {
    let server = MockServer::start().await.unwrap();
    let config = config(&server.base_url());
    let project = Project::new(&[("vibranium.config.yaml", &config), USERS, LISTS]);
    for _ in 0..2 {
        keyed(&project, &["run", "--no-cache"]).assert().code(0);
    }
    let stats = server.stats().await.unwrap();
    assert_eq!((stats.count("POST /users"), stats.count("DELETE /users/{id}")), (2, 2), "{stats:?}");
    assert!(!project.path().join(".vibranium/cache.db").exists(), "the cache is not opened");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_malformed_key_turns_the_cache_off_with_v5113() {
    let server = MockServer::start().await.unwrap();
    let config = config(&server.base_url());
    let project = Project::new(&[("vibranium.config.yaml", &config), USERS, LISTS]);
    let run = project.vc(&["run"]).env("VIB_CACHE_KEY", "not-a-key").output().unwrap();
    assert_eq!(run.status.code(), Some(0), "a cache that is off never fails the run: {}", said(&run));
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains("warning: cache: V5113") && stderr.contains("run every time this run"), "{stderr}");
    let stats = server.stats().await.unwrap();
    assert_eq!(stats.count("DELETE /users/{id}"), 1, "uncached, the cleanup runs at the end: {stats:?}");
}

#[test]
fn check_warns_of_cache_on_a_per_test_action() {
    let test = "tests:\n  - id: makesUser\n    before:\n      - call: users#create\n        cache: {for: 1h}\n    call: users#list\n";
    let project =
        Project::new(&[("vibranium.config.yaml", &config("http://x")), USERS, ("services/users/per.test.yaml", test)]);
    let checked = project.vc(&["check"]).output().unwrap();
    assert_eq!(checked.status.code(), Some(0), "{}", said(&checked));
    assert!(said(&checked).contains("V3202"), "{}", said(&checked));
}
