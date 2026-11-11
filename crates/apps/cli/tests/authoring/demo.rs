//! `vc demo` is the small built-in service the `vc init` sample calls (D175): `start --detach`
//! re-launches `vc` in the background and writes a pidfile, `stop` reads it and ends it. A
//! `start` that finds it running joins it, and its `stop` leaves it to the others. The pidfile and
//! joins live in a private folder of the user's in the temp folder.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Stdio};
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
    waited(launched(project.process(args))?)
}

/// `vc` started with no pipes.
fn launched(mut vc: std::process::Command) -> Option<Child> {
    vc.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().ok()
}

/// The exit code of `child`, or `None` when it runs past 20 s (then it is killed).
fn waited(mut child: Child) -> Option<i32> {
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
    said_by(project.process(args))
}

/// What `vc` prints, run with pipes: only for a `vc` that launches no demo.
fn said_by(vc: std::process::Command) -> String {
    let output = assert_cmd::Command::from_std(vc).timeout(Duration::from_secs(20)).output().unwrap();
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

/// `vc <args>` with `temp` as its temp folder, so a test sees only its own demo folder.
#[cfg(unix)]
fn in_temp(project: &Project, temp: &std::path::Path, args: &[&str]) -> std::process::Command {
    let mut vc = project.process(args);
    vc.env("TMPDIR", temp);
    vc
}

/// The user's demo folder in `temp`.
#[cfg(unix)]
fn demo_folder(temp: &std::path::Path) -> std::path::PathBuf {
    let uid = std::os::unix::fs::MetadataExt::uid(&std::fs::metadata(temp).unwrap());
    temp.join(format!("vibranium-demo-{uid}"))
}

/// Writes a join for a caller that has ended, made `age` ago.
#[cfg(unix)]
fn gone_join(users: &std::path::Path, age: Duration) -> std::path::PathBuf {
    let mut ended = std::process::Command::new("true").spawn().unwrap();
    let gone = ended.id();
    ended.wait().unwrap();
    std::fs::create_dir_all(users).unwrap();
    let join = users.join(format!("{gone}.0"));
    std::fs::File::create(&join).unwrap().set_modified(std::time::SystemTime::now() - age).unwrap();
    join
}

/// A join whose caller crashed before its `stop` (its pid is gone, and it joined over an hour
/// ago) no longer keeps the demo up.
#[cfg(unix)]
#[test]
fn a_join_whose_caller_is_gone_does_not_keep_the_demo_running() {
    let (project, temp) = (Project::empty(), tempfile::tempdir().unwrap());
    let port = "7437";
    let launched = waited(launched(in_temp(&project, temp.path(), &["demo", "start", "--port", port, "--detach"])).unwrap());
    let users = demo_folder(temp.path()).join("7437.users");
    gone_join(&users, Duration::from_secs(2 * 60 * 60));
    let stopped = said_by(in_temp(&project, temp.path(), &["demo", "stop", "--port", port]));
    assert_eq!(launched, Some(0));
    assert_eq!(stopped, "stopped the demo on port 7437\n");
    assert!(wait_until(7437, false), "the stale join is dropped and the demo ends");
    assert!(std::fs::read_dir(&users).unwrap().next().is_none(), "the stale join is pruned");
}

/// A `start` run by a shell line that ended before the run did (its caller is gone) still keeps
/// the demo up within the hour after it joined.
#[cfg(unix)]
#[test]
fn a_recent_join_whose_caller_is_gone_still_keeps_the_demo_running() {
    let (project, temp) = (Project::empty(), tempfile::tempdir().unwrap());
    let port = "7438";
    let launched = waited(launched(in_temp(&project, temp.path(), &["demo", "start", "--port", port, "--detach"])).unwrap());
    gone_join(&demo_folder(temp.path()).join("7438.users"), Duration::ZERO);
    let first = said_by(in_temp(&project, temp.path(), &["demo", "stop", "--port", port]));
    let still_up = wait_until(7438, true);
    let second = said_by(in_temp(&project, temp.path(), &["demo", "stop", "--port", port]));
    assert_eq!(launched, Some(0));
    assert_eq!(first, "left the demo on port 7438 running, as another `vc demo start` uses it\n");
    assert!(still_up, "the recent join keeps it running");
    assert_eq!(second, "stopped the demo on port 7438\n");
    assert!(wait_until(7438, false), "the second stop ends it");
}

#[cfg(unix)]
#[test]
fn the_pidfile_is_in_a_folder_only_the_user_can_open() {
    use std::os::unix::fs::PermissionsExt;
    let (project, temp) = (Project::empty(), tempfile::tempdir().unwrap());
    let launched = waited(launched(in_temp(&project, temp.path(), &["demo", "start", "--port", "7439", "--detach"])).unwrap());
    let folder = demo_folder(temp.path());
    let mode = std::fs::metadata(&folder).map(|metadata| metadata.permissions().mode() & 0o777);
    let pid = std::fs::read_to_string(folder.join("7439.pid"));
    let stopped = said_by(in_temp(&project, temp.path(), &["demo", "stop", "--port", "7439"]));
    assert_eq!(launched, Some(0));
    assert_eq!(mode.ok(), Some(0o700));
    assert!(pid.is_ok_and(|pid| pid.trim().parse::<u32>().is_ok()), "the pidfile holds the demo's pid");
    assert_eq!(stopped, "stopped the demo on port 7439\n");
}

/// Another user could plant a link where the demo folder goes; `vc` neither writes nor reads
/// through it.
#[cfg(unix)]
#[test]
fn refuses_a_demo_folder_that_is_a_link() {
    let (project, temp) = (Project::empty(), tempfile::tempdir().unwrap());
    let elsewhere = temp.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    std::fs::write(elsewhere.join("7440.pid"), "1").unwrap();
    std::os::unix::fs::symlink(&elsewhere, demo_folder(temp.path())).unwrap();
    let start = in_temp(&project, temp.path(), &["demo", "start", "--port", "7440", "--detach"]);
    let output = assert_cmd::Command::from_std(start).timeout(Duration::from_secs(20)).output().unwrap();
    let stopped = said_by(in_temp(&project, temp.path(), &["demo", "stop", "--port", "7440"]));
    let text = crate::said(&output);
    assert_eq!(output.status.code(), Some(2), "{text}");
    assert!(text.contains("vibranium-demo-") && text.contains(" is not a private folder of this user"), "{text}");
    assert!(get(7440, "/users").is_none(), "no demo started");
    assert_eq!(stopped, "no demo running on port 7440\n", "the planted pidfile is not read");
}

/// Two `start`s at once leave one demo, whose pidfile names it: the two `stop`s end it, and
/// none is left running.
#[test]
fn two_starts_at_once_leave_one_demo_that_their_stops_end() {
    let project = Project::empty();
    let start = || launched(project.process(&["demo", "start", "--port", "7441", "--detach"])).unwrap();
    let (first, second) = (start(), start());
    let started = (waited(first), waited(second));
    let stops = (printed(&project, &["demo", "stop", "--port", "7441"]), printed(&project, &["demo", "stop", "--port", "7441"]));
    assert_eq!(started, (Some(0), Some(0)));
    assert_eq!(stops.0, "left the demo on port 7441 running, as another `vc demo start` uses it\n");
    assert_eq!(stops.1, "stopped the demo on port 7441\n");
    assert!(wait_until(7441, false), "no demo is left running");
}
