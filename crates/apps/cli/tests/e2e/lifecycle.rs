//! `beforeAll` runs once per file, and cleanups run newest first on a pass, a failure and Ctrl-C.

use vc_mock_server::MockServer;

use crate::{CONFIG, ORDERS, Project, USERS, events, of_kind};

/// A user then an order, in steps, then `last` (a step or nothing).
fn makes_both(last: &str) -> String {
    format!("tests:\n  - id: makesBoth\n    steps:\n      - call: users#create\n      - call: orders#create\n{last}")
}

/// The apis whose cleanups ran, in the order they ran.
fn cleanups(stdout: &[u8]) -> Vec<String> {
    let events = events(stdout);
    let finished = of_kind(&events, "cleanupFinished");
    finished.iter().map(|event| event["cleanup"]["action"].as_str().unwrap_or_default().to_owned()).collect()
}

fn newest_first(ran: &[String]) {
    assert_eq!(ran.len(), 2, "{ran:?}");
    assert!(ran[0].contains("orders") && ran[1].contains("users"), "the order was made last: {ran:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn fifty_tests_share_one_before_all_request() {
    let server = MockServer::start().await.unwrap();
    let tests = (1..=50).map(|n| format!("  - id: listsUsers{n}\n    call: users#list\n")).collect::<Vec<_>>().concat();
    let file = format!("beforeAll:\n  - id: seed\n    call: orders#create\ntests:\n{tests}");
    let project = Project::new(&[CONFIG, USERS, ORDERS, ("services/users/many.test.yaml", &file)]);
    project.run(&server.base_url(), &["--concurrency", "16"]).assert().code(0);
    let stats = server.stats().await.unwrap();
    assert_eq!(stats.count("POST /orders"), 1, "{stats:?}");
    assert_eq!(stats.count("GET /users"), 50, "{stats:?}");
    assert_eq!(stats.count("DELETE /orders/{id}"), 1, "the file's cleanup ran once, after its tests");
}

#[tokio::test(flavor = "multi_thread")]
async fn cleanups_run_newest_first_after_a_pass() {
    let server = MockServer::start().await.unwrap();
    let test = makes_both("");
    let project = Project::new(&[CONFIG, USERS, ORDERS, ("services/users/both.test.yaml", &test)]);
    let output = project.run(&server.base_url(), &["--reporter", "json"]).assert().code(0).get_output().clone();
    newest_first(&cleanups(&output.stdout));
    let stats = server.stats().await.unwrap();
    assert_eq!((stats.count("DELETE /users/{id}"), stats.count("DELETE /orders/{id}")), (1, 1), "{stats:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn cleanups_run_newest_first_after_a_failure() {
    let server = MockServer::start().await.unwrap();
    let test = makes_both("      - call: users#list\n        expect: {status: 500}\n");
    let project = Project::new(&[CONFIG, USERS, ORDERS, ("services/users/both.test.yaml", &test)]);
    let output = project.run(&server.base_url(), &["--reporter", "json"]).assert().code(1).get_output().clone();
    newest_first(&cleanups(&output.stdout));
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn ctrl_c_runs_the_cleanups_and_exits_130() {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let server = MockServer::start().await.unwrap();
    let slow = "      - call: scripted#slow\n        vars: {ms: 20000}\n";
    let test = makes_both(slow);
    let files = [CONFIG, USERS, ORDERS, crate::SCRIPTED, ("services/users/both.test.yaml", &test)];
    let project = Project::new(&files);
    let base = format!("vars.baseUrl={}", server.base_url());
    let mut vc = Command::new(env!("CARGO_BIN_EXE_vc"));
    vc.current_dir(project.path()).args(["run", "--set", &base, "--reporter", "json"]);
    vc.env("NO_COLOR", "1").env("HOME", project.home()).env("XDG_CONFIG_HOME", project.home());
    let child = vc.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let started = Instant::now();
    while server.stats().await.unwrap().count("GET /slow") == 0 {
        assert!(started.elapsed() < Duration::from_secs(20), "the slow step never started");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let pid = child.id().to_string();
    assert!(Command::new("kill").args(["-INT", &pid]).status().unwrap().success());
    let output = tokio::task::spawn_blocking(move || child.wait_with_output()).await.unwrap().unwrap();
    assert!(started.elapsed() < Duration::from_secs(15), "Ctrl-C did not stop the slow request");
    assert_eq!(output.status.code(), Some(130), "{}", crate::said(&output));
    newest_first(&cleanups(&output.stdout));
    let stats = server.stats().await.unwrap();
    assert_eq!((stats.count("DELETE /users/{id}"), stats.count("DELETE /orders/{id}")), (1, 1), "{stats:?}");
}
