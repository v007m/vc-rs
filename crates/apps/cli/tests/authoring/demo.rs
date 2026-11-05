//! `vc demo` is the small built-in service the `vc init` sample calls (D175): `start --detach`
//! re-launches `vc` in the background and writes a pidfile, `stop` reads it and ends it.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use crate::Project;

/// A port of its own, so this test never meets the sample's demo on 7421.
const PORT: u16 = 7431;

/// The status line of `GET <path>` on the demo, or `None` when nothing answers.
fn get(path: &str) -> Option<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", PORT)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    write!(stream, "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").ok()?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply).ok()?;
    reply.lines().next().map(str::to_owned)
}

/// Polls until the demo answers (or stops answering, for `up == false`), for up to 10 s.
fn wait_until(up: bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if get("/users").is_some() == up {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn starts_in_the_background_answers_and_stops() {
    let project = Project::empty();
    let port = PORT.to_string();
    project.vc(&["demo", "start", "--port", &port, "--detach"]).timeout(Duration::from_secs(10)).assert().code(0);
    let up = wait_until(true);
    let users = get("/users");
    let echo = get("/echo");
    project.vc(&["demo", "stop", "--port", &port]).assert().code(0);
    assert!(up, "the demo answers on {PORT}");
    assert!(users.is_some_and(|line| line.contains(" 200 ")), "GET /users is 200");
    assert!(echo.is_some(), "it has an echo endpoint");
    assert!(wait_until(false), "`vc demo stop` ends it");
}

/// The sample's `afterAll` stops the demo even when `beforeAll` failed to start it, so stopping
/// nothing is not an error.
#[test]
fn stopping_a_demo_that_is_not_running_succeeds_and_says_so() {
    let project = Project::empty();
    let output = project.vc(&["demo", "stop", "--port", "7432"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(crate::said(&output).contains("7432"), "names the port");
}
