//! Retries, the transport's `http.retry` backoff, polls, loops under `parallel` and the per-host
//! connection limit.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Instant;

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
    let config = (
        "vibranium.config.yaml",
        "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n\
                  vars: {baseUrl: 'http://x'}\ndefaults: {retry: {attempts: 2, delay: 10ms}}\n",
    );
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

/// `--set defaults.retry.attempts=1` retries a test the project gives no `retry`.
#[tokio::test(flavor = "multi_thread")]
async fn set_defaults_retry_retries_a_test_without_its_own() {
    let server = MockServer::start().await.unwrap();
    let test = "tests:\n  - id: passesOnTheRetry\n    call: scripted#flaky\n    expect: {status: 200}\n";
    let project = Project::new(&[CONFIG, SCRIPTED, ("services/scripted/flaky.test.yaml", test)]);
    let args = ["--set", "defaults.retry.attempts=1", "--set", "defaults.retry.delay=10ms", "--reporter", "json"];
    let output = project.run(&server.base_url(), &args).assert().code(0).get_output().clone();
    let events = events(&output.stdout);
    let finished = of_kind(&events, "testFinished");
    assert_eq!((&finished[0]["status"], &finished[0]["attempts"]), (&"flaky".into(), &2.into()), "{events:#?}");
    assert_eq!(server.stats().await.unwrap().count("GET /flaky"), 2);
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

/// A poll that gives up still reports what the server last answered, so the events show why.
#[tokio::test(flavor = "multi_thread")]
async fn a_poll_that_gives_up_reports_its_last_exchange() {
    let server = MockServer::start().await.unwrap();
    let test = "tests:\n  - id: givesUp\n    call: scripted#poll\n    query: {after: '1000'}\n\
                \x20   poll: {until: {body: {match: {status: done}}}, every: 10ms, timeout: 100ms}\n";
    let project = Project::new(&[CONFIG, SCRIPTED, ("services/scripted/poll.test.yaml", test)]);
    let output = project.run(&server.base_url(), &["--reporter", "json"]).assert().code(1).get_output().clone();
    let events = events(&output.stdout);
    let exchanged = of_kind(&events, "exchanged");
    assert_eq!(exchanged.len(), 1, "{events:#?}");
    assert_eq!(exchanged[0]["status"], 200, "{events:#?}");
    assert!(exchanged[0]["url"].as_str().is_some_and(|url| url.contains("/poll/job?after=1000")), "{events:#?}");
    let sent = server.stats().await.unwrap().count("GET /poll/{id}");
    assert!(sent > 1, "it polled more than once, and reported only the last: {sent}");
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

/// `defaults.perHost` in the config caps the connections to one host, whatever `--concurrency`.
#[tokio::test(flavor = "multi_thread")]
async fn the_configs_per_host_caps_requests_in_flight() {
    let server = MockServer::start().await.unwrap();
    let config = (
        "vibranium.config.yaml",
        "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\n\
         defaults:\n  concurrency: 12\n  perHost: 3\n",
    );
    let tests = (1..=12)
        .map(|n| format!("  - id: sleeps{n}\n    call: scripted#slow\n    vars: {{ms: 60}}\n"))
        .collect::<Vec<_>>()
        .concat();
    let file = format!("tests:\n{tests}");
    let project = Project::new(&[config, SCRIPTED, ("services/scripted/hosts.test.yaml", &file)]);
    project.run(&server.base_url(), &[]).assert().code(0);
    let stats = server.stats().await.unwrap();
    assert_eq!(stats.count("GET /slow"), 12);
    assert_eq!(stats.max_in_flight_total, 3, "{stats:?}");
}

/// A project whose `http.retry` resends a 503 twice, waiting `base` then twice `base` unless
/// `max` caps it, with no jitter.
fn backing_off(max: &str) -> Project {
    let config = format!(
        "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {{baseUrl: 'http://x'}}\n\
         http:\n  retry:\n    attempts: 2\n    on: [503]\n    backoff: {{base: 400ms, max: {max}, jitter: false}}\n"
    );
    let test = "tests:\n  - id: passesThirdTime\n    call: scripted#flaky\n    query: {failTimes: '2'}\n\
                \x20   expect: {status: 200}\n";
    Project::new(&[("vibranium.config.yaml", &config), SCRIPTED, ("services/scripted/backoff.test.yaml", test)])
}

/// When a request arrived, and when its answer was about to be sent.
type Stamp = (Instant, Instant);

/// When each request reached a server on a free port that answers its first two requests 503 and
/// the rest 200: the instant it arrived, and the instant just before its answer was sent.
fn stamping_server() -> (String, mpsc::Receiver<Stamp>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (stamps, stamped) = mpsc::channel();
    let served = Arc::new(AtomicUsize::new(0));
    thread::spawn(move || {
        for connection in listener.incoming().flatten() {
            let (stamps, served) = (stamps.clone(), Arc::clone(&served));
            thread::spawn(move || answer(connection, &stamps, &served));
        }
    });
    (url, stamped)
}

/// Answers each request on `connection` until the client closes it, stamping it on `stamps`.
fn answer(mut connection: TcpStream, stamps: &mpsc::Sender<Stamp>, served: &AtomicUsize) {
    let (mut head, mut chunk) = (Vec::new(), [0; 1024]);
    while let Ok(read @ 1..) = connection.read(&mut chunk) {
        head.extend_from_slice(chunk.get(..read).unwrap_or_default());
        if !head.windows(4).any(|end| end == b"\r\n\r\n") {
            continue;
        }
        let arrived = Instant::now();
        head.clear();
        let status = if served.fetch_add(1, Ordering::SeqCst) < 2 { "503 Service Unavailable" } else { "200 OK" };
        let reply = format!("HTTP/1.1 {status}\r\ncontent-length: 0\r\n\r\n");
        if stamps.send((arrived, Instant::now())).is_err() || connection.write_all(reply.as_bytes()).is_err() {
            return;
        }
    }
}

/// The transport resends one request with an exponential `backoff`: 400 ms then 800 ms uncapped,
/// 100 ms twice with `max: 100ms`. It is one attempt of the test, not a test retry. Each wait is
/// timed at the server, from just before it answers to when the next request arrives, so neither a
/// loaded machine nor a reporter that reads its events late can make a wait look shorter. A loaded
/// machine can make one look longer, so only the least waits are checked here: the exact waits,
/// and that `max` caps them, are checked on a paused clock in the executor's
/// `the_configs_backoff_doubles_each_wait_up_to_its_max`.
#[test]
fn http_retry_backs_off_exponentially_up_to_its_max() {
    let waits = |max: &str| {
        let (url, stamped) = stamping_server();
        let output = backing_off(max).run(&url, &["--reporter", "json"]).output().unwrap();
        assert_eq!(output.status.code(), Some(0), "max {max}: {}", crate::said(&output));
        let events = events(&output.stdout);
        let finished = of_kind(&events, "testFinished");
        assert_eq!(finished[0]["attempts"], 1, "the request was resent, not the test: {events:#?}");
        let stamps = stamped.try_iter().collect::<Vec<_>>();
        assert_eq!(stamps.len(), 3, "max {max}: {stamps:?}");
        stamps.windows(2).map(|pair| pair[1].0.duration_since(pair[0].1).as_millis()).collect::<Vec<_>>()
    };
    let (uncapped, capped) = (waits("5s"), waits("100ms"));
    assert!(uncapped[0] >= 400 && uncapped[1] >= 800, "base, then twice base: {uncapped:?}");
    assert!(capped.iter().all(|wait| *wait >= 100), "max caps both waits at 100 ms, not 0: {capped:?}");
}
