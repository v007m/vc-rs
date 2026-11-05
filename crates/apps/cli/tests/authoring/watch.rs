//! `vc watch` runs the selection, then re-runs it whenever a file it depends on is saved, after
//! a 200 ms debounce (D180).

use std::io::{BufRead, BufReader};
use std::process::{Child, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use vc_mock_server::MockServer;

use crate::{CONFIG, CRUD, Project, USERS};

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
        let deadline = Instant::now() + Duration::from_secs(30);
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            let line = self.lines.recv_timeout(left).ok()?;
            if line.contains(" passed") {
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

#[tokio::test(flavor = "multi_thread")]
async fn runs_once_then_again_when_a_test_file_is_saved() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    project.serve_from(&server.base_url());
    let watch = Watch::start(&project, &["watch", "users.crud"]);
    let first = watch.next_summary().unwrap_or_else(|| panic!("no first run"));
    assert!(first.starts_with("2 passed"), "{first}");
    let more = format!("{}  - id: listsAgain\n    call: users#list\n", CRUD.1);
    project.write(CRUD.0, &more);
    let second = watch.next_summary().unwrap_or_else(|| panic!("no run after the save"));
    assert!(second.starts_with("3 passed"), "{second}");
}

#[tokio::test(flavor = "multi_thread")]
async fn saving_an_api_file_re_runs_the_tests_that_call_it() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    project.serve_from(&server.base_url());
    let watch = Watch::start(&project, &["watch", "users.crud"]);
    assert!(watch.next_summary().is_some(), "no first run");
    project.write(USERS.0, &format!("{}# saved again\n", USERS.1));
    assert!(watch.next_summary().is_some(), "no run after the api file was saved");
}
