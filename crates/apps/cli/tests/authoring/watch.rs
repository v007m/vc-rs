//! `vc watch` runs the selection, then re-runs it whenever a file it depends on is saved, after
//! a 200 ms debounce (D180).

use std::io::{BufRead, BufReader};
use std::process::{Child, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use vc_mock_server::MockServer;

use crate::{CONFIG, CRUD, Project, USERS, said};

/// A running `vc watch` whose stdout lines arrive on a channel; killed when dropped.
struct Watch {
    child: Child,
    lines: Receiver<String>,
}

impl Watch {
    fn start(project: &Project, args: &[&str]) -> Self {
        let mut child = project.process(args).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (send, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        Self { child, lines }
    }

    /// Waits up to 30 s for a summary line (`N passed …`) and returns it.
    fn next_summary(&self) -> Option<String> {
        self.summary_where(|_| true)
    }

    /// Waits up to 30 s for a summary line (`N passed …`) that `wanted` accepts, passing over the
    /// others: the OS may report the files the test wrote just before the watch began (macOS's
    /// file events do), and the run that starts is not the one the test waits for.
    fn summary_where(&self, wanted: impl Fn(&str) -> bool) -> Option<String> {
        let deadline = Instant::now() + Duration::from_secs(30);
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            let line = self.lines.recv_timeout(left).ok()?;
            if line.contains(" passed") && wanted(&line) {
                return Some(line);
            }
        }
        None
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The summary line `vc run users.crud` prints.
fn run_summary(project: &Project) -> String {
    let output = project.vc(&["run", "users.crud"]).output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.lines().find(|line| line.contains(" passed")).unwrap_or_else(|| panic!("no summary:\n{text}")).to_owned()
}

/// The units a duration the reports print ends in: `840ms`, `6.4s`, `2m 5s`, `12µs`.
const UNITS: [&str; 4] = ["ms", "s", "µs", "m"];

/// `line` with every number written as `#` and every duration, unit and all, as `#t`, so two
/// runs' timings compare equal however long each took: `4ms`, `1.0s` and `2m 5s` alike.
fn shape(line: &str) -> String {
    let mut shaped = String::new();
    let mut rest = line;
    while let Some(c) = rest.chars().next() {
        if c.is_ascii_digit() {
            rest = shape_number(&mut shaped, rest);
        } else {
            shaped.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    shaped
}

/// Writes the number `rest` starts with to `shaped`, as `#t` with its unit when it is a duration
/// (the second part of `2m 5s` joining the first), and returns the text after it.
fn shape_number<'a>(shaped: &mut String, rest: &'a str) -> &'a str {
    let number = rest.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(rest.len());
    let after = &rest[number..];
    let unit = after.find(|c: char| !c.is_alphabetic()).unwrap_or(after.len());
    if !UNITS.contains(&&after[..unit]) {
        shaped.push('#');
        return after;
    }
    if shaped.ends_with("#t ") {
        shaped.pop();
    } else {
        shaped.push_str("#t");
    }
    &after[unit..]
}

#[test]
fn shape_writes_a_duration_as_one_token_whatever_its_unit() {
    let line = |took: &str| shape(&format!("2 passed · 1 skipped  {took}"));
    assert_eq!(line("4ms"), "# passed · # skipped  #t");
    for took in ["1.04s", "1.0s", "12µs", "2m 5s", "1m 0s"] {
        assert_eq!(line(took), line("4ms"), "{took}");
    }
    assert_eq!(shape("run 12 of 340 (3 tests, 1.5s)"), "run # of # (# tests, #t)");
    assert_eq!(shape("10 tests in 2 files"), "# tests in # files");
}

#[tokio::test(flavor = "multi_thread")]
async fn runs_once_then_again_when_a_test_file_is_saved() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    project.serve_from(&server.base_url());
    let watch = Watch::start(&project, &["watch", "users.crud"]);
    let first = watch.next_summary().unwrap_or_else(|| panic!("no first run"));
    assert!(first.starts_with("2 passed"), "{first}");
    assert_eq!(shape(&first), shape(&run_summary(&project)), "watch prints the summary `vc run` prints");
    let more = format!("{}  - id: listsAgain\n    call: users#list\n", CRUD.1);
    project.write(CRUD.0, &more);
    let second = watch.summary_where(|line| line.starts_with("3 passed"));
    assert!(second.is_some(), "no run of the saved test file");
}

#[tokio::test(flavor = "multi_thread")]
async fn saving_an_api_file_re_runs_the_tests_that_call_it() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    project.serve_from(&server.base_url());
    let watch = Watch::start(&project, &["watch", "users.crud"]);
    assert!(watch.next_summary().is_some(), "no first run");
    let listed = "  - id: list\n    path: /users\n";
    assert!(USERS.1.contains(listed));
    project.write(USERS.0, &USERS.1.replacen(listed, &format!("{listed}    expect: {{status: 418}}\n"), 1));
    let second = watch.summary_where(|line| line.contains("1 failed"));
    assert!(second.is_some(), "no run after the api file was saved to expect a status `list` never answers");
}

#[test]
fn a_flag_the_run_refuses_ends_the_watch_with_exit_2() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    let output = project.vc(&["watch", "--profile", "nope"]).timeout(Duration::from_secs(20)).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
    assert!(said(&output).contains("error: --profile: no profile `nope`"), "{}", said(&output));
    assert!(!said(&output).contains("watching for saves"), "{}", said(&output));
}
