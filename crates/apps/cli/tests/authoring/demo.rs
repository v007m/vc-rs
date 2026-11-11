//! `vc demo` is the small built-in service the `vc init` sample calls (D175): `start --detach`
//! re-launches `vc` in the background and writes a pidfile, `stop` reads it and ends it. A
//! `start` that finds it running joins it, and its `stop` leaves it to the others.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::Project;

/// A port of its own, so this test never meets the sample's demo on 7421.
const PORT: u16 = 7431;

/// The status line of `GET <path>` on the demo on `port`, or `None` when nothing answers.
fn get(port: u16, path: &str) -> Option<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    write!(stream, "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").ok()?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply).ok()?;
    reply.lines().next().map(str::to_owned)
}

/// Polls until the demo on `port` answers (or stops answering, for `up == false`), for up to
/// 10 s.
fn wait_until(port: u16, up: bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if get(port, "/users").is_some() == up {
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
    let up = wait_until(PORT, true);
    let users = get(PORT, "/users");
    let echo = get(PORT, "/echo");
    let stopped = exit_code(&project, &["demo", "stop", "--port", &port]);
    assert_eq!((started, stopped), (Some(0), Some(0)), "`vc demo start --detach` and `stop` exit 0");
    assert!(up, "the demo answers on {PORT}");
    assert!(users.is_some_and(|line| line.contains(" 200 ")), "GET /users is 200");
    assert!(echo.is_some(), "it has an echo endpoint");
    assert!(wait_until(PORT, false), "`vc demo stop` ends it");
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

/// What `vc demo <args>` prints, run with pipes: only for a `vc` that launches no demo.
fn printed(project: &Project, args: &[&str]) -> String {
    let output = project.vc(args).timeout(Duration::from_secs(20)).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", crate::said(&output));
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn a_start_that_found_the_demo_running_leaves_it_to_the_one_that_launched_it() {
    let project = Project::empty();
    let port = "7434";
    let launched = exit_code(&project, &["demo", "start", "--port", port, "--detach"]);
    let joined = printed(&project, &["demo", "start", "--port", port, "--detach"]);
    let first = printed(&project, &["demo", "stop", "--port", port]);
    let still_up = wait_until(7434, true);
    let second = printed(&project, &["demo", "stop", "--port", port]);
    assert_eq!(launched, Some(0));
    assert_eq!(joined, "a demo already answers on port 7434\n");
    assert_eq!(first, "left the demo on port 7434 running, as another `vc demo start` uses it\n");
    assert!(still_up, "the first stop leaves it running");
    assert_eq!(second, "stopped the demo on port 7434\n");
    assert!(wait_until(7434, false), "the second stop ends it");
}

#[test]
fn refuses_a_port_another_program_holds() {
    let project = Project::empty();
    let _held = TcpListener::bind(("127.0.0.1", 7435)).unwrap();
    let output = project.vc(&["demo", "start", "--port", "7435", "--detach"]).timeout(Duration::from_secs(20)).output();
    let output = output.unwrap();
    let text = crate::said(&output);
    assert_eq!(output.status.code(), Some(2), "{text}");
    assert!(text.contains("error: port 7435 is taken by a program that is not the demo"), "{text}");
    assert!(text.contains("help: pick another `--port`, and change `sampleUrl`"), "{text}");
}

#[test]
fn in_the_foreground_it_says_where_it_listens_and_how_to_stop_it() {
    let project = Project::empty();
    let mut vc = project.process(&["demo", "start", "--port", "7436"]);
    let mut child = vc.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
    let pid = line.split("(pid ").nth(1).and_then(|rest| rest.split(')').next()).unwrap_or_default().to_owned();
    let ended = if cfg!(windows) {
        std::process::Command::new("taskkill").args(["/PID", &pid, "/F"]).status()
    } else {
        std::process::Command::new("kill").arg(&pid).status()
    };
    let exited = child.wait();
    assert!(line.starts_with("demo running on http://localhost:7436 (pid "), "{line}");
    assert!(line.ends_with("); Ctrl-C stops it\n"), "{line}");
    assert!(ended.is_ok_and(|status| status.success()) && exited.is_ok(), "the printed pid is the demo's");
}

/// A join whose caller crashed before its `stop` (its pid is gone) no longer keeps the demo up.
#[cfg(unix)]
#[test]
fn a_join_whose_caller_is_gone_does_not_keep_the_demo_running() {
    let project = Project::empty();
    let port = "7437";
    let launched = exit_code(&project, &["demo", "start", "--port", port, "--detach"]);
    let mut ended = std::process::Command::new("true").spawn().unwrap();
    let gone = ended.id();
    ended.wait().unwrap();
    let users = std::env::temp_dir().join("vibranium-demo-7437.users");
    std::fs::create_dir_all(&users).unwrap();
    std::fs::write(users.join(format!("{gone}.0")), "").unwrap();
    let stopped = printed(&project, &["demo", "stop", "--port", port]);
    assert_eq!(launched, Some(0));
    assert_eq!(stopped, "stopped the demo on port 7437\n");
    assert!(wait_until(7437, false), "the stale join is dropped and the demo ends");
    assert!(std::fs::read_dir(&users).unwrap().next().is_none(), "the stale join is pruned");
}
