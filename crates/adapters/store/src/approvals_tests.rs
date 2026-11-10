use std::time::{Duration, Instant};

use tempfile::TempDir;
use vc_interfaces::{ApprovalRequest, ApprovalStatus, Approvals, Decision, Verdict};
use vc_model::Method;
use vc_testkit::{agent_action_writer_suite, approvals_suite};

use super::SqliteApprovals;

/// A queue in a fresh history in a directory `dirs` keeps alive.
fn fresh(dirs: &mut Vec<TempDir>) -> SqliteApprovals {
    let dir = tempfile::tempdir().unwrap();
    let queue = SqliteApprovals::new(&dir.path().join(".vibranium/history.db"));
    dirs.push(dir);
    queue
}

fn request() -> ApprovalRequest {
    ApprovalRequest {
        agent: "claude".into(),
        tool: "call_api".into(),
        profile: "staging".into(),
        methods: vec![Method::Delete],
        summary: "DELETE /users/7".into(),
        arguments: "{}".into(),
    }
}

#[tokio::test]
async fn passes_the_approvals_suite() {
    let mut dirs = Vec::new();
    approvals_suite(&mut || Box::new(fresh(&mut dirs)) as Box<dyn Approvals>).await;
}

#[test]
fn passes_the_agent_action_writer_suite() {
    let mut dirs = Vec::new();
    let log = fresh(&mut dirs);
    agent_action_writer_suite(&log);
    let connection = rusqlite::Connection::open(dirs[0].path().join(".vibranium/history.db")).unwrap();
    let logged: Vec<(String, Option<String>)> = connection
        .prepare("SELECT result, approved_by FROM agent_actions ORDER BY id")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(logged, [("ok".to_owned(), Some("sam".to_owned())), ("denied".to_owned(), None)]);
}

#[test]
fn no_file_is_an_empty_queue() {
    let mut dirs = Vec::new();
    let queue = fresh(&mut dirs);
    assert_eq!(queue.list(false), Ok(Vec::new()));
    assert!(!dirs[0].path().join(".vibranium").exists(), "reading creates nothing");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_decision_from_another_process_ends_the_wait_at_once() {
    let mut dirs = Vec::new();
    let agent = fresh(&mut dirs);
    let id = agent.create(&request()).unwrap();
    let person = agent.clone();
    let decided = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        person.decide(&Decision { id, verdict: Verdict::Approve, by: "sam".into() })
    });
    let started = Instant::now();
    assert_eq!(agent.wait(id, Duration::from_secs(30)).await, Ok(ApprovalStatus::Approved));
    assert!(started.elapsed() < Duration::from_secs(10), "{:?}", started.elapsed());
    assert_eq!(decided.join().unwrap(), Ok(ApprovalStatus::Approved));
}
