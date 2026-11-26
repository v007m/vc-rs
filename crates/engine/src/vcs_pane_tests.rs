//! The git pane passes the status through, masks a diff and what a push or pull said, and
//! commits only project paths (D546–D550).

use std::sync::Arc;

use vc_interfaces::{VcsChange, VcsCommit, VcsFile, VcsStatus};
use vc_model::SourcePath;
use vc_testkit::{FakeVcs, MemoryEnvironment, MemorySources, VcsAsk};

use crate::sample::builder;
use crate::{Engine, EngineError};

/// The value of the declared secret `API_TOKEN`.
const TOKEN: &str = "tok-Kd4R-8wNq-Yb6T";

const CONFIG: &[u8] = b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n";

/// An engine over a project whose `.env` gives `API_TOKEN`, asking `vcs`.
fn engine(vcs: FakeVcs) -> Engine {
    let files: [(&str, &[u8]); 2] = [("vibranium.config.yaml", CONFIG), (".env.schema", b"API_TOKEN=\n")];
    builder(vcs)
        .sources(Arc::new(MemorySources::new(&files)))
        .environment(Arc::new(MemoryEnvironment::new(&[("API_TOKEN", TOKEN)])))
        .build()
        .unwrap()
}

fn path(text: &str) -> SourcePath {
    SourcePath::new(text).unwrap()
}

#[test]
fn the_status_is_the_version_controls_own() {
    let file = VcsFile { path: path("a.yaml"), change: VcsChange::Untracked, added: 3, removed: 0, binary: false };
    let status = VcsStatus { branch: "main".to_owned(), ahead: 2, files: vec![file], ..VcsStatus::default() };
    assert_eq!(engine(FakeVcs::default().with_status(status.clone())).vcs_status().unwrap(), status);
}

#[tokio::test]
async fn a_diff_leaves_with_its_secrets_masked() {
    let diff = format!("--- a/.env\n+++ b/.env\n-API_TOKEN=old\n+API_TOKEN={TOKEN}\n");
    let engine = engine(FakeVcs::default().with_diff(&path(".env"), &diff));
    let shown = engine.vcs_diff(".env").await.unwrap();
    assert!(!shown.contains(TOKEN) && shown.contains("+API_TOKEN="), "{shown}");
    let outside = engine.vcs_diff("../secrets.txt").await;
    assert!(matches!(outside, Err(EngineError::Edit { .. })), "{outside:?}");
}

#[test]
fn a_commit_takes_only_project_paths_and_its_message() {
    let (vcs, asks) = FakeVcs::default().recording();
    let engine = engine(vcs);
    let paths = ["services/a/a.api.yaml".to_owned(), ".env.schema".to_owned()];
    assert_eq!(engine.vcs_commit((&paths, "add a")).unwrap(), "abc1234");
    let commit = VcsCommit { paths: vec![path("services/a/a.api.yaml"), path(".env.schema")], message: "add a".into() };
    assert_eq!(asks.take(), [VcsAsk::Commit(commit)]);
    let outside = engine.vcs_commit((&["/etc/passwd".to_owned()], "m"));
    assert!(matches!(outside, Err(EngineError::Edit { .. })), "{outside:?}");
    assert!(asks.take().is_empty(), "nothing outside the project is committed");
}

#[tokio::test]
async fn a_push_and_a_pull_say_what_git_said_and_a_refusal_is_a_vcs_error() {
    let (vcs, asks) = FakeVcs::default().recording();
    let engine = engine(vcs);
    assert_eq!(engine.vcs_push().await.unwrap(), "pushed");
    assert_eq!(engine.vcs_pull().await.unwrap(), "pulled");
    assert_eq!(asks.take(), [VcsAsk::Push, VcsAsk::Pull]);
    let refused = self::engine(FakeVcs::default().failing("Not possible to fast-forward")).vcs_pull().await;
    let Err(EngineError::Vcs { reason, .. }) = refused else { panic!("expected a vcs error: {refused:?}") };
    assert_eq!(reason, "cannot pull: Not possible to fast-forward");
}
