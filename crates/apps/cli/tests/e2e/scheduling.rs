//! Retries, polls, loops under `parallel` and the per-host connection limit.

use vc_mock_server::MockServer;

use crate::{CONFIG, Project, SCRIPTED, events, of_kind};

#[tokio::test(flavor = "multi_thread")]
async fn a_pass_after_a_failed_attempt_is_flaky() {
    let server = MockServer::start().await.unwrap();
    let test = "tests:\n  - id: passesSecondTime\n    call: scripted#flaky\n    expect: {status: 200}\n\
                \x20   retry: {attempts: 2, delay: 10ms}\n";
    let project = Project::new(&[CONFIG, SCRIPTED, ("services/scripted/flaky.test.yaml", test)]);
    let output = project.run(&server.base_url(), &["--reporter", "json"]).assert().code(0).get_output().clone();
    let events = events(&output.stdout);
    let finished = of_kind(&events, "testFinished");
    assert_eq!(finished.len(), 1, "{events:#?}");
    assert_eq!((&finished[0]["status"], &finished[0]["attempts"]), (&"flaky".into(), &2.into()), "{events:#?}");
    assert_eq!(server.stats().await.unwrap().count("GET /flaky"), 2);
}

/// `defaults.retry` re-runs a test that writes no `retry`; a test's own `retry` wins (D126).
#[tokio::test(flavor = "multi_thread")]
async fn defaults_retry_applies_unless_the_test_writes_its_own() {
    let server = MockServer::start().await.unwrap();
    let config = ("vibranium.config.yaml", "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n\
                  vars: {baseUrl: 'http://x'}\ndefaults: {retry: {attempts: 2, delay: 10ms}}\n");
    let test = "tests:\n  - id: passesThroughDefaults\n    call: scripted#flaky\n    vars: {key: a}\n\
                \x20   expect: {status: 200}\n  - id: failsWithItsOwn\n    call: scripted#flaky\n    vars: {key: b}\n\
                \x20   expect: {status: 200}\n    retry: {attempts: 0}\n";
    let project = Project::new(&[config, SCRIPTED, ("services/scripted/flaky.test.yaml", test)]);
    let output = project.run(&server.base_url(), &["--reporter", "json"]).assert().code(1).get_output().clone();
    let events = events(&output.stdout);
    let finished = of_kind(&events, "testFinished");
    let status = |id: &str| {
        let found = finished.iter().find(|event| event.to_string().contains(id));
        found.map(|event| (event["status"].clone(), event["attempts"].clone()))
    };
    assert_eq!(status("passesThroughDefaults"), Some(("flaky".into(), 2.into())), "{events:#?}");
    assert_eq!(status("failsWithItsOwn"), Some(("failed".into(), 1.into())), "{events:#?}");
    assert_eq!(server.stats().await.unwrap().count("GET /flaky"), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_poll_resends_until_its_check_holds() {
    let server = MockServer::start().await.unwrap();
    let test = "tests:\n  - id: waitsForTheJob\n    call: scripted#poll\n\
                \x20   poll: {until: {body: {match: {status: done}}}, every: 10ms, timeout: 5s}\n\
                \x20   expect: {body: {match: {attempt: 3}}}\n";
    let project = Project::new(&[CONFIG, SCRIPTED, ("services/scripted/poll.test.yaml", test)]);
    project.run(&server.base_url(), &[]).assert().code(0);
    assert_eq!(server.stats().await.unwrap().count("GET /poll/{id}"), 3, "pending, pending, done");
}

/// The iterations finish out of order, but what they save is in iteration order.
#[tokio::test(flavor = "multi_thread")]
async fn a_parallel_loop_saves_in_iteration_order() {
    let server = MockServer::start().await.unwrap();
    let test = "tests:\n  - id: sleepsInTurn\n    steps:\n\
                \x20     - call: scripted#slow\n        forEach: [240, 160, 80, 200, 40, 120]\n\
                \x20       parallel: 3\n        vars:\n          ms: ${item}\n        save:\n          slept: ${response.body.sleptMs}\n\
                \x20     - call: scripted#echo\n        body:\n          slept: ${var.slept}\n\
                \x20       expect: {body: {match: {body: {slept: [240, 160, 80, 200, 40, 120]}}}}\n";
    let project = Project::new(&[CONFIG, SCRIPTED, ("services/scripted/loop.test.yaml", test)]);
    let output = project.run(&server.base_url(), &[]).assert().get_output().clone();
    assert_eq!(output.status.code(), Some(0), "{}", crate::said(&output));
    let stats = server.stats().await.unwrap();
    assert_eq!(stats.count("GET /slow"), 6);
    let most = stats.max_in_flight.get("GET /slow").copied().unwrap_or_default();
    assert!((2..=3).contains(&most), "at most `parallel` at a time, and more than one: {most}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_per_host_limit_caps_requests_in_flight() {
    let server = MockServer::start().await.unwrap();
    let tests = (1..=12)
        .map(|n| format!("  - id: sleeps{n}\n    call: scripted#slow\n    vars: {{ms: 60}}\n"))
        .collect::<Vec<_>>()
        .concat();
    let file = format!("tests:\n{tests}");
    let project = Project::new(&[CONFIG, SCRIPTED, ("services/scripted/hosts.test.yaml", &file)]);
    project.run(&server.base_url(), &["--concurrency", "12"]).env("VIB_DEFAULTS_PER_HOST", "2").assert().code(0);
    let stats = server.stats().await.unwrap();
    assert_eq!(stats.count("GET /slow"), 12);
    assert_eq!(stats.max_in_flight_total, 2, "{stats:?}");
}
