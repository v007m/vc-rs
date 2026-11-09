use std::fs;
use std::path::Path;
use std::sync::Arc;

use vc_adapter_system::FsSources;
use vc_interfaces::{Approvals as _, Verdict};
use vc_testkit::{FakeVcs, MemoryApprovals};

use super::WriteRequest;
use crate::sample::builder;
use crate::{Engine, EngineError};

const CONFIG: &str = "syntaxVersion: 2\nvars: {baseUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n    agents: {writeDefinitions: true}\n  - id: staging\n    agents: {writeDefinitions: ask}\n  - id: bare\n";
const USERS: &str = "baseUrl: ${var.baseUrl}\napis:\n  - id: list\n    path: /users\n";
const TEST: &str = "tests:\n  - id: listsUsers\n    call: users#list\n";
const BROKEN: &str = "tests:\n  - id: listsUsers\n    call: users#nothing\n";
const NEW: &str = "services/users/crud.test.yaml";

fn project(queue: &MemoryApprovals) -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("vibranium.config.yaml"), CONFIG).unwrap();
    fs::create_dir_all(dir.path().join("services/users")).unwrap();
    fs::write(dir.path().join("services/users/users.api.yaml"), USERS).unwrap();
    let folder = Arc::new(FsSources::new(dir.path()).unwrap());
    let engine = builder(FakeVcs::default())
        .sources(folder.clone())
        .writer(folder)
        .approvals(Arc::new(queue.clone()), Arc::new(queue.clone()))
        .build()
        .unwrap();
    (dir, engine)
}

fn write(path: &str, text: &str, profile: &str) -> WriteRequest {
    WriteRequest { path: path.into(), text: text.into(), agent: "claude".into(), profiles: vec![profile.into()] }
}

fn exists(dir: &Path) -> bool {
    dir.join(NEW).exists()
}

#[tokio::test]
async fn a_write_the_profile_allows_is_written_and_logged() {
    let queue = MemoryApprovals::new();
    let (dir, engine) = project(&queue);
    let changes = engine.write_definition(&write(NEW, TEST, "dev")).await.unwrap();
    assert_eq!(changes.titles, [format!("Create {NEW}")]);
    assert_eq!(fs::read_to_string(dir.path().join(NEW)).unwrap(), TEST);
    let logged = queue.actions();
    assert_eq!((logged[0].tool.as_str(), logged[0].result.as_str()), ("write_definition", "ok"));
}

#[tokio::test]
async fn a_write_that_would_break_the_project_is_refused_before_anyone_is_asked() {
    let queue = MemoryApprovals::answering(Verdict::Approve);
    let (dir, engine) = project(&queue);
    let refused = engine.write_definition(&write(NEW, BROKEN, "staging")).await;
    let Err(EngineError::Edit { reason, .. }) = refused else { panic!("{refused:?}") };
    assert!(reason.starts_with("nothing was written"), "{reason}");
    assert_eq!(queue.list(false).unwrap(), []);
    assert_eq!(queue.actions()[0].result, format!("failed: {reason}"));
    assert!(!exists(dir.path()));
}

#[tokio::test]
async fn a_write_that_asks_waits_for_a_person() {
    let queue = MemoryApprovals::answering(Verdict::Deny);
    let (dir, engine) = project(&queue);
    let refused = engine.write_definition(&write(NEW, TEST, "staging")).await;
    let Err(EngineError::Refused { reason, .. }) = refused else { panic!("{refused:?}") };
    assert_eq!(reason, "request 1 was denied");
    assert_eq!(queue.list(false).unwrap()[0].request.summary, format!("write {NEW}"));
    assert!(!exists(dir.path()));
    let refused = engine.write_definition(&write(NEW, TEST, "bare")).await;
    assert!(matches!(refused, Err(EngineError::Refused { .. })), "{refused:?}");
}

#[tokio::test]
async fn only_api_test_and_load_files_in_the_project_can_be_written() {
    let queue = MemoryApprovals::new();
    let (_dir, engine) = project(&queue);
    for (path, said) in [
        ("vibranium.config.yaml", "an agent cannot write vibranium.config.yaml"),
        ("../crud.test.yaml", "`../crud.test.yaml` is not a path in the project"),
        ("notes.md", "`notes.md` is not a definition file"),
    ] {
        let refused = engine.write_definition(&write(path, TEST, "dev")).await;
        let Err(EngineError::Edit { reason, .. }) = refused else { panic!("{path}: {refused:?}") };
        assert_eq!(reason, said);
    }
    let logged: Vec<String> = queue.actions().into_iter().map(|action| action.result).collect();
    assert_eq!(
        logged,
        [
            "failed: an agent cannot write vibranium.config.yaml",
            "failed: `../crud.test.yaml` is not a path in the project",
            "failed: `notes.md` is not a definition file",
        ]
    );
}
