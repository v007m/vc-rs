//! `beforeAll` runs once per file, cleanups run newest first on a pass, a failure and Ctrl-C, an
//! api's `save` runs only on a status its `saveOn` matches (D191), `afterEach` follows every test
//! and `afterAll` the last one, and a `standalone: false` test runs only where `test:` reuses it.

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

#[tokio::test(flavor = "multi_thread")]
async fn a_test_expecting_400_skips_the_apis_save_unless_its_save_on_says_400() {
    let server = MockServer::start().await.unwrap();
    let apis = concat!(
        "baseUrl: ${var.baseUrl}\napis:\n  - id: answer\n    path: /status/${var.code}\n    vars: {code: 200}\n",
        "    save: {answerId: '${response.body.id}'}\n",
    );
    let test =
        "tests:\n  - id: rejectsBadCode\n    call: codes#answer\n    vars: {code: 400}\n    expect: {status: 400}\n";
    let saving = format!("{apis}    saveOn: 4xx\n");
    for (apis, exit) in [(apis, 0), (saving.as_str(), 1)] {
        let files = [CONFIG, ("services/codes/codes.api.yaml", apis), ("services/codes/rejects.test.yaml", test)];
        let output = Project::new(&files).run(&server.base_url(), &[]).output().unwrap();
        assert_eq!(output.status.code(), Some(exit), "{}", crate::said(&output));
    }
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

/// A `beforeAll` call with `retry` runs again until the flaky route answers, and the test reads
/// what the passing attempt saved; without `retry` the test is blocked.
#[tokio::test(flavor = "multi_thread")]
async fn a_before_all_call_retries_until_it_passes() {
    let file = |retry: &str| {
        format!(
            "beforeAll:\n  - id: warm\n    call: scripted#flaky\n    query: {{failTimes: '2'}}\n{retry}\
             \x20   save:\n      attempt: ${{response.body.attempt}}\n\
             tests:\n  - id: readsTheWarmUp\n    call: scripted#echo\n    body:\n      attempt: ${{var.warm.attempt}}\n\
             \x20   expect: {{body: {{match: {{body: {{attempt: 3}}}}}}}}\n"
        )
    };
    for (retry, exit, sent) in [("    retry: {attempts: 2, delay: 10ms}\n", 0, 3), ("", 1, 1)] {
        let server = MockServer::start().await.unwrap();
        let file = file(retry);
        let project = Project::new(&[CONFIG, crate::SCRIPTED, ("services/scripted/warm.test.yaml", &file)]);
        let output = project.run(&server.base_url(), &[]).output().unwrap();
        assert_eq!(output.status.code(), Some(exit), "{}", crate::said(&output));
        assert_eq!(server.stats().await.unwrap().count("GET /flaky"), sent, "{retry}");
    }
}

/// Three tests echoing, the second expecting `status`, with `afterEach` listing the users and
/// `afterAll` checking the order `beforeAll` made with `size(response.body) == <orders>`.
fn after_blocks(orders: u8, status: u16) -> String {
    format!(
        "beforeAll:\n  - id: seed\n    call: orders#create\nafterEach:\n  - call: users#list\n\
         afterAll:\n  - call: orders#list\n    expect: {{cel: {{seeded: 'size(response.body) == {orders}'}}}}\n\
         tests:\n  - id: echoesOnce\n    call: scripted#echo\n  - id: echoesWithStatus\n    call: scripted#echo\n\
         \x20   expect: {{status: {status}}}\n  - id: echoesAgain\n    call: scripted#echo\n"
    )
}

/// `afterEach` runs after every test, a failed one too; `afterAll` runs once, after the last
/// test and before the file's cleanups, so it still sees the order `beforeAll` made, and a
/// failed `expect` in it is reported. The exit code counts tests only (cli-workflow's table),
/// so it is not asserted for that case.
#[tokio::test(flavor = "multi_thread")]
async fn after_each_follows_every_test_and_after_all_runs_once_before_the_file_cleanups() {
    for (orders, status, exit) in [(1, 200, Some(0)), (0, 200, None), (1, 500, Some(1))] {
        let server = MockServer::start().await.unwrap();
        let file = after_blocks(orders, status);
        let files = [CONFIG, USERS, ORDERS, crate::SCRIPTED, ("services/scripted/after.test.yaml", &file)];
        let output = Project::new(&files).run(&server.base_url(), &[]).output().unwrap();
        let case = format!("orders {orders}, status {status}");
        let said = crate::said(&output);
        match exit {
            Some(exit) => assert_eq!(output.status.code(), Some(exit), "{case}: {said}"),
            None => assert!(said.contains("V5102") && said.contains("cel.seeded"), "{case}: {said}"),
        }
        let stats = server.stats().await.unwrap();
        assert_eq!(stats.count("GET /users"), 3, "{case}: afterEach after each test: {stats:?}");
        let orders = (stats.count("GET /orders"), stats.count("DELETE /orders/{id}"));
        assert_eq!(orders, (1, 1), "{case}: afterAll once, then the file's cleanup: {stats:?}");
    }
}

/// A flow test that makes a user, `standalone` or not, and a test that reuses it with `test:`.
fn reused(standalone: bool) -> [(&'static str, String); 2] {
    let flow = format!(
        "tests:\n  - id: makesUser\n    standalone: {standalone}\n    steps:\n      - call: users#create\n\
         \x20       save: {{userId: '${{response.body.id}}'}}\n"
    );
    let test = "tests:\n  - id: listsAfterMaking\n    steps:\n      - test: shared.flows#makesUser\n\
                \x20     - call: users#list\n        expect: {cel: {made: 'size(response.body) == 1'}}\n";
    [("services/shared/flows.test.yaml", flow), ("services/users/reuse.test.yaml", test.to_owned())]
}

/// `standalone: false` keeps a test from running on its own: only its reuse makes a user. With
/// `standalone: true` it also runs, and reports, as a test of its own.
#[tokio::test(flavor = "multi_thread")]
async fn a_test_that_is_not_standalone_runs_only_where_it_is_reused() {
    for (standalone, ran) in [(false, 1_usize), (true, 2)] {
        let server = MockServer::start().await.unwrap();
        let [flow, test] = reused(standalone);
        let files = [CONFIG, USERS, (flow.0, flow.1.as_str()), (test.0, test.1.as_str())];
        let args = ["--reporter", "json", "--concurrency", "1"];
        let output = Project::new(&files).run(&server.base_url(), &args).output().unwrap();
        assert_eq!(output.status.code(), Some(0), "{}", crate::said(&output));
        let events = events(&output.stdout);
        assert_eq!(of_kind(&events, "testFinished").len(), ran, "standalone: {standalone}: {events:#?}");
        let made = server.stats().await.unwrap().count("POST /users");
        assert_eq!(usize::try_from(made), Ok(ran), "standalone: {standalone}");
    }
}
