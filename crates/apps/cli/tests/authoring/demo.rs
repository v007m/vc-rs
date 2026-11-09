//! `vc demo` is the small built-in service the `vc init` sample calls (D175): `start --detach`
//! re-launches `vc` in the background and writes a pidfile, `stop` reads it and ends it.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::Stdio;
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

/// The exit code of `vc <args>` run with no pipes, or `None` when it runs past 20 s (then it is
/// killed). On Windows a detached demo inherits `vc`'s handles, so a pipe the test read `vc`'s
/// output from would stay open until `stop`, and reading it to the end would never finish.
fn exit_code(project: &Project, args: &[&str]) -> Option<i32> {
    let mut vc = project.process(args);
    let mut child = vc.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().ok()?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().ok()? {
            return status.code();
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    child.kill().ok();
    None
}

#[test]
fn starts_in_the_background_answers_and_stops() {
    let project = Project::empty();
    let port = PORT.to_string();
    let started = exit_code(&project, &["demo", "start", "--port", &port, "--detach"]);
    let up = wait_until(true);
    let users = get("/users");
    let echo = get("/echo");
    let stopped = exit_code(&project, &["demo", "stop", "--port", &port]);
    assert_eq!((started, stopped), (Some(0), Some(0)), "`vc demo start --detach` and `stop` exit 0");
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
    let output = project.vc(&["demo", "stop", "--port", "7432"]).timeout(Duration::from_secs(20)).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(crate::said(&output).contains("7432"), "names the port");
}
