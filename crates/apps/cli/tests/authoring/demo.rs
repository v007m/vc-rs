//! `vc demo` is the small built-in service the `vc init` sample calls (D175): `start --detach`
//! re-launches `vc` in the background and writes a pidfile, `stop` reads it and ends it. A
//! `start` that finds it running joins it, and its `stop` leaves it to the others. The pidfile and
//! joins live in a private folder of the user's in the temp folder.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use crate::Project;
use crate::port::free_port;

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
pub(crate) fn wait_until(port: u16, up: bool) -> bool {
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
pub(crate) fn exit_code(project: &Project, args: &[&str]) -> Option<i32> {
    waited(launched(project.process(args))?)
}

/// `vc` started with no pipes.
fn launched(mut vc: std::process::Command) -> Option<Child> {
    vc.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().ok()
}

/// `vc` started with its stdout and stderr going to the file `log`, which a detached demo that
/// inherits it never holds open the way it would a pipe (see [`exit_code`]).
fn logged(mut vc: std::process::Command, log: &std::path::Path) -> Child {
    let file = std::fs::File::create(log).unwrap();
    vc.stdin(Stdio::null()).stdout(file.try_clone().unwrap()).stderr(file).spawn().unwrap()
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
    let (project, port) = (Project::empty(), free_port());
    let at = port.to_string();
    let started = exit_code(&project, &["demo", "start", "--port", &at, "--detach"]);
    let up = wait_until(port, true);
    let users = get(port, "/users");
    let echo = get(port, "/echo");
    let stopped = exit_code(&project, &["demo", "stop", "--port", &at]);
    assert_eq!((started, stopped), (Some(0), Some(0)), "`vc demo start --detach` and `stop` exit 0");
    assert!(up, "the demo answers on {port}");
    assert!(users.is_some_and(|line| line.contains(" 200 ")), "GET /users is 200");
    assert!(echo.is_some(), "it has an echo endpoint");
    assert!(wait_until(port, false), "`vc demo stop` ends it");
}

/// The sample's `afterAll` stops the demo even when `beforeAll` failed to start it, so stopping
/// nothing is not an error.
#[test]
fn stopping_a_demo_that_is_not_running_succeeds_and_says_so() {
    let (project, port) = (Project::empty(), free_port().to_string());
    let output = project.vc(&["demo", "stop", "--port", &port]).timeout(Duration::from_secs(20)).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(crate::said(&output).contains(&port), "names the port");
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
    let (project, port) = (Project::empty(), free_port());
    let at = &port.to_string();
    let launched = exit_code(&project, &["demo", "start", "--port", at, "--detach"]);
    let joined = printed(&project, &["demo", "start", "--port", at, "--detach"]);
    let first = printed(&project, &["demo", "stop", "--port", at]);
    let still_up = wait_until(port, true);
    let second = printed(&project, &["demo", "stop", "--port", at]);
    assert_eq!(launched, Some(0));
    assert_eq!(joined, format!("a demo already answers on port {port}\n"));
    assert_eq!(first, format!("left the demo on port {port} running, as another `vc demo start` uses it\n"));
    assert!(still_up, "the first stop leaves it running");
    assert_eq!(second, format!("stopped the demo on port {port}\n"));
    assert!(wait_until(port, false), "the second stop ends it");
}

#[test]
fn refuses_a_port_another_program_holds() {
    let (project, held) = (Project::empty(), TcpListener::bind(("127.0.0.1", 0)).unwrap());
    let port = held.local_addr().unwrap().port();
    let mut start = project.vc(&["demo", "start", "--port", &port.to_string(), "--detach"]);
    let output = start.timeout(Duration::from_secs(20)).output().unwrap();
    let text = crate::said(&output);
    assert_eq!(output.status.code(), Some(2), "{text}");
    assert!(text.contains(&format!("error: port {port} is taken by a program that is not the demo")), "{text}");
    assert!(text.contains("help: pick another `--port`, and change `sampleUrl`"), "{text}");
}

/// An echo server that answers `/echo` as the demo does but names no pid is not the demo, so
/// `start` neither joins it nor lets the tests call it.
#[test]
fn refuses_an_echo_server_that_names_no_demo_pid() {
    let (project, held) = (Project::empty(), TcpListener::bind(("127.0.0.1", 0)).unwrap());
    let port = held.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in held.incoming().flatten() {
            let mut lines = BufReader::new(&stream).lines();
            while lines.next().is_some_and(|line| line.is_ok_and(|line| !line.is_empty())) {}
            let body = r#"{"method":"GET","path":"/echo"}"#;
            let reply = format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n{body}", body.len());
            (&stream).write_all(reply.as_bytes()).ok();
        }
    });
    let mut start = project.vc(&["demo", "start", "--port", &port.to_string(), "--detach"]);
    let output = start.timeout(Duration::from_secs(20)).output().unwrap();
    let text = crate::said(&output);
    assert_eq!(output.status.code(), Some(2), "{text}");
    assert!(text.contains(&format!("error: port {port} is taken by a program that is not the demo")), "{text}");
}

#[test]
fn in_the_foreground_it_says_where_it_listens_and_how_to_stop_it() {
    let (project, port) = (Project::empty(), free_port());
    let mut vc = project.process(&["demo", "start", "--port", &port.to_string()]);
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
    assert!(line.starts_with(&format!("demo running on http://localhost:{port} (pid ")), "{line}");
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
    let (project, temp, port) = (Project::empty(), tempfile::tempdir().unwrap(), free_port());
    let at = &port.to_string();
    let launched =
        waited(launched(in_temp(&project, temp.path(), &["demo", "start", "--port", at, "--detach"])).unwrap());
    let users = demo_folder(temp.path()).join(format!("{port}.users"));
    gone_join(&users, Duration::from_hours(2));
    let stopped = said_by(in_temp(&project, temp.path(), &["demo", "stop", "--port", at]));
    assert_eq!(launched, Some(0));
    assert_eq!(stopped, format!("stopped the demo on port {port}\n"));
    assert!(wait_until(port, false), "the stale join is dropped and the demo ends");
    assert!(!users.exists(), "the stale join is pruned, and its folder goes with the demo");
}

/// A `start` run by a shell line that ended before the run did (its caller is gone) still keeps
/// the demo up within the hour after it joined.
#[cfg(unix)]
#[test]
fn a_recent_join_whose_caller_is_gone_still_keeps_the_demo_running() {
    let (project, temp, port) = (Project::empty(), tempfile::tempdir().unwrap(), free_port());
    let at = &port.to_string();
    let launched =
        waited(launched(in_temp(&project, temp.path(), &["demo", "start", "--port", at, "--detach"])).unwrap());
    gone_join(&demo_folder(temp.path()).join(format!("{port}.users")), Duration::ZERO);
    let first = said_by(in_temp(&project, temp.path(), &["demo", "stop", "--port", at]));
    let still_up = wait_until(port, true);
    let second = said_by(in_temp(&project, temp.path(), &["demo", "stop", "--port", at]));
    assert_eq!(launched, Some(0));
    assert_eq!(first, format!("left the demo on port {port} running, as another `vc demo start` uses it\n"));
    assert!(still_up, "the recent join keeps it running");
    assert_eq!(second, format!("stopped the demo on port {port}\n"));
    assert!(wait_until(port, false), "the second stop ends it");
}

#[cfg(unix)]
#[test]
fn the_pidfile_is_in_a_folder_only_the_user_can_open() {
    use std::os::unix::fs::PermissionsExt;
    let (project, temp, port) = (Project::empty(), tempfile::tempdir().unwrap(), free_port());
    let at = &port.to_string();
    let launched =
        waited(launched(in_temp(&project, temp.path(), &["demo", "start", "--port", at, "--detach"])).unwrap());
    let folder = demo_folder(temp.path());
    let mode = std::fs::metadata(&folder).map(|metadata| metadata.permissions().mode() & 0o777);
    let pid = std::fs::read_to_string(folder.join(format!("{port}.pid")));
    let stopped = said_by(in_temp(&project, temp.path(), &["demo", "stop", "--port", at]));
    assert_eq!(launched, Some(0));
    assert_eq!(mode.ok(), Some(0o700));
    assert!(pid.is_ok_and(|pid| pid.trim().parse::<u32>().is_ok()), "the pidfile holds the demo's pid");
    assert_eq!(stopped, format!("stopped the demo on port {port}\n"));
}

/// Another user could plant a link where the demo folder goes; `vc` neither writes nor reads
/// through it.
#[cfg(unix)]
#[test]
fn refuses_a_demo_folder_that_is_a_link() {
    let (project, temp, port) = (Project::empty(), tempfile::tempdir().unwrap(), free_port());
    let (elsewhere, at) = (temp.path().join("elsewhere"), &port.to_string());
    std::fs::create_dir(&elsewhere).unwrap();
    std::fs::write(elsewhere.join(format!("{port}.pid")), "1").unwrap();
    std::os::unix::fs::symlink(&elsewhere, demo_folder(temp.path())).unwrap();
    let start = in_temp(&project, temp.path(), &["demo", "start", "--port", at, "--detach"]);
    let output = assert_cmd::Command::from_std(start).timeout(Duration::from_secs(20)).output().unwrap();
    let stopped = said_by(in_temp(&project, temp.path(), &["demo", "stop", "--port", at]));
    let text = crate::said(&output);
    assert_eq!(output.status.code(), Some(2), "{text}");
    assert!(text.contains("vibranium-demo-") && text.contains(" is not a private folder of this user"), "{text}");
    assert!(get(port, "/users").is_none(), "no demo started");
    assert_eq!(stopped, format!("no demo running on port {port}\n"), "the planted pidfile is not read");
}

/// Two `start`s at once leave one demo, whose pidfile names it: the two `stop`s end it, and
/// none is left running.
#[test]
fn two_starts_at_once_leave_one_demo_that_their_stops_end() {
    let (project, logs, port) = (Project::empty(), tempfile::tempdir().unwrap(), free_port());
    let at = &port.to_string();
    let start =
        |log: &str| logged(project.process(&["demo", "start", "--port", at, "--detach"]), &logs.path().join(log));
    let (first, second) = (start("first"), start("second"));
    let started = (waited(first), waited(second));
    let said = |log: &str| std::fs::read_to_string(logs.path().join(log)).unwrap_or_default();
    let (first, second) = (said("first"), said("second"));
    let stops =
        (printed(&project, &["demo", "stop", "--port", at]), printed(&project, &["demo", "stop", "--port", at]));
    let output = format!("the first start said:\n{first}\nthe second start said:\n{second}");
    assert_eq!(started, (Some(0), Some(0)), "{output}");
    let joined = format!("a demo already answers on port {port}");
    let joins = [&first, &second].iter().filter(|said| said.contains(&joined)).count();
    assert_eq!(joins, 1, "one start launches the demo and the other joins it:\n{output}");
    assert_eq!(stops.0, format!("left the demo on port {port} running, as another `vc demo start` uses it\n"));
    assert_eq!(stops.1, format!("stopped the demo on port {port}\n"));
    assert!(wait_until(port, false), "no demo is left running");
}

/// A project whose `demo.users` file starts the demo on `port` in its `beforeAll` and stops it
/// in its `afterAll`, as the `vc init` sample does, and whose `demo.peak` load file mixes its one
/// test, which lists the users: a ramp to one user, then a second at one.
fn loading(port: u16) -> Project {
    let config = format!(
        "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {{baseUrl: 'http://localhost:{port}'}}\n"
    );
    let api = "baseUrl: ${var.baseUrl}\napis:\n  - id: list\n    path: /users\n    expect: {status: 200}\n";
    let tests = format!(
        "beforeAll:\n  - command: vc demo start --port {port} --detach\nafterAll:\n  - command: vc demo stop --port {port}\n\
         tests:\n  - id: listsUsers\n    call: demo#list\n"
    );
    let load = "mix:\n  - {test: demo.users#listsUsers, weight: 1}\nstages:\n  - {duration: 1s, users: 1}\n  - {duration: 1s, users: 1}\n";
    let files = [
        ("vibranium.config.yaml", config.as_str()),
        ("services/demo/demo.api.yaml", api),
        ("services/demo/users.test.yaml", tests.as_str()),
        ("load/demo/peak.load.yaml", load),
    ];
    Project::new(&files)
}

/// A load run's exit code, iterations and errors.
type Loaded = (Option<i32>, Option<u64>, Option<u64>);

/// The [`Loaded`] of `vc load demo.peak --json`.
fn loaded(project: &Project) -> Loaded {
    let output = project.vc(&["load", "demo.peak", "--json"]).timeout(Duration::from_secs(60)).output().unwrap();
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).unwrap_or_else(|_| panic!("{}", crate::said(&output)));
    (output.status.code(), report["iterations"].as_u64(), report["errors"].as_u64())
}

/// A load run runs each mixed file's `afterAll`, so the sample's `vc demo stop` runs after every
/// `vc load`: it must leave a demo the user started running, load after load (C19).
#[test]
fn a_demo_the_user_started_outlives_two_load_runs_that_start_and_stop_it() {
    let port = free_port();
    let (project, at) = (loading(port), &port.to_string());
    let launched = exit_code(&project, &["demo", "start", "--port", at, "--detach"]);
    let runs = [loaded(&project), loaded(&project)];
    let still_up = get(port, "/users");
    let stopped = printed(&project, &["demo", "stop", "--port", at]);
    assert_eq!(launched, Some(0));
    for &(code, iterations, errors) in &runs {
        assert_eq!((code, errors), (Some(0), Some(0)), "each load runs clean: {runs:?}");
        assert!(iterations.is_some_and(|count| count > 0), "{runs:?}");
    }
    assert!(still_up.is_some_and(|line| line.contains(" 200 ")), "the demo still answers after both loads");
    assert_eq!(stopped, format!("stopped the demo on port {port}\n"), "the loads' joins were all let go");
    assert!(wait_until(port, false), "the user's stop ends it");
}

/// A real stop takes the pidfile and the joins' folder with the demo, and keeps only the port's
/// lock file for the next `start` (D297). The test's `vc` uses the project's own temp folder, so
/// the user's has none of them.
#[cfg(unix)]
#[test]
fn a_real_stop_leaves_only_the_port_s_lock_file_and_none_in_the_user_s_temp_folder() {
    let (project, port) = (Project::empty(), free_port());
    let at = &port.to_string();
    let launched = exit_code(&project, &["demo", "start", "--port", at, "--detach"]);
    let joined = printed(&project, &["demo", "start", "--port", at, "--detach"]);
    let folder = demo_folder(project.home());
    let users = folder.join(format!("{port}.users")).is_dir();
    let stops =
        [printed(&project, &["demo", "stop", "--port", at]), printed(&project, &["demo", "stop", "--port", at])];
    let left: Vec<String> = std::fs::read_dir(&folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(launched, Some(0));
    assert_eq!(joined, format!("a demo already answers on port {port}\n"));
    assert!(users, "the join is noted in the port's joins folder");
    assert_eq!(stops[1], format!("stopped the demo on port {port}\n"));
    assert_eq!(left, [format!("{port}.lock")], "only the lock file is kept");
    let outside = demo_folder(&std::env::temp_dir()).join(format!("{port}.lock"));
    assert!(!outside.exists(), "the user's temp folder is left alone: {}", outside.display());
    assert!(wait_until(port, false), "the second stop ends it");
}
